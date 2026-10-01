use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("banco de dados: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("criptografia: {0}")]
    Crypto(String),
    #[error("rede: {0}")]
    Net(String),
    #[error("pareamento: {0}")]
    Pairing(String),
    #[error("{0}")]
    Other(String),
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Other(e.to_string())
    }
}
impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Other(e.to_string())
    }
}
impl From<snow::Error> for AppError {
    fn from(e: snow::Error) -> Self {
        AppError::Crypto(e.to_string())
    }
}
impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self {
        AppError::Other(e.to_string())
    }
}

// Tauri commands serialize errors as plain strings for the UI.
impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
