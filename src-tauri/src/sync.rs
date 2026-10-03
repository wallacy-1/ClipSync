//! Regras de sincronização: quem recebe o quê, fila offline, anti-loop e expiração.
use crate::clipboard;
use crate::db::content_hash;
use crate::events;
use crate::models::{now_ms, ClipboardItem};
use crate::state::AppState;
use crate::transport::protocol::Message;
use std::time::Duration;
use tauri::{AppHandle, Manager};

fn auto_targets(state: &AppState, item: &ClipboardItem) -> Vec<String> {
    let s = state.settings();
    if !s.auto_send || state.is_paused() {
        return vec![];
    }
    if !s.synced_types.iter().any(|t| t == item.content_type.sync_category()) {
        return vec![];
    }
    state
        .db
        .list_devices()
        .unwrap_or_default()
        .into_iter()
        .filter(|d| d.auto_send && !d.blocked)
        .map(|d| d.device_id)
        .collect()
}

fn dispatch(app: &AppHandle, item: &ClipboardItem, targets: &[String]) {
    let state = app.state::<AppState>();
    if targets.is_empty() {
        return;
    }
    events::emit(app, events::SYNC_STARTED, serde_json::json!({ "itemId": item.id, "targets": targets }));
    let mut sent = 0;
    let mut queued = 0;
    for dev in targets {
        let wire = ClipboardItem { sync_state: "synced".into(), ..item.clone() };
        if state.send_to(dev, Message::Clip { item: wire }) {
            log::info!("[SYNC] Item sent id={} type={} to={}", item.id, item.content_type.as_str(), dev);
            sent += 1;
            // Fica na fila até o Ack para sobreviver a queda de conexão no meio.
            let _ = state.db.enqueue(&item.id, dev);
        } else {
            log::info!("[SYNC] Item queued id={} type={} for={}", item.id, item.content_type.as_str(), dev);
            queued += 1;
            let _ = state.db.enqueue(&item.id, dev);
        }
    }
    let new_state = if queued > 0 { "queued" } else if sent > 0 { "sending" } else { "local" };
    let _ = state.db.set_sync_state(&item.id, new_state);
    events::emit(
        app,
        events::CLIPBOARD_SENT,
        serde_json::json!({ "itemId": item.id, "sent": sent, "queued": queued, "state": new_state }),
    );
    events::emit(app, events::SYNC_STATE, state.sync_status());
}

/// Item copiado localmente (chamado pelo watcher depois de salvar no banco).
pub fn on_local_clip(app: &AppHandle, item: &ClipboardItem) {
    let state = app.state::<AppState>();
    let targets = auto_targets(&state, item);
    dispatch(app, item, &targets);
}

/// "Enviar para": envio explícito ignora as regras automáticas (mas não bloqueios).
pub fn send_item(app: &AppHandle, item_id: &str, device_ids: &[String]) -> crate::error::Result<()> {
    let state = app.state::<AppState>();
    let Some(item) = state.db.get_item(item_id)? else {
        return Err(crate::error::AppError::Other("item não encontrado".into()));
    };
    let stored = state.db.list_devices()?;
    let targets: Vec<String> = device_ids
        .iter()
        .filter(|id| stored.iter().any(|d| &d.device_id == *id && !d.blocked))
        .cloned()
        .collect();
    dispatch(app, &item, &targets);
    Ok(())
}

/// Conexão estabelecida: esvazia a fila pendente daquele dispositivo.
pub fn on_connected(app: &AppHandle, device_id: &str) {
    let state = app.state::<AppState>();
    let pending = state.db.outbox_for(device_id).unwrap_or_default();
    if pending.is_empty() {
        return;
    }
    log::info!("[SYNC] Flushing outbox count={} to={device_id}", pending.len());
    for item_id in pending {
        match state.db.get_item(&item_id) {
            Ok(Some(item)) => {
                if item.expires_at.map(|e| e <= now_ms()).unwrap_or(false) {
                    let _ = state.db.dequeue(&item_id, device_id);
                    continue;
                }
                let wire = ClipboardItem { sync_state: "synced".into(), ..item };
                if state.send_to(device_id, Message::Clip { item: wire }) {
                    let _ = state.db.set_sync_state(&item_id, "sending");
                }
            }
            _ => {
                let _ = state.db.dequeue(&item_id, device_id);
            }
        }
    }
    events::emit(app, events::SYNC_STATE, state.sync_status());
}

pub fn on_ack(app: &AppHandle, device_id: &str, item_id: &str) {
    let state = app.state::<AppState>();
    let _ = state.db.dequeue(item_id, device_id);
    let remaining: u32 = state.db.outbox_counts().unwrap_or_default().values().sum();
    let still_pending = state
        .db
        .list_devices()
        .unwrap_or_default()
        .iter()
        .any(|d| state.db.outbox_for(&d.device_id).map(|v| v.iter().any(|i| i == item_id)).unwrap_or(false));
    if !still_pending {
        let _ = state.db.set_sync_state(item_id, "synced");
        events::emit(app, events::SYNC_COMPLETED, serde_json::json!({ "itemId": item_id }));
    }
    let _ = remaining;
    events::emit(app, events::SYNC_STATE, state.sync_status());
}

pub fn on_clip_received(app: &AppHandle, from_device_id: &str, mut item: ClipboardItem) {
    let state = app.state::<AppState>();
    let settings = state.settings();
    let dev = state.db.get_device(from_device_id).ok().flatten();
    let Some(dev) = dev else { return };
    if dev.blocked {
        return;
    }

    // O item mantém a origem verdadeira e nunca é reenviado daqui — isso, mais o
    // hash guardado ao escrever no clipboard, impede o ping-pong entre dispositivos.
    item.source_device_id = from_device_id.to_string();
    item.sync_state = "synced".into();
    item.is_pinned = false;
    // Expiração local segue as regras locais.
    item.expires_at = expiry_for(&settings, item.is_sensitive, item.created_at.max(now_ms()));

    let inserted = state.db.insert_item(&item).unwrap_or(false);
    if !inserted {
        log::info!("[SYNC] Duplicate item ignored id={} from={from_device_id}", item.id);
        return;
    }
    let _ = state.db.dedupe_content(&content_hash(&item.content), &item.id);

    let apply = settings.auto_receive && dev.auto_receive && !state.is_paused();
    log::info!("[SYNC] Item stored id={} type={} from={from_device_id} apply_clipboard={apply}", item.id, item.content_type.as_str());
    if apply {
        // `write_local` registra o hash para watcher e IME ignorarem o eco.
        if let Err(e) = clipboard::write_local(app, &item.content) {
            log::warn!("falha ao escrever no clipboard: {e}");
        }
    }
    crate::notify::text_received(app, &item, from_device_id, apply);
    events::emit(
        app,
        events::CLIPBOARD_RECEIVED,
        serde_json::json!({ "item": item, "fromDeviceId": from_device_id, "applied": apply }),
    );
}

pub fn expiry_for(settings: &crate::models::Settings, sensitive: bool, from: i64) -> Option<i64> {
    if sensitive && settings.hide_sensitive && settings.sensitive_expire_minutes > 0 {
        return Some(from + settings.sensitive_expire_minutes as i64 * 60_000);
    }
    if settings.auto_expire_hours > 0 {
        return Some(from + settings.auto_expire_hours as i64 * 3_600_000);
    }
    None
}

pub fn spawn_expiry_task(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            let state = app.state::<AppState>();
            match state.db.purge_expired() {
                Ok(ids) if !ids.is_empty() => {
                    let mut revealed = state.revealed.lock().unwrap();
                    for id in &ids {
                        revealed.remove(id);
                    }
                    drop(revealed);
                    log::info!("[EXPIRY] Items expired count={}", ids.len());
                    events::emit(&app, events::CLIPBOARD_EXPIRED, serde_json::json!({ "ids": ids }));
                }
                Ok(_) => {}
                Err(e) => log::warn!("expiração falhou: {e}"),
            }
        }
    });
}
