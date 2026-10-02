//! Task do emissor: `FileOffer → FileAccept(offset) → DataFrame* → FileDone → FileResult`.
//! Lê o arquivo em chunks de `CHUNK_SIZE`, fatiados em quadros ≤ `DATA_MAX`,
//! com no máximo `WINDOW_CHUNKS` chunks sem `FileAck`. Nunca carrega o arquivo
//! inteiro; a memória é ~1 chunk + a fila bounded da conexão.
use super::manager::{apply, set_progress, Ctl};
use super::state::{CancelledBy, Event};
use super::source::{self, TransferSource};
use super::{dest, hash, ACK_TIMEOUT_SECS, WINDOW_CHUNKS};
use crate::state::AppState;
use crate::transport::protocol::{transfer_id_bytes, DataFrame, FileCancelReason, FileOffer, FileRejectReason, Message, CAP_FILE_ASK_V1, DATA_MAX};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::sync::mpsc;

/// Quanto esperamos pelo `FileAccept` e pelo `FileResult`.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
const RESULT_TIMEOUT: Duration = Duration::from_secs(180);
const BUSY_RETRY: Duration = Duration::from_secs(2);

enum Stop {
    /// Estado já aplicado; só sair.
    Done,
}

/// Apaga a cópia temporária de uma origem não-seekável ao sair da task.
struct TempGuard(Option<std::path::PathBuf>);
impl Drop for TempGuard {
    fn drop(&mut self) {
        if let Some(p) = &self.0 {
            let _ = std::fs::remove_file(p);
        }
    }
}

fn send(app: &AppHandle, device_id: &str, msg: Message) -> bool {
    app.state::<AppState>().send_to(device_id, msg)
}

fn cancel_peer(app: &AppHandle, device_id: &str, id: &str, reason: FileCancelReason) {
    send(app, device_id, Message::FileCancel { transfer_id: id.to_string(), reason });
}

/// Trata eventos de controle comuns a qualquer fase. `Some(Stop)` = a task acaba.
fn common(app: &AppHandle, id: &str, device_id: &str, ctl: &Ctl) -> Option<Stop> {
    match ctl {
        Ctl::Cancel(by) => {
            let _ = apply(app, id, Event::Cancel(*by));
            match by {
                CancelledBy::Local => cancel_peer(app, device_id, id, FileCancelReason::User),
                CancelledBy::System => cancel_peer(app, device_id, id, FileCancelReason::Revoked),
                CancelledBy::Remote => {}
            }
            Some(Stop::Done)
        }
        Ctl::Disconnected => {
            let _ = apply(app, id, Event::Disconnected);
            Some(Stop::Done)
        }
        // "Busy" é transitório (o peer ainda está fechando a task da conexão anterior,
        // ou recebe outro arquivo nosso): volta para a fila e tenta de novo em instantes.
        Ctl::Rejected(FileRejectReason::Busy) => {
            let _ = apply(app, id, Event::Disconnected);
            let app2 = app.clone();
            let dev = device_id.to_string();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(BUSY_RETRY).await;
                super::manager::kick(&app2, &dev);
            });
            Some(Stop::Done)
        }
        Ctl::Rejected(r) => {
            let _ = apply(app, id, Event::Rejected(format!("{r:?}").to_lowercase()));
            Some(Stop::Done)
        }
        _ => None,
    }
}

/// A conexão sumiu debaixo da task. Se havia um cancelamento na fila de controle,
/// ele vale mais do que "desconectou" (senão a transferência ficaria `waiting`).
fn lost_connection(app: &AppHandle, id: &str, device_id: &str, ctl_rx: &mut mpsc::UnboundedReceiver<Ctl>) -> Stop {
    while let Ok(c) = ctl_rx.try_recv() {
        if let Ctl::Cancel(_) = c {
            if let Some(s) = common(app, id, device_id, &c) {
                return s;
            }
        }
    }
    let _ = apply(app, id, Event::Disconnected);
    Stop::Done
}

fn fail(app: &AppHandle, id: &str, device_id: &str, error: &str, notify_peer: bool) -> Stop {
    let _ = apply(app, id, Event::Fail(error.into()));
    if notify_peer {
        let reason = if error.starts_with("source") { FileCancelReason::SourceUnavailable } else { FileCancelReason::Error };
        cancel_peer(app, device_id, id, reason);
    }
    Stop::Done
}

pub async fn run(app: AppHandle, id: String, device_id: String, data_tx: mpsc::Sender<DataFrame>, mut ctl_rx: mpsc::UnboundedReceiver<Ctl>) {
    let _ = run_inner(&app, &id, &device_id, data_tx, &mut ctl_rx).await;
}

async fn run_inner(app: &AppHandle, id: &str, device_id: &str, data_tx: mpsc::Sender<DataFrame>, ctl_rx: &mut mpsc::UnboundedReceiver<Ctl>) -> Result<(), Stop> {
    let state = app.state::<AppState>();
    let Ok(Some(rec)) = state.db.transfer_get(id) else { return Err(Stop::Done) };
    let Some(source_str) = rec.source.clone() else { return Err(fail(app, id, device_id, "source missing", false)) };
    let src = TransferSource::parse(&source_str);

    // Abre a origem (caminho ou content:// via ContentResolver → fd). Fora do runtime:
    // no Android uma fonte não-seekável é copiada para disco privado aqui.
    let tmp_dir = dest::outgoing_tmp_dir(app).ok();
    let opened = {
        let src2 = src.clone();
        match tokio::task::spawn_blocking(move || source::open(&src2, tmp_dir.as_deref())).await {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => return Err(fail(app, id, device_id, &e.to_string(), true)),
            Err(e) => return Err(fail(app, id, device_id, &format!("source open: {e}"), true)),
        }
    };
    let _tmp_guard = TempGuard(opened.temp_copy.clone());
    let size = opened.size;
    if size != rec.size {
        return Err(fail(app, id, device_id, "source changed", true));
    }

    // Origem seekável: SHA-256 antes da oferta (uma leitura extra, fora do runtime).
    let sha = match rec.sha256_expected.clone() {
        Some(s) => s,
        None => {
            let dup = match opened.file.try_clone() {
                Ok(d) => d,
                Err(e) => return Err(fail(app, id, device_id, &format!("source unreadable: {e}"), true)),
            };
            match hash::sha256_open_file(dup).await {
                Ok(s) => {
                    let _ = state.db.transfer_set_hashes(id, Some(&s), None);
                    s
                }
                Err(e) => return Err(fail(app, id, device_id, &format!("source unreadable: {e}"), true)),
            }
        }
    };
    // Cancelado/desconectado enquanto hasheava?
    while let Ok(c) = ctl_rx.try_recv() {
        if let Some(s) = common(app, id, device_id, &c) {
            return Err(s);
        }
    }

    let chunk = rec.chunk_size as u64;
    let total_chunks = super::total_chunks(size, rec.chunk_size);
    apply(app, id, Event::Offer).map_err(|_| Stop::Done)?;
    let offer = FileOffer {
        transfer_id: id.to_string(),
        name: rec.original_name.clone(),
        mime: rec.mime.clone(),
        size,
        chunk_size: rec.chunk_size,
        total_chunks,
        sha256: Some(sha.clone()),
        mtime: opened.file.metadata().ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64),
        purpose: rec.purpose.clone(),
    };
    if !send(app, device_id, Message::FileOffer(offer)) {
        return Err(lost_connection(app, id, device_id, ctl_rx));
    }

    // Aguarda FileAccept{offset}. Peer que pergunta ao usuário ganha minutos, não segundos.
    let accept_timeout = if super::manager::peer_has_cap(app, device_id, CAP_FILE_ASK_V1) {
        Duration::from_secs(super::ASK_WAIT_SECS)
    } else {
        HANDSHAKE_TIMEOUT
    };
    let offset = loop {
        match tokio::time::timeout(accept_timeout, ctl_rx.recv()).await {
            Ok(Some(Ctl::Accepted { offset })) => break offset,
            Ok(Some(c)) => {
                if let Some(s) = common(app, id, device_id, &c) {
                    return Err(s);
                }
            }
            Ok(None) => return Err(Stop::Done),
            Err(_) => return Err(fail(app, id, device_id, "accept timeout", true)),
        }
    };
    if offset > size || offset % chunk != 0 {
        cancel_peer(app, device_id, id, FileCancelReason::Protocol);
        return Err(fail(app, id, device_id, "protocol: bad resume offset", false));
    }
    apply(app, id, Event::Accepted).map_err(|_| Stop::Done)?;
    log::info!("[FILE] Sending id={id} to={device_id} size={size} from_offset={offset} chunks={total_chunks}");

    // O hash (dup do fd) moveu a posição: posiciona sempre.
    let mut file = tokio::fs::File::from_std(opened.file);
    if file.seek(std::io::SeekFrom::Start(offset)).await.is_err() {
        return Err(fail(app, id, device_id, "source not seekable", true));
    }

    let tid = transfer_id_bytes(id).ok_or(Stop::Done)?;
    let mut sent = offset;
    let mut acked = offset;
    set_progress(app, id, acked, size);
    let mut chunk_index = (offset / chunk) as u32;
    let mut buf = vec![0u8; rec.chunk_size as usize];
    let window = WINDOW_CHUNKS as u64 * chunk;

    while sent < size {
        // Controle pendente (cancel/disconnect/ack) sem bloquear.
        while let Ok(c) = ctl_rx.try_recv() {
            if let Ctl::Ack { received_through } = c {
                acked = acked.max(received_through.min(size));
                set_progress(app, id, acked, size);
            } else if let Some(s) = common(app, id, device_id, &c) {
                return Err(s);
            }
        }
        // Janela cheia: espera ACK (ou desiste da conexão por timeout).
        while sent - acked >= window {
            match tokio::time::timeout(Duration::from_secs(ACK_TIMEOUT_SECS), ctl_rx.recv()).await {
                Ok(Some(Ctl::Ack { received_through })) => {
                    acked = acked.max(received_through.min(size));
                    set_progress(app, id, acked, size);
                }
                Ok(Some(c)) => {
                    if let Some(s) = common(app, id, device_id, &c) {
                        return Err(s);
                    }
                }
                Ok(None) => return Err(Stop::Done),
                Err(_) => {
                    // Conexão provavelmente morta: derruba-a; o caminho normal de
                    // desconexão nos manda `Disconnected` e a retomada acontece depois.
                    log::warn!("[FILE] ack timeout id={id}; fechando conexão com {device_id}");
                    send(app, device_id, Message::Bye);
                    let _ = apply(app, id, Event::Disconnected);
                    return Err(Stop::Done);
                }
            }
        }

        let n = (size - sent).min(chunk) as usize;
        if let Err(e) = file.read_exact(&mut buf[..n]).await {
            return Err(fail(app, id, device_id, &format!("source read failed: {e}"), true));
        }
        let frag_count = n.div_ceil(DATA_MAX).max(1) as u16;
        for (i, piece) in buf[..n].chunks(DATA_MAX).enumerate() {
            let frame = DataFrame { transfer_id: tid, chunk_index, frag_index: i as u16, frag_count, data: piece.to_vec() };
            // Bounded: bloqueia quando a conexão não drenou os quadros anteriores.
            if data_tx.send(frame).await.is_err() {
                return Err(lost_connection(app, id, device_id, ctl_rx));
            }
        }
        sent += n as u64;
        chunk_index += 1;
    }

    // Todos os bytes entregues à conexão: Done + espera o veredito.
    if !send(app, device_id, Message::FileDone { transfer_id: id.to_string(), sha256: sha }) {
        return Err(lost_connection(app, id, device_id, ctl_rx));
    }
    apply(app, id, Event::AllBytes).map_err(|_| Stop::Done)?;

    loop {
        match tokio::time::timeout(RESULT_TIMEOUT, ctl_rx.recv()).await {
            Ok(Some(Ctl::Ack { received_through })) => {
                acked = acked.max(received_through.min(size));
                set_progress(app, id, acked, size);
            }
            Ok(Some(Ctl::Result { ok: true, .. })) => {
                set_progress(app, id, size, size);
                let _ = apply(app, id, Event::Verified);
                log::info!("[FILE] Completed id={id} to={device_id} size={size}");
                return Ok(());
            }
            Ok(Some(Ctl::Result { ok: false, error })) => {
                let _ = apply(app, id, Event::VerifyFailed(error.unwrap_or_else(|| "mismatch".into())));
                return Err(Stop::Done);
            }
            Ok(Some(c)) => {
                if let Some(s) = common(app, id, device_id, &c) {
                    return Err(s);
                }
            }
            Ok(None) => return Err(Stop::Done),
            Err(_) => return Err(fail(app, id, device_id, "result timeout", true)),
        }
    }
}
