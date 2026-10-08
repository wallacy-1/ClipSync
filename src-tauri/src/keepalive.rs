//! Ajuste "Manter conectado" (só Android): liga/desliga o `KeepAliveService`.
//! Sem ele o Android congela o processo fora do app e corta os sockets.
use crate::state::AppState;
use tauri::{AppHandle, Manager};

/// Deixa o serviço como o ajuste pede. Do segundo plano o sistema pode recusar o
/// start; `MainActivity.onResume` chama de novo.
pub fn apply(app: &AppHandle) {
    let on = app.state::<AppState>().settings().keep_alive;
    if let Err(e) = platform::set(on) {
        log::warn!("[KEEPALIVE] falha ao {}: {e}", if on { "ligar" } else { "desligar" });
    }
}

#[cfg(target_os = "android")]
mod platform {
    use crate::android_jni::with_class;
    use crate::error::{AppError, Result};
    use jni::objects::JValue;

    const CLASS: &str = "app.clipsync.desktop.keepalive.KeepAliveService";

    pub fn set(on: bool) -> Result<()> {
        with_class(CLASS, |env, class, ctx| {
            if on {
                let started = env
                    .call_static_method(class, "start", "(Landroid/content/Context;)Z", &[JValue::Object(ctx)])
                    .and_then(|v| v.z())
                    .map_err(|e| AppError::Other(format!("KeepAliveService.start: {e}")))?;
                if !started {
                    log::info!("[KEEPALIVE] start recusado em segundo plano; tenta no próximo resume");
                }
            } else {
                env.call_static_method(class, "stop", "()V", &[])
                    .map_err(|e| AppError::Other(format!("KeepAliveService.stop: {e}")))?;
            }
            Ok(())
        })
    }
}

#[cfg(not(target_os = "android"))]
mod platform {
    pub fn set(_on: bool) -> crate::error::Result<()> {
        Ok(())
    }
}
