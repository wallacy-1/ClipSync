//! Nome de arquivo vindo do peer → nome seguro para o disco local.
//!
//! Regra: o nome remoto é **dado**, nunca caminho. Só sai daqui um único
//! componente, sem separadores, sem `..`, sem nomes reservados do Windows,
//! sem controles, limitado em bytes, e o caminho final é sempre
//! `dest_dir.join(safe)` verificado por [`resolve_in`].
use std::path::{Component, Path, PathBuf};

/// Limite em bytes UTF-8 do nome (NTFS/ext4/f2fs aceitam 255; deixamos folga
/// para sufixos ` (2)` e para o `.part`).
pub const MAX_NAME_BYTES: usize = 200;
pub const FALLBACK_NAME: &str = "arquivo";

const WINDOWS_RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2",
    "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

fn is_bad_char(c: char) -> bool {
    matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0') || c.is_control()
}

/// Sanitiza um nome remoto. Determinística, idempotente, nunca vazia.
pub fn sanitize(remote: &str) -> String {
    // 1. Só o último componente, seja qual for o separador usado pela origem.
    let last = remote.rsplit(['/', '\\']).next().unwrap_or("");
    // 2. Troca caracteres proibidos/controles por '_' e normaliza espaços.
    let mut s: String = last.chars().map(|c| if is_bad_char(c) { '_' } else { c }).collect();
    s = s.trim().to_string();
    // 3. Windows não aceita ponto/espaço no fim.
    while s.ends_with('.') || s.ends_with(' ') {
        s.pop();
    }
    // 4. `.`/`..` e vazio.
    if s.is_empty() || s == "." || s == ".." || s.chars().all(|c| c == '.') {
        s = FALLBACK_NAME.into();
    }
    // 5. Nomes reservados (com ou sem extensão): CON, CON.txt…
    let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    if WINDOWS_RESERVED.contains(&stem.as_str()) {
        s = format!("_{s}");
    }
    // 6. Limite em bytes, preservando extensão curta e sem cortar no meio de um char.
    if s.len() > MAX_NAME_BYTES {
        let (stem, ext) = split_ext(&s);
        let ext = if ext.len() <= 16 { ext } else { String::new() };
        let budget = MAX_NAME_BYTES.saturating_sub(ext.len());
        let mut cut = stem.as_str();
        while cut.len() > budget {
            let mut idx = cut.len() - 1;
            while !cut.is_char_boundary(idx) {
                idx -= 1;
            }
            cut = &cut[..idx];
        }
        let cut = cut.trim_end_matches([' ', '.']);
        s = format!("{}{}", if cut.is_empty() { FALLBACK_NAME } else { cut }, ext);
    }
    s
}

/// `("foto", ".jpg")`; `("arquivo", "")`; dotfiles (`.env`) ficam sem extensão.
pub fn split_ext(name: &str) -> (String, String) {
    match name.rfind('.') {
        Some(i) if i > 0 && i < name.len() - 1 => (name[..i].to_string(), name[i..].to_string()),
        _ => (name.to_string(), String::new()),
    }
}

/// `nome.ext` → `nome (n).ext` para o primeiro `n ≥ 2` em que `exists` é falso.
/// `exists` recebe o nome candidato (só o nome, não o caminho).
pub fn dedupe_name(safe: &str, mut exists: impl FnMut(&str) -> bool) -> String {
    if !exists(safe) {
        return safe.to_string();
    }
    let (stem, ext) = split_ext(safe);
    for n in 2..10_000 {
        let candidate = format!("{stem} ({n}){ext}");
        if !exists(&candidate) {
            return candidate;
        }
    }
    format!("{stem} ({}){ext}", uuid::Uuid::new_v4().simple())
}

/// Junta `dest_dir` + nome já sanitizado e garante que o resultado é um filho
/// direto de `dest_dir` (defesa em profundidade contra qualquer sanitização futura
/// com bug). Não toca no disco.
pub fn resolve_in(dest_dir: &Path, safe_name: &str) -> Option<PathBuf> {
    let p = Path::new(safe_name);
    let mut comps = p.components();
    match (comps.next(), comps.next()) {
        (Some(Component::Normal(_)), None) => {}
        _ => return None,
    }
    let full = dest_dir.join(safe_name);
    if full.parent() != Some(dest_dir) {
        return None;
    }
    Some(full)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_paths_and_traversal() {
        assert_eq!(sanitize("../../etc/passwd"), "passwd");
        assert_eq!(sanitize("..\\..\\Windows\\system32\\x.dll"), "x.dll");
        assert_eq!(sanitize("C:\\evil\\a.txt"), "a.txt");
        assert_eq!(sanitize("/tmp/../x"), "x");
        assert_eq!(sanitize(".."), FALLBACK_NAME);
        assert_eq!(sanitize("..."), FALLBACK_NAME);
        assert_eq!(sanitize("a/.."), FALLBACK_NAME);
    }

    #[test]
    fn replaces_invalid_chars_and_trailing_dots_spaces() {
        assert_eq!(sanitize("a<b>:c\"d|e?f*g.txt"), "a_b__c_d_e_f_g.txt");
        assert_eq!(sanitize("  nome.txt.  "), "nome.txt");
        assert_eq!(sanitize("tab\tnul\0.txt"), "tab_nul_.txt");
        assert_eq!(sanitize(""), FALLBACK_NAME);
        assert_eq!(sanitize("   "), FALLBACK_NAME);
    }

    #[test]
    fn windows_reserved_names_are_prefixed() {
        assert_eq!(sanitize("CON"), "_CON");
        assert_eq!(sanitize("con.txt"), "_con.txt");
        assert_eq!(sanitize("LPT1.log"), "_LPT1.log");
        assert_eq!(sanitize("CONSOLE.txt"), "CONSOLE.txt"); // não é reservado
    }

    #[test]
    fn unicode_is_preserved() {
        assert_eq!(sanitize("relatório ✓ 日本.pdf"), "relatório ✓ 日本.pdf");
        assert_eq!(sanitize("🎉 festa.png"), "🎉 festa.png");
    }

    #[test]
    fn long_names_are_cut_on_char_boundary_keeping_extension() {
        let long = format!("{}.pdf", "é".repeat(300)); // 600 bytes de stem
        let s = sanitize(&long);
        assert!(s.len() <= MAX_NAME_BYTES, "{}", s.len());
        assert!(s.ends_with(".pdf"));
        assert!(std::str::from_utf8(s.as_bytes()).is_ok());
        let no_ext = "x".repeat(500);
        assert_eq!(sanitize(&no_ext).len(), MAX_NAME_BYTES);
        let huge_ext = format!("a.{}", "b".repeat(300));
        assert!(sanitize(&huge_ext).len() <= MAX_NAME_BYTES);
    }

    #[test]
    fn idempotent() {
        for n in ["../x", "CON", "a<b>.txt", "relatório ✓.pdf", &"z".repeat(400)] {
            let once = sanitize(n);
            assert_eq!(sanitize(&once), once);
        }
    }

    #[test]
    fn dedupe_adds_counter() {
        let taken = ["foto.jpg", "foto (2).jpg"];
        assert_eq!(dedupe_name("foto.jpg", |n| taken.contains(&n)), "foto (3).jpg");
        assert_eq!(dedupe_name("livre.jpg", |n| taken.contains(&n)), "livre.jpg");
        assert_eq!(dedupe_name("semext", |n| n == "semext"), "semext (2)");
        assert_eq!(dedupe_name(".env", |n| n == ".env"), ".env (2)");
    }

    #[test]
    fn resolve_in_only_accepts_direct_children() {
        let dir = Path::new("/downloads/ClipSync");
        assert_eq!(resolve_in(dir, "a.txt"), Some(dir.join("a.txt")));
        assert!(resolve_in(dir, "../a.txt").is_none());
        assert!(resolve_in(dir, "sub/a.txt").is_none());
        assert!(resolve_in(dir, "..").is_none());
        assert!(resolve_in(dir, "").is_none());
        #[cfg(windows)]
        assert!(resolve_in(dir, "C:\\a.txt").is_none());
    }
}
