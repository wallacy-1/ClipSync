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
    pub sync_state: SyncState,
    pub metadata: serde_json::Value,
}

/// Estado de sincronização local do item. Vai no fio dentro de `Clip`, mas o receptor
/// sempre sobrescreve; `other` só protege contra valor desconhecido de outra versão.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncState {
    Sending,
    Queued,
    Synced,
    #[serde(other)]
    Local,
}

impl SyncState {
    pub fn as_str(self) -> &'static str {
        match self {
            SyncState::Local => "local",
            SyncState::Sending => "sending",
            SyncState::Queued => "queued",
            SyncState::Synced => "synced",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "sending" => SyncState::Sending,
            "queued" => SyncState::Queued,
            "synced" => SyncState::Synced,
            _ => SyncState::Local,
        }
    }
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
    /// Espelho da entrada no registro (`autostart.rs`), que é a fonte da verdade.
    pub launch_at_login: bool,
    /// A primeira execução já decidiu o padrão (ligado no build instalado).
    pub autostart_initialized: bool,
    /// Arquivo recebido: "auto" (sempre aceita), "limit" (aceita até `file_auto_accept_mb`,
    /// pergunta acima), "ask" (sempre pergunta). Imagem do clipboard não passa por aqui.
    pub file_accept: String,
    pub file_auto_accept_mb: u32,
    /// Toasts do Windows (arquivo recebido, pedido de aceite) com a janela sem foco.
    pub notifications: bool,
    /// Também avisar texto recebido (desligado: o clipboard muda o tempo todo).
    pub notify_text: bool,
    /// Android: serviço em primeiro plano que mantém a conexão viva fora do app.
    pub keep_alive: bool,
    /// Windows: atalhos globais; string vazia = desligado.
    pub shortcuts: Shortcuts,
    /// Aparência: "system" (segue o sistema), "light" ou "dark".
    pub theme: String,
    /// Captura e envia o HTML junto do texto (e escreve o recebido com formatação).
    pub keep_formatting: bool,
    /// Consulta o GitHub sozinho atrás de versão nova (15 s após abrir e a cada 6 h). O manual sempre funciona.
    pub check_updates: bool,
    /// Idioma da interface: "auto" (segue o sistema), "pt-BR" ou "en".
    pub language: String,
    /// "pt-BR" ou "en" já resolvido pelo front (`navigator.language` quando "auto"); é o que o Rust lê
    /// (`i18n.rs`). Antes do primeiro bootstrap vale pt-BR.
    pub language_resolved: String,
}

/// Combinações no formato "Ctrl+Alt+Shift+V" (o mesmo que o `global-hotkey` entende).
/// Padrão em Ctrl+Alt+Shift: Ctrl+Shift+V/S/P são de "colar sem formatação",
/// "Salvar como" e paletas de comando em muitos apps, e o atalho global rouba a tecla deles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Shortcuts {
    pub open_panel: String,
    pub paste_last: String,
    pub send_last: String,
    pub toggle_pause: String,
}

impl Default for Shortcuts {
    fn default() -> Self {
        Self {
            open_panel: "Ctrl+Alt+Shift+V".into(),
            paste_last: "Ctrl+Alt+Shift+C".into(),
            send_last: "Ctrl+Alt+Shift+E".into(),
            toggle_pause: "Ctrl+Alt+Shift+P".into(),
        }
    }
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
            launch_at_login: false,
            autostart_initialized: false,
            file_accept: "limit".into(),
            file_auto_accept_mb: 100,
            notifications: true,
            notify_text: false,
            keep_alive: false,
            shortcuts: Shortcuts::default(),
            theme: "system".into(),
            keep_formatting: true,
            check_updates: true,
            language: "auto".into(),
            language_resolved: "pt-BR".into(),
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
