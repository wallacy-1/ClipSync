//! `ClipboardProvider`: abstração sobre o clipboard nativo. A UI nunca fala com
//! APIs de plataforma; recebe só `clipboard.changed` / `clipboard.received` /
//! `clipboard.expired`.
use crate::content;
use crate::db::content_hash;
use crate::error::Result;
use crate::events;
use crate::models::{now_ms, ClipboardItem};
use crate::state::AppState;
use crate::sync;
use std::time::Duration;
use tauri::{AppHandle, Manager};

pub mod dedupe;
pub mod image;

#[cfg(any(target_os = "android", target_os = "ios"))]
pub mod android;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod windows;

pub trait ClipboardProvider: Send + Sync {
    fn read_text(&self) -> Result<Option<String>>;
    fn write_text(&self, text: &str) -> Result<()>;
    /// Dica barata de "mudou desde a última vez?" — quando a plataforma não
    /// oferece, devolve `true` e o watcher compara por hash.
    fn changed_hint(&self) -> bool {
        true
    }
    /// Conteúdo marcado pela origem como "não monitorar" (gerenciadores de senha).
    fn is_excluded_by_source(&self) -> bool {
        false
    }
    fn poll_interval(&self) -> Duration {
        Duration::from_millis(700)
    }
}

pub fn provider(app: &AppHandle) -> Box<dyn ClipboardProvider> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        Box::new(android::AndroidClipboard::new(app.clone()))
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        Box::new(windows::WindowsClipboard::new(app.clone()))
    }
}

const MAX_TEXT_BYTES: usize = 1024 * 1024;

/// De onde veio um conteúdo de clipboard. Só para log/diagnóstico: o
/// tratamento é o mesmo para todas as origens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub enum ClipOrigin {
    /// Polling do app / leitura ao voltar ao primeiro plano.
    Poll,
    /// ClipSync Keyboard (IME) com o núcleo vivo.
    Ime,
    /// Inbox do IME, drenado quando o núcleo subiu.
    ImeInbox,
}

impl ClipOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            ClipOrigin::Poll => "poll",
            ClipOrigin::Ime => "ime",
            ClipOrigin::ImeInbox => "ime-inbox",
        }
    }
}

/// Ponto único de entrada de clipboard local, para qualquer origem: anti-loop,
/// dedupe, classificação, SQLite, evento e sync. Retorna o item salvo, se
/// virou histórico (`None` = ignorado ou inválido, nunca erro).
pub fn handle_text(app: &AppHandle, text: String, excluded_by_source: bool, origin: ClipOrigin) -> Option<ClipboardItem> {
    let state = app.state::<AppState>();
    if text.trim().is_empty() || text.len() > MAX_TEXT_BYTES {
        return None;
    }
    let hash = content_hash(&text);
    match state.clip_dedupe.lock().unwrap().decide(&hash, now_ms()) {
        dedupe::Decision::WrittenBySelf => {
            log::info!("[CLIPBOARD] Change ignored (written by ClipSync) origin={} hash={}", origin.as_str(), &hash[..8]);
            return None;
        }
        dedupe::Decision::Duplicate => {
            log::debug!("[CLIPBOARD] Change ignored (duplicate event) origin={} hash={}", origin.as_str(), &hash[..8]);
            return None;
        }
        dedupe::Decision::New => {}
    }
    let settings = state.settings();
    if excluded_by_source && settings.ignore_password_managers {
        log::debug!("cópia marcada como sensível pela origem; ignorada");
        return None;
    }
    let cls = content::classify(&text);
    let created_at = now_ms();
    let item = ClipboardItem {
        id: uuid::Uuid::new_v4().to_string(),
        content: text,
        content_type: cls.content_type,
        source_device_id: state.self_id(),
        created_at,
        expires_at: sync::expiry_for(&settings, cls.is_sensitive, created_at),
        is_pinned: false,
        is_sensitive: cls.is_sensitive,
        sync_state: "local".into(),
        metadata: serde_json::json!({}),
    };
    if !state.db.insert_item(&item).unwrap_or(false) {
        return None;
    }
    log::info!(
        "[CLIPBOARD] Change detected origin={} id={} type={} sensitive={} bytes={} hash={}",
        origin.as_str(),
        item.id,
        item.content_type.as_str(),
        item.is_sensitive,
        item.content.len(),
        &hash[..8]
    );
    let _ = state.db.dedupe_content(&hash, &item.id);
    events::emit(app, events::CLIPBOARD_CHANGED, &item);
    sync::on_local_clip(app, &item);
    Some(item)
}

/// Escreve no clipboard local marcando o hash para o watcher **e o IME**
/// ignorarem o eco (anti-loop). Toda escrita local de conteúdo remoto passa aqui.
pub fn write_local(app: &AppHandle, text: &str) -> Result<()> {
    let state = app.state::<AppState>();
    let hash = content_hash(text);
    log::info!("[CLIPBOARD] Writing to system clipboard bytes={} hash={}", text.len(), &hash[..8]);
    state.clip_dedupe.lock().unwrap().note_written(&hash);
    provider(app).write_text(text)
}

/// Leitura por polling (watcher e `clipboard_poll_now`): só processa se o hash
/// mudou desde a última leitura — memória compartilhada com o IME, para a mesma
/// cópia não entrar duas vezes.
pub fn poll_once(app: &AppHandle, prov: &dyn ClipboardProvider) -> Option<ClipboardItem> {
    let Some(text) = prov.read_text().ok().flatten() else {
        // Sem texto: pode ser uma imagem (segue pela transferência de arquivos).
        image::poll(app);
        return None;
    };
    let h = content_hash(&text);
    let changed = app.state::<AppState>().clip_dedupe.lock().unwrap().poll_changed(&h);
    if !changed {
        return None;
    }
    handle_text(app, text, prov.is_excluded_by_source(), ClipOrigin::Poll)
}

pub fn spawn_watcher(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let prov = provider(&app);
        // Ignora o que já estava no clipboard quando o app abriu.
        let _ = prov.changed_hint();
        match prov.read_text() {
            Ok(Some(t)) => app.state::<AppState>().clip_dedupe.lock().unwrap().prime(&content_hash(&t)),
            _ => image::prime(&app),
        }
        loop {
            tokio::time::sleep(prov.poll_interval()).await;
            if !prov.changed_hint() {
                continue;
            }
            poll_once(&app, prov.as_ref());
        }
    });
}
