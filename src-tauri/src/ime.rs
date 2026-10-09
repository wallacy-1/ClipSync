//! ClipSync Keyboard (IME opcional do Android) via `ImeBridge.kt`/JNI; fora do Android `supported = false`.
//! Também é a ponte de clipboard do teclado: `CoreBridge.nativeOnClip` → `clipboard::handle_text`.
use crate::error::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImeTheme {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImeStatus {
    /// Plataforma tem o teclado (só Android).
    pub supported: bool,
    /// Habilitado em Configurações → Teclados.
    pub enabled: bool,
    pub selected: bool,
    pub theme: String,
    pub themes: Vec<ImeTheme>,
}

impl ImeStatus {
    #[cfg_attr(target_os = "android", allow(dead_code))]
    fn unsupported() -> Self {
        Self { supported: false, enabled: false, selected: false, theme: "default".into(), themes: vec![] }
    }
}

#[tauri::command]
pub fn ime_status() -> Result<ImeStatus> {
    platform::status()
}

#[tauri::command]
pub fn ime_open_settings() -> Result<()> {
    platform::open_settings()
}

#[tauri::command]
pub fn ime_show_picker() -> Result<()> {
    platform::show_picker()
}

#[tauri::command]
pub fn ime_set_theme(theme: String) -> Result<ImeStatus> {
    platform::set_theme(&theme)?;
    platform::status()
}

#[cfg(target_os = "android")]
mod platform {
    use super::ImeStatus;
    use crate::android_jni::{jstring_to_string, with_class};
    use crate::error::{AppError, Result};
    use jni::objects::JValue;

    const CLASS: &str = "app.clipsync.desktop.ime.ImeBridge";
    pub use super::platform_inbox::{drain_inbox, set_core_ready};

    pub fn status() -> Result<ImeStatus> {
        let json = with_class(CLASS, |env, class, ctx| {
            let out = env
                .call_static_method(class, "status", "(Landroid/content/Context;)Ljava/lang/String;", &[JValue::Object(ctx)])
                .and_then(|v| v.l())
                .map_err(|e| AppError::Other(format!("ImeBridge.status: {e}")))?;
            jstring_to_string(env, out)
        })?;
        #[derive(serde::Deserialize)]
        struct Raw {
            enabled: bool,
            selected: bool,
            theme: String,
            themes: Vec<super::ImeTheme>,
        }
        let raw: Raw = serde_json::from_str(&json)?;
        Ok(ImeStatus { supported: true, enabled: raw.enabled, selected: raw.selected, theme: raw.theme, themes: raw.themes })
    }

    fn call_void(method: &str) -> Result<()> {
        with_class(CLASS, |env, class, ctx| {
            env.call_static_method(class, method, "(Landroid/content/Context;)V", &[JValue::Object(ctx)])
                .map_err(|e| AppError::Other(format!("ImeBridge.{method}: {e}")))?;
            Ok(())
        })
    }

    pub fn open_settings() -> Result<()> {
        call_void("openSettings")
    }

    pub fn show_picker() -> Result<()> {
        call_void("showPicker")
    }

    pub fn set_theme(theme: &str) -> Result<()> {
        with_class(CLASS, |env, class, ctx| {
            let js = env.new_string(theme).map_err(|e| AppError::Other(e.to_string()))?;
            env.call_static_method(
                class,
                "setTheme",
                "(Landroid/content/Context;Ljava/lang/String;)V",
                &[JValue::Object(ctx), JValue::Object(&js)],
            )
            .map_err(|e| AppError::Other(format!("ImeBridge.setTheme: {e}")))?;
            Ok(())
        })
    }
}

#[cfg(not(target_os = "android"))]
mod platform {
    use super::ImeStatus;
    use crate::error::{AppError, Result};

    pub fn status() -> Result<ImeStatus> {
        Ok(ImeStatus::unsupported())
    }
    pub fn open_settings() -> Result<()> {
        Err(AppError::Other(crate::i18n::tr("err.keyboard_android_only").into()))
    }
    pub fn show_picker() -> Result<()> {
        Err(AppError::Other(crate::i18n::tr("err.keyboard_android_only").into()))
    }
    pub fn set_theme(_theme: &str) -> Result<()> {
        Err(AppError::Other(crate::i18n::tr("err.keyboard_android_only").into()))
    }
    pub fn set_core_ready(_ready: bool) {}
    pub fn drain_inbox(_app: &tauri::AppHandle) {}
}

use std::sync::OnceLock;
use tauri::AppHandle;

static APP: OnceLock<AppHandle> = OnceLock::new();

/// Itens do inbox mais velhos que isto são descartados sem processar
/// (espelha a limpeza automática do clipboard do Android 13+, ~1 h).
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub const INBOX_TTL_MS: i64 = 60 * 60 * 1_000;

/// Handle global do app (existe após o `setup`), para callbacks JNI fora de comandos.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub fn app_handle() -> Option<&'static AppHandle> {
    APP.get()
}

/// Chamado no fim do `setup`: a partir daqui o IME pode entregar direto.
pub fn attach(app: &AppHandle) {
    let _ = APP.set(app.clone());
    platform::set_core_ready(true);
    let h = app.clone();
    tauri::async_runtime::spawn_blocking(move || drain_inbox(&h));
}

/// Processa o que o IME guardou enquanto o núcleo não existia. Cada item só é
/// removido do inbox depois de `handle_text` (ignorado por dedupe/anti-loop
/// também conta como processado).
pub fn drain_inbox(app: &AppHandle) {
    platform::drain_inbox(app);
}

#[cfg(target_os = "android")]
mod bridge {
    use super::APP;
    use crate::clipboard::{self, ClipOrigin};
    use jni::objects::{JClass, JString};
    use jni::sys::{jboolean, JNI_FALSE, JNI_TRUE};
    use jni::JNIEnv;

    /// `CoreBridge.nativeOnClip(text, sensitive): Boolean` — `false` = núcleo
    /// indisponível, o Kotlin guarda no inbox. Chamado fora da thread principal.
    #[no_mangle]
    pub extern "system" fn Java_app_clipsync_desktop_ime_CoreBridge_nativeOnClip(
        mut env: JNIEnv,
        _class: JClass,
        text: JString,
        sensitive: jboolean,
    ) -> jboolean {
        let Some(app) = APP.get() else {
            return JNI_FALSE;
        };
        let text: String = match env.get_string(&text) {
            Ok(s) => s.into(),
            Err(_) => return JNI_TRUE, // inválido: não há o que guardar
        };
        let sensitive = sensitive != JNI_FALSE;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            clipboard::handle_text(app, text, None, sensitive, ClipOrigin::Ime);
        }));
        if result.is_ok() {
            JNI_TRUE
        } else {
            log::error!("[CLIPBOARD] handle_text entrou em pânico na ponte do IME");
            JNI_FALSE
        }
    }
}

#[cfg(target_os = "android")]
mod wake_bridge {
    use super::APP;
    use jni::objects::JClass;
    use jni::JNIEnv;

    /// `CoreBridge.nativeWake()`: teclado apareceu ou app voltou à frente.
    #[no_mangle]
    pub extern "system" fn Java_app_clipsync_desktop_ime_CoreBridge_nativeWake(_env: JNIEnv, _class: JClass) {
        let Some(app) = APP.get() else { return };
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::transport::wake(app))).is_err() {
            log::error!("[TRANSPORT] wake entrou em pânico na ponte do IME");
        }
    }
}

#[cfg(target_os = "android")]
mod platform_inbox {
    use super::INBOX_TTL_MS;
    use crate::android_jni::{jstring_to_string, with_class};
    use crate::clipboard::{self, ClipOrigin};
    use crate::error::{AppError, Result};
    use crate::models::now_ms;
    use jni::objects::JValue;
    use tauri::AppHandle;

    const CLASS: &str = "app.clipsync.desktop.ime.CoreBridge";

    #[derive(serde::Deserialize)]
    struct InboxEntry {
        id: String,
        text: String,
        ts: i64,
        #[serde(default)]
        sensitive: bool,
    }

    pub fn set_core_ready(ready: bool) {
        let r = with_class(CLASS, |env, class, _ctx| {
            env.call_static_method(class, "setReady", "(Z)V", &[JValue::Bool(ready as u8)])
                .map_err(|e| AppError::Other(format!("CoreBridge.setReady: {e}")))?;
            Ok(())
        });
        if let Err(e) = r {
            log::warn!("[IME] não foi possível sinalizar núcleo pronto: {e}");
        }
    }

    fn list() -> Result<Vec<InboxEntry>> {
        let json = with_class(CLASS, |env, class, ctx| {
            let out = env
                .call_static_method(class, "listInbox", "(Landroid/content/Context;)Ljava/lang/String;", &[JValue::Object(ctx)])
                .and_then(|v| v.l())
                .map_err(|e| AppError::Other(format!("CoreBridge.listInbox: {e}")))?;
            jstring_to_string(env, out)
        })?;
        Ok(serde_json::from_str(&json)?)
    }

    fn remove(id: &str) -> Result<()> {
        with_class(CLASS, |env, class, ctx| {
            let js = env.new_string(id).map_err(|e| AppError::Other(e.to_string()))?;
            env.call_static_method(
                class,
                "removeInbox",
                "(Landroid/content/Context;Ljava/lang/String;)V",
                &[JValue::Object(ctx), JValue::Object(&js)],
            )
            .map_err(|e| AppError::Other(format!("CoreBridge.removeInbox: {e}")))?;
            Ok(())
        })
    }

    pub fn drain_inbox(app: &AppHandle) {
        let entries = match list() {
            Ok(e) => e,
            Err(e) => {
                log::warn!("[IME] inbox ilegível: {e}");
                return;
            }
        };
        if entries.is_empty() {
            return;
        }
        log::info!("[IME] Draining inbox count={}", entries.len());
        let now = now_ms();
        for e in entries {
            if now - e.ts > INBOX_TTL_MS {
                log::info!("[IME] Inbox item expired id={} age_ms={}", e.id, now - e.ts);
                let _ = remove(&e.id);
                continue;
            }
            // Só remove depois de passar pelo pipeline (pânico = fica para a próxima).
            let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                clipboard::handle_text(app, e.text, None, e.sensitive, ClipOrigin::ImeInbox)
            }))
            .is_ok();
            if ok {
                if let Err(err) = remove(&e.id) {
                    log::warn!("[IME] não removeu item do inbox id={}: {err}", e.id);
                }
            }
        }
    }
}
