//! Identidade criptográfica estável de cada instalação.
//!
//! O `device_id` deriva da chave pública estática (X25519, usada no Noise);
//! o nome é só metadado editável pelo usuário.
use crate::error::Result;
use crate::keystore::Keystore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use zeroize::Zeroize;

pub const NOISE_PATTERN: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2s";
pub const NOISE_PAIRING_PATTERN: &str = "Noise_XXpsk3_25519_ChaChaPoly_BLAKE2s";

#[derive(Serialize, Deserialize)]
struct StoredIdentity {
    private_key: Vec<u8>,
    public_key: Vec<u8>,
    name: String,
}

impl Drop for StoredIdentity {
    fn drop(&mut self) {
        self.private_key.zeroize();
    }
}

#[derive(Clone)]
pub struct LocalIdentity {
    pub device_id: String,
    pub name: String,
    pub platform: String,
    pub public_key: Vec<u8>,
    private_key: Vec<u8>,
}

impl Drop for LocalIdentity {
    fn drop(&mut self) {
        self.private_key.zeroize();
    }
}

impl LocalIdentity {
    pub fn private_key(&self) -> &[u8] {
        &self.private_key
    }

    pub fn public_key_hex(&self) -> String {
        hex::encode(&self.public_key)
    }

    /// Resumo público (enviado a outros dispositivos).
    pub fn summary(&self) -> DeviceIdentity {
        DeviceIdentity {
            device_id: self.device_id.clone(),
            name: self.name.clone(),
            platform: self.platform.clone(),
            public_key: self.public_key_hex(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceIdentity {
    pub device_id: String,
    pub name: String,
    pub platform: String,
    pub public_key: String,
}

pub fn device_id_from_public_key(pk: &[u8]) -> String {
    let h = Sha256::digest(pk);
    hex::encode(&h[..16])
}

pub fn current_platform() -> String {
    if cfg!(target_os = "android") {
        "android".into()
    } else if cfg!(target_os = "windows") {
        "windows".into()
    } else if cfg!(target_os = "macos") {
        "macos".into()
    } else if cfg!(target_os = "ios") {
        "ios".into()
    } else {
        "linux".into()
    }
}

fn default_name() -> String {
    let base = if cfg!(target_os = "android") {
        "Android".to_string()
    } else {
        gethostname::gethostname().to_string_lossy().to_string()
    };
    match std::env::var("CLIPSYNC_PROFILE") {
        Ok(p) if !p.trim().is_empty() => format!("{base}-{}", p.trim()),
        _ => base,
    }
}

pub fn load_or_create(app_dir: &Path, ks: &Keystore) -> Result<LocalIdentity> {
    let path = app_dir.join("identity.bin");
    let stored: StoredIdentity = match std::fs::read(&path) {
        Ok(sealed) => {
            let plain = ks.open_sealed(&sealed)?;
            serde_json::from_slice(&plain)?
        }
        Err(_) => {
            let kp = snow::Builder::new(NOISE_PATTERN.parse().unwrap()).generate_keypair()?;
            let s = StoredIdentity {
                private_key: kp.private,
                public_key: kp.public,
                name: default_name(),
            };
            std::fs::create_dir_all(app_dir)?;
            let plain = serde_json::to_vec(&s)?;
            std::fs::write(&path, ks.seal(&plain)?)?;
            s
        }
    };
    Ok(LocalIdentity {
        device_id: device_id_from_public_key(&stored.public_key),
        name: stored.name.clone(),
        platform: current_platform(),
        public_key: stored.public_key.clone(),
        private_key: stored.private_key.clone(),
    })
}

pub fn rename(app_dir: &Path, ks: &Keystore, id: &LocalIdentity, name: &str) -> Result<()> {
    let s = StoredIdentity {
        private_key: id.private_key.clone(),
        public_key: id.public_key.clone(),
        name: name.to_string(),
    };
    let plain = serde_json::to_vec(&s)?;
    std::fs::write(app_dir.join("identity.bin"), ks.seal(&plain)?)?;
    Ok(())
}
