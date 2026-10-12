//! Guarda a chave-de-encriptação-de-chaves (KEK) fora do banco e dos arquivos
//! de configuração, e cifra as chaves privadas em repouso com ChaCha20-Poly1305.
//!
//! Windows: KEK no Credential Manager (crate `keyring`).
//! Android: KEK gerada aleatoriamente e cifrada por uma chave AES-GCM não
//!          exportável do Android Keystore (`KeyGuard.kt`, via JNI/tao); só o blob
//!          cifrado toca o disco (diretório privado do app).
use crate::error::{AppError, Result};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};
use rand::RngCore;
use std::path::Path;

const KEK_LEN: usize = 32;
const NONCE_LEN: usize = 12;

pub struct Keystore {
    kek: [u8; KEK_LEN],
}

impl Keystore {
    pub fn open(app_dir: &Path) -> Result<Self> {
        let kek = load_or_create_kek(app_dir)?;
        Ok(Self { kek })
    }

    pub fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.kek));
        let mut nonce = [0u8; NONCE_LEN];
        rand::thread_rng().fill_bytes(&mut nonce);
        let ct = cipher
            .encrypt(Nonce::from_slice(&nonce), plaintext)
            .map_err(|e| AppError::Crypto(e.to_string()))?;
        let mut out = nonce.to_vec();
        out.extend_from_slice(&ct);
        Ok(out)
    }

    pub fn open_sealed(&self, sealed: &[u8]) -> Result<Vec<u8>> {
        if sealed.len() < NONCE_LEN {
            return Err(AppError::Crypto("blob de identidade inválido".into()));
        }
        let (nonce, ct) = sealed.split_at(NONCE_LEN);
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.kek));
        cipher
            .decrypt(Nonce::from_slice(nonce), ct)
            .map_err(|_| AppError::Crypto("não foi possível decifrar a identidade".into()))
    }
}

fn random_kek() -> [u8; KEK_LEN] {
    let mut k = [0u8; KEK_LEN];
    rand::thread_rng().fill_bytes(&mut k);
    k
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn load_or_create_kek(_app_dir: &Path) -> Result<[u8; KEK_LEN]> {
    let entry = keyring::Entry::new("app.clipsync", "identity-kek")
        .map_err(|e| AppError::Crypto(e.to_string()))?;
    match entry.get_password() {
        Ok(hexk) => {
            let bytes = hex::decode(hexk).map_err(|e| AppError::Crypto(e.to_string()))?;
            let mut k = [0u8; KEK_LEN];
            if bytes.len() != KEK_LEN {
                return Err(AppError::Crypto("KEK com tamanho inválido".into()));
            }
            k.copy_from_slice(&bytes);
            Ok(k)
        }
        Err(keyring::Error::NoEntry) => {
            let k = random_kek();
            entry
                .set_password(&hex::encode(k))
                .map_err(|e| AppError::Crypto(e.to_string()))?;
            Ok(k)
        }
        Err(e) => Err(AppError::Crypto(e.to_string())),
    }
}

#[cfg(target_os = "android")]
fn load_or_create_kek(app_dir: &Path) -> Result<[u8; KEK_LEN]> {
    let path = app_dir.join("kek.sealed");
    if let Ok(sealed) = std::fs::read(&path) {
        let plain = android_keyguard::open(&sealed)?;
        if plain.len() == KEK_LEN {
            let mut k = [0u8; KEK_LEN];
            k.copy_from_slice(&plain);
            return Ok(k);
        }
        return Err(AppError::Crypto("KEK cifrada com tamanho inválido".into()));
    }
    let k = random_kek();
    let sealed = android_keyguard::seal(&k)?;
    std::fs::create_dir_all(app_dir)?;
    std::fs::write(&path, sealed)?;
    Ok(k)
}

#[cfg(target_os = "ios")]
fn load_or_create_kek(app_dir: &Path) -> Result<[u8; KEK_LEN]> {
    // iOS não é alvo: fallback em arquivo no sandbox do app.
    let path = app_dir.join("kek.bin");
    if let Ok(bytes) = std::fs::read(&path) {
        if bytes.len() == KEK_LEN {
            let mut k = [0u8; KEK_LEN];
            k.copy_from_slice(&bytes);
            return Ok(k);
        }
    }
    let k = random_kek();
    std::fs::create_dir_all(app_dir)?;
    std::fs::write(&path, k)?;
    Ok(k)
}

#[cfg(target_os = "android")]
mod android_keyguard {
    use crate::android_jni::with_class;
    use crate::error::{AppError, Result};
    use jni::objects::{JByteArray, JValue};

    const CLASS: &str = "app.clipsync.desktop.KeyGuard";

    fn call(method: &str, input: &[u8]) -> Result<Vec<u8>> {
        with_class(CLASS, |env, class, _ctx| {
            let arg = env.byte_array_from_slice(input).map_err(|e| AppError::Crypto(e.to_string()))?;
            let out = env
                .call_static_method(class, method, "([B)[B", &[JValue::Object(&arg)])
                .and_then(|v| v.l())
                .map_err(|e| AppError::Crypto(format!("KeyGuard.{method}: {e}")))?;
            let arr = JByteArray::from(out);
            env.convert_byte_array(&arr).map_err(|e| AppError::Crypto(e.to_string()))
        })
        .map_err(|e| match e {
            AppError::Crypto(_) => e,
            other => AppError::Crypto(other.to_string()),
        })
    }

    pub fn seal(plain: &[u8]) -> Result<Vec<u8>> {
        call("seal", plain)
    }
    pub fn open(sealed: &[u8]) -> Result<Vec<u8>> {
        call("open", sealed)
    }
}
