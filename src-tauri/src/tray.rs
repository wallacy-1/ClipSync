//! Windows (desktop): ícone na bandeja, menu, painel rápido e atalhos globais.
#![cfg(not(any(target_os = "android", target_os = "ios")))]

use crate::clipboard;
use crate::error::{AppError, Result};
use crate::events;
use crate::state::AppState;
use crate::sync;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, ShortcutState};

pub const TRAY_WINDOW: &str = "tray";
pub const MAIN_WINDOW: &str = "main";

pub fn setup(app: &AppHandle) -> anyhow::Result<()> {
    // Painel rápido: janela só do desktop (por isso criada aqui, não no tauri.conf.json).
    WebviewWindowBuilder::new(app, TRAY_WINDOW, WebviewUrl::App("index.html#/tray".into()))
        .title("ClipSync — Painel rápido")
        .inner_size(400.0, 560.0)
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .build()?;

    let open = MenuItem::with_id(app, "open", "Abrir ClipSync", true, Some("Ctrl+Shift+V"))?;
    let paste = MenuItem::with_id(app, "paste_last", "Colar último item", true, Some("Ctrl+Shift+1"))?;
    let send = MenuItem::with_id(app, "send_last", "Enviar último item", true, Some("Ctrl+Shift+S"))?;
    let pause = MenuItem::with_id(app, "pause_1h", "Pausar sync por 1 h", true, None::<&str>)?;
    let resume = MenuItem::with_id(app, "resume", "Retomar sync", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&open, &paste, &send, &pause, &resume, &PredefinedMenuItem::separator(app)?, &quit],
    )?;

    let icon = app.default_window_icon().cloned().ok_or_else(|| anyhow::anyhow!("ícone padrão ausente"))?;
    TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("ClipSync")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let app = app.clone();
            match event.id.as_ref() {
                "open" => show_main(&app),
                "paste_last" => paste_last(&app),
                "send_last" => send_last(&app),
                "pause_1h" => set_paused(&app, true, Some(60)),
                "resume" => set_paused(&app, false, None),
                "quit" => app.exit(0),
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, position, .. } = event {
                toggle_tray_window(tray.app_handle(), Some(position));
            }
        })
        .build(app)?;

    register_shortcuts(app)?;
    Ok(())
}

fn register_shortcuts(app: &AppHandle) -> anyhow::Result<()> {
    app.plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(|app, shortcut, event| {
                if event.state() != ShortcutState::Pressed {
                    return;
                }
                let mods = Modifiers::CONTROL | Modifiers::SHIFT;
                if shortcut.matches(mods, Code::KeyV) {
                    toggle_tray_window(app, None);
                } else if shortcut.matches(mods, Code::Digit1) {
                    paste_last(app);
                } else if shortcut.matches(mods, Code::KeyS) {
                    send_last(app);
                } else if shortcut.matches(mods, Code::KeyP) {
                    let paused = app.state::<AppState>().is_paused();
                    set_paused(app, !paused, None);
                }
            })
            .build(),
    )?;
    let gs = app.global_shortcut();
    for combo in ["ctrl+shift+v", "ctrl+shift+1", "ctrl+shift+s", "ctrl+shift+p"] {
        if let Err(e) = gs.register(combo) {
            log::warn!("atalho {combo} indisponível: {e}");
        }
    }
    Ok(())
}

fn main_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(MAIN_WINDOW)
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = main_window(app) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    if let Some(t) = app.get_webview_window(TRAY_WINDOW) {
        let _ = t.hide();
    }
}

pub fn toggle_tray_window(app: &AppHandle, at: Option<PhysicalPosition<f64>>) {
    let Some(w) = app.get_webview_window(TRAY_WINDOW) else { return };
    if w.is_visible().unwrap_or(false) {
        let _ = w.hide();
        return;
    }
    // Posiciona perto do cursor (ou do ícone), sem sair do monitor.
    let pos = at.or_else(|| app.cursor_position().ok());
    if let (Some(p), Ok(size)) = (pos, w.outer_size()) {
        let monitor = w.monitor_from_point(p.x, p.y).ok().flatten().or_else(|| w.primary_monitor().ok().flatten());
        if let Some(m) = monitor {
            let ms = m.size();
            let mp = m.position();
            let mut x = p.x as i32 - size.width as i32 / 2;
            let mut y = p.y as i32 - size.height as i32 - 12;
            x = x.max(mp.x + 8).min(mp.x + ms.width as i32 - size.width as i32 - 8);
            if y < mp.y + 8 {
                y = p.y as i32 + 12;
            }
            let _ = w.set_position(tauri::Position::Physical(PhysicalPosition::new(x, y)));
        }
    }
    let _ = w.show();
    let _ = w.set_focus();
    events::emit(app, events::TRAY_SHOWN, ());
}

fn paste_last(app: &AppHandle) {
    let state = app.state::<AppState>();
    if let Ok(Some(item)) = state.db.latest_item() {
        if let Err(e) = clipboard::write_local(app, &item.content) {
            log::warn!("colar último falhou: {e}");
        }
    }
}

fn send_last(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Ok(Some(item)) = state.db.latest_item() else { return };
    let targets: Vec<String> = state
        .db
        .list_devices()
        .unwrap_or_default()
        .into_iter()
        .filter(|d| !d.blocked)
        .map(|d| d.device_id)
        .collect();
    if let Err(e) = sync::send_item(app, &item.id, &targets) {
        log::warn!("enviar último falhou: {e}");
    }
}

fn set_paused(app: &AppHandle, paused: bool, minutes: Option<u32>) {
    let state = app.state::<AppState>();
    {
        let mut p = state.paused_until.lock().unwrap();
        *p = if !paused {
            None
        } else {
            match minutes {
                Some(m) => Some(crate::models::now_ms() + m as i64 * 60_000),
                None => Some(0),
            }
        };
    }
    events::emit(app, events::SYNC_STATE, state.sync_status());
}

/// Ações da barra de título customizada e do painel rápido.
pub fn window_action(app: &AppHandle, action: &str) -> Result<()> {
    match action {
        "minimize" => {
            if let Some(w) = main_window(app) {
                w.minimize()?;
            }
        }
        "toggle_maximize" => {
            if let Some(w) = main_window(app) {
                if w.is_maximized()? {
                    w.unmaximize()?;
                } else {
                    w.maximize()?;
                }
            }
        }
        "close" => {
            // Fechar = esconder na bandeja; o app segue sincronizando.
            if let Some(w) = main_window(app) {
                w.hide()?;
            }
        }
        "show_main" => show_main(app),
        "toggle_tray" => toggle_tray_window(app, None),
        "hide_tray" => {
            if let Some(w) = app.get_webview_window(TRAY_WINDOW) {
                w.hide()?;
            }
        }
        other => return Err(AppError::Other(format!("ação desconhecida: {other}"))),
    }
    Ok(())
}
