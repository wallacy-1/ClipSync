//! Formato rico (HTML) junto do texto: o item continua sendo texto e o HTML vai em
//! `metadata.html`. Aqui ficam os limites, o acesso ao anexo e a sanitização do
//! recebido (o HTML vai parar em apps de terceiros).
use crate::models::ClipboardItem;

/// HTML maior que isso é descartado (o texto segue). Texto (1 MB) + HTML cabem em `MAX_MESSAGE`.
pub const MAX_HTML_BYTES: usize = 512 * 1024;

pub fn html_of(item: &ClipboardItem) -> Option<&str> {
    item.metadata.get("html").and_then(|v| v.as_str()).filter(|h| !h.trim().is_empty())
}

/// Troca (ou remove, com `None`) o HTML do item sem mexer no resto do `metadata`.
pub fn set_html(item: &mut ClipboardItem, html: Option<String>) {
    if !item.metadata.is_object() {
        item.metadata = serde_json::json!({});
    }
    if let Some(m) = item.metadata.as_object_mut() {
        match html {
            Some(h) => m.insert("html".into(), serde_json::Value::String(h)),
            None => m.remove("html"),
        };
    }
}

/// Cópia para a UI: o HTML (até 512 KB por item) fica no backend; a UI só precisa saber que o
/// item é formatado (`metadata.formatted = true`). Não mexe no fio nem no banco.
pub fn for_ui(item: &ClipboardItem) -> ClipboardItem {
    let mut out = item.clone();
    let formatted = html_of(item).is_some();
    set_html(&mut out, None);
    if let (true, Some(m)) = (formatted, out.metadata.as_object_mut()) {
        m.insert("formatted".into(), serde_json::Value::Bool(true));
    }
    out
}

/// HTML capturado localmente: sensível nunca guarda, vazio ou grande demais é descartado.
pub fn usable_local(html: Option<String>, sensitive: bool) -> Option<String> {
    html.filter(|h| !sensitive && !h.trim().is_empty() && h.len() <= MAX_HTML_BYTES)
}

/// HTML recebido de um peer: com o ajuste desligado (ou item sensível) é descartado; senão
/// sanitizado, e na dúvida sobra só o texto.
pub fn normalize_incoming(item: &mut ClipboardItem, keep_formatting: bool) {
    let html = html_of(item).map(str::to_string);
    let clean = html.and_then(|h| if keep_formatting && h.len() <= MAX_HTML_BYTES { sanitize_html(&h) } else { None });
    let clean = clean.filter(|_| !item.is_sensitive);
    set_html(item, clean);
}

/// Atributos cujo valor é uma URL.
const URL_ATTRS: [&str; 12] =
    ["href", "src", "action", "formaction", "xlink:href", "background", "poster", "data", "cite", "codebase", "lowsrc", "dynsrc"];
/// Elementos removidos junto com o conteúdo.
const DROP_WITH_CONTENT: [&str; 8] = ["script", "style", "iframe", "object", "applet", "noscript", "template", "frameset"];
/// Elementos removidos só a tag (vazios ou sem utilidade num fragmento).
const DROP_TAG: [&str; 5] = ["embed", "link", "meta", "base", "frame"];

struct Tag {
    closing: bool,
    name: String,
    attrs: Vec<(String, Option<String>)>,
    self_closing: bool,
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

/// Lê a tag que começa em `s[start] == '<'`. `None` = malformada/ambígua (o chamador desiste).
fn parse_tag(s: &str, start: usize) -> Option<(Tag, usize)> {
    let b = s.as_bytes();
    let mut j = start + 1;
    let closing = *b.get(j)? == b'/';
    if closing {
        j += 1;
    }
    let name_start = j;
    while j < b.len() && !is_ws(b[j]) && b[j] != b'/' && b[j] != b'>' {
        if !(b[j].is_ascii_alphanumeric() || b[j] == b'-' || b[j] == b':') {
            return None;
        }
        j += 1;
    }
    if j == name_start {
        return None;
    }
    let name = s[name_start..j].to_ascii_lowercase();
    let mut attrs = Vec::new();
    let mut self_closing = false;
    loop {
        while j < b.len() && (is_ws(b[j]) || (b[j] == b'/' && b.get(j + 1) != Some(&b'>'))) {
            j += 1;
        }
        match b.get(j)? {
            b'>' => return Some((Tag { closing, name, attrs, self_closing }, j + 1)),
            b'/' => {
                self_closing = true;
                j += 1;
                continue;
            }
            _ => {}
        }
        let an = j;
        while j < b.len() && !is_ws(b[j]) && !matches!(b[j], b'=' | b'>' | b'/') {
            if matches!(b[j], b'"' | b'\'' | b'<' | b'`') {
                return None;
            }
            j += 1;
        }
        if j == an {
            return None; // '=' sem nome
        }
        let attr = s[an..j].to_ascii_lowercase();
        while j < b.len() && is_ws(b[j]) {
            j += 1;
        }
        if b.get(j) != Some(&b'=') {
            attrs.push((attr, None));
            continue;
        }
        j += 1;
        while j < b.len() && is_ws(b[j]) {
            j += 1;
        }
        let value = match *b.get(j)? {
            q @ (b'"' | b'\'') => {
                let vs = j + 1;
                let len = s[vs..].find(q as char)?;
                j = vs + len + 1;
                s[vs..vs + len].to_string()
            }
            _ => {
                let vs = j;
                while j < b.len() && !is_ws(b[j]) && b[j] != b'>' {
                    if matches!(b[j], b'"' | b'\'' | b'<' | b'=' | b'`') {
                        return None;
                    }
                    j += 1;
                }
                s[vs..j].to_string()
            }
        };
        attrs.push((attr, Some(value)));
    }
}

/// `Some(true)` = URL aceita, `Some(false)` = atributo descartado, `None` = suspeita de ofuscação.
fn url_ok(attr: &str, value: &str) -> Option<bool> {
    let lower = value.to_ascii_lowercase();
    if ["&#", "&colon", "&tab;", "&newline;"].iter().any(|p| lower.contains(p)) {
        return None;
    }
    let compact: String = lower.chars().filter(|c| !c.is_whitespace() && !c.is_control()).collect();
    if ["javascript:", "vbscript:", "livescript:"].iter().any(|p| compact.starts_with(p)) {
        return Some(false);
    }
    if compact.starts_with("data:") {
        return Some(attr == "src" && compact.starts_with("data:image/") && !compact.starts_with("data:image/svg"));
    }
    Some(true)
}

fn style_ok(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    !["expression", "javascript:", "behavior", "-moz-binding", "@import", "&#", "\\"].iter().any(|p| lower.contains(p))
}

/// Sanitiza o HTML recebido antes de ele ir ao clipboard: remove `script`/`style`/`iframe`/`object`/`embed`…
/// (com conteúdo, quando tem), comentários, atributos `on*`, `srcdoc` e URLs `javascript:`/`data:` perigosas.
/// Conservadora: tag malformada, bloco sem fechamento ou URL ofuscada → `None` (cai para só texto).
pub fn sanitize_html(input: &str) -> Option<String> {
    let lower = input.to_ascii_lowercase();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        if input.as_bytes()[i] != b'<' {
            let next = input[i..].find('<').map_or(input.len(), |p| i + p);
            out.push_str(&input[i..next]);
            i = next;
            continue;
        }
        if lower[i..].starts_with("<!--") {
            i += 4 + lower[i + 4..].find("-->")? + 3;
            continue;
        }
        let next = *input.as_bytes().get(i + 1)?;
        if next == b'!' || next == b'?' {
            // doctype, CDATA, instrução de processamento: sem utilidade num fragmento
            i += input[i..].find('>')? + 1;
            continue;
        }
        if !(next.is_ascii_alphabetic() || next == b'/') {
            out.push_str("&lt;");
            i += 1;
            continue;
        }
        let (tag, end) = parse_tag(input, i)?;
        i = end;
        let name = tag.name.as_str();
        if DROP_TAG.contains(&name) || (tag.closing && DROP_WITH_CONTENT.contains(&name)) {
            continue;
        }
        if DROP_WITH_CONTENT.contains(&name) {
            if !tag.self_closing {
                let close = format!("</{name}");
                let at = i + lower[i..].find(&close)?;
                i = at + lower[at..].find('>')? + 1;
            }
            continue;
        }
        out.push('<');
        if tag.closing {
            out.push('/');
            out.push_str(name);
            out.push('>');
            continue;
        }
        out.push_str(name);
        for (n, v) in &tag.attrs {
            if n.starts_with("on") || n == "srcdoc" {
                continue;
            }
            if let Some(v) = v {
                if n == "style" && !style_ok(v) {
                    continue;
                }
                if URL_ATTRS.contains(&n.as_str()) && !url_ok(n, v)? {
                    continue;
                }
            }
            out.push(' ');
            out.push_str(n);
            if let Some(v) = v {
                out.push_str("=\"");
                out.push_str(&v.replace('"', "&quot;"));
                out.push('"');
            }
        }
        out.push_str(if tag.self_closing { " />" } else { ">" });
    }
    (!out.trim().is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::ContentType;
    use crate::models::SyncState;

    fn s(h: &str) -> Option<String> {
        sanitize_html(h)
    }

    #[test]
    fn keeps_plain_formatting() {
        let h = "<table><tr><td style=\"color:red\">1</td></tr></table><p>oi <b>mundo</b> <a href=\"https://a.com/?x=1&amp;y=2\">link</a><br/></p>";
        assert_eq!(
            s(h).unwrap(),
            "<table><tr><td style=\"color:red\">1</td></tr></table><p>oi <b>mundo</b> <a href=\"https://a.com/?x=1&amp;y=2\">link</a><br /></p>"
        );
        assert_eq!(s("<p>ação — ok</p>").unwrap(), "<p>ação — ok</p>");
    }

    #[test]
    fn strips_script_iframe_object_embed_style() {
        let h = "<p>a</p><SCRIPT type=\"x\">alert(1)</SCRIPT><iframe src=\"https://x\"></iframe><object data=\"x\"><param name=a></object><embed src=\"x\"><style>p{}</style><p>b</p>";
        assert_eq!(s(h).unwrap(), "<p>a</p><p>b</p>");
    }

    #[test]
    fn strips_event_handlers_and_js_urls() {
        assert_eq!(s("<img src=\"a.png\" onerror=\"x()\" ONCLICK='y()'>").unwrap(), "<img src=\"a.png\">");
        assert_eq!(s("<a/onclick=x href=\"  JaVa\tScript:alert(1)\">t</a>").unwrap(), "<a>t</a>");
        assert_eq!(s("<a href='vbscript:x' title='a\"b'>t</a>").unwrap(), "<a title=\"a&quot;b\">t</a>");
        assert_eq!(s("<a href=\"data:text/html,<b>\">t</a>").unwrap(), "<a>t</a>");
        assert_eq!(s("<img src=\"data:image/png;base64,AAAA\">").unwrap(), "<img src=\"data:image/png;base64,AAAA\">");
        assert_eq!(s("<p style=\"width:expression(alert(1))\">t</p>").unwrap(), "<p>t</p>");
    }

    #[test]
    fn doubt_falls_back_to_text() {
        assert!(s("<a href=\"java&#115;cript:alert(1)\">t</a>").is_none());
        assert!(s("<a href=\"javascript&colon;alert(1)\">t</a>").is_none());
        assert!(s("<script>alert(1)").is_none());
        assert!(s("<iframe srcdoc=\"x\">").is_none());
        assert!(s("<a href=\"x").is_none());
        assert!(s("<p title=a\"b>t</p>").is_none());
        assert!(s("<scr<script>ipt>x</scr</script>ipt>").is_none());
        assert!(s("").is_none());
        assert!(s("<script>x</script>").is_none());
    }

    #[test]
    fn drops_comments_and_escapes_stray_lt() {
        assert_eq!(s("<!--StartFragment--><p>a < b</p><!--EndFragment-->").unwrap(), "<p>a &lt; b</p>");
    }

    fn item(html: Option<&str>, sensitive: bool) -> ClipboardItem {
        let mut it = ClipboardItem {
            id: "1".into(),
            content: "oi".into(),
            content_type: ContentType::Text,
            source_device_id: "d".into(),
            created_at: 0,
            expires_at: None,
            is_pinned: false,
            is_sensitive: sensitive,
            sync_state: SyncState::Local,
            metadata: serde_json::json!({ "outro": 1 }),
        };
        set_html(&mut it, html.map(str::to_string));
        it
    }

    #[test]
    fn incoming_is_sanitized_and_gated() {
        let mut it = item(Some("<b>oi</b><script>x</script>"), false);
        normalize_incoming(&mut it, true);
        assert_eq!(html_of(&it), Some("<b>oi</b>"));
        assert_eq!(it.metadata["outro"], 1);

        let mut off = item(Some("<b>oi</b>"), false);
        normalize_incoming(&mut off, false);
        assert!(html_of(&off).is_none());

        let mut sens = item(Some("<b>oi</b>"), true);
        normalize_incoming(&mut sens, true);
        assert!(html_of(&sens).is_none());

        let mut big = item(Some(&"a".repeat(MAX_HTML_BYTES + 1)), false);
        normalize_incoming(&mut big, true);
        assert!(html_of(&big).is_none());
    }

    #[test]
    fn for_ui_replaces_html_with_flag() {
        let it = item(Some("<b>oi</b>"), false);
        let ui = for_ui(&it);
        assert!(ui.metadata.get("html").is_none());
        assert_eq!(ui.metadata["formatted"], true);
        assert_eq!(ui.metadata["outro"], 1);
        assert_eq!(ui.content, "oi");
        // o original (banco/fio) não muda
        assert_eq!(html_of(&it), Some("<b>oi</b>"));
        let plain = for_ui(&item(None, false));
        assert!(plain.metadata.get("formatted").is_none());
    }

    #[test]
    fn local_html_limits() {
        assert_eq!(usable_local(Some("<b>x</b>".into()), false).as_deref(), Some("<b>x</b>"));
        assert!(usable_local(Some("<b>x</b>".into()), true).is_none());
        assert!(usable_local(Some("  ".into()), false).is_none());
        assert!(usable_local(Some("a".repeat(MAX_HTML_BYTES)), false).is_some());
        assert!(usable_local(Some("a".repeat(MAX_HTML_BYTES + 1)), false).is_none());
    }
}
