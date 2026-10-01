//! Formato de quadros na rede. Tudo depois do `PreHello` é cifrado pelo Noise.
//!
//! Quadro: `[u32 BE tamanho][bytes]`. Dentro do plaintext de cada quadro, o
//! primeiro byte é a **tag**:
//! - `0` / `1`: fatia de uma [`Message`] JSON (última / continua) — o clipboard
//!   usa só isto e o formato é idêntico ao da versão 1;
//! - `2`: [`DataFrame`] binário (dados de arquivo), nunca JSON.
//!
//! Mensagens JSON maiores que o limite do Noise (64 KiB) são fatiadas em `CHUNK`.
use crate::error::{AppError, Result};
use crate::identity::DeviceIdentity;
use crate::models::ClipboardItem;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// v2 = v1 + quadros binários (tag 2) + `caps` no `Hello` + mensagens `File*`.
pub const PROTOCOL_VERSION: u32 = 2;
pub const MAX_FRAME: usize = 65535;
pub const CHUNK: usize = 60_000;
pub const MAX_MESSAGE: usize = 4 * 1024 * 1024;

pub const TAG_JSON_LAST: u8 = 0;
pub const TAG_JSON_MORE: u8 = 1;
pub const TAG_DATA: u8 = 2;

pub const CAP_FILE_V1: &str = "file-v1";

/// Primeira mensagem, em texto puro (não carrega dados do usuário): só diz
/// qual handshake usar.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreHello {
    pub v: u32,
    /// "session" | "pair"
    pub mode: String,
    pub pairing_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileOffer {
    pub transfer_id: String,
    /// Nome como está na origem. O receptor **nunca** o usa como caminho sem sanitizar.
    pub name: String,
    pub mime: Option<String>,
    pub size: u64,
    pub chunk_size: u32,
    pub total_chunks: u32,
    /// Conhecido antes de enviar quando a origem é barata/seekável; `None` quando
    /// só será conhecido no `FileDone` (fonte não-seekável, ex.: alguns providers SAF).
    pub sha256: Option<String>,
    pub mtime: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileRejectReason {
    Declined,
    Unknown,
    TooLarge,
    NoSpace,
    Busy,
    Disabled,
    Protocol,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileCancelReason {
    User,
    Revoked,
    SourceUnavailable,
    Protocol,
    Expired,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "camelCase")]
pub enum Message {
    Hello {
        identity: DeviceIdentity,
        /// Ausente em peers v1: `serde(default)` mantém compatibilidade de formato.
        #[serde(default)]
        caps: Vec<String>,
    },
    Clip { item: ClipboardItem },
    Ack { id: String },
    Ping,
    Pong,
    Bye,

    // Só para peers que anunciaram CAP_FILE_V1.
    FileOffer(FileOffer),
    /// `offset > 0` = retomada: o receptor é a autoridade do que já tem durável.
    FileAccept { transfer_id: String, offset: u64 },
    FileReject { transfer_id: String, reason: FileRejectReason },
    /// Cumulativo: bytes contíguos já gravados e sincronizados em disco.
    FileAck { transfer_id: String, received_through: u64 },
    /// Emissor terminou; `sha256` do arquivo inteiro, calculado em streaming.
    FileDone { transfer_id: String, sha256: String },
    /// Receptor verificou o hash e finalizou (ou falhou).
    FileResult { transfer_id: String, ok: bool, error: Option<String> },
    FileCancel { transfer_id: String, reason: FileCancelReason },
}

/// Cabeçalho: `[TAG_DATA][transfer_id 16][chunk_index u32][frag_index u16][frag_count u16]`.
pub const DATA_HEADER: usize = 1 + 16 + 4 + 2 + 2;
#[allow(dead_code)]
pub const DATA_MAX: usize = CHUNK - DATA_HEADER;

/// Fragmento de um chunk lógico de arquivo. Um chunk de 256 KiB vira
/// `frag_count` quadros consecutivos com o mesmo `chunk_index`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataFrame {
    pub transfer_id: [u8; 16],
    pub chunk_index: u32,
    pub frag_index: u16,
    pub frag_count: u16,
    pub data: Vec<u8>,
}

impl DataFrame {
    #[allow(dead_code)]
    pub fn encode(&self) -> Result<Vec<u8>> {
        if self.data.len() > DATA_MAX {
            return Err(AppError::Net("fragmento de dados maior que o permitido".into()));
        }
        if self.frag_count == 0 || self.frag_index >= self.frag_count {
            return Err(AppError::Net("índice de fragmento inválido".into()));
        }
        let mut out = Vec::with_capacity(DATA_HEADER + self.data.len());
        out.push(TAG_DATA);
        out.extend_from_slice(&self.transfer_id);
        out.extend_from_slice(&self.chunk_index.to_be_bytes());
        out.extend_from_slice(&self.frag_index.to_be_bytes());
        out.extend_from_slice(&self.frag_count.to_be_bytes());
        out.extend_from_slice(&self.data);
        Ok(out)
    }

    /// `plain` inclui a tag.
    pub fn decode(plain: &[u8]) -> Result<Self> {
        if plain.len() < DATA_HEADER || plain[0] != TAG_DATA {
            return Err(AppError::Net("quadro de dados malformado".into()));
        }
        let mut transfer_id = [0u8; 16];
        transfer_id.copy_from_slice(&plain[1..17]);
        let chunk_index = u32::from_be_bytes(plain[17..21].try_into().unwrap());
        let frag_index = u16::from_be_bytes(plain[21..23].try_into().unwrap());
        let frag_count = u16::from_be_bytes(plain[23..25].try_into().unwrap());
        if frag_count == 0 || frag_index >= frag_count {
            return Err(AppError::Net("índice de fragmento inválido".into()));
        }
        Ok(Self { transfer_id, chunk_index, frag_index, frag_count, data: plain[DATA_HEADER..].to_vec() })
    }
}

#[derive(Debug)]
pub enum Inbound {
    Msg(Message),
    Data(DataFrame),
}

/// `transfer_id` textual (UUID) ↔ 16 bytes do cabeçalho binário.
#[allow(dead_code)]
pub fn transfer_id_bytes(id: &str) -> Option<[u8; 16]> {
    uuid::Uuid::parse_str(id).ok().map(|u| *u.as_bytes())
}
pub fn transfer_id_string(b: &[u8; 16]) -> String {
    uuid::Uuid::from_bytes(*b).to_string()
}

pub async fn write_frame(stream: &mut TcpStream, bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_FRAME {
        return Err(AppError::Net("quadro maior que o permitido".into()));
    }
    stream.write_u32(bytes.len() as u32).await?;
    stream.write_all(bytes).await?;
    stream.flush().await?;
    Ok(())
}

pub async fn read_frame(stream: &mut TcpStream) -> Result<Vec<u8>> {
    let len = stream.read_u32().await? as usize;
    if len > MAX_FRAME {
        return Err(AppError::Net("quadro maior que o permitido".into()));
    }
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;
    Ok(buf)
}

pub struct SecureChannel {
    pub stream: TcpStream,
    pub noise: snow::TransportState,
    buf: Vec<u8>,
    // Estado de leitura resumível: `recv_any` pode ser cancelada no meio de um
    // quadro (é um braço de `select!`); nada do que já foi lido pode se perder.
    rd_hdr: [u8; 4],
    rd_hdr_len: usize,
    rd_body: Vec<u8>,
    rd_body_len: usize,
    rd_acc: Vec<u8>,
}

impl SecureChannel {
    pub fn new(stream: TcpStream, noise: snow::TransportState) -> Self {
        Self {
            stream,
            noise,
            buf: vec![0u8; MAX_FRAME],
            rd_hdr: [0; 4],
            rd_hdr_len: 0,
            rd_body: Vec::new(),
            rd_body_len: 0,
            rd_acc: Vec::new(),
        }
    }

    async fn send_plain(&mut self, payload: &[u8]) -> Result<()> {
        let len = self.noise.write_message(payload, &mut self.buf)?;
        write_frame(&mut self.stream, &self.buf[..len]).await
    }

    /// Mensagem JSON, fatiada em quadros tag 0/1 (formato idêntico ao da v1).
    pub async fn send(&mut self, msg: &Message) -> Result<()> {
        let plain = serde_json::to_vec(msg)?;
        let chunks: Vec<&[u8]> = if plain.is_empty() { vec![&[][..]] } else { plain.chunks(CHUNK).collect() };
        let n = chunks.len();
        for (i, chunk) in chunks.into_iter().enumerate() {
            let mut payload = Vec::with_capacity(chunk.len() + 1);
            payload.push(if i + 1 < n { TAG_JSON_MORE } else { TAG_JSON_LAST });
            payload.extend_from_slice(chunk);
            self.send_plain(&payload).await?;
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn send_data(&mut self, frame: &DataFrame) -> Result<()> {
        let payload = frame.encode()?;
        self.send_plain(&payload).await
    }

    /// Lê um quadro cifrado de forma **cancel-safe**: cada `read` guarda o que
    /// veio no estado da struct, então uma cancelação entre polls não perde bytes.
    async fn read_frame_resumable(&mut self) -> Result<Vec<u8>> {
        while self.rd_hdr_len < 4 {
            let n = self.stream.read(&mut self.rd_hdr[self.rd_hdr_len..]).await?;
            if n == 0 {
                return Err(AppError::Net("conexão fechada".into()));
            }
            self.rd_hdr_len += n;
        }
        if self.rd_body.is_empty() {
            let len = u32::from_be_bytes(self.rd_hdr) as usize;
            if len > MAX_FRAME || len == 0 {
                return Err(AppError::Net("quadro maior que o permitido".into()));
            }
            self.rd_body = vec![0u8; len];
            self.rd_body_len = 0;
        }
        while self.rd_body_len < self.rd_body.len() {
            let n = self.stream.read(&mut self.rd_body[self.rd_body_len..]).await?;
            if n == 0 {
                return Err(AppError::Net("conexão fechada no meio de um quadro".into()));
            }
            self.rd_body_len += n;
        }
        self.rd_hdr_len = 0;
        self.rd_body_len = 0;
        Ok(std::mem::take(&mut self.rd_body))
    }

    /// Próxima mensagem **ou** quadro de dados. Cancel-safe (pode ser usada em `select!`).
    pub async fn recv_any(&mut self) -> Result<Inbound> {
        loop {
            let ct = self.read_frame_resumable().await?;
            let len = self.noise.read_message(&ct, &mut self.buf)?;
            if len == 0 {
                return Err(AppError::Net("quadro vazio".into()));
            }
            match self.buf[0] {
                TAG_DATA => {
                    if !self.rd_acc.is_empty() {
                        return Err(AppError::Net("quadro de dados no meio de uma mensagem".into()));
                    }
                    return Ok(Inbound::Data(DataFrame::decode(&self.buf[..len])?));
                }
                TAG_JSON_LAST | TAG_JSON_MORE => {
                    let more = self.buf[0] == TAG_JSON_MORE;
                    self.rd_acc.extend_from_slice(&self.buf[1..len]);
                    if self.rd_acc.len() > MAX_MESSAGE {
                        return Err(AppError::Net("mensagem grande demais".into()));
                    }
                    if !more {
                        let acc = std::mem::take(&mut self.rd_acc);
                        return Ok(Inbound::Msg(serde_json::from_slice(&acc)?));
                    }
                }
                _ => return Err(AppError::Net("tag de quadro desconhecida".into())),
            }
        }
    }

    /// Próxima mensagem JSON; um quadro de dados aqui é erro de protocolo
    /// (usado no handshake de aplicação, antes de qualquer transferência).
    pub async fn recv(&mut self) -> Result<Message> {
        match self.recv_any().await? {
            Inbound::Msg(m) => Ok(m),
            Inbound::Data(_) => Err(AppError::Net("quadro de dados inesperado".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer() -> FileOffer {
        FileOffer {
            transfer_id: "0c4f3a1e-5d2b-4e9a-8f6c-1a2b3c4d5e6f".into(),
            name: "relatório ✓.pdf".into(),
            mime: Some("application/pdf".into()),
            size: 1_000_000,
            chunk_size: 262_144,
            total_chunks: 4,
            sha256: None,
            mtime: Some(1_700_000_000_000),
        }
    }

    #[test]
    fn hello_without_caps_still_parses() {
        // Exatamente o que um peer v1 manda.
        let v1 = r#"{"t":"hello","identity":{"deviceId":"d","name":"n","platform":"windows","publicKey":"aa"}}"#;
        let m: Message = serde_json::from_str(v1).unwrap();
        match m {
            Message::Hello { caps, .. } => assert!(caps.is_empty()),
            _ => panic!(),
        }
    }

    #[test]
    fn clip_and_ack_wire_format_unchanged() {
        let a = serde_json::to_string(&Message::Ack { id: "x".into() }).unwrap();
        assert_eq!(a, r#"{"t":"ack","id":"x"}"#);
        let p = serde_json::to_string(&Message::Ping).unwrap();
        assert_eq!(p, r#"{"t":"ping"}"#);
    }

    #[test]
    fn file_messages_round_trip() {
        let msgs = vec![
            Message::FileOffer(offer()),
            Message::FileAccept { transfer_id: "t".into(), offset: 300 << 20 },
            Message::FileReject { transfer_id: "t".into(), reason: FileRejectReason::NoSpace },
            Message::FileAck { transfer_id: "t".into(), received_through: 1 << 20 },
            Message::FileDone { transfer_id: "t".into(), sha256: "ab".repeat(32) },
            Message::FileResult { transfer_id: "t".into(), ok: false, error: Some("checksum".into()) },
            Message::FileCancel { transfer_id: "t".into(), reason: FileCancelReason::Revoked },
        ];
        for m in msgs {
            let s = serde_json::to_string(&m).unwrap();
            let back: Message = serde_json::from_str(&s).unwrap();
            assert_eq!(serde_json::to_string(&back).unwrap(), s);
        }
        let s = serde_json::to_string(&Message::FileReject { transfer_id: "t".into(), reason: FileRejectReason::TooLarge }).unwrap();
        assert!(s.contains(r#""reason":"tooLarge""#));
    }

    #[test]
    fn data_frame_round_trip_and_limits() {
        let id = transfer_id_bytes("0c4f3a1e-5d2b-4e9a-8f6c-1a2b3c4d5e6f").unwrap();
        let f = DataFrame { transfer_id: id, chunk_index: 7, frag_index: 2, frag_count: 5, data: vec![9u8; DATA_MAX] };
        let enc = f.encode().unwrap();
        assert_eq!(enc[0], TAG_DATA);
        assert_eq!(enc.len(), CHUNK);
        assert_eq!(DataFrame::decode(&enc).unwrap(), f);
        assert_eq!(transfer_id_string(&id), "0c4f3a1e-5d2b-4e9a-8f6c-1a2b3c4d5e6f");

        let too_big = DataFrame { data: vec![0; DATA_MAX + 1], ..f.clone() };
        assert!(too_big.encode().is_err());
        let bad_idx = DataFrame { frag_index: 5, ..f.clone() };
        assert!(bad_idx.encode().is_err());
        assert!(DataFrame::decode(&enc[..DATA_HEADER - 1]).is_err());
        let mut wrong_tag = enc.clone();
        wrong_tag[0] = TAG_JSON_LAST;
        assert!(DataFrame::decode(&wrong_tag).is_err());
    }

    /// Sessão Noise real em loopback: JSON grande (fatiado), quadro binário e
    /// JSON pequeno passam pelo mesmo canal e chegam na ordem.
    #[tokio::test]
    async fn secure_channel_multiplexes_json_and_data() {
        use crate::transport::noise::{handshake_initiator, handshake_responder};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let k1 = snow::Builder::new(crate::identity::NOISE_PATTERN.parse().unwrap()).generate_keypair().unwrap();
        let k2 = snow::Builder::new(crate::identity::NOISE_PATTERN.parse().unwrap()).generate_keypair().unwrap();

        let server = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let hs = handshake_responder(&mut s, &k2.private, None).await.unwrap();
            let mut ch = SecureChannel::new(s, hs.transport);
            let mut got = Vec::new();
            for _ in 0..3 {
                got.push(ch.recv_any().await.unwrap());
            }
            got
        });

        let mut s = TcpStream::connect(addr).await.unwrap();
        let hs = handshake_initiator(&mut s, &k1.private, None).await.unwrap();
        let mut ch = SecureChannel::new(s, hs.transport);
        let big = Message::Clip {
            item: ClipboardItem {
                id: "i".into(),
                content: "x".repeat(150_000), // > 2 fatias
                content_type: crate::content::ContentType::Text,
                source_device_id: "d".into(),
                created_at: 0,
                expires_at: None,
                is_pinned: false,
                is_sensitive: false,
                sync_state: "local".into(),
                metadata: serde_json::json!({}),
            },
        };
        ch.send(&big).await.unwrap();
        let frame = DataFrame { transfer_id: [1u8; 16], chunk_index: 0, frag_index: 0, frag_count: 1, data: vec![7u8; 1000] };
        ch.send_data(&frame).await.unwrap();
        ch.send(&Message::FileAck { transfer_id: "t".into(), received_through: 1000 }).await.unwrap();

        let got = server.await.unwrap();
        assert!(matches!(&got[0], Inbound::Msg(Message::Clip { item }) if item.content.len() == 150_000));
        assert!(matches!(&got[1], Inbound::Data(f) if *f == frame));
        assert!(matches!(&got[2], Inbound::Msg(Message::FileAck { received_through: 1000, .. })));
    }

    /// `recv_any` é braço de `select!` no loop da conexão e pode ser cancelada no
    /// meio de um quadro. Aqui o receptor cancela a leitura o tempo todo (timeouts
    /// minúsculos) enquanto o emissor goteja o quadro byte a byte: nada se perde.
    #[tokio::test]
    async fn recv_any_is_cancel_safe() {
        use crate::transport::noise::{handshake_initiator, handshake_responder};
        use tokio::io::AsyncWriteExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let k1 = snow::Builder::new(crate::identity::NOISE_PATTERN.parse().unwrap()).generate_keypair().unwrap();
        let k2 = snow::Builder::new(crate::identity::NOISE_PATTERN.parse().unwrap()).generate_keypair().unwrap();

        let server = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let hs = handshake_responder(&mut s, &k2.private, None).await.unwrap();
            let mut ch = SecureChannel::new(s, hs.transport);
            let mut got = Vec::new();
            let mut cancels = 0u32;
            while got.len() < 2 {
                match tokio::time::timeout(std::time::Duration::from_micros(300), ch.recv_any()).await {
                    Ok(r) => got.push(r.unwrap()),
                    Err(_) => cancels += 1,
                }
            }
            (got, cancels)
        });

        let mut s = TcpStream::connect(addr).await.unwrap();
        let hs = handshake_initiator(&mut s, &k1.private, None).await.unwrap();
        let mut ch = SecureChannel::new(s, hs.transport);
        // Dois quadros escritos "a conta-gotas": cabeçalho byte a byte, corpo em pedaços.
        let mut buf = vec![0u8; MAX_FRAME];
        let frames = [
            { let mut p = vec![TAG_JSON_LAST]; p.extend_from_slice(&serde_json::to_vec(&Message::Ack { id: "slow".into() }).unwrap()); p },
            DataFrame { transfer_id: [9u8; 16], chunk_index: 3, frag_index: 0, frag_count: 1, data: vec![1u8; 5000] }.encode().unwrap(),
        ];
        for plain in frames {
            let n = ch.noise.write_message(&plain, &mut buf).unwrap();
            for b in (n as u32).to_be_bytes() {
                ch.stream.write_all(&[b]).await.unwrap();
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            }
            for piece in buf[..n].chunks(700) {
                ch.stream.write_all(piece).await.unwrap();
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        }
        let (got, cancels) = server.await.unwrap();
        assert!(cancels > 0, "o teste precisa ter cancelado leituras");
        assert!(matches!(&got[0], Inbound::Msg(Message::Ack { id }) if id == "slow"));
        assert!(matches!(&got[1], Inbound::Data(f) if f.chunk_index == 3 && f.data.len() == 5000));
    }
}
