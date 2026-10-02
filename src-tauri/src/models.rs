//! Tipos compartilhados entre banco, rede e UI (serializados em camelCase).
use crate::content::ContentType;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardItem {
    pub id: String,
    pub content: String,
    pub content_type: ContentType,
    pub source_device_id: String,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub is_pinned: bool,
    pub is_sensitive: bool,
    /// Estado de sincronização local: synced | sending | queued | failed | local
    pub sync_state: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceState {
    Discovered,
    Pairing,
    Paired,
    Connecting,
    Connected,
    Disconnected,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub device_id: String,
    pub name: String,
    pub platform: String,
    pub public_key: String,
    pub state: DeviceState,
    pub is_self: bool,
    pub auto_send: bool,
    pub auto_receive: bool,
    pub last_seen: Option<i64>,
    pub address: Option<String>,
    pub paired_at: Option<i64>,
    pub last_error: Option<String>,
    pub queued: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub device_name: String,
    pub auto_send: bool,
    pub auto_receive: bool,
    /// Categorias sincronizadas: text, link, code, email, sensitive, image
    pub synced_types: Vec<String>,
    /// Versão da lista de categorias; sobe quando surge categoria nova ligada por padrão.
    /// Campo ausente (configuração salva antes dele) = 0.
    #[serde(default)]
    pub types_rev: u32,
    pub lan_only: bool,
    pub ignore_password_managers: bool,
    /// Horas até apagar do histórico; 0 = nunca
    pub auto_expire_hours: u32,
    /// Ocultar tokens/cartões e expirar em 2 min
    pub hide_sensitive: bool,
    pub sensitive_expire_minutes: u32,
    pub excluded_apps: Vec<String>,
    pub launch_at_login: bool,
}

impl Settings {
    pub const TYPES_REV: u32 = 1;

    /// Liga as categorias novas em configurações salvas antes delas existirem.
    pub fn upgrade_types(&mut self) {
        if self.types_rev < 1 && !self.synced_types.iter().any(|t| t == "image") {
            self.synced_types.push("image".into());
        }
        self.types_rev = self.types_rev.max(Self::TYPES_REV);
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            device_name: String::new(),
            auto_send: true,
            auto_receive: true,
            synced_types: vec!["text".into(), "link".into(), "code".into(), "email".into(), "image".into()],
            types_rev: Settings::TYPES_REV,
            lan_only: true,
            ignore_password_managers: true,
            auto_expire_hours: 24,
            hide_sensitive: true,
            sensitive_expire_minutes: 2,
            excluded_apps: vec!["Bitwarden".into(), "1Password".into()],
            launch_at_login: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub paused: bool,
    pub paused_until: Option<i64>,
    pub online: bool,
    pub connected_count: u32,
    pub queued_count: u32,
    pub last_error: Option<String>,
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
