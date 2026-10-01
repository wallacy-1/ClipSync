//! Pareamento: vínculo explícito entre dois dispositivos por QR Code (ou código
//! digitado). O código vira a PSK do handshake Noise XXpsk3 — quem não tem o
//! código não conclui o handshake, e ao final ambos guardam a chave pública
//! estática do outro como confiável.
use crate::error::{AppError, Result};
use crate::models::now_ms;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::net::IpAddr;

pub const PAIRING_TTL_MS: i64 = 3 * 60 * 1000;
const ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";

#[derive(Debug, Clone)]
pub struct PairingSession {
    pub id: String,
    pub code: String,
    pub psk: [u8; 32],
    pub started_at: i64,
    pub expires_at: i64,
}

/// Conteúdo do QR mostrado no PC. Inclui endereço para o celular conectar
/// mesmo quando o multicast mDNS não chega até ele.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingPayload {
    pub v: u32,
    pub pairing_id: String,
    pub device_id: String,
    pub name: String,
    pub platform: String,
    pub public_key: String,
    pub addrs: Vec<IpAddr>,
    pub port: u16,
    pub code: String,
}

pub fn generate_code() -> String {
    let mut rng = rand::thread_rng();
    let pick = |rng: &mut rand::rngs::ThreadRng| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char;
    let a: String = (0..3).map(|_| pick(&mut rng)).collect();
    let b: String = (0..3).map(|_| pick(&mut rng)).collect();
    format!("{a}-{b}")
}

pub fn normalize_code(code: &str) -> String {
    let raw: String = code
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if raw.len() == 6 {
        format!("{}-{}", &raw[..3], &raw[3..])
    } else {
        raw
    }
}

pub fn psk_from_code(code: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"clipsync-pairing-v1|");
    h.update(normalize_code(code).as_bytes());
    let out = h.finalize();
    let mut psk = [0u8; 32];
    psk.copy_from_slice(&out);
    psk
}

impl PairingSession {
    pub fn new() -> Self {
        let code = generate_code();
        let now = now_ms();
        Self {
            id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
            psk: psk_from_code(&code),
            code,
            started_at: now,
            expires_at: now + PAIRING_TTL_MS,
        }
    }

    pub fn is_expired(&self) -> bool {
        now_ms() > self.expires_at
    }
}

pub fn qr_svg(payload: &PairingPayload) -> Result<String> {
    let json = serde_json::to_string(payload)?;
    let code = qrcode::QrCode::new(json.as_bytes()).map_err(|e| AppError::Pairing(e.to_string()))?;
    Ok(code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(148, 148)
        .quiet_zone(false)
        .dark_color(qrcode::render::svg::Color("#201e1d"))
        .light_color(qrcode::render::svg::Color("transparent"))
        .build())
}

pub fn parse_payload(raw: &str) -> Result<PairingPayload> {
    let p: PairingPayload =
        serde_json::from_str(raw.trim()).map_err(|_| AppError::Pairing("QR Code não é do ClipSync".into()))?;
    if p.v != 1 {
        return Err(AppError::Pairing("versão de pareamento não suportada".into()));
    }
    Ok(p)
}
