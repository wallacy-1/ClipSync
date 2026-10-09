//! Clipboard no Windows (e outros desktops): leitura/escrita pelo plugin do
//! Tauri; no Windows o número de sequência evita ler o clipboard a cada tick e
//! a flag `ExcludeClipboardContentFromMonitorProcessing` respeita gerenciadores
//! de senha.
use super::ClipboardProvider;
use crate::error::{AppError, Result};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;

pub struct WindowsClipboard {
    app: AppHandle,
    last_seq: AtomicU32,
}

impl WindowsClipboard {
    pub fn new(app: AppHandle) -> Self {
        Self { app, last_seq: AtomicU32::new(0) }
    }
}

#[cfg(windows)]
fn sequence_number() -> u32 {
    // SAFETY: função sem argumentos que só lê um contador do sistema.
    unsafe { windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber() }
}

#[cfg(windows)]
fn excluded_format_available() -> bool {
    use windows_sys::Win32::System::DataExchange::{IsClipboardFormatAvailable, RegisterClipboardFormatW};
    let name: Vec<u16> = "ExcludeClipboardContentFromMonitorProcessing\0".encode_utf16().collect();
    // SAFETY: `name` é UTF-16 terminado em NUL e vive até o fim da chamada.
    unsafe {
        let fmt = RegisterClipboardFormatW(name.as_ptr());
        fmt != 0 && IsClipboardFormatAvailable(fmt) != 0
    }
}

impl ClipboardProvider for WindowsClipboard {
    fn read_text(&self) -> Result<Option<String>> {
        match self.app.clipboard().read_text() {
            Ok(t) => Ok(Some(t)),
            Err(_) => Ok(None), // sem texto (imagem, arquivo, vazio)
        }
    }

    fn write_text(&self, text: &str) -> Result<()> {
        self.app.clipboard().write_text(text.to_string()).map_err(|e| AppError::Other(e.to_string()))
    }

    #[cfg(windows)]
    fn read_html(&self) -> Result<Option<String>> {
        // O plugin só fala texto; o CF_HTML sai pelo `arboard` (sem HTML = erro, vira `None`).
        Ok(arboard::Clipboard::new().ok().and_then(|mut cb| cb.get().html().ok()))
    }

    #[cfg(windows)]
    fn write_rich(&self, text: &str, html: &str) -> Result<()> {
        // CF_HTML + texto numa operação só.
        let mut cb = arboard::Clipboard::new().map_err(|e| AppError::Other(e.to_string()))?;
        cb.set_html(html, Some(text)).map_err(|e| AppError::Other(e.to_string()))
    }

    #[cfg(windows)]
    fn changed_hint(&self) -> bool {
        let seq = sequence_number();
        self.last_seq.swap(seq, Ordering::Relaxed) != seq
    }

    #[cfg(windows)]
    fn is_excluded_by_source(&self) -> bool {
        excluded_format_available()
    }

    fn poll_interval(&self) -> Duration {
        Duration::from_millis(400)
    }
}
