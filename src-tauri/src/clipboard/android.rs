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

    fn poll_interval(&self) -> Duration {
        Duration::from_millis(1200)
    }
}
