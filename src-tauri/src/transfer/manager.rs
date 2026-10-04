//! Orquestra as transferências: tasks ativas, roteamento das mensagens `File*`, fila por device.
//! - Mensagens já chegam autenticadas; só se confere que o `transfer_id` pertence àquele device.
//! - `conn_id` amarra a task à conexão que a criou: a queda de uma conexão antiga não afeta a nova.
//! - Estado só muda via [`apply`] → `state::transition` → SQLite → evento.
use super::state::{transition, CancelledBy, Direction, Event, TransferState};
use super::store::{NewTransfer, TransferRecord};
use super::source::{self, TransferSource};
use super::{dest, names, receiver, sender, validate_offer, ASK_TTL_SECS, CHUNK_SIZE, MAX_FILE_SIZE, MAX_OPEN_PER_DEVICE};
use crate::db::StoredDevice;
use crate::error::{AppError, Result};
use crate::events;
use crate::models::now_ms;
use crate::state::AppState;
use crate::transport::protocol::{
    transfer_id_string, DataFrame, FileCancelReason, FileOffer, FileRejectReason, Message, CAP_FILE_ASK_V1, CAP_FILE_V1,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum Ctl {
    Accepted { offset: u64 },
    Rejected(FileRejectReason),
    Ack { received_through: u64 },
    Done { sha256: String },
    Result { ok: bool, error: Option<String> },
    Cancel(CancelledBy),
    Disconnected,
}

pub struct Active {
    pub device_id: String,
    pub conn_id: u64,
    pub direction: Direction,
    pub ctl: mpsc::UnboundedSender<Ctl>,
    /// Receptor: quadros de dados (bounded → backpressure até o disco).
    pub frames: Option<mpsc::Sender<DataFrame>>,
}

/// Oferta esperando o usuário decidir ("Aceitar/Recusar"). A linha já existe como `incoming`.
pub struct PendingOffer {
    pub device_id: String,
    pub conn_id: u64,
    pub offer: FileOffer,
    pub part: PathBuf,
    pub dest_dir: PathBuf,
}

#[derive(Default)]
pub struct TransferManager {
    active: Mutex<HashMap<String, Active>>,
    asks: Mutex<HashMap<String, PendingOffer>>,
    /// Aceitas pelo usuário enquanto a conexão do emissor tinha caído: entram direto na reoferta.
    accepted: Mutex<HashSet<String>>,
    last_progress_emit: Mutex<HashMap<String, Instant>>,
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    last_fgs_update: Mutex<HashMap<String, Instant>>,
}

/// Quadros de dados na fila de um receptor antes de o loop da conexão bloquear
/// (e, com ele, o TCP → backpressure até o emissor).
pub const RECEIVER_QUEUE_FRAMES: usize = 16;
const PROGRESS_EVERY: Duration = Duration::from_millis(250);

fn mgr(app: &AppHandle) -> tauri::State<'_, AppState> {
    app.state::<AppState>()
}

/// Erro = transição inválida (logada, estado intocado).
pub fn apply(app: &AppHandle, id: &str, event: Event) -> Result<TransferRecord> {
    let state = mgr(app);
    let rec = state.db.transfer_get(id)?.ok_or_else(|| AppError::Other(format!("transferência {id} não encontrada")))?;
    let next = transition(rec.direction, &rec.state, event.clone()).map_err(|e| {
        log::warn!("[FILE] {e} id={id}");
        AppError::Other(e.to_string())
    })?;
    if next == rec.state {
        return Ok(rec);
    }
    state.db.transfer_set_state(id, &next)?;
    let rec = state.db.transfer_get(id)?.unwrap_or(rec);
    log::info!("[FILE] {} id={} dir={} state={} reason={:?} bytes={}/{}", rec.safe_name, id, rec.direction.as_str(), next.kind(), next.reason(), rec.bytes_done, rec.size);
    events::emit(app, events::TRANSFER_STATE, &rec);
    if rec.direction == Direction::Out && matches!(next, TransferState::Completed | TransferState::Cancelled { .. }) {
        if let Some(src) = rec.source.as_deref() {
            super::folder::release(app, src);
        }
    }
    Ok(rec)
}

pub fn set_progress(app: &AppHandle, id: &str, bytes_done: u64, size: u64) {
    let state = mgr(app);
    let _ = state.db.transfer_set_progress(id, bytes_done);
    let mut last = state.transfers.last_progress_emit.lock().unwrap();
    let now = Instant::now();
    let due = last.get(id).map(|t| now.duration_since(*t) >= PROGRESS_EVERY).unwrap_or(true) || bytes_done == size;
    if due {
        last.insert(id.to_string(), now);
        drop(last);
        events::emit(app, events::TRANSFER_PROGRESS, serde_json::json!({ "id": id, "bytesDone": bytes_done, "size": size }));
        let (dir, name) = {
            let map = state.transfers.active.lock().unwrap();
            let dir = map.get(id).map(|a| a.direction);
            (dir, String::new())
        };
        if let Some(dir) = dir {
            let percent = if size == 0 { 100 } else { ((bytes_done as u128 * 100) / size as u128) as i32 };
            let name = state.db.transfer_get(id).ok().flatten().map(|r| r.safe_name).unwrap_or(name);
            fgs::on_progress(app, id, dir, &name, percent);
        }
    }
}

fn register(app: &AppHandle, id: &str, a: Active) {
    let dir = a.direction;
    mgr(app).transfers.active.lock().unwrap().insert(id.to_string(), a);
    fgs::on_active(app, id, dir, 0);
}

/// Chamado pela própria task ao terminar (qualquer motivo).
pub fn unregister(app: &AppHandle, id: &str) {
    let remaining = {
        let state = mgr(app);
        let mut map = state.transfers.active.lock().unwrap();
        map.remove(id);
        map.len()
    };
    mgr(app).transfers.last_progress_emit.lock().unwrap().remove(id);
    mgr(app).transfers.last_fgs_update.lock().unwrap().remove(id);
    if remaining == 0 {
        fgs::on_idle(app);
    }
}

/// Foreground Service do Android: só enquanto há transferência ativa. No-op fora
/// do Android. Fronteira explícita: IME = clipboard em background; FGS = transferência.
mod fgs {
    use super::*;

    pub fn on_active(app: &AppHandle, id: &str, dir: Direction, percent: i32) {
        #[cfg(target_os = "android")]
        {
            let state = mgr(app);
            let name = state.db.transfer_get(id).ok().flatten().map(|r| r.safe_name).unwrap_or_default();
            let title = if dir == Direction::Out { "Enviando arquivo" } else { "Recebendo arquivo" };
            if !super::super::android::fgs_start(title, &name, percent, id) {
                log::warn!("[FILE] FGS recusado (app em segundo plano?); a transferência segue sem ele id={id}");
            }
        }
        #[cfg(not(target_os = "android"))]
        let _ = (app, id, dir, percent);
    }

    pub fn on_progress(app: &AppHandle, id: &str, dir: Direction, name: &str, percent: i32) {
        #[cfg(target_os = "android")]
        {
            // ≤ 1 atualização/s: o NotificationService descarta acima de ~5/s por pacote.
            {
                let state = mgr(app);
                let mut last = state.transfers.last_fgs_update.lock().unwrap();
                let now = Instant::now();
                if last.get(id).map(|t| now.duration_since(*t) < Duration::from_secs(1)).unwrap_or(false) && percent < 100 {
                    return;
                }
                last.insert(id.to_string(), now);
            }
            let title = if dir == Direction::Out { "Enviando arquivo" } else { "Recebendo arquivo" };
            super::super::android::fgs_update(title, &format!("{name} · {percent}%"), percent, id);
        }
        #[cfg(not(target_os = "android"))]
        let _ = (app, id, dir, name, percent);
    }

    pub fn on_idle(app: &AppHandle) {
        #[cfg(target_os = "android")]
        {
            let _ = app;
            super::super::android::fgs_stop();
        }
        #[cfg(not(target_os = "android"))]
        let _ = app;
    }
}

/// Activity voltou ao primeiro plano: retenta o FGS que foi recusado em segundo plano.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub fn fgs_retry_if_needed(app: &AppHandle) {
    #[cfg(target_os = "android")]
    {
        let first = {
            let state = mgr(app);
            let map = state.transfers.active.lock().unwrap();
            map.iter().next().map(|(id, a)| (id.clone(), a.direction))
        };
        let Some((id, dir)) = first else { return };
        if super::android::fgs_running() {
            return;
        }
        let percent = mgr(app).db.transfer_get(&id).ok().flatten().map(|r| if r.size == 0 { 100 } else { ((r.bytes_done as u128 * 100) / r.size as u128) as i32 }).unwrap_or(0);
        log::info!("[FILE] Activity em primeiro plano com transferência ativa sem FGS: tentando de novo id={id}");
        fgs::on_active(app, &id, dir, percent);
    }
    #[cfg(not(target_os = "android"))]
    let _ = app;
}

fn is_active(app: &AppHandle, id: &str) -> bool {
    mgr(app).transfers.active.lock().unwrap().contains_key(id)
}

fn ctl_of(app: &AppHandle, device_id: &str, id: &str) -> Option<mpsc::UnboundedSender<Ctl>> {
    let state = mgr(app);
    let map = state.transfers.active.lock().unwrap();
    map.get(id).filter(|a| a.device_id == device_id).map(|a| a.ctl.clone())
}

/// Conexão atual com o device, se anuncia `file-v1`: `(conn_id, data_tx)`.
fn file_conn(app: &AppHandle, device_id: &str) -> Option<(u64, mpsc::Sender<DataFrame>)> {
    let state = mgr(app);
    let conns = state.connections.lock().unwrap();
    let c = conns.get(device_id)?;
    if !c.caps.iter().any(|x| x == CAP_FILE_V1) {
        return None;
    }
    Some((c.conn_id, c.data_tx.clone()))
}

pub fn peer_supports_files(app: &AppHandle, device_id: &str) -> bool {
    file_conn(app, device_id).is_some()
}

pub fn peer_has_cap(app: &AppHandle, device_id: &str, cap: &str) -> bool {
    let state = mgr(app);
    let conns = state.connections.lock().unwrap();
    conns.get(device_id).map(|c| c.caps.iter().any(|x| x == cap)).unwrap_or(false)
}

/// Enfileira o envio de uma origem local (caminho ou `content://` no Android) para
/// um device pareado. Retorna a linha criada; o envio real começa quando houver
/// conexão com `file-v1`.
pub fn send_file(app: &AppHandle, device_id: &str, source_str: &str) -> Result<TransferRecord> {
    send_file_with(app, device_id, source_str, None)
}

/// [`send_file`] com finalidade: `Some("clipboard")` = imagem copiada (o peer põe no
/// clipboard em vez de Downloads).
pub fn send_file_with(app: &AppHandle, device_id: &str, source_str: &str, purpose: Option<&str>) -> Result<TransferRecord> {
    let state = mgr(app);
    let dev = state.db.get_device(device_id)?.ok_or_else(|| AppError::Other("dispositivo não pareado".into()))?;
    if dev.blocked {
        return Err(AppError::Other("dispositivo bloqueado".into()));
    }
    let src = TransferSource::parse(source_str);
    let info = source::describe(&src)?;
    if info.size > MAX_FILE_SIZE {
        return Err(AppError::Other(format!("arquivo maior que o limite de {} GiB", MAX_FILE_SIZE >> 30)));
    }
    if state.db.transfer_open_for_device(device_id)?.len() >= MAX_OPEN_PER_DEVICE {
        return Err(AppError::Other("fila de transferências cheia para este dispositivo".into()));
    }
    let safe = names::sanitize(&info.name);
    let id = uuid::Uuid::new_v4().to_string();
    state.db.transfer_insert(&NewTransfer {
        id: &id,
        direction: Direction::Out,
        device_id,
        original_name: &info.name,
        safe_name: &safe,
        mime: info.mime.as_deref(),
        size: info.size,
        chunk_size: CHUNK_SIZE,
        sha256_expected: None,
        state: &TransferState::Queued,
        source: Some(&src.as_str()),
        temp_path: None,
        purpose,
    })?;
    let rec = state.db.transfer_get(&id)?.ok_or_else(|| AppError::Other("falha ao registrar".into()))?;
    log::info!("[FILE] Queued id={} name={:?} size={} to={} purpose={:?}", id, safe, info.size, device_id, purpose);
    events::emit(app, events::TRANSFER_STATE, &rec);
    kick(app, device_id);
    Ok(rec)
}

pub fn list(app: &AppHandle, limit: u32) -> Result<Vec<TransferRecord>> {
    mgr(app).db.transfer_list(limit)
}

pub fn clear_finished(app: &AppHandle) -> Result<u32> {
    let n = mgr(app).db.transfer_clear_finished()?;
    log::info!("[FILE] Cleared finished count={n}");
    super::folder::purge(app);
    Ok(n)
}

/// Cancelamento local (usuário). Avisa o peer se houver conexão.
pub fn cancel(app: &AppHandle, id: &str) -> Result<TransferRecord> {
    let state = mgr(app);
    let rec = state.db.transfer_get(id)?.ok_or_else(|| AppError::Other("transferência não encontrada".into()))?;
    if rec.state.is_terminal() {
        return Ok(rec);
    }
    if take_ask(app, id).is_some() {
        // Pedido de aceite aberto: cancelar = recusar.
        return decline(app, id);
    }
    if let Some(ctl) = ctl_of(app, &rec.device_id, id) {
        // A task fecha arquivo, apaga .part, aplica o estado e avisa o peer.
        let _ = ctl.send(Ctl::Cancel(CancelledBy::Local));
        return Ok(rec);
    }
    // Sem task (queued/waiting): resolve aqui.
    let rec = apply(app, id, Event::Cancel(CancelledBy::Local))?;
    cleanup_part(app, &rec);
    let notified = state.send_to(&rec.device_id, Message::FileCancel { transfer_id: id.to_string(), reason: FileCancelReason::User });
    if !notified {
        // O peer pode ter um .part esperando por nós: avisamos quando reconectar.
        let _ = state.db.transfer_mark_pending_notify(id);
    }
    Ok(rec)
}

/// Recoloca na fila uma transferência de saída `waiting`/`failed`/`cancelled`
/// (nova tentativa do zero quando terminal). Receptor não retoma por conta própria.
pub fn resume(app: &AppHandle, id: &str) -> Result<TransferRecord> {
    let state = mgr(app);
    let rec = state.db.transfer_get(id)?.ok_or_else(|| AppError::Other("transferência não encontrada".into()))?;
    if rec.direction != Direction::Out {
        return Err(AppError::Other("só o lado que envia pode retomar; o outro reoferece ao reconectar".into()));
    }
    if is_active(app, id) {
        return Ok(rec);
    }
    match rec.state {
        TransferState::Queued | TransferState::Offered => {}
        TransferState::Waiting { .. } => {
            apply(app, id, Event::Reconnected)?;
        }
        // Nova tentativa: política do manager, fora da máquina (terminal → queued).
        TransferState::Failed { .. } | TransferState::Cancelled { .. } => {
            state.db.transfer_set_state(id, &TransferState::Queued)?;
            state.db.transfer_set_progress(id, 0)?;
            state.db.transfer_bump_attempts(id)?;
        }
        _ => return Err(AppError::Other("transferência em andamento".into())),
    }
    kick(app, &rec.device_id);
    state.db.transfer_get(id)?.ok_or_else(|| AppError::Other("transferência não encontrada".into()))
}

/// Subida do processo: nada está em voo → `waiting`; expira/apaga o velho;
/// remove `.part` órfãos.
pub fn startup(app: &AppHandle) {
    let state = mgr(app);
    match state.db.transfer_sweep_on_startup() {
        Ok(ids) if !ids.is_empty() => log::info!("[FILE] Startup sweep: {} transfer(s) → waiting", ids.len()),
        Err(e) => log::warn!("[FILE] sweep falhou: {e}"),
        _ => {}
    }
    match state.db.transfer_purge(now_ms()) {
        Ok((expired, deleted)) => {
            for t in &expired {
                cleanup_part(app, t);
            }
            if !expired.is_empty() || deleted > 0 {
                log::info!("[FILE] Purge: expired={} deleted={deleted}", expired.len());
            }
        }
        Err(e) => log::warn!("[FILE] purge falhou: {e}"),
    }
    #[cfg(target_os = "android")]
    match super::android::cleanup_pending() {
        Ok(n) if n > 0 => log::info!("[FILE] MediaStore: {n} pending row(s) removed"),
        Err(e) => log::warn!("[FILE] MediaStore cleanup: {e}"),
        _ => {}
    }
    // `.part` só tem dono enquanto a linha não é terminal: `waiting`/`queued`/`incoming`
    // mantêm (retomada); `completed`/`cancelled`/`failed` ou sem linha → órfão.
    let known: Vec<String> = state.db.transfer_list(10_000).map(|v| v.into_iter().filter(|t| !t.state.is_terminal()).map(|t| t.id).collect()).unwrap_or_default();
    let n = dest::remove_orphans(app, &known);
    if n > 0 {
        log::info!("[FILE] Removed {n} orphan .part file(s)");
    }
    if let Ok(dir) = dest::outgoing_tmp_dir(app) {
        let n = dest::remove_outgoing_tmp(&dir);
        if n > 0 {
            log::info!("[FILE] Removed {n} orphan outgoing tmp copy(ies)");
        }
    }
    crate::clipboard::image::purge_old(app);
    super::folder::purge(app);
}

pub fn on_connected(app: &AppHandle, device_id: &str) {
    let state = mgr(app);
    if let Ok(ids) = state.db.transfer_take_pending_notify(device_id) {
        for id in ids {
            log::info!("[FILE] Notifying peer of cancel decided offline id={id} to={device_id}");
            state.send_to(device_id, Message::FileCancel { transfer_id: id, reason: FileCancelReason::User });
        }
    }
    kick(app, device_id);
}

/// A conexão `conn_id` com `device_id` caiu: as tasks amarradas a ela param e
/// deixam a transferência retomável (`waiting`). Tasks de uma conexão mais nova
/// (substituição) não são tocadas.
pub fn on_disconnected(app: &AppHandle, device_id: &str, conn_id: u64) {
    let ctls: Vec<mpsc::UnboundedSender<Ctl>> = {
        let state = mgr(app);
        let map = state.transfers.active.lock().unwrap();
        map.values().filter(|a| a.device_id == device_id && a.conn_id == conn_id).map(|a| a.ctl.clone()).collect()
    };
    for c in ctls {
        let _ = c.send(Ctl::Disconnected);
    }
}

/// Começa a próxima transferência de saída pendente para o device, se houver
/// conexão com `file-v1` e nenhuma saída ativa.
pub fn kick(app: &AppHandle, device_id: &str) {
    let Some((conn_id, data_tx)) = file_conn(app, device_id) else {
        if mgr(app).is_connected(device_id) {
            log::debug!("[FILE] peer {device_id} não anuncia {CAP_FILE_V1}; fila aguarda");
        }
        return;
    };
    let state = mgr(app);
    {
        let map = state.transfers.active.lock().unwrap();
        if map.values().any(|a| a.device_id == device_id && a.direction == Direction::Out) {
            return;
        }
    }
    let pending = state.db.transfer_pending_for_device(device_id).unwrap_or_default();
    let Some(next) = pending.into_iter().find(|t| t.direction == Direction::Out) else { return };
    let (ctl_tx, ctl_rx) = mpsc::unbounded_channel();
    register(app, &next.id, Active { device_id: device_id.to_string(), conn_id, direction: Direction::Out, ctl: ctl_tx, frames: None });
    let app2 = app.clone();
    let id = next.id.clone();
    let dev = device_id.to_string();
    tauri::async_runtime::spawn(async move {
        sender::run(app2.clone(), id.clone(), dev.clone(), data_tx, ctl_rx).await;
        unregister(&app2, &id);
        kick(&app2, &dev);
    });
}

pub fn on_message(app: &AppHandle, device_id: &str, conn_id: u64, msg: Message) {
    match msg {
        Message::FileOffer(offer) => on_offer(app, device_id, conn_id, offer),
        Message::FileAccept { transfer_id, offset } => route(app, device_id, &transfer_id, Ctl::Accepted { offset }),
        Message::FileReject { transfer_id, reason } => route(app, device_id, &transfer_id, Ctl::Rejected(reason)),
        Message::FileAck { transfer_id, received_through } => route(app, device_id, &transfer_id, Ctl::Ack { received_through }),
        Message::FileDone { transfer_id, sha256 } => route(app, device_id, &transfer_id, Ctl::Done { sha256 }),
        Message::FileResult { transfer_id, ok, error } => route(app, device_id, &transfer_id, Ctl::Result { ok, error }),
        Message::FileCancel { transfer_id, reason } => on_remote_cancel(app, device_id, &transfer_id, reason),
        _ => {}
    }
}

fn route(app: &AppHandle, device_id: &str, id: &str, ctl: Ctl) {
    match ctl_of(app, device_id, id) {
        Some(tx) => {
            let _ = tx.send(ctl);
        }
        None => log::debug!("[FILE] mensagem para transferência sem task: id={id} from={device_id} ctl={ctl:?}"),
    }
}

fn on_remote_cancel(app: &AppHandle, device_id: &str, id: &str, reason: FileCancelReason) {
    log::info!("[FILE] Cancelled by peer id={id} from={device_id} reason={reason:?}");
    if take_ask(app, id).is_some() {
        log::info!("[FILE] Ask withdrawn by sender id={id}");
    }
    if let Some(tx) = ctl_of(app, device_id, id) {
        let _ = tx.send(Ctl::Cancel(CancelledBy::Remote));
        return;
    }
    let state = mgr(app);
    if let Ok(Some(rec)) = state.db.transfer_get_for_device(device_id, id) {
        if !rec.state.is_terminal() {
            if let Ok(rec) = apply(app, id, Event::Cancel(CancelledBy::Remote)) {
                cleanup_part(app, &rec);
            }
        }
    }
}

fn reject(app: &AppHandle, device_id: &str, id: &str, reason: FileRejectReason) {
    log::info!("[FILE] Offer rejected id={id} from={device_id} reason={reason:?}");
    mgr(app).send_to(device_id, Message::FileReject { transfer_id: id.to_string(), reason });
}

/// `FileOffer` de um device pareado: valida, decide (auto-aceite por device), cria
/// ou retoma a linha e sobe a task do receptor, que responde `FileAccept{offset}`.
fn on_offer(app: &AppHandle, device_id: &str, conn_id: u64, offer: FileOffer) {
    let state = mgr(app);
    let id = offer.transfer_id.clone();
    if uuid::Uuid::parse_str(&id).is_err() {
        return reject(app, device_id, &id, FileRejectReason::Protocol);
    }
    if is_active(app, &id) {
        // Task presa a uma conexão anterior (substituída por esta): manda-a parar;
        // o peer recebe Busy e reoferece em instantes, já com a task encerrada.
        let stale = {
            let map = state.transfers.active.lock().unwrap();
            map.get(&id).filter(|a| a.conn_id != conn_id).map(|a| a.ctl.clone())
        };
        if let Some(c) = stale {
            let _ = c.send(Ctl::Disconnected);
        }
        return reject(app, device_id, &id, FileRejectReason::Busy);
    }
    if let Err(e) = validate_offer(offer.size, offer.chunk_size, offer.total_chunks) {
        let reason = if e == "tooLarge" { FileRejectReason::TooLarge } else { FileRejectReason::Protocol };
        return reject(app, device_id, &id, reason);
    }
    if !file_conn(app, device_id).map(|(c, _)| c == conn_id).unwrap_or(false) {
        return reject(app, device_id, &id, FileRejectReason::Protocol);
    }
    let Ok(Some(dev)) = state.db.get_device(device_id) else {
        return reject(app, device_id, &id, FileRejectReason::Protocol);
    };
    if dev.blocked {
        return reject(app, device_id, &id, FileRejectReason::Declined);
    }

    // Só uma recepção ativa por device.
    {
        let map = state.transfers.active.lock().unwrap();
        if map.values().any(|a| a.device_id == device_id && a.direction == Direction::In) {
            return reject(app, device_id, &id, FileRejectReason::Busy);
        }
    }

    let existing = state.db.transfer_get(&id).ok().flatten();
    let existing = match existing {
        // Id conhecido mas de outro device: nunca.
        Some(r) if r.device_id != device_id => return reject(app, device_id, &id, FileRejectReason::Protocol),
        Some(r) if r.direction != Direction::In => return reject(app, device_id, &id, FileRejectReason::Protocol),
        Some(r) if r.state.is_terminal() => return reject(app, device_id, &id, FileRejectReason::Unknown),
        Some(r) => {
            // Retomada: a oferta tem de descrever o mesmo arquivo.
            if r.size != offer.size || r.chunk_size != offer.chunk_size || (r.sha256_expected.is_some() && offer.sha256.is_some() && r.sha256_expected != offer.sha256) {
                return reject(app, device_id, &id, FileRejectReason::Protocol);
            }
            Some(r)
        }
        None => None,
    };

    // Imagem copiada: vai para o diretório privado e depois para o clipboard, nunca Downloads.
    let clip_image = crate::clipboard::image::is_clipboard(offer.purpose.as_deref());
    if clip_image && offer.size > crate::clipboard::image::MAX_BYTES {
        return reject(app, device_id, &id, FileRejectReason::TooLarge);
    }
    let dest_dir = if clip_image { crate::clipboard::image::dir(app) } else { dest::dest_dir(app) };
    let dest_dir = match dest_dir {
        Ok(d) => d,
        Err(e) => {
            log::warn!("[FILE] destino indisponível: {e}");
            return reject(app, device_id, &id, FileRejectReason::NoSpace);
        }
    };
    let part = match dest::part_path(app, &id) {
        Ok(p) => p,
        Err(_) => return reject(app, device_id, &id, FileRejectReason::NoSpace),
    };

    if existing.is_none() {
        if state.db.transfer_open_for_device(device_id).map(|v| v.len()).unwrap_or(0) >= MAX_OPEN_PER_DEVICE {
            return reject(app, device_id, &id, FileRejectReason::Busy);
        }
        if let Some(free) = dest::free_space(&dest_dir) {
            if free < dest::required_space(offer.size) {
                return reject(app, device_id, &id, FileRejectReason::NoSpace);
            }
        }
        let safe = names::sanitize(&offer.name);
        if names::resolve_in(&dest_dir, &safe).is_none() {
            return reject(app, device_id, &id, FileRejectReason::Protocol);
        }
        if let Err(e) = state.db.transfer_insert(&NewTransfer {
            id: &id,
            direction: Direction::In,
            device_id,
            original_name: &offer.name,
            safe_name: &safe,
            mime: offer.mime.as_deref(),
            size: offer.size,
            chunk_size: offer.chunk_size,
            sha256_expected: offer.sha256.as_deref(),
            state: &TransferState::Incoming,
            source: None,
            temp_path: part.to_str(),
            purpose: offer.purpose.as_deref(),
        }) {
            log::warn!("[FILE] não registrou oferta: {e}");
            return reject(app, device_id, &id, FileRejectReason::Protocol);
        }
        if let Ok(Some(rec)) = state.db.transfer_get(&id) {
            events::emit(app, events::TRANSFER_STATE, &rec);
        }
    } else if let Some(sha) = offer.sha256.as_deref() {
        let _ = state.db.transfer_set_hashes(&id, Some(sha), None);
    }

    // Decide só o que ainda não foi aceito: linha nova ou `incoming` de uma oferta anterior
    // (reoferta após queda/reinício). Imagem do clipboard é sempre guardada, como o texto.
    let undecided = existing.as_ref().map(|r| matches!(r.state, TransferState::Incoming)).unwrap_or(true);
    if undecided && !clip_image {
        match decide(app, &dev, device_id, &id, offer.size) {
            Decision::Accept => {}
            Decision::Decline => {
                let _ = apply(app, &id, Event::Decline);
                return reject(app, device_id, &id, FileRejectReason::Declined);
            }
            Decision::Ask => return ask(app, PendingOffer { device_id: device_id.to_string(), conn_id, offer, part, dest_dir }),
        }
    }
    start_receiver(app, PendingOffer { device_id: device_id.to_string(), conn_id, offer, part, dest_dir });
}

#[derive(Debug, PartialEq, Eq)]
enum Decision {
    Accept,
    Ask,
    Decline,
}

/// Política de aceite (ver `Settings::file_accept`). Peer sem `file-ask-v1` desistiria em
/// 30 s: para ele vale a regra antiga (aceita se "receber automático", senão recusa).
fn decide(app: &AppHandle, dev: &StoredDevice, device_id: &str, id: &str, size: u64) -> Decision {
    let state = mgr(app);
    if state.transfers.accepted.lock().unwrap().remove(id) {
        return Decision::Accept;
    }
    let s = state.settings();
    let auto_ok = dev.auto_receive && s.auto_receive && !state.is_paused();
    if !peer_has_cap(app, device_id, CAP_FILE_ASK_V1) {
        return if auto_ok { Decision::Accept } else { Decision::Decline };
    }
    policy(auto_ok, &s.file_accept, s.file_auto_accept_mb, size)
}

/// "Receber automático" desligado (device, global ou pausa) = sempre perguntar.
fn policy(auto_ok: bool, mode: &str, limit_mb: u32, size: u64) -> Decision {
    if !auto_ok {
        return Decision::Ask;
    }
    match mode {
        "auto" => Decision::Accept,
        "ask" => Decision::Ask,
        _ if size <= limit_mb as u64 * 1024 * 1024 => Decision::Accept,
        _ => Decision::Ask,
    }
}

/// Guarda a oferta e avisa a UI (`transfer:ask`). Sem resposta em `ASK_TTL_SECS`, recusa.
fn ask(app: &AppHandle, p: PendingOffer) {
    let state = mgr(app);
    let id = p.offer.transfer_id.clone();
    log::info!("[FILE] Asking user id={id} from={} name={:?} size={}", p.device_id, p.offer.name, p.offer.size);
    let fresh = state.transfers.asks.lock().unwrap().insert(id.clone(), p).is_none();
    if let Ok(Some(rec)) = state.db.transfer_get(&id) {
        events::emit(app, events::TRANSFER_ASK, &rec);
        if fresh {
            crate::notify::ask(app, &rec);
        }
    }
    if fresh {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(ASK_TTL_SECS)).await;
            let open = mgr(&app).transfers.asks.lock().unwrap().contains_key(&id);
            if open {
                log::info!("[FILE] Ask expired id={id}");
                let _ = decline(&app, &id);
            }
        });
    }
}

fn take_ask(app: &AppHandle, id: &str) -> Option<PendingOffer> {
    mgr(app).transfers.asks.lock().unwrap().remove(id)
}

pub fn asks(app: &AppHandle) -> Vec<TransferRecord> {
    let state = mgr(app);
    let ids: Vec<String> = state.transfers.asks.lock().unwrap().keys().cloned().collect();
    ids.iter().filter_map(|id| state.db.transfer_get(id).ok().flatten()).collect()
}

/// Usuário aceitou. Conexão ainda é a da oferta → começa agora; senão aceita na reoferta.
pub fn accept(app: &AppHandle, id: &str) -> Result<()> {
    let p = take_ask(app, id).ok_or_else(|| AppError::Other("pedido não encontrado ou expirado".into()))?;
    log::info!("[FILE] Accepted by user id={id}");
    if file_conn(app, &p.device_id).map(|(c, _)| c == p.conn_id).unwrap_or(false) {
        start_receiver(app, p);
    } else {
        mgr(app).transfers.accepted.lock().unwrap().insert(id.to_string());
    }
    Ok(())
}

/// Usuário recusou (ou o pedido expirou): `cancelled/local` aqui, `FileReject{Declined}` lá.
pub fn decline(app: &AppHandle, id: &str) -> Result<TransferRecord> {
    let _ = take_ask(app, id);
    mgr(app).transfers.accepted.lock().unwrap().remove(id);
    let rec = apply(app, id, Event::Decline)?;
    log::info!("[FILE] Declined by user id={id}");
    reject(app, &rec.device_id, id, FileRejectReason::Declined);
    Ok(rec)
}

fn start_receiver(app: &AppHandle, p: PendingOffer) {
    let PendingOffer { device_id, conn_id, offer, part, dest_dir } = p;
    let id = offer.transfer_id.clone();
    let (ctl_tx, ctl_rx) = mpsc::unbounded_channel();
    let (frames_tx, frames_rx) = mpsc::channel(RECEIVER_QUEUE_FRAMES);
    register(app, &id, Active { device_id: device_id.clone(), conn_id, direction: Direction::In, ctl: ctl_tx, frames: Some(frames_tx) });
    let app2 = app.clone();
    let dev_id = device_id;
    let id2 = id;
    tauri::async_runtime::spawn(async move {
        receiver::run(receiver::Params { app: app2.clone(), id: id2.clone(), device_id: dev_id, offer, part, dest_dir, frames_rx, ctl_rx }).await;
        unregister(&app2, &id2);
    });
}

/// Quadro de dados de `device_id`: entrega à task do receptor. `await` aqui é o
/// backpressure: se o disco não acompanha, o loop da conexão para de ler o
/// socket e o TCP segura o emissor.
pub async fn on_data(app: &AppHandle, device_id: &str, frame: DataFrame) {
    let id = transfer_id_string(&frame.transfer_id);
    let tx = {
        let state = mgr(app);
        let map = state.transfers.active.lock().unwrap();
        map.get(&id).filter(|a| a.device_id == device_id).and_then(|a| a.frames.clone())
    };
    match tx {
        Some(tx) => {
            if tx.send(frame).await.is_err() {
                log::debug!("[FILE] data frame após fim da task id={id}");
            }
        }
        None => log::debug!("[FILE] data frame sem receptor id={id} from={device_id}"),
    }
}

/// Remove o `.part` de uma transferência (qualquer erro é ignorado).
pub fn cleanup_part(app: &AppHandle, rec: &TransferRecord) {
    let _ = app;
    if let Some(p) = rec.temp_path.as_deref() {
        let _ = std::fs::remove_file(p);
    }
}

/// Revogação/bloqueio do device: cancela tudo (ativo ou na fila), apaga `.part`
/// e, enquanto a conexão ainda existe, avisa o peer.
pub fn cancel_all_for_device(app: &AppHandle, device_id: &str) {
    let state = mgr(app);
    let open = state.db.transfer_open_for_device(device_id).unwrap_or_default();
    // Avisa o peer enquanto a conexão ainda existe (o chamador manda Bye em seguida).
    for rec in &open {
        state.send_to(device_id, Message::FileCancel { transfer_id: rec.id.clone(), reason: FileCancelReason::Revoked });
    }
    // Tasks ativas param sozinhas; o resto é resolvido aqui.
    for rec in open {
        if let Some(ctl) = ctl_of(app, device_id, &rec.id) {
            let _ = ctl.send(Ctl::Cancel(CancelledBy::System));
            continue;
        }
        if let Ok(rec) = apply(app, &rec.id, Event::Cancel(CancelledBy::System)) {
            cleanup_part(app, &rec);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{policy, Decision};

    #[test]
    fn accept_policy() {
        const MB: u64 = 1024 * 1024;
        assert_eq!(policy(true, "auto", 100, 5_000 * MB), Decision::Accept);
        assert_eq!(policy(true, "ask", 100, 1), Decision::Ask);
        assert_eq!(policy(true, "limit", 100, 100 * MB), Decision::Accept);
        assert_eq!(policy(true, "limit", 100, 100 * MB + 1), Decision::Ask);
        assert_eq!(policy(true, "limit", 0, 1), Decision::Ask);
        // Receber automático desligado: pergunta, nunca recusa calado.
        assert_eq!(policy(false, "auto", 100, 1), Decision::Ask);
    }
}
