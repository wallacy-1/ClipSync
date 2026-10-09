//! Máquina de estados de uma transferência. Pura: sem I/O nem `AppHandle`.
//!
//! ```text
//! emissor : Queued → Offered → Transferring → Verifying → Completed | Failed
//! receptor: Incoming → Transferring → Verifying → Completed | Failed
//! ativo ──conexão cai──► Waiting ──reconectou──► Offered (emissor) | aguarda Offer (receptor)
//! Cancelled{by} a partir de qualquer estado não terminal.
//! ```
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    Out,
    In,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Out => "out",
            Direction::In => "in",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "out" => Some(Direction::Out),
            "in" => Some(Direction::In),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CancelledBy {
    Local,
    Remote,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TransferState {
    /// Emissor: na fila, ainda não ofertado (sem conexão ou peer ocupado).
    Queued,
    /// Emissor: `FileOffer` enviado, aguardando `FileAccept`/`FileReject`.
    Offered,
    /// Receptor: `FileOffer` recebido, aguardando decisão local (auto ou prompt).
    Incoming,
    /// Dados em trânsito (os dois lados).
    Transferring,
    /// Receptor: todos os bytes gravados, calculando/comparando SHA-256 e finalizando.
    /// Emissor: `FileDone` enviado, aguardando `FileResult`.
    Verifying,
    /// Conexão caiu no meio; retomável. `reason` explica por quê.
    Waiting { reason: String },
    Completed,
    Failed { error: String },
    Cancelled { by: CancelledBy },
}

impl TransferState {
    pub fn is_terminal(&self) -> bool {
        matches!(self, TransferState::Completed | TransferState::Failed { .. } | TransferState::Cancelled { .. })
    }

    /// Nome curto persistido em `file_transfers.state`.
    pub fn kind(&self) -> &'static str {
        match self {
            TransferState::Queued => "queued",
            TransferState::Offered => "offered",
            TransferState::Incoming => "incoming",
            TransferState::Transferring => "transferring",
            TransferState::Verifying => "verifying",
            TransferState::Waiting { .. } => "waiting",
            TransferState::Completed => "completed",
            TransferState::Failed { .. } => "failed",
            TransferState::Cancelled { .. } => "cancelled",
        }
    }

    /// Detalhe persistido em `file_transfers.state_reason`.
    pub fn reason(&self) -> Option<String> {
        match self {
            TransferState::Waiting { reason } => Some(reason.clone()),
            TransferState::Failed { error } => Some(error.clone()),
            TransferState::Cancelled { by } => Some(
                match by {
                    CancelledBy::Local => "local",
                    CancelledBy::Remote => "remote",
                    CancelledBy::System => "system",
                }
                .into(),
            ),
            _ => None,
        }
    }

    pub fn from_db(kind: &str, reason: Option<&str>) -> Option<Self> {
        Some(match kind {
            "queued" => TransferState::Queued,
            "offered" => TransferState::Offered,
            "incoming" => TransferState::Incoming,
            "transferring" => TransferState::Transferring,
            "verifying" => TransferState::Verifying,
            "waiting" => TransferState::Waiting { reason: reason.unwrap_or("disconnected").into() },
            "completed" => TransferState::Completed,
            "failed" => TransferState::Failed { error: reason.unwrap_or("unknown").into() },
            "cancelled" => TransferState::Cancelled {
                by: match reason {
                    Some("remote") => CancelledBy::Remote,
                    Some("system") => CancelledBy::System,
                    _ => CancelledBy::Local,
                },
            },
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Emissor: conexão com o peer disponível e peer anuncia `file-v1`.
    Offer,
    /// Emissor: peer aceitou (com offset de retomada).
    Accepted,
    Rejected(String),
    /// Receptor: decisão local (auto ou usuário).
    Accept,
    Decline,
    /// Receptor: recebeu o último byte / emissor: enviou o último byte + `FileDone`.
    AllBytes,
    /// Receptor: hash confere e arquivo finalizado / emissor: `FileResult{ok:true}`.
    Verified,
    /// Receptor: hash não confere / emissor: `FileResult{ok:false}`.
    VerifyFailed(String),
    /// Conexão caiu (qualquer lado, qualquer estado ativo).
    Disconnected,
    /// Conexão voltou (emissor volta a ofertar; receptor volta a aguardar Offer).
    Reconnected,
    /// Cancelamento: local (usuário), remoto (`FileCancel`) ou sistema (revogação, expiração).
    Cancel(CancelledBy),
    /// Erro irrecuperável (origem sumiu, disco cheio no meio, protocolo).
    Fail(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidTransition {
    pub from: TransferState,
    pub event: Event,
}

impl std::fmt::Display for InvalidTransition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "transição inválida: {:?} + {:?}", self.from, self.event)
    }
}

/// Aplica um evento. Erro = a combinação não faz sentido (bug ou peer malicioso);
/// o chamador loga e ignora, nunca altera o estado.
pub fn transition(dir: Direction, from: &TransferState, event: Event) -> std::result::Result<TransferState, InvalidTransition> {
    use Direction::*;
    use TransferState as S;
    let invalid = || InvalidTransition { from: from.clone(), event: event.clone() };

    if from.is_terminal() {
        return Err(invalid());
    }
    // Transições comuns a qualquer estado não terminal.
    match &event {
        Event::Cancel(by) => return Ok(S::Cancelled { by: *by }),
        Event::Fail(e) => return Ok(S::Failed { error: e.clone() }),
        Event::Disconnected => {
            return match from {
                // Já parado: fica como está.
                S::Queued | S::Waiting { .. } => Ok(from.clone()),
                _ => Ok(S::Waiting { reason: "disconnected".into() }),
            };
        }
        _ => {}
    }

    let next = match (dir, from, &event) {
        // Emissor
        (Out, S::Queued, Event::Offer) => S::Offered,
        (Out, S::Waiting { .. }, Event::Reconnected) => S::Queued,
        (Out, S::Waiting { .. }, Event::Offer) => S::Offered,
        (Out, S::Offered, Event::Accepted) => S::Transferring,
        (Out, S::Offered, Event::Rejected(r)) => S::Failed { error: format!("rejected:{r}") },
        (Out, S::Transferring, Event::AllBytes) => S::Verifying,
        (Out, S::Verifying, Event::Verified) => S::Completed,
        (Out, S::Verifying, Event::VerifyFailed(e)) => S::Failed { error: format!("checksum:{e}") },

        // Receptor
        (In, S::Incoming, Event::Accept) => S::Transferring,
        (In, S::Incoming, Event::Decline) => S::Cancelled { by: CancelledBy::Local },
        // Retomada: o emissor reoferta; o receptor aceita direto do Waiting.
        (In, S::Waiting { .. }, Event::Accept) => S::Transferring,
        (In, S::Waiting { .. }, Event::Reconnected) => from.clone(),
        (In, S::Transferring, Event::AllBytes) => S::Verifying,
        (In, S::Verifying, Event::Verified) => S::Completed,
        (In, S::Verifying, Event::VerifyFailed(e)) => S::Failed { error: format!("checksum:{e}") },

        _ => return Err(invalid()),
    };
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use Direction::*;
    use TransferState as S;

    fn run(dir: Direction, start: S, events: &[Event]) -> S {
        events.iter().fold(start, |s, e| transition(dir, &s, e.clone()).unwrap())
    }

    #[test]
    fn sender_happy_path() {
        let s = run(Out, S::Queued, &[Event::Offer, Event::Accepted, Event::AllBytes, Event::Verified]);
        assert_eq!(s, S::Completed);
    }

    #[test]
    fn receiver_happy_path() {
        let s = run(In, S::Incoming, &[Event::Accept, Event::AllBytes, Event::Verified]);
        assert_eq!(s, S::Completed);
    }

    #[test]
    fn disconnect_mid_transfer_then_resume_sender() {
        let s = run(Out, S::Queued, &[Event::Offer, Event::Accepted, Event::Disconnected]);
        assert_eq!(s, S::Waiting { reason: "disconnected".into() });
        let s = run(Out, s, &[Event::Reconnected, Event::Offer, Event::Accepted]);
        assert_eq!(s, S::Transferring);
    }

    #[test]
    fn disconnect_mid_transfer_then_resume_receiver() {
        let s = run(In, S::Incoming, &[Event::Accept, Event::Disconnected]);
        assert!(matches!(s, S::Waiting { .. }));
        // Emissor reoferta → receptor aceita com offset direto do Waiting.
        let s = run(In, s, &[Event::Reconnected, Event::Accept, Event::AllBytes, Event::Verified]);
        assert_eq!(s, S::Completed);
    }

    #[test]
    fn disconnect_while_queued_keeps_queued() {
        assert_eq!(transition(Out, &S::Queued, Event::Disconnected).unwrap(), S::Queued);
    }

    #[test]
    fn cancel_and_fail_from_any_non_terminal() {
        for s in [S::Queued, S::Offered, S::Incoming, S::Transferring, S::Verifying, S::Waiting { reason: "x".into() }] {
            assert_eq!(transition(Out, &s, Event::Cancel(CancelledBy::Remote)).unwrap(), S::Cancelled { by: CancelledBy::Remote });
            assert_eq!(transition(In, &s, Event::Fail("disk".into())).unwrap(), S::Failed { error: "disk".into() });
        }
    }

    #[test]
    fn terminal_states_are_frozen() {
        for s in [S::Completed, S::Failed { error: "e".into() }, S::Cancelled { by: CancelledBy::Local }] {
            for e in [Event::Offer, Event::Accept, Event::Cancel(CancelledBy::Local), Event::Disconnected, Event::Fail("x".into())] {
                assert!(transition(Out, &s, e.clone()).is_err());
                assert!(transition(In, &s, e).is_err());
            }
        }
    }

    #[test]
    fn nonsense_transitions_are_rejected() {
        assert!(transition(Out, &S::Queued, Event::Accepted).is_err()); // aceito sem ofertar
        assert!(transition(Out, &S::Transferring, Event::Verified).is_err()); // verificado antes do fim
        assert!(transition(In, &S::Incoming, Event::AllBytes).is_err()); // bytes antes de aceitar
        assert!(transition(In, &S::Incoming, Event::Offer).is_err()); // receptor não oferta
        assert!(transition(Out, &S::Offered, Event::Accept).is_err()); // emissor não "aceita"
    }

    #[test]
    fn rejection_and_checksum_failures_are_failed_with_reason() {
        assert_eq!(transition(Out, &S::Offered, Event::Rejected("noSpace".into())).unwrap(), S::Failed { error: "rejected:noSpace".into() });
        assert_eq!(transition(In, &S::Verifying, Event::VerifyFailed("mismatch".into())).unwrap(), S::Failed { error: "checksum:mismatch".into() });
    }

    #[test]
    fn db_round_trip() {
        for s in [
            S::Queued,
            S::Offered,
            S::Incoming,
            S::Transferring,
            S::Verifying,
            S::Waiting { reason: "disconnected".into() },
            S::Completed,
            S::Failed { error: "checksum:x".into() },
            S::Cancelled { by: CancelledBy::Remote },
            S::Cancelled { by: CancelledBy::System },
        ] {
            let back = S::from_db(s.kind(), s.reason().as_deref()).unwrap();
            assert_eq!(back, s);
        }
        assert!(S::from_db("bogus", None).is_none());
        assert_eq!(Direction::parse("in"), Some(In));
        assert_eq!(Direction::parse("x"), None);
    }
}
