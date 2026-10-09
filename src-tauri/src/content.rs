//! Identificação do tipo de conteúdo copiado (texto classificado; imagem vem do `clipboard::image`).
use once_cell::sync::Lazy;
use regex::Regex;
use crate::error::AppError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentType {
    Text,
    Link,
    Code,
    Command,
    Email,
    Token,
    Card,
    /// Imagem copiada: `content` é `img:<sha256 do PNG>`, o arquivo vive em `metadata.path`.
    Image,
}

impl ContentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContentType::Text => "text",
            ContentType::Link => "link",
            ContentType::Code => "code",
            ContentType::Command => "command",
            ContentType::Email => "email",
            ContentType::Token => "token",
            ContentType::Card => "card",
            ContentType::Image => "image",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "link" => ContentType::Link,
            "code" => ContentType::Code,
            "command" => ContentType::Command,
            "email" => ContentType::Email,
            "token" => ContentType::Token,
            "card" => ContentType::Card,
            "image" => ContentType::Image,
            _ => ContentType::Text,
        }
    }
    /// Categoria usada pelas regras de sincronização ("Tipos sincronizados").
    pub fn sync_category(&self) -> &'static str {
        match self {
            ContentType::Link => "link",
            ContentType::Code | ContentType::Command => "code",
            ContentType::Email => "email",
            ContentType::Token | ContentType::Card => "sensitive",
            ContentType::Text => "text",
            ContentType::Image => "image",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Classification {
    pub content_type: ContentType,
    pub is_sensitive: bool,
}

static URL: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^((https?|ftp)://|www\.)[^\s]+$").unwrap());
static EMAIL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}$").unwrap());
static TOKEN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"(?i)(sk_(live|test)_[A-Za-z0-9]{10,}",
        r"|ghp_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}",
        r"|AKIA[0-9A-Z]{16}",
        r"|xox[baprs]-[A-Za-z0-9\-]{10,}",
        r"|eyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}",
        r"|-----BEGIN [A-Z ]*PRIVATE KEY-----",
        r"|(api[_\-]?key|secret|token|password|senha)\s*[:=]\s*\S{8,})"
    ))
    .unwrap()
});
static CARD: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(?:\d[ \-]?){13,19}$").unwrap());
static COMMAND: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(\$ |sudo |docker |git |npm |pnpm |yarn |cargo |ssh |curl |wget |kubectl |pip |python |node |ls |cd |rm |cp |mv |mkdir |chmod |chown |grep |cat |tar |scp |make |brew |apt |winget |choco |powershell |pwsh )").unwrap()
});
static CODE_HINTS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(=>|;\s*$|\{\s*$|^\s*(const|let|var|fn|def|class|import|export|pub|use|function|return|if|for|while)\b|</?[a-zA-Z][^>]*>|^\s*#include|\)\s*\{)").unwrap()
});

fn luhn_ok(digits: &str) -> bool {
    let d: Vec<u32> = digits.chars().filter_map(|c| c.to_digit(10)).collect();
    if d.len() < 13 {
        return false;
    }
    let mut sum = 0;
    for (i, n) in d.iter().rev().enumerate() {
        let mut v = *n;
        if i % 2 == 1 {
            v *= 2;
            if v > 9 {
                v -= 9;
            }
        }
        sum += v;
    }
    sum % 10 == 0
}

pub fn classify(text: &str) -> Classification {
    let t = text.trim();
    let single_line = !t.contains('\n');

    if single_line && CARD.is_match(t) && luhn_ok(t) {
        return Classification { content_type: ContentType::Card, is_sensitive: true };
    }
    if TOKEN.is_match(t) {
        return Classification { content_type: ContentType::Token, is_sensitive: true };
    }
    if single_line && URL.is_match(t) {
        return Classification { content_type: ContentType::Link, is_sensitive: false };
    }
    if single_line && EMAIL.is_match(t) {
        return Classification { content_type: ContentType::Email, is_sensitive: false };
    }
    if single_line && COMMAND.is_match(t) {
        return Classification { content_type: ContentType::Command, is_sensitive: false };
    }
    let lines = t.lines().count();
    let code_lines = t.lines().filter(|l| CODE_HINTS.is_match(l)).count();
    if code_lines > 0 && (lines == 1 || code_lines * 2 >= lines) {
        return Classification { content_type: ContentType::Code, is_sensitive: false };
    }
    Classification { content_type: ContentType::Text, is_sensitive: false }
}

/// Alvo seguro para "Abrir" um item: `link` só http/https (`www.` ganha `https://`);
/// `email` vira `mailto:` sem ter como injetar assunto/corpo. Qualquer outra coisa é erro.
pub fn link_target(content: &str, ty: ContentType) -> Result<String, AppError> {
    let t = content.trim();
    // Espaço ou controle no meio: não é um alvo único e abrir poderia ser ambíguo.
    let bad_chars = t.chars().any(|c| c.is_whitespace() || c.is_control());
    match ty {
        ContentType::Link => {
            let not_web = || AppError::Other(crate::i18n::tr("err.open_not_web").into());
            if bad_chars {
                return Err(not_web());
            }
            let lower = t.to_ascii_lowercase();
            let rest = if let Some(r) = lower.strip_prefix("https://") {
                r
            } else if let Some(r) = lower.strip_prefix("http://") {
                r
            } else if lower.starts_with("www.") {
                return if lower.len() > 4 { Ok(format!("https://{t}")) } else { Err(not_web()) };
            } else {
                return Err(not_web());
            };
            // Precisa de um host depois do esquema.
            if rest.split(['/', '?', '#']).next().unwrap_or("").is_empty() {
                return Err(not_web());
            }
            Ok(t.to_string())
        }
        ContentType::Email => {
            if t.is_empty() || bad_chars || t.contains(['?', '#']) || !t.contains('@') {
                return Err(AppError::Other(crate::i18n::tr("err.open_bad_email").into()));
            }
            Ok(format!("mailto:{t}"))
        }
        _ => Err(AppError::Other(crate::i18n::tr("err.open_nothing").into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_types() {
        assert_eq!(classify("https://tauri.app/v2").content_type, ContentType::Link);
        assert_eq!(classify("HTTP://Example.com/a").content_type, ContentType::Link);
        assert_eq!(classify("www.example.com/a").content_type, ContentType::Link);
        assert_eq!(classify("docker compose up -d").content_type, ContentType::Command);
        assert_eq!(classify("ana@example.com").content_type, ContentType::Email);
        assert!(classify("sk_live_51NfQ2mB7xKpLd8vR").is_sensitive);
        assert!(classify("4532 0151 1283 0366").is_sensitive);
        assert_eq!(classify("const a = 1;").content_type, ContentType::Code);
        assert_eq!(classify("Rua Aurora 128").content_type, ContentType::Text);
    }

    #[test]
    fn link_target_only_web() {
        let l = |s: &str| link_target(s, ContentType::Link);
        assert_eq!(l("https://example.com").unwrap(), "https://example.com");
        assert_eq!(l("  HTTP://Example.com/a?b=1  ").unwrap(), "HTTP://Example.com/a?b=1");
        assert_eq!(l("www.example.com").unwrap(), "https://www.example.com");
        for bad in ["javascript:alert(1)", "file:///C:/Windows/notepad.exe", "ms-settings:", "intent://x#Intent;end", "ftp://x.com/a",
                    "https://", "http:///x", "www.", "example.com", "https://a.com b", "https://a.com
x"] {
            assert!(l(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn link_target_email_and_other_types() {
        assert_eq!(link_target(" ana@example.com ", ContentType::Email).unwrap(), "mailto:ana@example.com");
        for bad in ["ana@example.com?subject=x", "ana@example.com
bcc:x@y.z", "a na@example.com", "semarroba", ""] {
            assert!(link_target(bad, ContentType::Email).is_err(), "{bad}");
        }
        assert!(link_target("https://example.com", ContentType::Text).is_err());
        assert!(link_target("https://example.com", ContentType::Token).is_err());
    }
}
