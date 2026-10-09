//! Windows (desktop): ícone na bandeja, menu, painel rápido e atalhos globais.
#![cfg(not(any(target_os = "android", target_os = "ios")))]

use crate::clipboard;
use crate::error::{AppError, Result};
use crate::events;
use crate::i18n::{tr, trf};
use crate::models::Shortcuts;
use crate::state::AppState;
use crate::sync;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Wry};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

pub const TRAY_WINDOW: &str = "tray";
pub const MAIN_WINDOW: &str = "main";

pub fn setup(app: &AppHandle) -> anyhow::Result<()> {
    // Painel rápido: janela só do desktop (por isso criada aqui, não no tauri.conf.json).
    WebviewWindowBuilder::new(app, TRAY_WINDOW, WebviewUrl::App("index.html#/tray".into()))
        .title(tr("tray.window"))
        .inner_size(400.0, 560.0)
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .build()?;

    // Os textos são (re)aplicados por `refresh_texts` ao mudar o idioma; aqui entram os do idioma atual.
    let item = |id: &str, key: &str| MenuItem::with_id(app, id, tr(key), true, None::<&str>);
    let open = item("open", "tray.open")?;
    let paste = item("paste_last", "tray.paste_last")?;
    let send = item("send_last", "tray.send_last")?;
    let pause_15 = item("pause_15", "tray.pause_15")?;
    let pause_60 = item("pause_60", "tray.pause_60")?;
    let pause_480 = item("pause_480", "tray.pause_480")?;
    let pause_0 = item("pause_0", "tray.pause_0")?;
    let pause = Submenu::with_items(app, tr("tray.pause"), true, &[&pause_15, &pause_60, &pause_480, &pause_0])?;
    let resume = item("resume", "tray.resume")?;
    let quit = item("quit", "tray.quit")?;
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
                "pause_15" => set_paused(&app, true, Some(15)),
                "pause_60" => set_paused(&app, true, Some(60)),
                "pause_480" => set_paused(&app, true, Some(480)),
                "pause_0" => set_paused(&app, true, None),
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

    app.manage(TrayMenu { open, paste, send, pause, pause_items: [pause_15, pause_60, pause_480, pause_0], resume, quit });
    register_shortcuts(app)?;
    Ok(())
}

/// Itens do menu: `open`/`paste`/`send` exibem o atalho ao lado (atualizado quando ele muda);
/// todos têm o texto refeito quando o idioma muda.
struct TrayMenu {
    open: MenuItem<Wry>,
    paste: MenuItem<Wry>,
    send: MenuItem<Wry>,
    pause: Submenu<Wry>,
    pause_items: [MenuItem<Wry>; 4],
    resume: MenuItem<Wry>,
    quit: MenuItem<Wry>,
}

/// Aplica o idioma atual (`i18n::tr`) ao menu da bandeja e ao título do painel rápido.
pub fn refresh_texts(app: &AppHandle) {
    if let Some(m) = app.try_state::<TrayMenu>() {
        let _ = m.open.set_text(tr("tray.open"));
        let _ = m.paste.set_text(tr("tray.paste_last"));
        let _ = m.send.set_text(tr("tray.send_last"));
        let _ = m.pause.set_text(tr("tray.pause"));
        for (it, key) in m.pause_items.iter().zip(["tray.pause_15", "tray.pause_60", "tray.pause_480", "tray.pause_0"]) {
            let _ = it.set_text(tr(key));
        }
        let _ = m.resume.set_text(tr("tray.resume"));
        let _ = m.quit.set_text(tr("tray.quit"));
    }
    if let Some(w) = app.get_webview_window(TRAY_WINDOW) {
        let _ = w.set_title(tr("tray.window"));
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    OpenPanel,
    PasteLast,
    SendLast,
    TogglePause,
}

impl Action {
    /// Mesmo nome do campo em `Shortcuts` (camelCase), usado pela UI.
    fn key(self) -> &'static str {
        match self {
            Action::OpenPanel => "openPanel",
            Action::PasteLast => "pasteLast",
            Action::SendLast => "sendLast",
            Action::TogglePause => "togglePause",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Action::OpenPanel => tr("sc.open_panel"),
            Action::PasteLast => tr("sc.paste_last"),
            Action::SendLast => tr("sc.send_last"),
            Action::TogglePause => tr("sc.toggle_pause"),
        }
    }
}

fn combos(s: &Shortcuts) -> [(Action, &str); 4] {
    [
        (Action::OpenPanel, s.open_panel.as_str()),
        (Action::PasteLast, s.paste_last.as_str()),
        (Action::SendLast, s.send_last.as_str()),
        (Action::TogglePause, s.toggle_pause.as_str()),
    ]
}

/// Atalhos registrados agora (id do atalho -> ação) e os que falharam na inicialização.
#[derive(Default)]
struct Registry {
    active: Mutex<HashMap<u32, (Action, Shortcut)>>,
    failed: Mutex<Vec<String>>,
}

fn register_shortcuts(app: &AppHandle) -> anyhow::Result<()> {
    app.manage(Registry::default());
    app.plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(|app, shortcut, event| {
                if event.state() != ShortcutState::Pressed {
                    return;
                }
                let action = app.state::<Registry>().active.lock().unwrap().get(&shortcut.id()).map(|(a, _)| *a);
                match action {
                    Some(Action::OpenPanel) => toggle_tray_window(app, None),
                    Some(Action::PasteLast) => paste_last(app),
                    Some(Action::SendLast) => send_last(app),
                    Some(Action::TogglePause) => {
                        let paused = app.state::<AppState>().is_paused();
                        set_paused(app, !paused, None);
                    }
                    None => {}
                }
            })
            .build(),
    )?;
    let shortcuts = app.state::<AppState>().settings().shortcuts;
    let failed = register_all(app, &shortcuts);
    for (action, combo, e) in &failed {
        log::warn!("atalho {combo} ({}) indisponível: {e}", action.label());
    }
    *app.state::<Registry>().failed.lock().unwrap() = failed.iter().map(|(a, _, _)| a.key().to_string()).collect();
    update_menu(app, &shortcuts);
    Ok(())
}

/// A UI escreve "Win"; o `global-hotkey` chama a tecla de "Super".
fn parse(combo: &str) -> std::result::Result<Shortcut, String> {
    let combo = combo.split('+').map(|t| if t.trim().eq_ignore_ascii_case("win") { "Super" } else { t }).collect::<Vec<_>>().join("+");
    Shortcut::from_str(&combo).map_err(|e| e.to_string())
}

/// Registra o que der; devolve as combinações inválidas ou que outro programa já ocupa.
fn register_all(app: &AppHandle, s: &Shortcuts) -> Vec<(Action, String, String)> {
    let gs = app.global_shortcut();
    let reg = app.state::<Registry>();
    let mut failed = Vec::new();
    for (action, combo) in combos(s) {
        if combo.trim().is_empty() {
            continue;
        }
        let res = parse(combo).and_then(|sc| gs.register(sc).map(|_| sc).map_err(|e| e.to_string()));
        match res {
            Ok(sc) => {
                reg.active.lock().unwrap().insert(sc.id(), (action, sc));
            }
            Err(e) => failed.push((action, combo.to_string(), e)),
        }
    }
    failed
}

fn unregister_all(app: &AppHandle) {
    let gs = app.global_shortcut();
    let active: Vec<Shortcut> = app.state::<Registry>().active.lock().unwrap().drain().map(|(_, (_, sc))| sc).collect();
    for sc in active {
        let _ = gs.unregister(sc);
    }
}

fn update_menu(app: &AppHandle, s: &Shortcuts) {
    let Some(menu) = app.try_state::<TrayMenu>() else { return };
    let accel = |c: &str| if c.trim().is_empty() { None } else { Some(c.to_string()) };
    let _ = menu.open.set_accelerator(accel(&s.open_panel));
    let _ = menu.paste.set_accelerator(accel(&s.paste_last));
    let _ = menu.send.set_accelerator(accel(&s.send_last));
}

pub fn shortcut_failures(app: &AppHandle) -> Vec<String> {
    app.try_state::<Registry>().map(|r| r.failed.lock().unwrap().clone()).unwrap_or_default()
}

pub fn suspend_shortcuts(app: &AppHandle, suspend: bool) {
    if app.try_state::<Registry>().is_none() {
        return;
    }
    unregister_all(app);
    if !suspend {
        register_all(app, &app.state::<AppState>().settings().shortcuts);
    }
}

/// Troca os atalhos: valida, registra os novos e, se algum já estiver em uso por outro
/// programa, volta aos antigos e diz qual falhou.
pub fn change_shortcuts(app: &AppHandle, old: &Shortcuts, new: &Shortcuts) -> Result<()> {
    let mut seen: Vec<(Modifiers, Code, Action)> = Vec::new();
    for (action, combo) in combos(new) {
        if combo.trim().is_empty() {
            continue;
        }
        let sc = parse(combo).map_err(|_| AppError::Other(trf("sc.invalid", &[("combo", combo)])))?;
        if let Some(why) = conflict_reason(sc.mods, sc.key) {
            return Err(AppError::Other(trf("sc.reason", &[("combo", combo), ("why", why)])));
        }
        if let Some((_, _, other)) = seen.iter().find(|(m, k, _)| *m == sc.mods && *k == sc.key) {
            return Err(AppError::Other(trf("sc.dup", &[("combo", combo), ("other", other.label())])));
        }
        seen.push((sc.mods, sc.key, action));
    }

    unregister_all(app);
    let failed = register_all(app, new);
    if let Some((action, combo, _)) = failed.first() {
        unregister_all(app);
        register_all(app, old);
        return Err(AppError::Other(trf("sc.taken", &[("combo", combo), ("action", action.label())])));
    }
    app.state::<Registry>().failed.lock().unwrap().clear();
    update_menu(app, new);
    Ok(())
}

/// Combinações que o Windows ou apps comuns já usam. O `RegisterHotKey` aceita várias
/// delas, mas o atalho global tiraria a tecla de todos os outros programas.
fn conflict_reason(mods: Modifiers, key: Code) -> Option<&'static str> {
    use Code::*;
    let ctrl = mods.contains(Modifiers::CONTROL);
    let alt = mods.contains(Modifiers::ALT);
    let shift = mods.contains(Modifiers::SHIFT);
    let win = mods.contains(Modifiers::SUPER);
    if !ctrl && !alt && !win {
        return Some(tr("sc.why_modifier"));
    }
    if win && !ctrl && !alt {
        return Some(tr("sc.why_win"));
    }
    if !win && !shift && ctrl != alt {
        return Some(tr(if ctrl { "sc.why_ctrl" } else { "sc.why_alt" }));
    }
    if ctrl && alt && !shift && !win {
        if key == Delete {
            return Some(tr("sc.why_windows"));
        }
        // Em teclado ABNT2, Ctrl+Alt = AltGr: essas teclas digitam / ? ° ₢ ¹ ² ³ £ ¢ ¬ § ª º.
        if matches!(
            key,
            KeyQ | KeyW | KeyE | KeyC | Digit1 | Digit2 | Digit3 | Digit4 | Digit5 | Digit6 | Equal | BracketLeft | BracketRight
        ) {
            return Some(tr("sc.why_altgr"));
        }
    }
    if ctrl && shift && !alt && !win {
        let common = match key {
            KeyV => Some(tr("sc.why_paste_plain")),
            KeyS => Some(tr("sc.why_save_as")),
            KeyP => Some(tr("sc.why_palette")),
            KeyN => Some(tr("sc.why_new_folder")),
            KeyT => Some(tr("sc.why_reopen_tab")),
            KeyZ => Some(tr("sc.why_redo")),
            KeyI | KeyJ | KeyC => Some(tr("sc.why_devtools")),
            KeyW => Some(tr("sc.why_close_window")),
            KeyB => Some(tr("sc.why_bookmarks")),
            Escape => Some(tr("sc.why_task_manager")),
            Tab => Some(tr("sc.why_prev_tab")),
            Delete => Some(tr("sc.why_clear_data")),
            _ => None,
        };
        if common.is_some() {
            return common;
        }
    }
    if alt && !ctrl && matches!(key, F4 | Tab | Escape | Space) {
        return Some(tr("sc.why_windows"));
    }
    None
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
        // Imagem decodifica o PNG (e pode repetir a escrita): fora da thread do menu/atalho.
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            if let Err(e) = clipboard::copy_item(&app, &item) {
                log::warn!("colar último falhou: {e}");
            }
        });
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
    sync::set_paused(app, paused, minutes);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn reason(combo: &str) -> Option<&'static str> {
        let sc = parse(combo).unwrap();
        conflict_reason(sc.mods, sc.key)
    }

    #[test]
    fn defaults_are_free() {
        for (_, combo) in combos(&Shortcuts::default()) {
            assert_eq!(reason(combo), None, "{combo}");
        }
    }

    #[test]
    fn common_combos_are_rejected() {
        for combo in ["Ctrl+Shift+V", "Ctrl+Shift+S", "Ctrl+Shift+P", "Ctrl+C", "Alt+F4", "Win+V", "Shift+A", "Ctrl+Alt+Q", "Ctrl+Alt+1"] {
            assert!(reason(combo).is_some(), "{combo}");
        }
        assert_eq!(reason("Ctrl+Shift+1"), None);
        assert_eq!(reason("Ctrl+Alt+V"), None);
    }
}
