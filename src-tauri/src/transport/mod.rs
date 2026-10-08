//! Camada de transporte P2P, independente da UI:
//! descoberta (mDNS) · conexão (TCP) · autenticação + cifra (Noise) · envio/recebimento.
pub mod connection;
pub mod discovery;
pub mod noise;
pub mod pairing;
pub mod protocol;

use crate::error::{AppError, Result};
use crate::events;
use crate::models::now_ms;
use crate::state::AppState;
use discovery::Discovery;
use protocol::Message;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::net::TcpListener;

const PREFERRED_PORT: u16 = 47_820;

pub struct TransportHandle {
    pub discovery: Discovery,
}

/// Sobe listener TCP, anuncia via mDNS, inicia busca e o loop de reconexão.
pub async fn start(app: AppHandle) -> Result<()> {
    let state = app.state::<AppState>();

    let listener = match TcpListener::bind(("0.0.0.0", PREFERRED_PORT)).await {
        Ok(l) => l,
        Err(_) => TcpListener::bind(("0.0.0.0", 0)).await?,
    };
    let port = listener.local_addr()?.port();
    state.listen_port.store(port, Ordering::Relaxed);
    log::info!("[TRANSPORT] Listening on 0.0.0.0:{port}");

    let discovery = Discovery::start(&app).map_err(|e| AppError::Net(e.to_string()))?;
    let pairing = state.pairing.lock().unwrap().is_some();
    if let Err(e) = discovery.announce(&state, pairing) {
        log::warn!("mDNS indisponível: {e}");
    }
    app.manage(TransportHandle { discovery });

    let app_accept = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    log::info!("[TRANSPORT] Inbound connection from {addr}");
                    let app = app_accept.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) =
                            connection::run(app, stream, connection::Role::Responder, connection::Mode::Session).await
                        {
                            log::info!("[TRANSPORT] Inbound {addr} closed: {e}");
                        }
                    });
                }
                Err(e) => {
                    log::warn!("accept falhou: {e}");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    });

    // Reconexão: tenta pareados conhecidos (descobertos ou último endereço).
    let app_re = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(8)).await;
            let state = app_re.state::<AppState>();
            let stored = state.db.list_devices().unwrap_or_default();
            for d in stored {
                if d.blocked || state.is_connected(&d.device_id) {
                    continue;
                }
                maybe_connect(app_re.clone(), d.device_id.clone());
            }
        }
    });

    events::emit(&app, events::SYNC_STATE, state.sync_status());
    Ok(())
}

/// Regra anti-duplicata: o dispositivo de id menor inicia. O outro só inicia se,
/// passados 20 s desde que viu o par, ainda não há conexão (cobre o caso em que o
/// multicast não chega até o outro lado, comum no Android).
fn should_initiate(self_id: &str, peer_id: &str, seen_since: Option<i64>) -> bool {
    if self_id < peer_id {
        return true;
    }
    match seen_since {
        Some(t) => now_ms() - t > 20_000,
        None => true,
    }
}

pub fn candidate_addrs(state: &AppState, device_id: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(p) = state.peers.lock().unwrap().get(device_id) {
        for a in &p.addrs {
            out.push(SocketAddr::new(*a, p.port).to_string());
        }
    }
    if let Ok(Some(d)) = state.db.get_device(device_id) {
        if let Some(addr) = d.last_address {
            if !out.contains(&addr) {
                out.push(addr);
            }
        }
    }
    // Só IPv4: o listener escuta em 0.0.0.0. Endereços da mesma sub-rede primeiro.
    out.retain(|a| a.parse::<SocketAddr>().map(|s| s.is_ipv4()).unwrap_or(false));
    let local = local_addrs();
    out.sort_by_key(|a| {
        let ip = a.parse::<SocketAddr>().map(|s| s.ip()).ok();
        match ip {
            Some(IpAddr::V4(v4)) => !local.iter().any(|l| matches!(l, IpAddr::V4(lv4) if lv4.octets()[..3] == v4.octets()[..3])),
            _ => true,
        }
    });
    out
}

/// Processo voltou a rodar (teclado aberto, app na frente): reconecta já em vez de
/// esperar o laço de 8 s, e pinga as conexões abertas para derrubar logo as que o
/// Android matou enquanto o processo estava congelado.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub fn wake(app: &AppHandle) {
    let state = app.state::<AppState>();
    for d in state.db.list_devices().unwrap_or_default() {
        if d.blocked {
            continue;
        }
        if !state.send_to(&d.device_id, Message::Ping) {
            maybe_connect(app.clone(), d.device_id.clone());
        }
    }
}

/// Abre conexão com um dispositivo pareado se fizer sentido agora.
pub fn maybe_connect(app: AppHandle, device_id: String) {
    let state = app.state::<AppState>();
    let Ok(Some(d)) = state.db.get_device(&device_id) else { return };
    if d.blocked || state.is_connected(&device_id) || state.connecting.lock().unwrap().contains(&device_id) {
        return;
    }
    let seen_since = state.peers.lock().unwrap().get(&device_id).map(|p| p.first_seen);
    if !should_initiate(&state.self_id(), &device_id, seen_since) {
        return;
    }
    let addrs = candidate_addrs(&state, &device_id);
    if addrs.is_empty() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        for addr in addrs {
            let app2 = app.clone();
            connection::connect_session(app2, device_id.clone(), addr).await;
            let st = app.state::<AppState>();
            if st.is_connected(&device_id) {
                break;
            }
        }
    });
}

pub fn local_addrs() -> Vec<IpAddr> {
    let mut out = Vec::new();
    if let Ok(ip) = local_ip_address::local_ip() {
        if ip.is_ipv4() {
            out.push(ip);
        }
    }
    if let Ok(list) = local_ip_address::list_afinet_netifas() {
        for (_, ip) in list {
            if ip.is_loopback() || out.contains(&ip) {
                continue;
            }
            if let IpAddr::V4(v4) = ip {
                if v4.is_link_local() {
                    continue;
                }
                out.push(ip);
            }
        }
    }
    out
}

pub fn reannounce(app: &AppHandle) {
    let state = app.state::<AppState>();
    let pairing = state.pairing.lock().unwrap().as_ref().map(|p| !p.is_expired()).unwrap_or(false);
    if let Some(t) = app.try_state::<TransportHandle>() {
        if let Err(e) = t.discovery.announce(&state, pairing) {
            log::warn!("falha ao reanunciar: {e}");
        }
    }
}
