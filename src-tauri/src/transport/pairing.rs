//! Pareamento: vínculo explícito entre dois dispositivos por QR Code (ou código
//! digitado). Antes do Noise os dois lados rodam um PAKE (SPAKE2) com o segredo
//! do pareamento; a chave que sai dele vira a PSK do handshake Noise XXpsk3.
//!
//! Por que PAKE: o código digitado tem ~30 bits. Usado direto como PSK, quem se
//! passasse pelo PC (mDNS falso) receberia a 3ª mensagem do Noise e testaria
//! todos os códigos offline em segundos. Com SPAKE2 cada conexão só testa um
//! palpite, e o lado que exibe o código encerra a sessão após
//! [`MAX_PAIR_FAILURES`] falhas. O QR nem usa o código: leva um segredo
//! aleatório de 256 bits.
use crate::error::{AppError, Result};
use crate::models::now_ms;
use rand::{Rng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use spake2::{Ed25519Group, Identity, Password, Spake2};
use std::net::IpAddr;

pub const PAIRING_TTL_MS: i64 = 3 * 60 * 1000;
/// Handshakes de pareamento que falham antes de a sessão ser encerrada.
pub const MAX_PAIR_FAILURES: u32 = 5;
/// Versão do conteúdo do QR. v2 = segredo aleatório em vez do código.
pub const PAYLOAD_VERSION: u32 = 2;
const ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";

/// Qual segredo da sessão o iniciador usa. Vai no `PreHello`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairAuth {
    /// Segredo de 256 bits lido do QR.
    Qr,
    /// Código de 6 caracteres digitado.
    Code,
}

impl PairAuth {
    pub fn as_str(self) -> &'static str {
        match self {
            PairAuth::Qr => "qr",
            PairAuth::Code => "code",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "qr" => Some(PairAuth::Qr),
            "code" => Some(PairAuth::Code),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PairingSession {
    pub id: String,
    pub code: String,
    /// Hex de 32 bytes aleatórios; só aparece no QR.
    pub secret: String,
    pub started_at: i64,
    pub expires_at: i64,
    pub failures: u32,
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
    pub secret: String,
}

/// Máximo de endereços no QR: cada IPv6 pesa ~40 bytes no JSON e o QR precisa continuar legível.
pub const QR_MAX_ADDRS: usize = 4;

/// Endereços que vão no QR: IPv4 primeiro (peer antigo só fala v4), depois IPv6, no máximo [`QR_MAX_ADDRS`].
pub fn qr_addrs(addrs: &[IpAddr]) -> Vec<IpAddr> {
    let (v4, v6): (Vec<IpAddr>, Vec<IpAddr>) = addrs.iter().copied().partition(|a| a.is_ipv4());
    v4.into_iter().chain(v6).take(QR_MAX_ADDRS).collect()
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

impl PairingSession {
    pub fn new() -> Self {
        let mut secret = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut secret);
        let now = now_ms();
        Self {
            id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
            code: generate_code(),
            secret: hex::encode(secret),
            started_at: now,
            expires_at: now + PAIRING_TTL_MS,
            failures: 0,
        }
    }

    pub fn is_expired(&self) -> bool {
        now_ms() > self.expires_at
    }

    /// Senha do PAKE para o modo escolhido pelo iniciador.
    pub fn password(&self, auth: PairAuth) -> Vec<u8> {
        match auth {
            PairAuth::Qr => self.secret.as_bytes().to_vec(),
            PairAuth::Code => self.code.as_bytes().to_vec(),
        }
    }
}

/// Senha do PAKE do lado do iniciador (o mesmo formato de [`PairingSession::password`]).
pub fn password_for(auth: PairAuth, secret_or_code: &str) -> Vec<u8> {
    match auth {
        PairAuth::Qr => secret_or_code.trim().to_ascii_lowercase().into_bytes(),
        PairAuth::Code => normalize_code(secret_or_code).into_bytes(),
    }
}

/// SPAKE2 simétrico: os dois lados mandam uma mensagem e derivam a mesma chave
/// só se usaram a mesma senha. O modo entra na identidade, então um segredo de
/// QR nunca casa com um código.
pub struct Pake(Spake2<Ed25519Group>);

impl Pake {
    pub fn start(auth: PairAuth, password: &[u8]) -> (Self, Vec<u8>) {
        let id = format!("clipsync-pair-v2|{}", auth.as_str());
        let (state, msg) = Spake2::<Ed25519Group>::start_symmetric(&Password::new(password), &Identity::new(id.as_bytes()));
        (Self(state), msg)
    }

    /// PSK do Noise derivada da chave do PAKE.
    pub fn finish(self, inbound: &[u8]) -> Result<[u8; 32]> {
        let key = self.0.finish(inbound).map_err(|_| AppError::Crypto("mensagem de pareamento inválida".into()))?;
        let mut h = Sha256::new();
        h.update(b"clipsync-pair-psk-v2|");
        h.update(&key);
        Ok(h.finalize().into())
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
    #[derive(Deserialize)]
    struct Version {
        v: u32,
    }
    let v = serde_json::from_str::<Version>(raw.trim()).map_err(|_| AppError::Pairing(crate::i18n::tr("err.qr_not_clipsync").into()))?.v;
    if v != PAYLOAD_VERSION {
        return Err(AppError::Pairing(crate::i18n::tr("err.qr_other_version").into()));
    }
    let p: PairingPayload =
        serde_json::from_str(raw.trim()).map_err(|_| AppError::Pairing(crate::i18n::tr("err.qr_not_clipsync").into()))?;
    if p.secret.len() != 64 || hex::decode(&p.secret).is_err() {
        return Err(AppError::Pairing(crate::i18n::tr("err.qr_invalid").into()));
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(a: (PairAuth, &[u8]), b: (PairAuth, &[u8])) -> (Result<[u8; 32]>, Result<[u8; 32]>) {
        let (pa, ma) = Pake::start(a.0, a.1);
        let (pb, mb) = Pake::start(b.0, b.1);
        (pa.finish(&mb), pb.finish(&ma))
    }

    #[test]
    fn pake_same_password_same_psk() {
        let s = PairingSession::new();
        let (a, b) = run((PairAuth::Code, &s.password(PairAuth::Code)), (PairAuth::Code, &password_for(PairAuth::Code, &s.code.to_lowercase().replace('-', ""))));
        assert_eq!(a.unwrap(), b.unwrap());
        let (a, b) = run((PairAuth::Qr, &s.password(PairAuth::Qr)), (PairAuth::Qr, &password_for(PairAuth::Qr, &s.secret)));
        assert_eq!(a.unwrap(), b.unwrap());
    }

    #[test]
    fn pake_wrong_password_or_mode_differs() {
        let (a, b) = run((PairAuth::Code, b"ABC-DEF"), (PairAuth::Code, b"ABC-DEG"));
        assert_ne!(a.unwrap(), b.unwrap());
        let (a, b) = run((PairAuth::Qr, b"ABC-DEF"), (PairAuth::Code, b"ABC-DEF"));
        assert_ne!(a.unwrap(), b.unwrap());
    }

    #[test]
    fn qr_limits_addrs_v4_first_and_stays_encodable() {
        let many: Vec<IpAddr> = ["2001:db8:aaaa:bbbb:cccc:dddd:eeee:1", "fd12:3456:789a:1::2", "192.168.0.5", "10.0.0.2", "172.16.0.9", "2001:db8::7"]
            .iter()
            .map(|s| s.parse().unwrap())
            .collect();
        let q = qr_addrs(&many);
        assert_eq!(q.len(), QR_MAX_ADDRS);
        assert!(q[..3].iter().all(|a| a.is_ipv4()) && q[3].is_ipv6());
        let s = PairingSession::new();
        let p = PairingPayload {
            v: PAYLOAD_VERSION,
            pairing_id: s.id.clone(),
            device_id: uuid::Uuid::new_v4().to_string(),
            name: "Computador do escritório".into(),
            platform: "windows".into(),
            public_key: "ab".repeat(32),
            addrs: q,
            port: 47820,
            secret: s.secret.clone(),
        };
        assert!(qr_svg(&p).is_ok());
    }

    #[test]
    fn payload_requires_v2_and_secret() {
        let s = PairingSession::new();
        let p = PairingPayload {
            v: PAYLOAD_VERSION,
            pairing_id: s.id.clone(),
            device_id: "d".into(),
            name: "n".into(),
            platform: "windows".into(),
            public_key: "aa".into(),
            addrs: vec![],
            port: 47820,
            secret: s.secret.clone(),
        };
        let raw = serde_json::to_string(&p).unwrap();
        assert_eq!(parse_payload(&raw).unwrap().secret, s.secret);
        let v1 = raw.replace("\"v\":2", "\"v\":1");
        assert!(parse_payload(&v1).is_err());
        let short = raw.replace(&s.secret, "abcd");
        assert!(parse_payload(&short).is_err());
    }
}
