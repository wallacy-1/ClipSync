//! Cada instalação é um nó independente: identidade e histórico próprios, P2P cifrado na LAN.
#[cfg(target_os = "android")]
mod android_jni;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod autostart;
mod clipboard;
mod commands;
mod content;
mod db;
mod error;
mod events;
mod identity;
mod ime;
mod keystore;
mod models;
mod state;
mod sync;
mod transfer;
mod transport;
mod notify;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod tray;

use state::AppState;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicU16;
use std::sync::{Mutex, RwLock};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("clipsync=debug,clipsync_lib=debug"))
        .try_init();

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init());

    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        builder = builder.plugin(tauri_plugin_barcode_scanner::init());
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        // Precisa ser o primeiro plugin. Perfis de teste (várias instâncias de propósito) ficam de fora.
        if std::env::var("CLIPSYNC_PROFILE").map(|p| p.trim().is_empty()).unwrap_or(true) {
            builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| tray::show_main(app)));
        }
        builder = builder
            .plugin(tauri_plugin_dialog::init())
            .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec![autostart::ARG])));
    }

    builder = builder
        .setup(|app| {
            let handle = app.handle().clone();
            let mut app_dir = handle.path().app_data_dir()?;
            // Perfil alternativo (testes com várias instâncias na mesma máquina).
            if let Ok(profile) = std::env::var("CLIPSYNC_PROFILE") {
                if !profile.trim().is_empty() {
                    app_dir = app_dir.join("profiles").join(profile.trim());
                }
            }
            std::fs::create_dir_all(&app_dir)?;

            let keystore = keystore::Keystore::open(&app_dir)?;
            let identity = identity::load_or_create(&app_dir, &keystore)?;
            let db = db::Db::open(&app_dir)?;
            let settings = db.load_settings()?;
            log::info!("[IDENTITY] device_id={} platform={} name={:?} dir={}", identity.device_id, identity.platform, identity.name, app_dir.display());

            handle.manage(AppState {
                app_dir,
                identity: RwLock::new(identity),
                keystore,
                db,
                settings: RwLock::new(settings),
                paused_until: Mutex::new(None),
                clip_dedupe: Mutex::new(Default::default()),
                peers: Mutex::new(HashMap::new()),
                connections: Mutex::new(HashMap::new()),
                connecting: Mutex::new(HashSet::new()),
                pairing: Mutex::new(None),
                listen_port: AtomicU16::new(0),
                last_error: Mutex::new(None),
                transfers: Default::default(),
            });

            let h = handle.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = transport::start(h.clone()).await {
                    log::error!("transporte não iniciou: {e}");
                    let st = h.state::<AppState>();
                    *st.last_error.lock().unwrap() = Some(e.to_string());
                    events::emit(&h, events::SYNC_FAILED, serde_json::json!({ "reason": e.to_string() }));
                }
            });

            clipboard::spawn_watcher(handle.clone());
            sync::spawn_expiry_task(handle.clone());
            transfer::manager::startup(&handle);
            // ClipSync Keyboard: núcleo pronto para receber clips do IME + drena o inbox.
            ime::attach(&handle);

            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            {
                tray::setup(&handle)?;
                autostart::sync_on_startup(&handle);
                // A janela nasce escondida (tauri.conf.json); no login do Windows fica só a bandeja.
                if autostart::launched_at_login() {
                    log::info!("[AUTOSTART] started at login: tray only");
                } else {
                    tray::show_main(&handle);
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } if window.label() == tray::MAIN_WINDOW => {
                    // Fechar a janela não encerra o app: ele vive na bandeja.
                    api.prevent_close();
                    let _ = window.hide();
                }
                tauri::WindowEvent::Focused(false) if window.label() == tray::TRAY_WINDOW => {
                    let _ = window.hide();
                }
                _ => {}
            }
            #[cfg(any(target_os = "android", target_os = "ios"))]
            let _ = (window, event);
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::set_onboarded,
            commands::set_keyboard_prompt_seen,
            commands::list_items,
            commands::latest_item,
            commands::copy_item,
            commands::pin_item,
            commands::delete_item,
            commands::clear_history,
            commands::reveal_item,
            commands::send_item,
            commands::retry_queue,
            commands::sync_status,
            commands::set_paused,
            commands::clipboard_poll_now,
            commands::get_settings,
            commands::set_settings,
            commands::list_devices,
            commands::set_device_flags,
            commands::remove_device,
            commands::connect_device,
            commands::start_pairing,
            commands::cancel_pairing,
            commands::pair_with_payload,
            commands::pair_with_code,
            commands::window_action,
            commands::quit,
            ime::ime_status,
            ime::ime_open_settings,
            ime::ime_show_picker,
            ime::ime_set_theme,
            commands::transfer_send,
            commands::transfer_list,
            commands::transfer_clear_finished,
            commands::transfer_remove,
            commands::transfer_cancel,
            commands::transfer_resume,
            commands::transfer_asks,
            commands::transfer_accept,
            commands::transfer_decline,
            commands::transfer_peer_supports,
            commands::transfer_pick_file,
            commands::transfer_open,
            commands::transfer_reveal,
            commands::share_take,
            commands::notify_permission,
            commands::notify_permission_request,
            commands::share_text,
        ]);

    builder
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o ClipSync");
}
