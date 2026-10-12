use crate::clipboard;
use crate::db::ItemFilter;
use crate::error::{AppError, Result};
use crate::events;
use crate::identity::{self, DeviceIdentity};
use crate::clipboard::rich;
use crate::models::{ClipboardItem, Device, Settings, SyncStatus};
use crate::state::AppState;
use crate::sync;
use crate::transport::{self, connection, pairing};
use serde::Serialize;
use std::net::SocketAddr;
use tauri::{AppHandle, Manager, State};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub platform: String,
    pub me: DeviceIdentity,
    pub settings: Settings,
    pub devices: Vec<Device>,
    pub items: Vec<ClipboardItem>,
    pub sync: SyncStatus,
    pub listen_port: u16,
    pub total_items: u32,
    pub pinned_items: u32,
    pub onboarded: bool,
    pub keyboard_prompt_seen: bool,
}

/// Fora da thread principal: a UI tem tempo limite e um lock preso aqui não trava a janela.
/// Os logs mostram no logcat/console se a chamada chegou e quanto levou.
#[tauri::command]
pub async fn bootstrap(app: AppHandle) -> Result<Bootstrap> {
    log::info!("[BOOT] bootstrap requested");
    let started = std::time::Instant::now();
    let r = tauri::async_runtime::spawn_blocking(move || build_bootstrap(&app.state::<AppState>()))
        .await
        .map_err(|e| AppError::Other(e.to_string()))?;
    match &r {
        Ok(_) => log::info!("[BOOT] bootstrap done in {} ms", started.elapsed().as_millis()),
        Err(e) => log::warn!("[BOOT] bootstrap failed in {} ms: {e}", started.elapsed().as_millis()),
    }
    r
}

fn build_bootstrap(state: &AppState) -> Result<Bootstrap> {
    let me = state.identity.read().unwrap().summary();
    let (total, pinned) = state.db.count_items()?;
    Ok(Bootstrap {
        platform: identity::current_platform(),
        me: DeviceIdentity { name: state.display_name(), ..me },
        settings: state.settings(),
        devices: state.devices(),
        items: state.db.list_items(&ItemFilter { limit: 200, ..Default::default() })?.iter().map(rich::for_ui).collect(),
        sync: state.sync_status(),
        listen_port: state.port(),
        total_items: total,
        pinned_items: pinned,
        onboarded: state.db.get_flag("onboarded")?.as_deref() == Some("1"),
        keyboard_prompt_seen: state.db.get_flag("keyboard_prompt_seen")?.as_deref() == Some("1"),
    })
}

#[tauri::command]
pub fn set_keyboard_prompt_seen(state: State<'_, AppState>) -> Result<()> {
    state.db.set_flag("keyboard_prompt_seen", "1")
}

#[tauri::command]
pub fn set_onboarded(state: State<'_, AppState>) -> Result<()> {
    state.db.set_flag("onboarded", "1")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemsPage {
    pub items: Vec<ClipboardItem>,
    pub total: u32,
    pub pinned: u32,
}

#[tauri::command]
pub fn list_items(
    state: State<'_, AppState>,
    query: Option<String>,
    category: Option<String>,
    limit: Option<u32>,
) -> Result<ItemsPage> {
    let items = state.db.list_items(&ItemFilter { query, category, limit: limit.unwrap_or(200) })?;
    let (total, pinned) = state.db.count_items()?;
    Ok(ItemsPage { items: items.iter().map(rich::for_ui).collect(), total, pinned })
}

#[tauri::command]
pub fn latest_item(state: State<'_, AppState>) -> Result<Option<ClipboardItem>> {
    Ok(state.db.latest_item()?.as_ref().map(rich::for_ui))
}

/// Async + `spawn_blocking`: imagem decodifica um PNG, o que não pode travar a thread principal.
#[tauri::command]
pub async fn copy_item(app: AppHandle, id: String) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(item) = app.state::<AppState>().db.get_item(&id)? else {
            return Err(AppError::Other(crate::i18n::tr("err.item_not_found").into()));
        };
        clipboard::copy_item(&app, &item)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Miniatura (ou, com `full`, a imagem inteira se for pequena) de um item `image`, como data URL.
/// A UI não lê o disco: só este comando, e só arquivos referenciados por item de histórico.
#[tauri::command]
pub async fn item_image(app: AppHandle, id: String, full: bool) -> Result<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(item) = app.state::<AppState>().db.get_item(&id)? else {
            return Err(AppError::Other(crate::i18n::tr("err.item_not_found").into()));
        };
        if item.content_type != crate::content::ContentType::Image {
            return Err(AppError::Other(crate::i18n::tr("err.not_image").into()));
        }
        clipboard::image::data_url(&app, &item, full)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Só desktop.
#[tauri::command]
pub fn reveal_item_file(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<()> {
    let Some(item) = state.db.get_item(&id)? else {
        return Err(AppError::Other(crate::i18n::tr("err.item_not_found").into()));
    };
    let path = clipboard::image::file_of(&app, &item)?;
    tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub fn pin_item(state: State<'_, AppState>, id: String, pinned: bool) -> Result<()> {
    state.db.set_pinned(&id, pinned)
}

#[tauri::command]
pub fn delete_item(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<()> {
    let item = state.db.get_item(&id)?;
    state.db.delete_item(&id)?;
    if let Some(item) = item {
        clipboard::image::remove_files(&app, &item);
    }
    Ok(())
}

#[tauri::command]
pub fn clear_history(app: AppHandle, state: State<'_, AppState>, keep_pinned: bool) -> Result<u32> {
    let images = state.db.image_items(keep_pinned)?;
    let n = state.db.clear_items(keep_pinned)?;
    for it in &images {
        clipboard::image::remove_files(&app, it);
    }
    events::emit(&app, events::CLIPBOARD_EXPIRED, serde_json::json!({ "ids": [], "cleared": true }));
    Ok(n)
}

#[tauri::command]
pub fn reveal_item(state: State<'_, AppState>, id: String) -> Result<String> {
    let Some(item) = state.db.get_item(&id)? else {
        return Err(AppError::Other(crate::i18n::tr("err.item_not_found").into()));
    };
    Ok(item.content)
}

/// Abre o link/e-mail de um item no app padrão. A UI só passa o id: o alvo vem do banco
/// e passa por `link_target` (só http/https e mailto), nunca por shell.
#[tauri::command]
pub fn open_item(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<()> {
    use tauri_plugin_opener::OpenerExt;
    let Some(item) = state.db.get_item(&id)? else {
        return Err(AppError::Other(crate::i18n::tr("err.item_not_found").into()));
    };
    let target = crate::content::link_target(&item.content, item.content_type)?;
    app.opener().open_url(target, None::<&str>).map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub fn send_item(app: AppHandle, item_id: String, device_ids: Vec<String>) -> Result<()> {
    sync::send_item(&app, &item_id, &device_ids)
}

#[tauri::command]
pub fn retry_queue(app: AppHandle, state: State<'_, AppState>) -> Result<SyncStatus> {
    for d in state.db.list_devices()? {
        if state.is_connected(&d.device_id) {
            sync::on_connected(&app, &d.device_id);
        } else {
            transport::maybe_connect(app.clone(), d.device_id.clone());
        }
    }
    Ok(state.sync_status())
}

#[tauri::command]
pub fn sync_status(state: State<'_, AppState>) -> Result<SyncStatus> {
    Ok(state.sync_status())
}

#[tauri::command]
pub fn set_paused(app: AppHandle, paused: bool, minutes: Option<u32>) -> Result<SyncStatus> {
    Ok(sync::set_paused(&app, paused, minutes))
}

/// Android: lê o clipboard agora (ao voltar ao primeiro plano).
#[tauri::command]
pub fn clipboard_poll_now(app: AppHandle) -> Result<Option<ClipboardItem>> {
    // Também é um bom momento para drenar o que o IME guardou com o núcleo morto.
    crate::ime::drain_inbox(&app);
    let prov = clipboard::provider(&app);
    Ok(clipboard::poll_once(&app, prov.as_ref()).as_ref().map(rich::for_ui))
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings> {
    Ok(state.settings())
}

#[tauri::command]
pub fn set_settings(app: AppHandle, state: State<'_, AppState>, settings: Settings) -> Result<Settings> {
    let old = state.settings();
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    if old.launch_at_login != settings.launch_at_login {
        crate::autostart::set(&app, settings.launch_at_login)?;
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    if old.shortcuts != settings.shortcuts {
        crate::tray::change_shortcuts(&app, &old.shortcuts, &settings.shortcuts)?;
    }
    state.db.save_settings(&settings)?;
    *state.settings.write().unwrap() = settings.clone();
    if old.language_resolved != settings.language_resolved {
        crate::i18n::apply(&settings);
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        crate::tray::refresh_texts(&app);
    }
    if old.device_name != settings.device_name {
        let name = state.display_name();
        let id = state.identity.read().unwrap().clone();
        identity::rename(&state.app_dir, &state.keystore, &id, &name)?;
        state.identity.write().unwrap().name = name;
        transport::reannounce(&app);
        connection::announce_identity(&app);
    }
    if old.keep_alive != settings.keep_alive {
        crate::keepalive::apply(&app);
    }
    events::emit(&app, events::SETTINGS_CHANGED, &settings);
    events::emit(&app, events::DEVICE_UPDATED, state.devices());
    Ok(settings)
}

/// Ações cujo atalho não registrou na inicialização (outro programa já usa).
#[tauri::command]
pub fn shortcut_failures(app: AppHandle) -> Vec<String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    return crate::tray::shortcut_failures(&app);
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = app;
        Vec::new()
    }
}

/// Solta os atalhos enquanto a tela grava uma combinação nova (senão o próprio atalho
/// atual nunca chega à janela).
#[tauri::command]
pub fn shortcuts_suspend(app: AppHandle, suspend: bool) {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    crate::tray::suspend_shortcuts(&app, suspend);
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let _ = (app, suspend);
}

#[tauri::command]
pub fn list_devices(state: State<'_, AppState>) -> Result<Vec<Device>> {
    Ok(state.devices())
}

#[tauri::command]
pub fn set_device_flags(
    app: AppHandle,
    state: State<'_, AppState>,
    device_id: String,
    auto_send: Option<bool>,
    auto_receive: Option<bool>,
    blocked: Option<bool>,
) -> Result<Vec<Device>> {
    state.db.set_device_flags(&device_id, auto_send, auto_receive, blocked)?;
    if blocked == Some(true) {
        crate::transfer::manager::cancel_all_for_device(&app, &device_id);
        state.send_to(&device_id, crate::transport::protocol::Message::Bye);
    }
    let devices = state.devices();
    events::emit(&app, events::DEVICE_UPDATED, &devices);
    Ok(devices)
}

/// Endereço que o usuário digitou para um dispositivo pareado (o IP mudou e o mDNS não passa).
/// É só uma dica de rota: o Noise continua exigindo a chave pareada.
#[tauri::command]
pub fn set_device_address(app: AppHandle, state: State<'_, AppState>, device_id: String, addr: String) -> Result<Vec<Device>> {
    let Some(dev) = state.db.get_device(&device_id)? else {
        return Err(AppError::Other(crate::i18n::tr("err.device_not_found").into()));
    };
    if dev.blocked {
        return Err(AppError::Other(crate::i18n::tr("err.device_blocked").into()));
    }
    let sock = transport::parse_manual_addr(&addr, transport::PREFERRED_PORT)?;
    state.db.set_last_address(&device_id, &sock.to_string())?;
    log::info!("[DEVICE] Address set manually id={device_id} addr={sock}");
    transport::maybe_connect(app.clone(), device_id);
    let devices = state.devices();
    events::emit(&app, events::DEVICE_UPDATED, &devices);
    Ok(devices)
}

/// Remove e revoga: a chave pública deixa de ser aceita em novos handshakes.
#[tauri::command]
pub fn remove_device(app: AppHandle, state: State<'_, AppState>, device_id: String) -> Result<Vec<Device>> {
    log::info!("[DEVICE] Removed and revoked id={device_id}");
    crate::transfer::manager::cancel_all_for_device(&app, &device_id);
    state.send_to(&device_id, crate::transport::protocol::Message::Bye);
    state.db.remove_device(&device_id, &state.self_id())?;
    state.connections.lock().unwrap().remove(&device_id);
    state.revoked.lock().unwrap().remove(&device_id);
    // Sem isso a entrada do mDNS (TTL longo) o mantém em "na rede", mesmo quando era um app reinstalado.
    state.peers.lock().unwrap().remove(&device_id);
    let devices = state.devices();
    events::emit(&app, events::DEVICE_UPDATED, &devices);
    Ok(devices)
}

#[tauri::command]
pub fn connect_device(app: AppHandle, state: State<'_, AppState>, device_id: String) -> Result<()> {
    state.revoked.lock().unwrap().remove(&device_id);
    transport::maybe_connect(app, device_id);
    Ok(())
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PairingInfo {
    pub pairing_id: String,
    pub code: String,
    pub qr_svg: String,
    pub started_at: i64,
    pub expires_at: i64,
    pub payload: String,
    /// Endereços deste aparelho para digitar no outro (`ip` ou `ip:porta`).
    pub addrs: Vec<String>,
}

#[tauri::command]
pub fn local_addresses(state: State<'_, AppState>) -> Vec<String> {
    transport::display_addrs(&transport::local_addrs(), state.port())
}

/// Lado que exibe o QR (normalmente o PC).
#[tauri::command]
pub fn start_pairing(app: AppHandle, state: State<'_, AppState>) -> Result<PairingInfo> {
    let session = pairing::PairingSession::new();
    let me = state.identity.read().unwrap().summary();
    let payload = pairing::PairingPayload {
        v: pairing::PAYLOAD_VERSION,
        pairing_id: session.id.clone(),
        device_id: me.device_id.clone(),
        name: state.display_name(),
        platform: me.platform.clone(),
        public_key: me.public_key.clone(),
        addrs: pairing::qr_addrs(&transport::local_addrs()),
        port: state.port(),
        secret: session.secret.clone(),
    };
    let qr_svg = pairing::qr_svg(&payload)?;
    let info = PairingInfo {
        pairing_id: session.id.clone(),
        code: session.code.clone(),
        qr_svg,
        started_at: session.started_at,
        expires_at: session.expires_at,
        payload: serde_json::to_string(&payload)?,
        addrs: transport::display_addrs(&transport::local_addrs(), state.port()),
    };
    log::info!("[PAIRING] Pairing started id={} expires_at={}", info.pairing_id, info.expires_at);
    *state.pairing.lock().unwrap() = Some(session);
    transport::reannounce(&app);
    events::emit(&app, events::PAIRING_STARTED, &info);

    let app2 = app.clone();
    let pid = info.pairing_id.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(pairing::PAIRING_TTL_MS as u64 + 500)).await;
        let st = app2.state::<AppState>();
        let expired = {
            let mut p = st.pairing.lock().unwrap();
            match p.as_ref() {
                Some(s) if s.id == pid && s.is_expired() => {
                    *p = None;
                    true
                }
                _ => false,
            }
        };
        if expired {
            log::info!("[PAIRING] Pairing expired id={pid}");
            transport::reannounce(&app2);
            events::emit(&app2, events::PAIRING_FAILED, serde_json::json!({ "pairingId": pid, "reason": "expired" }));
        }
    });
    Ok(info)
}

#[tauri::command]
pub fn cancel_pairing(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    *state.pairing.lock().unwrap() = None;
    transport::reannounce(&app);
    Ok(())
}

/// Rodadas do pareamento por QR. Logo após entrar na rede (ou com o aviso do firewall
/// aberto no PC) o primeiro connect costuma falhar; o segredo do QR está sempre certo,
/// então repetir é seguro e fica abaixo de `MAX_PAIR_FAILURES` do outro lado.
const QR_PAIR_ATTEMPTS: u32 = 3;

async fn pair_at(
    app: &AppHandle,
    addrs: Vec<String>,
    pairing_id: Option<String>,
    auth: pairing::PairAuth,
    secret_or_code: &str,
) -> Result<DeviceIdentity> {
    let password = pairing::password_for(auth, secret_or_code);
    let r = try_pair_addrs(app, addrs, pairing_id, auth, &password).await;
    if let Err(e) = &r {
        events::emit(app, events::PAIRING_FAILED, serde_json::json!({ "reason": e.to_string() }));
    }
    r
}

async fn try_pair_addrs(
    app: &AppHandle,
    addrs: Vec<String>,
    pairing_id: Option<String>,
    auth: pairing::PairAuth,
    password: &[u8],
) -> Result<DeviceIdentity> {
    let mut last_err = AppError::Pairing(crate::i18n::tr("err.pair_no_addr").into());
    for addr in addrs {
        match connection::connect_pair(app.clone(), addr.clone(), pairing_id.clone(), auth, password.to_vec()).await {
            // A conexão do pareamento já segue como sessão cifrada; nada a reconectar.
            Ok(id) => return Ok(id),
            Err(e) => {
                log::warn!("[PAIRING] Pairing attempt failed addr={addr}: {e}");
                let auth_failure = matches!(e, AppError::Pairing(_) | AppError::Crypto(_));
                last_err = e;
                if auth_failure {
                    break;
                }
            }
        }
    }
    Err(last_err)
}

/// Lado que escaneou o QR (normalmente o celular).
#[tauri::command]
pub async fn pair_with_payload(app: AppHandle, raw: String) -> Result<DeviceIdentity> {
    let p = pairing::parse_payload(&raw)?;
    let state = app.state::<AppState>();
    if p.device_id == state.self_id() {
        return Err(AppError::Pairing(crate::i18n::tr("err.pair_self").into()));
    }
    let password = pairing::password_for(pairing::PairAuth::Qr, &p.secret);
    let mut last_err = AppError::Pairing(crate::i18n::tr("err.pair_no_addr").into());
    for attempt in 1..=QR_PAIR_ATTEMPTS {
        if attempt > 1 {
            tokio::time::sleep(std::time::Duration::from_millis(1500 * u64::from(attempt - 1))).await;
        }
        // Remonta a cada rodada: o mDNS pode ter achado o PC enquanto isso.
        let mut addrs: Vec<String> = p.addrs.iter().map(|a| SocketAddr::new(*a, p.port).to_string()).collect();
        for a in transport::candidate_addrs(&state, &p.device_id) {
            if !addrs.contains(&a) {
                addrs.push(a);
            }
        }
        match try_pair_addrs(&app, addrs, Some(p.pairing_id.clone()), pairing::PairAuth::Qr, &password).await {
            Ok(id) => return Ok(id),
            Err(e) => {
                log::warn!("[PAIRING] QR attempt {attempt}/{QR_PAIR_ATTEMPTS} failed: {e}");
                last_err = e;
            }
        }
    }
    events::emit(&app, events::PAIRING_FAILED, serde_json::json!({ "reason": last_err.to_string() }));
    Err(last_err)
}

#[tauri::command]
pub async fn request_pairing(app: AppHandle, device_id: String) -> Result<()> {
    transport::request::send(&app, &device_id).await
}

/// Código digitado manualmente: procura na LAN um dispositivo anunciando pareamento.
/// `device_id`: depois de um pedido de pareamento já se sabe com quem parear; vai direto
/// aos endereços dele em vez de esperar o anúncio mDNS com pairing=1.
/// `addr`: endereço digitado (redes sem mDNS); pula a espera e vai direto a ele.
#[tauri::command]
pub async fn pair_with_code(app: AppHandle, code: String, device_id: Option<String>, addr: Option<String>) -> Result<DeviceIdentity> {
    let code = pairing::normalize_code(&code);
    if code.len() != 7 {
        return Err(AppError::Pairing(crate::i18n::tr("err.pair_code_len").into()));
    }
    if let Some(addr) = addr.as_deref().map(str::trim).filter(|a| !a.is_empty()) {
        let sock = transport::parse_manual_addr(addr, transport::PREFERRED_PORT)?;
        return pair_at(&app, vec![sock.to_string()], None, pairing::PairAuth::Code, &code).await;
    }
    let state = app.state::<AppState>();
    // O anúncio mDNS com pairing=1 pode levar alguns segundos para chegar.
    let mut candidates: Vec<(String, Vec<String>)> = Vec::new();
    for _ in 0..12 {
        candidates = {
            let peers = state.peers.lock().unwrap();
            peers
                .values()
                .filter(|p| match &device_id {
                    Some(id) => &p.device_id == id,
                    None => p.pairing,
                })
                .map(|p| (p.device_id.clone(), p.addrs.iter().map(|a| SocketAddr::new(*a, p.port).to_string()).collect()))
                .collect()
        };
        if !candidates.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    if candidates.is_empty() {
        return Err(AppError::Pairing(crate::i18n::tr("err.pair_none_listening").into()));
    }
    let mut last = AppError::Pairing(crate::i18n::tr("err.pair_bad_code").into());
    for (_id, addrs) in candidates {
        match pair_at(&app, addrs, None, pairing::PairAuth::Code, &code).await {
            Ok(id) => return Ok(id),
            Err(e) => last = e,
        }
    }
    Err(last)
}

#[tauri::command]
pub fn window_action(app: AppHandle, action: String) -> Result<()> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        crate::tray::window_action(&app, &action)?;
    }
    let _ = (&app, &action);
    Ok(())
}

#[tauri::command]
pub fn quit(app: AppHandle) {
    app.exit(0);
}

/// Pasta vira .zip antes (fora da thread principal: zipar pode demorar).
#[tauri::command]
pub async fn transfer_send(app: AppHandle, device_id: String, path: String) -> Result<crate::transfer::store::TransferRecord> {
    tauri::async_runtime::spawn_blocking(move || {
        let src = crate::transfer::folder::prepare(&app, &path)?;
        let r = crate::transfer::manager::send_file(&app, &device_id, &src);
        if r.is_err() && src != path {
            crate::transfer::folder::release(&app, &src);
        }
        r
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
pub fn transfer_list(app: AppHandle, limit: Option<u32>) -> Result<Vec<crate::transfer::store::TransferRecord>> {
    crate::transfer::manager::list(&app, limit.unwrap_or(100))
}

#[tauri::command]
pub fn transfer_clear_finished(app: AppHandle) -> Result<u32> {
    crate::transfer::manager::clear_finished(&app)
}

#[tauri::command]
pub fn transfer_remove(app: AppHandle, id: String) -> Result<()> {
    crate::transfer::manager::remove(&app, &id)
}

#[tauri::command]
pub fn transfer_cancel(app: AppHandle, id: String) -> Result<crate::transfer::store::TransferRecord> {
    crate::transfer::manager::cancel(&app, &id)
}

/// Pedidos "Aceitar/Recusar" abertos (arquivos recebidos acima do limite ou de device sem aceite automático).
#[tauri::command]
pub fn transfer_asks(app: AppHandle) -> Vec<crate::transfer::store::TransferRecord> {
    crate::transfer::manager::asks(&app)
}

#[tauri::command]
pub fn transfer_accept(app: AppHandle, id: String) -> Result<()> {
    crate::transfer::manager::accept(&app, &id)
}

#[tauri::command]
pub fn transfer_decline(app: AppHandle, id: String) -> Result<crate::transfer::store::TransferRecord> {
    crate::transfer::manager::decline(&app, &id)
}

#[tauri::command]
pub fn transfer_resume(app: AppHandle, id: String) -> Result<crate::transfer::store::TransferRecord> {
    crate::transfer::manager::resume(&app, &id)
}

/// Seletor de arquivos (vários). Retorna `{files: [{uri, name, size, mime}]}` ou
/// `{cancelled: true}`. `folder` (só PC) escolhe uma pasta, enviada como .zip.
#[tauri::command]
pub async fn transfer_pick_file(app: AppHandle, folder: Option<bool>) -> Result<serde_json::Value> {
    #[cfg(target_os = "android")]
    {
        let _ = (app, folder);
        if !crate::transfer::android::pick_file()? {
            return Err(AppError::Other(crate::i18n::tr("err.picker_no_activity").into()));
        }
        for _ in 0..(180 * 5) {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            if let Some(json) = crate::transfer::android::take_pick_result()? {
                return Ok(serde_json::from_str(&json)?);
            }
        }
        Err(AppError::Other(crate::i18n::tr("err.picker_no_reply").into()))
    }
    #[cfg(target_os = "ios")]
    {
        let _ = (app, folder);
        Err(AppError::Other(crate::i18n::tr("err.picker_unavailable").into()))
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        use tauri_plugin_dialog::DialogExt;
        let (tx, rx) = tokio::sync::oneshot::channel();
        let dialog = app.dialog().file();
        if folder.unwrap_or(false) {
            dialog.set_title("Enviar pasta").pick_folder(move |f| {
                let _ = tx.send(f.map(|f| vec![f]));
            });
        } else {
            dialog.set_title("Enviar arquivos").pick_files(move |f| {
                let _ = tx.send(f);
            });
        }
        let Some(picked) = rx.await.map_err(|_| AppError::Other(crate::i18n::tr("err.picker_no_reply").into()))? else {
            return Ok(serde_json::json!({ "cancelled": true }));
        };
        let mut files = Vec::new();
        for p in picked {
            let path = p.into_path().map_err(|e| AppError::Other(e.to_string()))?;
            let meta = std::fs::metadata(&path)?;
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("arquivo").to_string();
            let size = if meta.is_dir() { 0 } else { meta.len() };
            files.push(serde_json::json!({ "uri": path.to_string_lossy(), "name": name, "size": size, "mime": serde_json::Value::Null, "dir": meta.is_dir() }));
        }
        Ok(serde_json::json!({ "files": files }))
    }
}

#[tauri::command]
pub fn transfer_open(app: AppHandle, id: String) -> Result<()> {
    open_transfer(&app, &id, false)
}

/// Só desktop.
#[tauri::command]
pub fn transfer_reveal(app: AppHandle, id: String) -> Result<()> {
    open_transfer(&app, &id, true)
}

/// Abre o arquivo de uma transferência concluída, ou (`reveal`) mostra na pasta.
pub(crate) fn open_transfer(app: &AppHandle, id: &str, reveal: bool) -> Result<()> {
    let target = transfer_target(app, id)?;
    #[cfg(target_os = "android")]
    if !reveal {
        return match crate::transfer::android::open_view(&target)? {
            true => Ok(()),
            false => Err(AppError::Other(crate::i18n::tr("err.no_app_to_open").into())),
        };
    }
    let r = if reveal { tauri_plugin_opener::reveal_item_in_dir(&target) } else { tauri_plugin_opener::open_path(&target, None::<&str>) };
    r.map_err(|e| AppError::Other(e.to_string()))
}

/// Caminho abrível de uma transferência: destino final (recebida) ou origem (enviada).
fn transfer_target(app: &AppHandle, id: &str) -> Result<String> {
    let rec = app
        .state::<AppState>()
        .db
        .transfer_get(id)?
        .ok_or_else(|| AppError::Other(crate::i18n::tr("err.transfer_not_found").into()))?;
    if !matches!(rec.state, crate::transfer::state::TransferState::Completed) {
        return Err(AppError::Other(crate::i18n::tr("err.transfer_unfinished").into()));
    }
    let gone = || AppError::Other(crate::i18n::tr("err.file_gone").into());
    let target = rec.final_path.or(rec.source).ok_or_else(gone)?;
    // Movido/apagado depois, ou zip temporário de pasta (some ao terminar o envio).
    // `content://` (Android) não é caminho: quem abre é que valida.
    if !target.contains("://") && !std::path::Path::new(&target).exists() {
        return Err(gone());
    }
    Ok(target)
}

/// "Compartilhar → ClipSync" (Android): `{files:[{uri,name,size,mime}], text}` pendente,
/// consumido na leitura; `null` se não há (ou fora do Android).
#[tauri::command]
pub fn share_take() -> Result<Option<serde_json::Value>> {
    #[cfg(target_os = "android")]
    {
        match crate::transfer::android::share_take()? {
            Some(json) => Ok(Some(serde_json::from_str(&json)?)),
            None => Ok(None),
        }
    }
    #[cfg(not(target_os = "android"))]
    Ok(None)
}

/// Permissão de notificação do Android 13+ (progresso e Cancelar das transferências).
/// `granted` | `prompt` | `blocked` | `unsupported` (fora do Android e Android < 13).
#[tauri::command]
pub fn notify_permission() -> Result<String> {
    #[cfg(target_os = "android")]
    return crate::transfer::android::notify_status();
    #[cfg(not(target_os = "android"))]
    Ok("unsupported".into())
}

/// Mostra o diálogo do sistema (`prompt`) ou abre as configurações de notificação (`blocked`).
#[tauri::command]
pub fn notify_permission_request(open_settings: bool) -> Result<()> {
    #[cfg(target_os = "android")]
    {
        if open_settings {
            crate::transfer::android::notify_open_settings()
        } else {
            crate::transfer::android::notify_request().map(|_| ())
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = open_settings;
        Ok(())
    }
}

/// Se o poll acabou de registrar o mesmo texto, reaproveita o item: o receptor recusa o id repetido.
#[tauri::command]
pub fn share_text(app: AppHandle, state: State<'_, AppState>, text: String, device_ids: Vec<String>) -> Result<ClipboardItem> {
    let hash = crate::db::content_hash(&text);
    // Marca como visto: o próximo poll não cria (nem envia) outro item igual.
    let _ = state.clip_dedupe.lock().unwrap().decide(&hash, crate::models::now_ms());
    let recent = state.db.latest_item()?.filter(|i| {
        i.content_type != crate::content::ContentType::Image
            && crate::db::content_hash(&i.content) == hash
            && crate::models::now_ms() - i.created_at < 60_000
    });
    let item = match recent {
        Some(i) => i,
        None => crate::clipboard::ingest_shared_text(&app, text, false)?,
    };
    crate::sync::send_item(&app, &item.id, &device_ids)?;
    Ok(rich::for_ui(&item))
}

#[tauri::command]
pub fn transfer_peer_supports(app: AppHandle, device_id: String) -> bool {
    crate::transfer::manager::peer_supports_files(&app, &device_id)
}

/// Atualização pendente já encontrada pela verificação em segundo plano.
#[tauri::command]
pub fn update_status() -> Option<serde_json::Value> {
    crate::updater::pending().and_then(|u| serde_json::to_value(u).ok())
}

#[tauri::command]
pub async fn update_check(app: AppHandle) -> Result<Option<serde_json::Value>> {
    Ok(crate::updater::check(&app).await?.and_then(|u| serde_json::to_value(u).ok()))
}

#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<()> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        crate::updater::install(&app).await
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = app;
        Err(AppError::Other(crate::i18n::tr("err.update_desktop_only").into()))
    }
}

/// Android: abre no navegador a página da release da atualização pendente (o APK é baixado de lá).
#[tauri::command]
pub fn update_open(app: AppHandle) -> Result<()> {
    #[cfg(target_os = "android")]
    {
        use tauri_plugin_opener::OpenerExt;
        let url = crate::updater::pending_url().ok_or_else(|| AppError::Other(crate::i18n::tr("err.update_none").into()))?;
        app.opener().open_url(url, None::<&str>).map_err(|e| AppError::Other(e.to_string()))
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Err(AppError::Other(crate::i18n::tr("err.android_only").into()))
    }
}

/// Cor dos ícones das barras do sistema (só Android).
#[tauri::command]
pub fn set_system_bars(dark: bool) -> Result<()> {
    #[cfg(target_os = "android")]
    {
        crate::android_jni::with_class("app.clipsync.desktop.SystemBars", |env, class, _ctx| {
            env.call_static_method(class, "set", "(Z)V", &[jni::objects::JValue::Bool(dark as u8)])
                .map_err(|e| AppError::Other(format!("SystemBars.set: {e}")))?;
            Ok(())
        })
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = dark;
        Ok(())
    }
}
