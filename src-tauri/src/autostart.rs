//! "Iniciar com o Windows": entrada em `HKCU\...\Run` (plugin autostart) com `--autostart`,
//! que sobe o app só na bandeja. O registro é a fonte da verdade; `Settings::launch_at_login`
//! só o espelha (o usuário pode desligar pelo Gerenciador de Tarefas).
use crate::error::{AppError, Result};
use crate::state::AppState;
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;

pub const ARG: &str = "--autostart";

/// Este processo foi aberto pelo Windows no login (não pelo usuário).
pub fn launched_at_login() -> bool {
    std::env::args().any(|a| a == ARG)
}

/// Build de desenvolvimento ou perfil de teste nunca se registra sozinho: o boot do
/// Windows passaria a abrir um exe de `target/debug`.
fn may_auto_enable() -> bool {
    !cfg!(debug_assertions) && std::env::var("CLIPSYNC_PROFILE").map(|p| p.trim().is_empty()).unwrap_or(true)
}

/// Subida: na primeira execução liga (padrão do produto); depois só alinha a
/// configuração ao que está no registro.
pub fn sync_on_startup(app: &AppHandle) {
    let state = app.state::<AppState>();
    let mut s = state.settings();
    let launcher = app.autolaunch();
    let before = (s.launch_at_login, s.autostart_initialized);
    if !s.autostart_initialized {
        s.autostart_initialized = true;
        if may_auto_enable() {
            match launcher.enable() {
                Ok(()) => s.launch_at_login = true,
                Err(e) => log::warn!("[AUTOSTART] não ligou na primeira execução: {e}"),
            }
        }
    }
    if let Ok(on) = launcher.is_enabled() {
        s.launch_at_login = on;
    }
    if (s.launch_at_login, s.autostart_initialized) != before {
        log::info!("[AUTOSTART] launch_at_login={}", s.launch_at_login);
        let _ = state.db.save_settings(&s);
        *state.settings.write().unwrap() = s;
    }
}

/// Usuário mudou o ajuste.
pub fn set(app: &AppHandle, enabled: bool) -> Result<()> {
    let launcher = app.autolaunch();
    if launcher.is_enabled().ok() == Some(enabled) {
        return Ok(());
    }
    let r = if enabled { launcher.enable() } else { launcher.disable() };
    r.map_err(|e| AppError::Other(format!("não foi possível {} o início com o Windows: {e}", if enabled { "ligar" } else { "desligar" })))?;
    log::info!("[AUTOSTART] {}", if enabled { "enabled" } else { "disabled" });
    Ok(())
}
