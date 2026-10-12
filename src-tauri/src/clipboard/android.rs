//! Clipboard no Android. A partir do Android 10 só o app em primeiro plano lê
//! o clipboard, então além do polling a UI chama `clipboard_poll_now` ao
//! voltar para a frente (evento `visibilitychange`).
use super::ClipboardProvider;
use crate::error::{AppError, Result};
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;

pub struct AndroidClipboard {
    app: AppHandle,
}

impl AndroidClipboard {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl ClipboardProvider for AndroidClipboard {
    fn read_text(&self) -> Result<Option<String>> {
        match self.app.clipboard().read_text() {
            Ok(t) if !t.is_empty() => Ok(Some(t)),
            _ => Ok(None),
        }
    }

    fn write_text(&self, text: &str) -> Result<()> {
        self.app.clipboard().write_text(text.to_string()).map_err(|e| AppError::Other(e.to_string()))
    }

    fn read_html(&self) -> Result<Option<String>> {
        Ok(bridge::read_html())
    }

    fn write_rich(&self, text: &str, html: &str) -> Result<()> {
        if bridge::set_html(text, html) {
            Ok(())
        } else {
            Err(AppError::Other("ClipImageBridge.setHtml falhou".into()))
        }
    }

    fn poll_interval(&self) -> Duration {
        Duration::from_millis(1200)
    }
}

mod bridge {
    use crate::android_jni::{jstring_to_string, with_class};
    use crate::error::AppError;
    use jni::objects::JValue;

    const BRIDGE: &str = "app.clipsync.desktop.clip.ClipImageBridge";

    pub fn read_html() -> Option<String> {
        with_class(BRIDGE, |env, class, ctx| {
            let out = env
                .call_static_method(class, "readHtml", "(Landroid/content/Context;)Ljava/lang/String;", &[JValue::Object(ctx)])
                .and_then(|v| v.l())
                .map_err(|e| AppError::Other(format!("ClipImageBridge.readHtml: {e}")))?;
            if out.is_null() {
                return Ok(None);
            }
            jstring_to_string(env, out).map(Some)
        })
        .ok()?
    }

    pub fn set_html(text: &str, html: &str) -> bool {
        with_class(BRIDGE, |env, class, ctx| {
            let jt = env.new_string(text).map_err(|e| AppError::Other(e.to_string()))?;
            let jh = env.new_string(html).map_err(|e| AppError::Other(e.to_string()))?;
            env.call_static_method(
                class,
                "setHtml",
                "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)Z",
                &[JValue::Object(ctx), JValue::Object(&jt), JValue::Object(&jh)],
            )
            .and_then(|v| v.z())
            .map_err(|e| AppError::Other(format!("ClipImageBridge.setHtml: {e}")))
        })
        .unwrap_or(false)
    }
}
