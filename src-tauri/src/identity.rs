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

/// Nome das versões antigas no Android; trocado pelo do aparelho ao carregar.
const LEGACY_ANDROID_NAME: &str = "Android";

fn default_name() -> String {
    #[cfg(target_os = "android")]
    let base = android_device_name().unwrap_or_else(|| LEGACY_ANDROID_NAME.to_string());
    #[cfg(not(target_os = "android"))]
    let base = gethostname::gethostname().to_string_lossy().to_string();
    match std::env::var("CLIPSYNC_PROFILE") {
        Ok(p) if !p.trim().is_empty() => format!("{base}-{}", p.trim()),
        _ => base,
    }
}

/// Nome que o usuário deu ao aparelho (Configurações → Sobre o telefone); sem ele,
/// fabricante + modelo (ex.: "motorola moto g53 5G").
#[cfg(target_os = "android")]
fn android_device_name() -> Option<String> {
    use crate::android_jni::{jstring_to_string, with_class};
    use jni::objects::JValue;

    let user_set = with_class("android.provider.Settings$Global", |env, class, ctx| {
        let resolver = env
            .call_method(ctx, "getContentResolver", "()Landroid/content/ContentResolver;", &[])
            .and_then(|v| v.l())
            .map_err(|e| crate::error::AppError::Other(e.to_string()))?;
        let key = env.new_string("device_name").map_err(|e| crate::error::AppError::Other(e.to_string()))?;
        let out = env
            .call_static_method(
                class,
                "getString",
                "(Landroid/content/ContentResolver;Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&resolver), JValue::Object(&key)],
            )
            .and_then(|v| v.l())
            .map_err(|e| crate::error::AppError::Other(e.to_string()))?;
        if out.is_null() {
            return Ok(String::new());
        }
        jstring_to_string(env, out)
    });
    if let Ok(name) = user_set.map(|n| n.trim().to_string()) {
        if !name.is_empty() {
            return Some(name);
        }
    }

    let model = with_class("android.os.Build", |env, class, _ctx| {
        let mut field = |name: &str| -> Result<String> {
            let v = env
                .get_static_field(class, name, "Ljava/lang/String;")
                .and_then(|v| v.l())
                .map_err(|e| crate::error::AppError::Other(e.to_string()))?;
            if v.is_null() { Ok(String::new()) } else { jstring_to_string(env, v) }
        };
        let maker = field("MANUFACTURER")?;
        let model = field("MODEL")?;
        // Evita marca repetida quando o modelo já a traz.
        Ok(if model.to_lowercase().starts_with(&maker.to_lowercase()) { model } else { format!("{maker} {model}") })
    })
    .ok()?;
    let model = model.trim().to_string();
    (!model.is_empty()).then_some(model)
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
    let name = if cfg!(target_os = "android") && stored.name == LEGACY_ANDROID_NAME {
        default_name()
    } else {
        stored.name.clone()
    };
    Ok(LocalIdentity {
        device_id: device_id_from_public_key(&stored.public_key),
        name,
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
