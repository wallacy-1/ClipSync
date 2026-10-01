//! Handshake Noise XX (ou XXpsk3 no pareamento) usando a crate `snow`.
//! Nenhum primitivo criptográfico é implementado aqui.
use crate::error::{AppError, Result};
use crate::identity::{NOISE_PAIRING_PATTERN, NOISE_PATTERN};
use crate::transport::protocol::{read_frame, write_frame, MAX_FRAME};
use tokio::net::TcpStream;

pub struct HandshakeResult {
    pub transport: snow::TransportState,
    pub remote_static: Vec<u8>,
}

fn builder<'a>(private_key: &'a [u8], psk: Option<&'a [u8; 32]>) -> Result<snow::Builder<'a>> {
    let pattern = if psk.is_some() { NOISE_PAIRING_PATTERN } else { NOISE_PATTERN };
    let params = pattern.parse().map_err(|_| AppError::Crypto("padrão Noise inválido".into()))?;
    let mut b = snow::Builder::new(params).local_private_key(private_key);
    if let Some(p) = psk {
        b = b.psk(3, p);
    }
    Ok(b)
}

pub async fn handshake_initiator(
    stream: &mut TcpStream,
    private_key: &[u8],
    psk: Option<&[u8; 32]>,
) -> Result<HandshakeResult> {
    let mut hs = builder(private_key, psk)?.build_initiator()?;
    let mut buf = vec![0u8; MAX_FRAME];

    // -> e
    let n = hs.write_message(&[], &mut buf)?;
    write_frame(stream, &buf[..n]).await?;
    // <- e, ee, s, es
    let m = read_frame(stream).await?;
    hs.read_message(&m, &mut buf)?;
    // -> s, se [, psk]
    let n = hs.write_message(&[], &mut buf)?;
    write_frame(stream, &buf[..n]).await?;

    finish(hs)
}

pub async fn handshake_responder(
    stream: &mut TcpStream,
    private_key: &[u8],
    psk: Option<&[u8; 32]>,
) -> Result<HandshakeResult> {
    let mut hs = builder(private_key, psk)?.build_responder()?;
    let mut buf = vec![0u8; MAX_FRAME];

    let m = read_frame(stream).await?;
    hs.read_message(&m, &mut buf)?;
    let n = hs.write_message(&[], &mut buf)?;
    write_frame(stream, &buf[..n]).await?;
    let m = read_frame(stream).await?;
    hs.read_message(&m, &mut buf)?;

    finish(hs)
}

fn finish(hs: snow::HandshakeState) -> Result<HandshakeResult> {
    let remote_static = hs
        .get_remote_static()
        .ok_or_else(|| AppError::Crypto("peer sem chave estática".into()))?
        .to_vec();
    let transport = hs.into_transport_mode()?;
    Ok(HandshakeResult { transport, remote_static })
}
