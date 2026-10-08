//! Nomes de eventos emitidos para a UI. A camada React só conhece estes nomes.
use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub const DEVICE_DISCOVERED: &str = "device:discovered";
pub const DEVICE_CONNECTED: &str = "device:connected";
pub const DEVICE_DISCONNECTED: &str = "device:disconnected";
pub const DEVICE_UPDATED: &str = "device:updated";

pub const PAIRING_STARTED: &str = "pairing:started";
pub const PAIRING_COMPLETED: &str = "pairing:completed";
pub const PAIRING_FAILED: &str = "pairing:failed";
/// Outro dispositivo pediu para parear: `{ deviceId, name, platform }`.
pub const PAIRING_REQUEST: &str = "pairing:request";

pub const CLIPBOARD_CHANGED: &str = "clipboard:changed";
pub const CLIPBOARD_SENT: &str = "clipboard:sent";
pub const CLIPBOARD_RECEIVED: &str = "clipboard:received";
pub const CLIPBOARD_EXPIRED: &str = "clipboard:expired";
/// Arquivos copiados no Explorer: `CopiedFiles`; a UI oferece enviar (nunca automático).
#[cfg_attr(not(windows), allow(dead_code))]
pub const CLIPBOARD_FILES: &str = "clipboard:files";

pub const SYNC_STARTED: &str = "sync:started";
pub const SYNC_COMPLETED: &str = "sync:completed";
pub const SYNC_FAILED: &str = "sync:failed";
pub const SYNC_STATE: &str = "sync:state";

pub const SETTINGS_CHANGED: &str = "settings:changed";
#[cfg_attr(any(target_os = "android", target_os = "ios"), allow(dead_code))]
pub const TRAY_SHOWN: &str = "tray:shown";

/// Transferência de arquivos: `TransferRecord` completo a cada mudança de estado.
pub const TRANSFER_STATE: &str = "transfer:state";
/// `{ id, bytesDone, size }`, no máximo a cada ~250 ms por transferência.
pub const TRANSFER_PROGRESS: &str = "transfer:progress";
/// Oferta esperando "Aceitar/Recusar": `TransferRecord` (estado `incoming`).
pub const TRANSFER_ASK: &str = "transfer:ask";

/// "Compartilhar → ClipSync" pendente (Android); a UI busca com `share_take`.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub const SHARE_RECEIVED: &str = "share:received";

/// Nova versão encontrada pelo updater (desktop): `{ version, currentVersion, notes }`.
#[cfg_attr(any(target_os = "android", target_os = "ios"), allow(dead_code))]
pub const UPDATE_AVAILABLE: &str = "update:available";

pub fn emit<P: Serialize + Clone>(app: &AppHandle, name: &str, payload: P) {
    if let Err(e) = app.emit(name, payload) {
        log::warn!("falha ao emitir {name}: {e}");
    }
}
