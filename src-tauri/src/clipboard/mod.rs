//! `ClipboardProvider`: abstração sobre o clipboard nativo. A UI nunca fala com
//! APIs de plataforma; recebe só `clipboard.changed` / `clipboard.received` /
//! `clipboard.expired`.
use crate::content::{self, ContentType};
use crate::db::content_hash;
use crate::error::Result;
use crate::events;
use crate::models::{now_ms, ClipboardItem, SyncState};
use crate::state::AppState;
use crate::sync;
use std::time::Duration;
use tauri::{AppHandle, Manager};

pub mod dedupe;
#[cfg(windows)]
pub mod files;
pub mod image;
pub mod rich;

#[cfg(any(target_os = "android", target_os = "ios"))]
pub mod android;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod windows;

pub trait ClipboardProvider: Send + Sync {
    fn read_text(&self) -> Result<Option<String>>;
    fn write_text(&self, text: &str) -> Result<()>;
    /// HTML do clipboard atual (formato rico junto do texto); `None` onde a plataforma não lê.
    fn read_html(&self) -> Result<Option<String>> {
        Ok(None)
    }
    /// Texto + HTML numa escrita só; o padrão ignora o HTML.
    fn write_rich(&self, text: &str, _html: &str) -> Result<()> {
        self.write_text(text)
    }
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

pub const MAX_TEXT_BYTES: usize = 1024 * 1024;

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
/// `html` é o formato rico lido junto do texto (só com "Manter formatação" ligado); o IME passa `None`.
pub fn handle_text(app: &AppHandle, text: String, html: Option<String>, excluded_by_source: bool, origin: ClipOrigin) -> Option<ClipboardItem> {
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
    let item = new_item(&state, text, html);
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
    events::emit(app, events::CLIPBOARD_CHANGED, &rich::for_ui(&item));
    sync::on_local_clip(app, &item);
    Some(item)
}

fn new_item(state: &AppState, text: String, html: Option<String>) -> ClipboardItem {
    let settings = state.settings();
    let cls = content::classify(&text);
    let html = rich::usable_local(html.filter(|_| settings.keep_formatting), cls.is_sensitive);
    let created_at = now_ms();
    ClipboardItem {
        id: uuid::Uuid::new_v4().to_string(),
        content: text,
        content_type: cls.content_type,
        source_device_id: state.self_id(),
        created_at,
        expires_at: sync::expiry_for(&settings, cls.is_sensitive, created_at),
        is_pinned: false,
        is_sensitive: cls.is_sensitive,
        sync_state: SyncState::Local,
        metadata: match html {
            Some(h) => serde_json::json!({ "html": h }),
            None => serde_json::json!({}),
        },
    }
}

/// Texto compartilhado por outro app ("Compartilhar → ClipSync"): pedido explícito do
/// usuário, então sem dedupe nem regras automáticas — vira histórico e o chamador envia.
/// `sensitive` = a origem marcou o conteúdo como sensível (clipboard do Android 13+): vale mesmo que a classificação não pegue.
pub fn ingest_shared_text(app: &AppHandle, text: String, sensitive: bool) -> Result<ClipboardItem> {
    if text.trim().is_empty() || text.len() > MAX_TEXT_BYTES {
        return Err(crate::error::AppError::Other(crate::i18n::tr("err.text_too_big").into()));
    }
    let state = app.state::<AppState>();
    let mut item = new_item(&state, text, None);
    if sensitive && !item.is_sensitive {
        item.is_sensitive = true;
        item.expires_at = sync::expiry_for(&state.settings(), true, item.created_at);
    }
    state.db.insert_item(&item)?;
    let _ = state.db.dedupe_content(&content_hash(&item.content), &item.id);
    log::info!("[CLIPBOARD] Shared text id={} type={} bytes={}", item.id, item.content_type.as_str(), item.content.len());
    events::emit(app, events::CLIPBOARD_CHANGED, &item);
    Ok(item)
}

/// Escreve no clipboard local marcando o hash para o watcher **e o IME**
/// ignorarem o eco (anti-loop). Toda escrita local de conteúdo remoto passa aqui.
/// Com `html` (e "Manter formatação" ligado) escreve texto + HTML; o HTML é sanitizado aqui, o ponto
/// único de escrita, e na dúvida (ou falha da escrita rica) vai só o texto.
pub fn write_local(app: &AppHandle, text: &str, html: Option<&str>) -> Result<()> {
    let state = app.state::<AppState>();
    let hash = content_hash(text);
    let html = html.filter(|_| state.settings().keep_formatting).and_then(rich::sanitize_html);
    log::info!("[CLIPBOARD] Writing to system clipboard bytes={} hash={} html={}", text.len(), &hash[..8], html.is_some());
    state.clip_dedupe.lock().unwrap().note_written(&hash);
    let prov = provider(app);
    if let Some(h) = html {
        match prov.write_rich(text, &h) {
            Ok(()) => return Ok(()),
            Err(e) => log::warn!("[CLIPBOARD] escrita rica falhou, só texto: {e}"),
        }
    }
    prov.write_text(text)
}

/// "Copiar" de um item do histórico: texto pelo `write_local`; imagem, pelo caminho de imagem
/// (decodifica o PNG: chamar fora da thread principal).
pub fn copy_item(app: &AppHandle, item: &ClipboardItem) -> Result<()> {
    if item.content_type == ContentType::Image {
        return image::copy_item(app, item);
    }
    write_local(app, &item.content, rich::html_of(item))
}

/// Leitura por polling (watcher e `clipboard_poll_now`): só processa se o hash
/// mudou desde a última leitura — memória compartilhada com o IME, para a mesma
/// cópia não entrar duas vezes.
pub fn poll_once(app: &AppHandle, prov: &dyn ClipboardProvider) -> Option<ClipboardItem> {
    let Some(text) = prov.read_text().ok().flatten() else {
        // Sem texto: arquivos copiados no Explorer (só oferta) ou imagem (segue pela transferência).
        #[cfg(windows)]
        if files::poll(app) {
            return None;
        }
        image::poll(app);
        return None;
    };
    let h = content_hash(&text);
    let changed = app.state::<AppState>().clip_dedupe.lock().unwrap().poll_changed(&h);
    if !changed {
        return None;
    }
    let html = if app.state::<AppState>().settings().keep_formatting { prov.read_html().ok().flatten() } else { None };
    handle_text(app, text, html, prov.is_excluded_by_source(), ClipOrigin::Poll)
}

pub fn spawn_watcher(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let prov = provider(&app);
        // Ignora o que já estava no clipboard quando o app abriu.
        let _ = prov.changed_hint();
        match prov.read_text() {
            Ok(Some(t)) => app.state::<AppState>().clip_dedupe.lock().unwrap().prime(&content_hash(&t)),
            #[cfg(windows)]
            _ if files::prime(&app) => {}
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
