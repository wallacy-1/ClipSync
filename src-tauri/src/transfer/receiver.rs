//! Task do receptor: `FileOffer → FileAccept(offset) → DataFrame* → FileAck → FileDone → FileResult`.
//!
//! Escreve sequencialmente num `.part`; é a autoridade do offset durável
//! (`bytes_done` só avança em fronteira de chunk, depois de `sync_data`).
//! Qualquer quadro fora da sequência esperada é violação de protocolo: a
//! transferência falha e o `.part` é apagado — nunca se escreve fora do lugar.
use super::manager::{apply, set_progress, Ctl};
use super::state::{CancelledBy, Event};
use super::{dest, hash, ACK_EVERY_CHUNKS};
use crate::state::AppState;
use crate::transport::protocol::{DataFrame, FileCancelReason, FileOffer, Message, DATA_MAX};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::mpsc;

const ACK_INTERVAL: Duration = Duration::from_millis(250);
/// Sem `FileDone` depois do último byte: algo errado do outro lado.
const DONE_TIMEOUT: Duration = Duration::from_secs(60);

/// Valida a sequência estrita de quadros de um receptor (puro, testável):
/// próximo chunk, próximo fragmento, tamanhos coerentes, nunca além de `size`.
pub struct FrameSeq {
    size: u64,
    chunk: u64,
    received: u64,
    expected_chunk: u32,
    expected_frag: u16,
    expected_count: u16,
    chunk_bytes: u64,
}

pub struct Accepted {
    pub chunk_complete: bool,
    pub finished: bool,
}

impl FrameSeq {
    pub fn new(size: u64, chunk: u64, offset: u64) -> Self {
        Self { size, chunk, received: offset, expected_chunk: (offset / chunk.max(1)) as u32, expected_frag: 0, expected_count: 0, chunk_bytes: 0 }
    }

    pub fn received(&self) -> u64 {
        self.received
    }

    /// Aceita o quadro (avançando o estado) ou explica por que ele viola o protocolo.
    /// Um quadro rejeitado não altera nada.
    pub fn accept(&mut self, f: &DataFrame) -> std::result::Result<Accepted, &'static str> {
        let count = if self.expected_frag == 0 { f.frag_count } else { self.expected_count };
        if count == 0 {
            return Err("frag_count zero");
        }
        if f.chunk_index != self.expected_chunk || f.frag_index != self.expected_frag || f.frag_count != count {
            return Err("out-of-order or duplicate frame");
        }
        let len = f.data.len();
        if len == 0 || len > DATA_MAX {
            return Err("bad frame length");
        }
        let not_last_frag = (f.frag_index as usize + 1) < f.frag_count as usize;
        if not_last_frag && len != DATA_MAX {
            return Err("short fragment");
        }
        let chunk_bytes = if self.expected_frag == 0 { 0 } else { self.chunk_bytes };
        if chunk_bytes + len as u64 > self.chunk || self.received + len as u64 > self.size {
            return Err("frame overflows chunk or file");
        }
        let received = self.received + len as u64;
        let chunk_complete = f.frag_index + 1 == count;
        let finished = received == self.size;
        if chunk_complete && !finished && chunk_bytes + len as u64 != self.chunk {
            return Err("short chunk");
        }
        // Só agora o estado avança.
        self.expected_count = count;
        self.chunk_bytes = chunk_bytes + len as u64;
        self.received = received;
        if chunk_complete {
            self.expected_chunk += 1;
            self.expected_frag = 0;
        } else {
            self.expected_frag += 1;
        }
        Ok(Accepted { chunk_complete, finished })
    }
}

pub struct Params {
    pub app: AppHandle,
    pub id: String,
    pub device_id: String,
    pub offer: FileOffer,
    pub part: PathBuf,
    pub dest_dir: PathBuf,
    pub frames_rx: mpsc::Receiver<DataFrame>,
    pub ctl_rx: mpsc::UnboundedReceiver<Ctl>,
}

fn send(app: &AppHandle, device_id: &str, msg: Message) -> bool {
    app.state::<AppState>().send_to(device_id, msg)
}

struct Rx {
    app: AppHandle,
    id: String,
    device_id: String,
    size: u64,
    chunk: u64,
    part: PathBuf,
    dest_dir: PathBuf,
    safe_name: String,
    mime: Option<String>,
    offer_sha: Option<String>,
    /// `Some("clipboard")`: finaliza no diretório privado e aplica no clipboard.
    purpose: Option<String>,
}

impl Rx {
    fn fail(&self, error: &str, reason: FileCancelReason) {
        let _ = apply(&self.app, &self.id, Event::Fail(error.into()));
        send(&self.app, &self.device_id, Message::FileCancel { transfer_id: self.id.clone(), reason });
        let _ = std::fs::remove_file(&self.part);
    }

    fn cancel(&self, by: CancelledBy) {
        let _ = apply(&self.app, &self.id, Event::Cancel(by));
        let reason = match by {
            CancelledBy::Local => Some(FileCancelReason::User),
            CancelledBy::System => Some(FileCancelReason::Revoked),
            CancelledBy::Remote => None,
        };
        if let Some(reason) = reason {
            send(&self.app, &self.device_id, Message::FileCancel { transfer_id: self.id.clone(), reason });
        }
        let _ = std::fs::remove_file(&self.part);
    }

    /// Conexão caiu: guarda o que é durável e deixa retomável.
    async fn park(&self, file: &mut tokio::fs::File, durable: u64) {
        let _ = file.sync_data().await;
        let _ = self.app.state::<AppState>().db.transfer_set_progress(&self.id, durable);
        let _ = apply(&self.app, &self.id, Event::Disconnected);
    }

    /// Hash confere → move para o destino; senão apaga e falha. Sempre responde `FileResult`.
    async fn verify_and_finish(&self, file: tokio::fs::File, hasher: Sha256, done_sha: &str) {
        let _ = apply(&self.app, &self.id, Event::AllBytes);
        drop(file);
        let computed = hash::hex(hasher);
        let expected = self.offer_sha.as_deref().unwrap_or(done_sha);
        let ok = computed == expected && computed == done_sha;
        let state = self.app.state::<AppState>();
        let _ = state.db.transfer_set_hashes(&self.id, Some(expected), Some(&computed));
        // Comprimento final tem de bater com o anunciado.
        let len_ok = std::fs::metadata(&self.part).map(|m| m.len() == self.size).unwrap_or(false);
        if !ok || !len_ok {
            let why = if !len_ok { "length" } else { "mismatch" };
            log::warn!("[FILE] Checksum failed id={} expected={} got={} len_ok={len_ok}", self.id, &expected[..8.min(expected.len())], &computed[..8]);
            let _ = std::fs::remove_file(&self.part);
            let _ = apply(&self.app, &self.id, Event::VerifyFailed(why.into()));
            send(&self.app, &self.device_id, Message::FileResult { transfer_id: self.id.clone(), ok: false, error: Some(why.into()) });
            return;
        }
        let clip_image = crate::clipboard::image::is_clipboard(self.purpose.as_deref());
        let finalized = if clip_image {
            dest::finalize(&self.part, &self.dest_dir, &self.safe_name).map(|p| {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or(&self.safe_name).to_string();
                (p.to_string_lossy().into_owned(), name)
            })
        } else {
            dest::finalize_platform(&self.part, &self.dest_dir, &self.safe_name, self.mime.as_deref())
        };
        match finalized {
            Ok((final_path, name)) => {
                let _ = state.db.transfer_set_paths(&self.id, None, Some(&final_path), Some(&name));
                set_progress(&self.app, &self.id, self.size, self.size);
                let _ = apply(&self.app, &self.id, Event::Verified);
                log::info!("[FILE] Received id={} from={} size={} path={}", self.id, self.device_id, self.size, final_path);
                send(&self.app, &self.device_id, Message::FileResult { transfer_id: self.id.clone(), ok: true, error: None });
                if clip_image {
                    crate::clipboard::image::on_received(&self.app, &self.device_id, std::path::Path::new(&final_path));
                }
            }
            Err(e) => {
                let _ = std::fs::remove_file(&self.part);
                let _ = apply(&self.app, &self.id, Event::Fail(format!("finalize: {e}")));
                send(&self.app, &self.device_id, Message::FileResult { transfer_id: self.id.clone(), ok: false, error: Some("finalize".into()) });
            }
        }
    }
}

pub async fn run(p: Params) {
    let Params { app, id, device_id, offer, part, dest_dir, mut frames_rx, mut ctl_rx } = p;
    let state = app.state::<AppState>();
    let Ok(Some(rec)) = state.db.transfer_get(&id) else { return };
    let rx = Rx {
        app: app.clone(),
        id: id.clone(),
        device_id: device_id.clone(),
        size: offer.size,
        chunk: offer.chunk_size as u64,
        part: part.clone(),
        dest_dir,
        safe_name: rec.safe_name.clone(),
        mime: rec.mime.clone(),
        offer_sha: offer.sha256.clone(),
        purpose: offer.purpose.clone(),
    };

    // 1. Offset durável: o que a linha diz, limitado ao que o .part realmente tem,
    //    arredondado para baixo à fronteira de chunk. O .part é truncado a isso.
    let part_len = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    let mut offset = rec.bytes_done.min(part_len).min(rx.size);
    if rx.chunk > 0 {
        offset -= offset % rx.chunk;
    }
    let mut file = match tokio::fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).open(&part).await {
        Ok(f) => f,
        Err(e) => return rx.fail(&format!("part open: {e}"), FileCancelReason::Error),
    };
    if file.set_len(offset).await.is_err() || file.seek(std::io::SeekFrom::Start(offset)).await.is_err() {
        return rx.fail("part prepare", FileCancelReason::Error);
    }
    // 2. Retomada: re-hasheia o prefixo já em disco para continuar o SHA-256 incremental.
    let mut hasher = if offset > 0 {
        let p = part.clone();
        match tokio::task::spawn_blocking(move || hash::sha256_prefix_blocking(&p, Some(offset))).await {
            Ok(Ok(h)) => h,
            _ => return rx.fail("part rehash", FileCancelReason::Error),
        }
    } else {
        Sha256::new()
    };

    // 3. Aceita (a partir do Incoming ou do Waiting) e informa o offset.
    if apply(&app, &id, Event::Accept).is_err() {
        return rx.fail("state", FileCancelReason::Protocol);
    }
    let _ = state.db.transfer_set_progress(&id, offset);
    if !send(&app, &device_id, Message::FileAccept { transfer_id: id.clone(), offset }) {
        rx.park(&mut file, offset).await;
        return;
    }
    log::info!("[FILE] Receiving id={id} from={device_id} size={} from_offset={offset} part={}", rx.size, part.display());

    // 4. Loop de quadros/controle. Controle tem prioridade (biased).
    let mut seq = FrameSeq::new(rx.size, rx.chunk, offset);
    let mut durable = offset;
    let mut chunks_since_ack: u32 = 0;
    let mut last_ack = Instant::now();
    let mut done_sha: Option<String> = None;

    loop {
        let all_bytes = seq.received() == rx.size;
        if all_bytes {
            if let Some(sha) = done_sha.take() {
                return rx.verify_and_finish(file, hasher, &sha).await;
            }
        }
        tokio::select! {
            biased;
            ctl = ctl_rx.recv() => {
                match ctl {
                    Some(Ctl::Cancel(by)) => return rx.cancel(by),
                    Some(Ctl::Disconnected) | None => return rx.park(&mut file, durable).await,
                    Some(Ctl::Done { sha256 }) => { done_sha = Some(sha256); }
                    Some(other) => log::debug!("[FILE] ctl ignorado no receptor id={id}: {other:?}"),
                }
            }
            frame = frames_rx.recv(), if !all_bytes => {
                let Some(f) = frame else { return rx.park(&mut file, durable).await };
                // Sequência estrita: chunk/fragmento esperados, tamanhos coerentes.
                let acc = match seq.accept(&f) {
                    Ok(a) => a,
                    Err(why) => {
                        log::warn!("[FILE] Protocol violation id={id}: {why} (chunk={} frag={}/{} len={})", f.chunk_index, f.frag_index, f.frag_count, f.data.len());
                        return rx.fail(&format!("protocol: {why}"), FileCancelReason::Protocol);
                    }
                };
                if let Err(e) = file.write_all(&f.data).await {
                    return rx.fail(&format!("write: {e}"), FileCancelReason::Error);
                }
                hasher.update(&f.data);
                let received = seq.received();
                if acc.chunk_complete {
                    chunks_since_ack += 1;
                    let finished = acc.finished;
                    if chunks_since_ack >= ACK_EVERY_CHUNKS || last_ack.elapsed() >= ACK_INTERVAL || finished {
                        if let Err(e) = file.sync_data().await {
                            return rx.fail(&format!("fsync: {e}"), FileCancelReason::Error);
                        }
                        durable = received;
                        chunks_since_ack = 0;
                        last_ack = Instant::now();
                        set_progress(&app, &id, durable, rx.size);
                        send(&app, &device_id, Message::FileAck { transfer_id: id.clone(), received_through: durable });
                    }
                }
            }
            _ = tokio::time::sleep(DONE_TIMEOUT), if all_bytes => {
                return rx.fail("done timeout", FileCancelReason::Error);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(chunk: u32, frag: u16, count: u16, len: usize) -> DataFrame {
        DataFrame { transfer_id: [0; 16], chunk_index: chunk, frag_index: frag, frag_count: count, data: vec![0; len] }
    }

    #[test]
    fn accepts_a_well_formed_sequence_and_finishes() {
        let chunk = 2 * DATA_MAX as u64; // 2 fragmentos por chunk
        let size = chunk + 10; // 2 chunks; o último com 10 B
        let mut s = FrameSeq::new(size, chunk, 0);
        assert!(!s.accept(&frame(0, 0, 2, DATA_MAX)).unwrap().chunk_complete);
        let a = s.accept(&frame(0, 1, 2, DATA_MAX)).unwrap();
        assert!(a.chunk_complete && !a.finished);
        let a = s.accept(&frame(1, 0, 1, 10)).unwrap();
        assert!(a.chunk_complete && a.finished);
        assert_eq!(s.received(), size);
    }

    #[test]
    fn duplicate_and_out_of_order_frames_are_rejected() {
        let chunk = 2 * DATA_MAX as u64;
        let mut s = FrameSeq::new(10 * chunk, chunk, 0);
        s.accept(&frame(0, 0, 2, DATA_MAX)).unwrap();
        // duplicado
        assert_eq!(s.accept(&frame(0, 0, 2, DATA_MAX)).err(), Some("out-of-order or duplicate frame"));
        // pula chunk / count inconsistente
        assert_eq!(s.accept(&frame(1, 0, 2, DATA_MAX)).err(), Some("out-of-order or duplicate frame"));
        assert_eq!(s.accept(&frame(0, 1, 3, DATA_MAX)).err(), Some("out-of-order or duplicate frame"));
        // o correto ainda é aceito depois dos rejeitados (estado intocado)
        assert!(s.accept(&frame(0, 1, 2, DATA_MAX)).unwrap().chunk_complete);
        assert_eq!(s.received(), chunk);
    }

    #[test]
    fn size_violations_are_rejected() {
        let chunk = 2 * DATA_MAX as u64;
        let mut s = FrameSeq::new(chunk, chunk, 0);
        assert_eq!(s.accept(&frame(0, 0, 2, 0)).err(), Some("bad frame length"));
        assert_eq!(s.accept(&frame(0, 0, 2, DATA_MAX + 1)).err(), Some("bad frame length"));
        assert_eq!(s.accept(&frame(0, 0, 2, DATA_MAX - 1)).err(), Some("short fragment"));
        // chunk "cheio" antes da hora
        let mut s2 = FrameSeq::new(chunk, chunk, 0);
        assert_eq!(s2.accept(&frame(0, 0, 1, DATA_MAX)).err(), Some("short chunk"));
        // além do fim do arquivo
        let mut s3 = FrameSeq::new(5, chunk, 0);
        assert_eq!(s3.accept(&frame(0, 0, 1, 6)).err(), Some("frame overflows chunk or file"));
        assert!(s3.accept(&frame(0, 0, 1, 5)).unwrap().finished);
    }

    #[test]
    fn resume_offset_sets_expected_chunk() {
        let chunk = 2 * DATA_MAX as u64;
        let mut s = FrameSeq::new(4 * chunk, chunk, 2 * chunk);
        assert_eq!(s.accept(&frame(0, 0, 2, DATA_MAX)).err(), Some("out-of-order or duplicate frame"));
        assert!(s.accept(&frame(2, 0, 2, DATA_MAX)).is_ok());
    }
}
