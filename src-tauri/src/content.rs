//! Identificação do tipo de conteúdo copiado (por ora, só texto).
use once_cell::sync::Lazy;
use regex::Regex;
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
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Classification {
    pub content_type: ContentType,
    pub is_sensitive: bool,
}

static URL: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(https?|ftp)://[^\s]+$").unwrap());
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_types() {
        assert_eq!(classify("https://tauri.app/v2").content_type, ContentType::Link);
        assert_eq!(classify("docker compose up -d").content_type, ContentType::Command);
        assert_eq!(classify("ana@example.com").content_type, ContentType::Email);
        assert!(classify("sk_live_51NfQ2mB7xKpLd8vR").is_sensitive);
        assert!(classify("4532 0151 1283 0366").is_sensitive);
        assert_eq!(classify("const a = 1;").content_type, ContentType::Code);
        assert_eq!(classify("Rua Aurora 128").content_type, ContentType::Text);
    }
}
