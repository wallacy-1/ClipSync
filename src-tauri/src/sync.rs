//! Regras de sincronização: quem recebe o quê, fila offline, anti-loop e expiração.
use crate::clipboard;
use crate::db::content_hash;
use crate::events;
use crate::content::ContentType;
use crate::models::{now_ms, ClipboardItem, SyncState, SyncStatus};
use crate::state::AppState;
use crate::transport::protocol::{Message, CAP_CLIP_HTML_V1};
use std::time::Duration;
use tauri::{AppHandle, Manager};

/// Só texto vai em `Message::Clip`. Imagem segue pela transferência de arquivos (`purpose=clipboard`):
/// peer antigo não conhece o tipo `image`, e o PNG não cabe numa mensagem de clipboard.
fn travels_as_clip(t: ContentType) -> bool {
    t != ContentType::Image
}

fn auto_targets(state: &AppState, item: &ClipboardItem) -> Vec<String> {
    let s = state.settings();
    if !travels_as_clip(item.content_type) || !s.auto_send || state.is_paused() {
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

/// O item como vai no fio: `metadata.html` só para peer que anunciou `clip-html-v1` (peer antigo não
/// precisa dos até 512 KB a mais) e nunca em item sensível.
fn wire_item(item: &ClipboardItem, with_html: bool) -> ClipboardItem {
    let mut wire = ClipboardItem { sync_state: SyncState::Synced, ..item.clone() };
    if !with_html || item.is_sensitive {
        clipboard::rich::set_html(&mut wire, None);
    }
    wire
}

fn peer_takes_html(app: &AppHandle, dev: &str) -> bool {
    app.state::<AppState>().settings().keep_formatting && crate::transfer::manager::peer_has_cap(app, dev, CAP_CLIP_HTML_V1)
}

fn dispatch(app: &AppHandle, item: &ClipboardItem, targets: &[String]) {
    let state = app.state::<AppState>();
    if targets.is_empty() {
        return;
    }
    if !travels_as_clip(item.content_type) {
        log::warn!("[SYNC] Item {} ({}) não vai em Clip; ignorado", item.id, item.content_type.as_str());
        return;
    }
    events::emit(app, events::SYNC_STARTED, serde_json::json!({ "itemId": item.id, "targets": targets }));
    let mut sent = 0;
    let mut queued = 0;
    for dev in targets {
        // Enviado ou não, fica na fila até o Ack para sobreviver a queda de conexão no meio.
        let _ = state.db.enqueue(&item.id, dev);
        let wire = wire_item(item, peer_takes_html(app, dev));
        if state.send_to(dev, Message::Clip { item: wire }) {
            log::info!("[SYNC] Item sent id={} type={} to={}", item.id, item.content_type.as_str(), dev);
            sent += 1;
        } else {
            log::info!("[SYNC] Item queued id={} type={} for={}", item.id, item.content_type.as_str(), dev);
            queued += 1;
        }
    }
    let new_state = if queued > 0 { SyncState::Queued } else if sent > 0 { SyncState::Sending } else { SyncState::Local };
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
        return Err(crate::error::AppError::Other(crate::i18n::tr("err.item_not_found").into()));
    };
    let stored = state.db.list_devices()?;
    let targets: Vec<String> = device_ids
        .iter()
        .filter(|id| stored.iter().any(|d| &d.device_id == *id && !d.blocked))
        .cloned()
        .collect();
    if !travels_as_clip(item.content_type) {
        return clipboard::image::send_item(app, &item, &targets);
    }
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
                if !travels_as_clip(item.content_type) || item.expires_at.map(|e| e <= now_ms()).unwrap_or(false) {
                    let _ = state.db.dequeue(&item_id, device_id);
                    continue;
                }
                let wire = wire_item(&item, peer_takes_html(app, device_id));
                if state.send_to(device_id, Message::Clip { item: wire }) {
                    let _ = state.db.set_sync_state(&item_id, SyncState::Sending);
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
    if !state.db.outbox_has_item(item_id).unwrap_or(true) {
        let _ = state.db.set_sync_state(item_id, SyncState::Synced);
        events::emit(app, events::SYNC_COMPLETED, serde_json::json!({ "itemId": item_id }));
    }
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
    // Imagem só chega pela transferência de arquivos: um `Clip` image aqui apontaria `metadata.path`
    // para onde o peer quisesse.
    if !travels_as_clip(item.content_type) {
        log::warn!("[SYNC] Clip de imagem recusado id={} from={from_device_id}", item.id);
        return;
    }

    // Anti-loop: o repasse abaixo só acontece na primeira vez que o id chega (o banco
    // recusa repetido) e o hash guardado ao escrever no clipboard impede o eco local.
    item.source_device_id = from_device_id.to_string();
    item.sync_state = SyncState::Synced;
    item.is_pinned = false;
    item.expires_at = expiry_for(&settings, item.is_sensitive, item.created_at.max(now_ms()));
    clipboard::rich::normalize_incoming(&mut item, settings.keep_formatting);

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
        if let Err(e) = clipboard::write_local(app, &item.content, clipboard::rich::html_of(&item)) {
            log::warn!("falha ao escrever no clipboard: {e}");
        }
    }
    crate::notify::text_received(app, &item, from_device_id, apply);
    events::emit(
        app,
        events::CLIPBOARD_RECEIVED,
        serde_json::json!({ "item": clipboard::rich::for_ui(&item), "fromDeviceId": from_device_id, "applied": apply }),
    );

    // Repasse: celulares pareados só com o PC recebem o que o outro copiou.
    let targets: Vec<String> = auto_targets(&state, &item).into_iter().filter(|d| d != from_device_id).collect();
    if !targets.is_empty() {
        log::info!("[SYNC] Relaying id={} from={from_device_id} to={targets:?}", item.id);
        dispatch(app, &item, &targets);
    }
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
                Ok(gone) if !gone.is_empty() => {
                    log::info!("[EXPIRY] Items expired count={}", gone.len());
                    for it in &gone {
                        clipboard::image::remove_files(&app, it);
                    }
                    let ids: Vec<&str> = gone.iter().map(|i| i.id.as_str()).collect();
                    events::emit(&app, events::CLIPBOARD_EXPIRED, serde_json::json!({ "ids": ids }));
                }
                Ok(_) => {}
                Err(e) => log::warn!("expiração falhou: {e}"),
            }
        }
    });
}

/// Pausa/retoma a sincronização (comando e bandeja usam esta única cópia).
/// `minutes > 0` pausa até o prazo e agenda o aviso de fim; senão, pausa até retomar.
pub fn set_paused(app: &AppHandle, paused: bool, minutes: Option<u32>) -> SyncStatus {
    let state = app.state::<AppState>();
    let until = {
        let mut p = state.paused_until.lock().unwrap();
        *p = if !paused {
            None
        } else {
            match minutes {
                Some(m) if m > 0 => Some(now_ms() + m as i64 * 60_000),
                _ => Some(0),
            }
        };
        *p
    };
    if let Some(deadline) = until.filter(|v| *v > 0) {
        schedule_resume(app.clone(), deadline);
    }
    let status = state.sync_status();
    events::emit(app, events::SYNC_STATE, &status);
    status
}

/// Ao vencer o prazo, emite `sync:state` para a UI sair do "Pausado" sozinha.
/// Só age se a pausa ainda for a mesma (o usuário pode ter retomado ou pausado de novo).
/// Acorda no máximo a cada 30 s: o prazo é hora do relógio, que pode pular (suspensão, NTP).
fn schedule_resume(app: AppHandle, deadline: i64) {
    tauri::async_runtime::spawn(async move {
        loop {
            let wait = (deadline - now_ms()).clamp(0, 30_000) as u64;
            tokio::time::sleep(Duration::from_millis(wait)).await;
            let state = app.state::<AppState>();
            if *state.paused_until.lock().unwrap() != Some(deadline) {
                return;
            }
            // is_paused() limpa a pausa vencida.
            if !state.is_paused() {
                events::emit(&app, events::SYNC_STATE, &state.sync_status());
                return;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_never_travel_as_clip() {
        assert!(!travels_as_clip(ContentType::Image));
        for t in [ContentType::Text, ContentType::Link, ContentType::Code, ContentType::Command, ContentType::Email, ContentType::Token, ContentType::Card] {
            assert!(travels_as_clip(t), "{t:?}");
        }
        // Todo tipo novo precisa decidir se vai em `Clip`; imagem tem categoria própria.
        assert_eq!(ContentType::Image.sync_category(), "image");
    }

    fn item_with_html(sensitive: bool) -> ClipboardItem {
        ClipboardItem {
            id: "1".into(),
            content: "oi".into(),
            content_type: ContentType::Text,
            source_device_id: "d".into(),
            created_at: 0,
            expires_at: None,
            is_pinned: false,
            is_sensitive: sensitive,
            sync_state: SyncState::Local,
            metadata: serde_json::json!({ "html": "<b>oi</b>", "outro": 1 }),
        }
    }

    #[test]
    fn html_only_travels_to_peers_with_the_capability() {
        let it = item_with_html(false);
        let with = serde_json::to_value(wire_item(&it, true)).unwrap();
        assert_eq!(with["metadata"]["html"], "<b>oi</b>");
        let without = serde_json::to_value(wire_item(&it, false)).unwrap();
        assert!(without["metadata"].get("html").is_none());
        assert_eq!(without["metadata"]["outro"], 1);
        assert_eq!(without["content"], "oi");
        // Sensível nunca leva HTML, nem para quem tem a capability.
        assert!(wire_item(&item_with_html(true), true).metadata.get("html").is_none());
    }
}
