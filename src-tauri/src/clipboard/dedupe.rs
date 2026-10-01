//! Decisão única de "este conteúdo do clipboard deve virar um item?" para
//! todas as origens (watcher/poll do app, IME, inbox do IME).
//!
//! Três memórias, três perguntas:
//! - `last_written`: o próprio ClipSync escreveu isto (item recebido de outro
//!   dispositivo)? Então é eco — nunca reenviar (anti-loop).
//! - `last_handled`: acabamos de processar exatamente este hash? Então é o
//!   mesmo evento chegando por outro caminho (IME dispara 2–3× por cópia; IME e
//!   watcher veem a mesma cópia) — ignorar dentro de uma janela curta.
//! - `last_seen`: última leitura de *polling* (watcher / volta ao foreground).
//!   Poll só reage quando o hash muda; eventos (IME) não usam esta memória.

/// Janela em que o mesmo hash chegando de novo é o mesmo evento, não uma nova cópia.
pub const DUPLICATE_WINDOW_MS: i64 = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Conteúdo que o próprio ClipSync escreveu no clipboard (recebido de outro dispositivo).
    WrittenBySelf,
    /// Mesmo hash processado há menos de `DUPLICATE_WINDOW_MS`.
    Duplicate,
    /// Conteúdo novo: segue para classificação, SQLite e sync.
    New,
}

#[derive(Debug, Default)]
pub struct DedupeState {
    last_written: Option<String>,
    last_handled: Option<(String, i64)>,
    last_seen: Option<String>,
}

impl DedupeState {
    /// Chamar imediatamente **antes** de escrever no clipboard local um conteúdo
    /// vindo de outro dispositivo.
    pub fn note_written(&mut self, hash: &str) {
        self.last_written = Some(hash.to_string());
        // A escrita também é a última coisa vista pelo polling.
        self.last_seen = Some(hash.to_string());
    }

    /// Polling (watcher / `clipboard_poll_now`): só vale a pena processar se o
    /// hash mudou desde a última leitura. Registra a leitura.
    pub fn poll_changed(&mut self, hash: &str) -> bool {
        if self.last_seen.as_deref() == Some(hash) {
            return false;
        }
        self.last_seen = Some(hash.to_string());
        true
    }

    /// Registra o conteúdo presente no clipboard quando o app abriu, sem processá-lo.
    pub fn prime(&mut self, hash: &str) {
        self.last_seen = Some(hash.to_string());
    }

    /// Decisão para um conteúdo que chegou por qualquer origem. Registra o hash
    /// como visto/processado para as próximas chamadas.
    pub fn decide(&mut self, hash: &str, now_ms: i64) -> Decision {
        self.last_seen = Some(hash.to_string());
        if self.last_written.as_deref() == Some(hash) {
            return Decision::WrittenBySelf;
        }
        if let Some((h, t)) = &self.last_handled {
            if h == hash && now_ms - *t < DUPLICATE_WINDOW_MS {
                return Decision::Duplicate;
            }
        }
        self.last_handled = Some((hash.to_string(), now_ms));
        Decision::New
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_write_then_ime_callback_is_not_resent() {
        let mut d = DedupeState::default();
        d.note_written("abc");
        // O IME percebe a escrita do ClipSync e chama de volta.
        assert_eq!(d.decide("abc", 1_000), Decision::WrittenBySelf);
        // E de novo (o listener dispara 2–3×).
        assert_eq!(d.decide("abc", 1_050), Decision::WrittenBySelf);
        // O watcher também não reage: a escrita já é "vista".
        assert!(!d.poll_changed("abc"));
    }

    #[test]
    fn same_copy_arriving_three_times_makes_one_item() {
        let mut d = DedupeState::default();
        assert_eq!(d.decide("h1", 1_000), Decision::New);
        assert_eq!(d.decide("h1", 1_010), Decision::Duplicate);
        assert_eq!(d.decide("h1", 1_900), Decision::Duplicate);
    }

    #[test]
    fn same_text_copied_again_later_is_new() {
        let mut d = DedupeState::default();
        assert_eq!(d.decide("h1", 1_000), Decision::New);
        assert_eq!(d.decide("h1", 1_000 + DUPLICATE_WINDOW_MS), Decision::New);
    }

    #[test]
    fn ime_event_then_watcher_poll_does_not_repeat() {
        let mut d = DedupeState::default();
        assert_eq!(d.decide("h1", 1_000), Decision::New);
        // 1,2 s depois o watcher lê o mesmo clipboard.
        assert!(!d.poll_changed("h1"));
        // Conteúdo diferente volta a interessar ao watcher.
        assert!(d.poll_changed("h2"));
    }

    #[test]
    fn primed_startup_content_is_ignored_by_poll_but_inbox_can_still_deliver_it() {
        let mut d = DedupeState::default();
        d.prime("x");
        assert!(!d.poll_changed("x"));
        // O inbox do IME diz que o usuário copiou "x" com o núcleo morto: processa.
        assert_eq!(d.decide("x", 5_000), Decision::New);
    }

    #[test]
    fn a_new_local_copy_after_a_remote_write_is_sent() {
        let mut d = DedupeState::default();
        d.note_written("remote");
        assert_eq!(d.decide("local", 1_000), Decision::New);
        // Voltar a copiar exatamente o que veio do PC continua sendo eco.
        assert_eq!(d.decide("remote", 9_000), Decision::WrittenBySelf);
    }
}
