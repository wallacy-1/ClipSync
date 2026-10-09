use crate::db::StoredDevice;
use crate::error::{AppError, Result};
use crate::events;
use crate::identity::{device_id_from_public_key, DeviceIdentity};
use crate::models::{now_ms, DeviceState};
use crate::state::{AppState, ConnHandle};
use crate::sync;
use crate::transport::noise::{handshake_initiator, handshake_responder, HandshakeResult};
use crate::transport::pairing::{Pake, PairAuth, MAX_PAIR_FAILURES};
use crate::transfer;
use crate::transport::protocol::{read_frame, write_frame, DataFrame, Inbound, Message, PreHello, SecureChannel, CAP_CLIP_HTML_V1, CAP_CLIP_IMAGE_V1, CAP_FILE_ASK_V1, CAP_FILE_V1, PROTOCOL_VERSION};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

#[derive(Clone, Copy)]
pub enum Role {
    Initiator,
    Responder,
}

pub enum Mode {
    /// Conexão normal entre dispositivos já pareados.
    Session,
    /// Pareamento: PAKE com o segredo do QR ou o código digitado; a chave vira a PSK do Noise.
    Pair { auth: PairAuth, password: Vec<u8> },
}

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
const PING_EVERY: Duration = Duration::from_secs(20);
const IDLE_TIMEOUT: Duration = Duration::from_secs(70);

fn local_identity(state: &AppState) -> DeviceIdentity {
    let me = state.identity.read().unwrap().summary();
    DeviceIdentity { name: state.display_name(), ..me }
}

/// Renomeado: reenvia o `Hello` para quem já está conectado (peers antigos ignoram).
pub fn announce_identity(app: &AppHandle) {
    let state = app.state::<AppState>();
    let hello = Message::Hello { identity: local_identity(&state), caps: local_caps() };
    let ids: Vec<String> = state.connections.lock().unwrap().keys().cloned().collect();
    for id in ids {
        state.send_to(&id, hello.clone());
    }
}

fn on_identity_update(app: &AppHandle, device_id: &str, identity: DeviceIdentity) {
    let state = app.state::<AppState>();
    let Ok(Some(d)) = state.db.get_device(device_id) else { return };
    if identity.device_id != device_id || identity.public_key != d.public_key || identity.name.trim().is_empty() {
        return;
    }
    if identity.name == d.name && identity.platform == d.platform {
        return;
    }
    log::info!("[DEVICE] Renamed id={device_id} name={:?}", identity.name);
    if state.db.upsert_device(&StoredDevice { name: identity.name, platform: identity.platform, ..d }).is_ok() {
        events::emit(app, events::DEVICE_UPDATED, state.devices());
    }
}

fn local_caps() -> Vec<String> {
    // Transferência de arquivos: Windows e Android; iOS não é alvo.
    #[cfg(not(target_os = "ios"))]
    {
        vec![CAP_FILE_V1.to_string(), CAP_CLIP_IMAGE_V1.to_string(), CAP_FILE_ASK_V1.to_string(), CAP_CLIP_HTML_V1.to_string()]
    }
    #[cfg(target_os = "ios")]
    {
        let _ = (CAP_FILE_V1, CAP_CLIP_IMAGE_V1, CAP_FILE_ASK_V1, CAP_CLIP_HTML_V1);
        Vec::new()
    }
}

static CONN_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Quadros de dados de arquivo que o loop aceita da task emissora antes de ela
/// bloquear (≈ 8 × 60 KB). A janela de 2 MiB fica no emissor, não aqui.
const DATA_QUEUE_FRAMES: usize = 8;

fn file_msg_kind(m: &Message) -> &'static str {
    match m {
        Message::FileOffer(_) => "offer",
        Message::FileAccept { .. } => "accept",
        Message::FileReject { .. } => "reject",
        Message::FileAck { .. } => "ack",
        Message::FileDone { .. } => "done",
        Message::FileResult { .. } => "result",
        Message::FileCancel { .. } => "cancel",
        _ => "other",
    }
}

/// Conexão de saída para um dispositivo confiável já conhecido.
pub async fn connect_session(app: AppHandle, device_id: String, addr: String) {
    let state = app.state::<AppState>();
    {
        let mut c = state.connecting.lock().unwrap();
        if !c.insert(device_id.clone()) {
            return;
        }
    }
    events::emit(&app, events::DEVICE_UPDATED, state.devices());
    log::info!("[TRANSPORT] Connecting to {device_id} at {addr}");
    let res = async {
        let stream = tokio::time::timeout(Duration::from_secs(6), TcpStream::connect(&addr))
            .await
            .map_err(|_| AppError::Net(crate::i18n::trf("err.net_timeout_connect", &[("addr", &addr.to_string())])))??;
        run(app.clone(), stream, Role::Initiator, Mode::Session).await
    }
    .await;
    state.connecting.lock().unwrap().remove(&device_id);
    if let Err(e) = res {
        log::info!("[TRANSPORT] Outbound to {device_id} at {addr} ended: {e}");
    }
    events::emit(&app, events::DEVICE_UPDATED, state.devices());
}

/// Conexão de saída para parear (lado que escaneou o QR / digitou o código).
pub async fn connect_pair(
    app: AppHandle,
    addr: String,
    pairing_id: Option<String>,
    auth: PairAuth,
    password: Vec<u8>,
) -> Result<DeviceIdentity> {
    let stream = tokio::time::timeout(Duration::from_secs(8), TcpStream::connect(&addr))
        .await
        .map_err(|_| AppError::Net(crate::i18n::trf("err.net_unreachable", &[("addr", &addr.to_string())])))??;
    let (tx, rx) = tokio::sync::oneshot::channel();
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut paired_tx = Some(tx);
        let r = run_with_pairing_id(app2, stream, Role::Initiator, Mode::Pair { auth, password }, pairing_id, &mut paired_tx).await;
        if let Err(e) = r {
            log::debug!("sessão de pareamento encerrada: {e}");
            // Se o pareamento não chegou a completar, devolve o erro real.
            if let Some(tx) = paired_tx.take() {
                let _ = tx.send(Err(e));
            }
        }
    });
    tokio::time::timeout(Duration::from_secs(25), rx)
        .await
        .map_err(|_| AppError::Pairing(crate::i18n::tr("err.pair_no_reply").into()))?
        .map_err(|_| AppError::Pairing(crate::i18n::tr("err.pair_cancelled").into()))?
}

/// Sessão normal: a chave do handshake tem de ser de um device pareado e não bloqueado.
/// Troca `Hello` e atualiza nome/plataforma se mudaram. Retorna o device e as caps do peer.
async fn authenticate_session(
    state: &AppState,
    chan: &mut SecureChannel,
    remote_pk_hex: &str,
    remote_id: &str,
    peer_addr: &str,
) -> Result<(StoredDevice, Vec<String>)> {
    let Some(d) = state.db.device_by_public_key(remote_pk_hex)? else {
        log::warn!("[AUTH] Rejected unpaired device id={remote_id} from {peer_addr}");
        let _ = chan.send(&Message::Bye).await;
        return Err(AppError::Net(crate::i18n::tr("err.device_not_paired").into()));
    };
    if d.blocked {
        log::warn!("[AUTH] Rejected blocked device id={remote_id}");
        let _ = chan.send(&Message::Bye).await;
        return Err(AppError::Net(crate::i18n::tr("err.device_blocked").into()));
    }
    chan.send(&Message::Hello { identity: local_identity(state), caps: local_caps() }).await?;
    let caps = match tokio::time::timeout(HANDSHAKE_TIMEOUT, chan.recv()).await {
        Ok(Ok(Message::Hello { identity, caps })) => {
            if identity.name != d.name || identity.platform != d.platform {
                state.db.upsert_device(&StoredDevice { name: identity.name, platform: identity.platform, ..d.clone() })?;
            }
            caps
        }
        _ => return Err(AppError::Net("hello inválido".into())),
    };
    let stored = state.db.get_device(&d.device_id)?.unwrap_or(d);
    Ok((stored, caps))
}

/// Peer do handshake de pareamento: chave e id derivados do Noise, endereço discável.
struct PairPeer<'a> {
    remote_pk_hex: &'a str,
    remote_id: &'a str,
    dial_addr: Option<String>,
}

/// Pareamento: troca `Hello`, confere a identidade com a chave do handshake, grava o
/// device como confiável e encerra a sessão de pareamento. Retorna o device e as caps.
async fn complete_pairing(
    app: &AppHandle,
    chan: &mut SecureChannel,
    peer: PairPeer<'_>,
    role: Role,
    pairing_id: Option<String>,
    paired_tx: &mut Option<tokio::sync::oneshot::Sender<Result<DeviceIdentity>>>,
) -> Result<(StoredDevice, Vec<String>)> {
    let state = app.state::<AppState>();
    chan.send(&Message::Hello { identity: local_identity(&state), caps: local_caps() }).await?;
    let (identity, caps) = match tokio::time::timeout(HANDSHAKE_TIMEOUT, chan.recv()).await {
        Ok(Ok(Message::Hello { identity, caps })) => (identity, caps),
        _ => return Err(AppError::Pairing(crate::i18n::tr("err.pair_bad_code_or_silent").into())),
    };
    if identity.public_key != peer.remote_pk_hex || identity.device_id != peer.remote_id {
        return Err(AppError::Pairing(crate::i18n::tr("err.pair_identity").into()));
    }
    let d = StoredDevice {
        device_id: peer.remote_id.to_string(),
        name: identity.name.clone(),
        platform: identity.platform.clone(),
        public_key: peer.remote_pk_hex.to_string(),
        auto_send: true,
        auto_receive: true,
        blocked: false,
        last_seen: Some(now_ms()),
        last_address: peer.dial_addr,
        paired_at: now_ms(),
    };
    state.db.upsert_device(&d)?;
    log::info!("[PAIRING] Pairing completed with id={} name={:?} platform={}", peer.remote_id, identity.name, identity.platform);
    if let Role::Responder = role {
        *state.pairing.lock().unwrap() = None;
        if let Some(disc) = app.try_state::<super::TransportHandle>() {
            let _ = disc.discovery.announce(&state, false);
        }
    }
    events::emit(
        app,
        events::PAIRING_COMPLETED,
        serde_json::json!({ "deviceId": peer.remote_id, "name": identity.name, "pairingId": pairing_id }),
    );
    if let Some(tx) = paired_tx.take() {
        let _ = tx.send(Ok(identity));
    }
    Ok((d, caps))
}

/// Handshake de pareamento recusado no lado que exibe o código. Cada conexão testa um
/// só palpite (PAKE); após [`MAX_PAIR_FAILURES`] a sessão acaba e é preciso gerar outra.
fn note_pairing_failure(app: &AppHandle, pairing_id: &str) {
    let state = app.state::<AppState>();
    let exhausted = {
        let mut p = state.pairing.lock().unwrap();
        match p.as_mut() {
            Some(s) if s.id == pairing_id => {
                s.failures += 1;
                log::warn!("[PAIRING] Failed attempt {}/{MAX_PAIR_FAILURES} id={pairing_id}", s.failures);
                if s.failures >= MAX_PAIR_FAILURES {
                    *p = None;
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    };
    if exhausted {
        log::warn!("[PAIRING] Too many failed attempts, session closed id={pairing_id}");
        super::reannounce(app);
        events::emit(app, events::PAIRING_FAILED, serde_json::json!({ "pairingId": pairing_id, "reason": "too_many_attempts" }));
    }
}

pub async fn run(app: AppHandle, stream: TcpStream, role: Role, mode: Mode) -> Result<()> {
    run_with_pairing_id(app, stream, role, mode, None, &mut None).await
}

async fn run_with_pairing_id(
    app: AppHandle,
    mut stream: TcpStream,
    role: Role,
    mode: Mode,
    pairing_id: Option<String>,
    paired_tx: &mut Option<tokio::sync::oneshot::Sender<Result<DeviceIdentity>>>,
) -> Result<()> {
    let state = app.state::<AppState>();
    let peer_addr = stream.peer_addr().map(|a| a.to_string()).unwrap_or_default();
    let _ = stream.set_nodelay(true);

    // 1. PreHello em texto puro: escolhe o handshake. Nenhum dado do usuário aqui.
    let (mode, pairing_id) = match role {
        Role::Initiator => {
            let hello = PreHello {
                v: PROTOCOL_VERSION,
                mode: if matches!(mode, Mode::Pair { .. }) { "pair".into() } else { "session".into() },
                pairing_id: pairing_id.clone(),
                auth: match &mode {
                    Mode::Pair { auth, .. } => Some(auth.as_str().into()),
                    Mode::Session => None,
                },
                from: None,
            };
            write_frame(&mut stream, &serde_json::to_vec(&hello)?).await?;
            (mode, pairing_id)
        }
        Role::Responder => {
            let raw = tokio::time::timeout(HANDSHAKE_TIMEOUT, read_frame(&mut stream))
                .await
                .map_err(|_| AppError::Net(crate::i18n::tr("err.net_hello_timeout").into()))??;
            let hello: PreHello = serde_json::from_slice(&raw)?;
            if hello.v != PROTOCOL_VERSION {
                return Err(AppError::Net(crate::i18n::tr("err.net_protocol").into()));
            }
            if hello.mode == super::request::MODE {
                return super::request::on_request(&app, &mut stream, hello.from, &peer_addr).await;
            }
            if hello.mode == "pair" {
                let Some(auth) = hello.auth.as_deref().and_then(PairAuth::parse) else {
                    log::warn!("[AUTH] Pairing from {peer_addr} without PAKE (outdated peer)");
                    return Err(AppError::Pairing(crate::i18n::tr("err.pair_old_peer").into()));
                };
                let session = state.pairing.lock().unwrap().clone();
                match session {
                    // O QR carrega o id da sessão; o código digitado não.
                    Some(s)
                        if !s.is_expired()
                            && (auth == PairAuth::Code || hello.pairing_id.as_deref() == Some(s.id.as_str())) =>
                    {
                        (Mode::Pair { auth, password: s.password(auth) }, Some(s.id))
                    }
                    _ => return Err(AppError::Pairing(crate::i18n::tr("err.pair_no_session").into())),
                }
            } else {
                (Mode::Session, None)
            }
        }
    };
    let is_pairing = matches!(mode, Mode::Pair { .. });

    // 2. Pareamento: PAKE em claro (as mensagens não revelam o segredo) → PSK.
    //    Depois, handshake Noise (XXpsk3 no pareamento, XX na sessão).
    let private_key = state.identity.read().unwrap().private_key().to_vec();
    let hs: Result<HandshakeResult> = tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
        let psk = match &mode {
            Mode::Pair { auth, password } => {
                let (pake, outbound) = Pake::start(*auth, password);
                write_frame(&mut stream, &outbound).await?;
                let inbound = read_frame(&mut stream).await?;
                Some(pake.finish(&inbound)?)
            }
            Mode::Session => None,
        };
        match role {
            Role::Initiator => handshake_initiator(&mut stream, &private_key, psk.as_ref()).await,
            Role::Responder => handshake_responder(&mut stream, &private_key, psk.as_ref()).await,
        }
    })
    .await
    .map_err(|_| AppError::Net(crate::i18n::tr("err.net_handshake_timeout").into()))?;
    let hs = hs.map_err(|e| {
        log::warn!("[AUTH] Handshake failed with {peer_addr} pairing={is_pairing}: {e}");
        if is_pairing {
            if let (Role::Responder, Some(id)) = (role, pairing_id.as_deref()) {
                note_pairing_failure(&app, id);
            }
            AppError::Pairing(crate::i18n::tr("err.pair_bad_code_or_expired").into())
        } else {
            e
        }
    })?;

    let remote_pk_hex = hex::encode(&hs.remote_static);
    let remote_id = device_id_from_public_key(&hs.remote_static);
    // Endereço discável do peer: quem iniciou sabe o que discou; quem respondeu só
    // conhece a porta efêmera de origem, então usa a porta anunciada via mDNS.
    let dial_addr: Option<String> = match role {
        Role::Initiator => Some(peer_addr.clone()),
        Role::Responder => {
            let peers = state.peers.lock().unwrap();
            peers.get(&remote_id).and_then(|p| {
                let ip = peer_addr.rsplit_once(':').map(|(ip, _)| ip.to_string())?;
                Some(format!("{ip}:{}", p.port))
            })
        }
    };
    let mut chan = SecureChannel::new(stream, hs.transport);

    // 3. Autenticação: em sessão normal só aceitamos chave já pareada.
    let (stored, peer_caps) = match &mode {
        Mode::Session => authenticate_session(&state, &mut chan, &remote_pk_hex, &remote_id, &peer_addr).await?,
        Mode::Pair { .. } => {
            let peer = PairPeer { remote_pk_hex: &remote_pk_hex, remote_id: &remote_id, dial_addr: dial_addr.clone() };
            complete_pairing(&app, &mut chan, peer, role, pairing_id, paired_tx).await?
        }
    };

    // 4. Registrar conexão ativa (substitui duplicata anterior).
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let (data_tx, mut data_rx) = mpsc::channel::<DataFrame>(DATA_QUEUE_FRAMES);
    let conn_id = CONN_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    {
        let mut conns = state.connections.lock().unwrap();
        if let Some(old) = conns.insert(
            stored.device_id.clone(),
            ConnHandle { tx, data_tx, address: peer_addr.clone(), caps: peer_caps, conn_id },
        ) {
            let _ = old.tx.send(Message::Bye);
        }
    }
    state.db.touch_device(&stored.device_id, dial_addr.as_deref())?;
    *state.last_error.lock().unwrap() = None;
    log::info!("[DEVICE] Connected id={} name={:?} addr={} role={}", stored.device_id, stored.name, peer_addr, if matches!(role, Role::Initiator) { "initiator" } else { "responder" });
    events::emit(
        &app,
        events::DEVICE_CONNECTED,
        serde_json::json!({ "deviceId": stored.device_id, "name": stored.name, "state": DeviceState::Connected }),
    );
    events::emit(&app, events::DEVICE_UPDATED, state.devices());
    sync::on_connected(&app, &stored.device_id);
    transfer::manager::on_connected(&app, &stored.device_id);

    // 5. Loop de mensagens.
    let device_id = stored.device_id.clone();
    let mut ping = tokio::time::interval(PING_EVERY);
    ping.tick().await;
    let result: Result<()> = loop {
        // `biased`: rede → controle → ping → dados de arquivo. Um quadro de dados
        // (~60 KB) nunca segura Clip/Ack/FileCancel atrás de uma fila de arquivo.
        tokio::select! {
            biased;
            incoming = tokio::time::timeout(IDLE_TIMEOUT, chan.recv_any()) => {
                let msg = match incoming {
                    Ok(Ok(Inbound::Msg(m))) => m,
                    Ok(Ok(Inbound::Data(frame))) => {
                        // Backpressure até o disco do receptor: se a task não drena, paramos de ler o socket.
                        transfer::manager::on_data(&app, &device_id, frame).await;
                        continue;
                    }
                    Ok(Err(e)) => break Err(e),
                    Err(_) => break Err(AppError::Net(crate::i18n::tr("err.net_idle").into())),
                };
                match msg {
                    Message::Ping => { let _ = chan.send(&Message::Pong).await; }
                    Message::Pong => {}
                    Message::Bye => break Ok(()),
                    // Hello no meio da sessão = o peer mudou de nome.
                    Message::Hello { identity, .. } => on_identity_update(&app, &device_id, identity),
                    Message::Ack { id } => {
                        log::info!("[SYNC] Item acknowledged id={id} by={device_id}");
                        sync::on_ack(&app, &device_id, &id)
                    }
                    Message::Clip { item } => {
                        let id = item.id.clone();
                        log::info!("[SYNC] Item received id={} type={} from={} bytes={}", id, item.content_type.as_str(), device_id, item.content.len());
                        sync::on_clip_received(&app, &device_id, item);
                        let _ = chan.send(&Message::Ack { id }).await;
                    }
                    m @ (Message::FileOffer(_)
                    | Message::FileAccept { .. }
                    | Message::FileReject { .. }
                    | Message::FileAck { .. }
                    | Message::FileDone { .. }
                    | Message::FileResult { .. }
                    | Message::FileCancel { .. }) => {
                        log::debug!("[FILE] control message from={device_id} kind={}", file_msg_kind(&m));
                        transfer::manager::on_message(&app, &device_id, conn_id, m);
                    }
                }
            }
            outgoing = rx.recv() => {
                match outgoing {
                    Some(Message::Bye) | None => { let _ = chan.send(&Message::Bye).await; break Ok(()); }
                    Some(m) => if let Err(e) = chan.send(&m).await { break Err(e); }
                }
            }
            _ = ping.tick() => {
                if let Err(e) = chan.send(&Message::Ping).await { break Err(e); }
            }
            frame = data_rx.recv() => {
                match frame {
                    Some(f) => if let Err(e) = chan.send_data(&f).await { break Err(e); },
                    // Nunca acontece enquanto o ConnHandle existir; se acontecer, só ignora.
                    None => tokio::time::sleep(Duration::from_millis(50)).await,
                }
            }
        }
    };

    // 6. Encerramento.
    {
        let mut conns = state.connections.lock().unwrap();
        // Pelo `conn_id`: o endereço discado se repete entre reconexões e não identifica a conexão.
        if conns.get(&device_id).map(|c| c.conn_id == conn_id).unwrap_or(false) {
            conns.remove(&device_id);
        }
    }
    // Só as transferências amarradas a ESTA conexão param (uma conexão nova pode já estar no lugar).
    transfer::manager::on_disconnected(&app, &device_id, conn_id);
    state.db.touch_device(&device_id, None)?;
    log::info!("[DEVICE] Disconnected id={} name={:?} reason={}", device_id, stored.name, result.as_ref().err().map(|e| e.to_string()).unwrap_or_else(|| "bye".into()));
    events::emit(
        &app,
        events::DEVICE_DISCONNECTED,
        serde_json::json!({ "deviceId": device_id, "state": DeviceState::Disconnected }),
    );
    events::emit(&app, events::DEVICE_UPDATED, state.devices());
    result
}
