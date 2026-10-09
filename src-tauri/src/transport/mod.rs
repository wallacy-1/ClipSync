//! Camada de transporte P2P, independente da UI:
//! descoberta (mDNS) · conexão (TCP) · autenticação + cifra (Noise) · envio/recebimento.
pub mod connection;
pub mod discovery;
pub mod noise;
pub mod pairing;
pub mod protocol;
pub mod request;

use crate::error::{AppError, Result};
use crate::events;
use crate::models::now_ms;
use crate::state::AppState;
use discovery::Discovery;
use protocol::Message;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::net::TcpListener;

pub const PREFERRED_PORT: u16 = 47_820;

pub struct TransportHandle {
    pub discovery: Discovery,
}

/// Sobe listener TCP, anuncia via mDNS, inicia busca e o loop de reconexão.
pub async fn start(app: AppHandle) -> Result<()> {
    let state = app.state::<AppState>();

    let (listener_v4, listener_v6) = bind_listeners().await?;
    let port = listener_v4.local_addr()?.port();
    state.listen_port.store(port, Ordering::Relaxed);
    if listener_v6.is_some() {
        log::info!("[TRANSPORT] Listening on 0.0.0.0:{port} and [::]:{port}");
    } else {
        log::info!("[TRANSPORT] Listening on 0.0.0.0:{port} (IPv6 indisponível)");
    }

    let discovery = Discovery::start(&app).map_err(|e| AppError::Net(e.to_string()))?;
    let pairing = state.pairing.lock().unwrap().is_some();
    if let Err(e) = discovery.announce(&state, pairing) {
        log::warn!("mDNS indisponível: {e}");
    }
    app.manage(TransportHandle { discovery });

    for listener in std::iter::once(listener_v4).chain(listener_v6) {
        tauri::async_runtime::spawn(accept_loop(app.clone(), listener));
    }

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

/// IPv4 em `0.0.0.0:P` e IPv6 em `[::]:P` (só v6, para não colidir com o socket v4) na mesma porta.
/// A porta vem do IPv4 (47820 ou, ocupada, uma livre); se o bind IPv6 falhar, segue só com IPv4.
async fn bind_listeners() -> Result<(TcpListener, Option<TcpListener>)> {
    let v4 = match TcpListener::bind(("0.0.0.0", PREFERRED_PORT)).await {
        Ok(l) => l,
        Err(_) => TcpListener::bind(("0.0.0.0", 0)).await?,
    };
    let port = v4.local_addr()?.port();
    let v6 = match bind_v6(port) {
        Ok(l) => Some(l),
        Err(e) => {
            log::warn!("[TRANSPORT] Sem listener IPv6 em [::]:{port}: {e}");
            None
        }
    };
    Ok((v4, v6))
}

fn bind_v6(port: u16) -> std::io::Result<TcpListener> {
    use socket2::{Domain, Protocol, Socket, Type};
    let sock = Socket::new(Domain::IPV6, Type::STREAM, Some(Protocol::TCP))?;
    sock.set_only_v6(true)?;
    // Como o tokio faz: SO_REUSEADDR só em unix (no Windows deixaria outro processo roubar a porta).
    #[cfg(unix)]
    sock.set_reuse_address(true)?;
    sock.set_nonblocking(true)?;
    sock.bind(&SocketAddr::new(Ipv6Addr::UNSPECIFIED.into(), port).into())?;
    sock.listen(1024)?;
    TcpListener::from_std(sock.into())
}

async fn accept_loop(app: AppHandle, listener: TcpListener) {
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                log::info!("[TRANSPORT] Inbound connection from {addr}");
                let app = app.clone();
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
    rank_addrs(&local_addrs(), out)
}

/// IPv6 link-local (`fe80::/10`): só vale com scope id, que o outro lado não conhece.
pub fn is_link_local_v6(a: &Ipv6Addr) -> bool {
    a.segments()[0] & 0xffc0 == 0xfe80
}

/// Endereço próprio que vale anunciar/mostrar: nada de loopback, não especificado, multicast nem link-local.
fn is_shareable(ip: &IpAddr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() {
        return false;
    }
    match ip {
        IpAddr::V4(v4) => !v4.is_link_local(),
        IpAddr::V6(v6) => !is_link_local_v6(v6),
    }
}

/// Endereço de um peer descoberto na rede que vale tentar (IPv4 ou IPv6 global/ULA).
pub fn is_usable_peer_addr(ip: &IpAddr) -> bool {
    !ip.is_loopback() && !ip.is_unspecified() && !ip.is_multicast() && !matches!(ip, IpAddr::V6(v6) if is_link_local_v6(v6))
}

/// Ordena candidatos `ip:porta`: IPv4 da mesma /24 -> IPv6 da mesma /64 -> IPv4 outros -> IPv6 outros.
/// Descarta o que não é `SocketAddr` (ex.: IP sem porta) e IPv6 link-local; mantém a ordem relativa (sort estável).
fn rank_addrs(local: &[IpAddr], candidates: Vec<String>) -> Vec<String> {
    let mut parsed: Vec<(u8, String)> = candidates
        .into_iter()
        .filter_map(|a| {
            let ip = a.parse::<SocketAddr>().ok()?.ip();
            let rank = match ip {
                IpAddr::V4(v4) => {
                    if local.iter().any(|l| matches!(l, IpAddr::V4(lv4) if lv4.octets()[..3] == v4.octets()[..3])) { 0 } else { 2 }
                }
                IpAddr::V6(v6) => {
                    if is_link_local_v6(&v6) {
                        return None;
                    }
                    if local.iter().any(|l| matches!(l, IpAddr::V6(lv6) if lv6.segments()[..4] == v6.segments()[..4])) { 1 } else { 3 }
                }
            };
            Some((rank, a))
        })
        .collect();
    parsed.sort_by_key(|(r, _)| *r);
    parsed.into_iter().map(|(_, a)| a).collect()
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

/// Endereços deste aparelho para anunciar, mostrar e pôr no QR: IPv4 primeiro, depois IPv6 global/ULA.
pub fn local_addrs() -> Vec<IpAddr> {
    let mut v4 = Vec::new();
    let mut v6 = Vec::new();
    let mut push = |ip: IpAddr| {
        if !is_shareable(&ip) {
            return;
        }
        let list = if ip.is_ipv4() { &mut v4 } else { &mut v6 };
        if !list.contains(&ip) {
            list.push(ip);
        }
    };
    if let Ok(ip) = local_ip_address::local_ip() {
        push(ip);
    }
    if let Ok(list) = local_ip_address::list_afinet_netifas() {
        for (_, ip) in list {
            push(ip);
        }
    }
    v4.extend(v6);
    v4
}

/// Endereços para mostrar ao usuário: `ip`, ou `ip:porta` (`[ipv6]:porta`) se a porta não é a padrão.
pub fn display_addrs(addrs: &[IpAddr], port: u16) -> Vec<String> {
    addrs
        .iter()
        .filter(|ip| is_shareable(ip))
        .map(|ip| if port == PREFERRED_PORT { ip.to_string() } else { SocketAddr::new(*ip, port).to_string() })
        .collect()
}

/// Endereço digitado pelo usuário (`ip`, `ip:porta` ou `[ipv6]:porta`) -> `SocketAddr`.
pub fn parse_manual_addr(input: &str, default_port: u16) -> Result<SocketAddr> {
    let invalid = || AppError::Other(crate::i18n::tr("err.addr_invalid").into());
    let t = input.trim();
    let addr = if let Ok(sa) = t.parse::<SocketAddr>() {
        sa
    } else if let Ok(ip) = t.parse::<IpAddr>() {
        SocketAddr::new(ip, default_port)
    } else if t.contains('%') && t.matches(':').count() >= 2 {
        return Err(AppError::Other(crate::i18n::tr("err.addr_v6_scope").into()));
    } else {
        return Err(invalid());
    };
    if matches!(addr.ip(), IpAddr::V6(v6) if is_link_local_v6(&v6)) {
        return Err(AppError::Other(crate::i18n::tr("err.addr_v6_link_local").into()));
    }
    if addr.port() == 0 || addr.ip().is_unspecified() {
        return Err(invalid());
    }
    Ok(addr)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Result<SocketAddr> {
        parse_manual_addr(s, PREFERRED_PORT)
    }

    #[test]
    fn manual_addr_accepts_ipv4() {
        assert_eq!(parse("192.168.0.10").unwrap().to_string(), "192.168.0.10:47820");
        assert_eq!(parse("192.168.0.10:5000").unwrap().to_string(), "192.168.0.10:5000");
        assert_eq!(parse(" 10.0.0.2 ").unwrap().to_string(), "10.0.0.2:47820");
    }

    #[test]
    fn manual_addr_rejects_garbage() {
        for bad in ["abc", "999.1.1.1", "", "192.168.0.10:0", "0.0.0.0", "pc.local", "1.2.3.4:99999"] {
            assert!(parse(bad).unwrap_err().to_string().starts_with("Endereço inválido"), "{bad}");
        }
    }

    #[test]
    fn manual_addr_accepts_ipv6() {
        assert_eq!(parse("[::1]:47820").unwrap().to_string(), "[::1]:47820");
        assert_eq!(parse("2001:db8::1").unwrap().to_string(), "[2001:db8::1]:47820");
        assert_eq!(parse("[fd00::2]:5000").unwrap().to_string(), "[fd00::2]:5000");
        assert_eq!(parse("::1").unwrap().to_string(), "[::1]:47820");
    }

    #[test]
    fn manual_addr_rejects_link_local_and_scope() {
        for bad in ["fe80::1", "[fe80::1]:47820", "[febf::1]:1"] {
            assert!(parse(bad).unwrap_err().to_string().contains("fe80::"), "{bad}");
        }
        assert!(parse("fe80::1%12").unwrap_err().to_string().contains("interface"));
        assert!(parse("::").unwrap_err().to_string().starts_with("Endereço inválido"));
        assert!(parse("[2001:db8::1]:0").unwrap_err().to_string().starts_with("Endereço inválido"));
        assert!(parse("2001:zz::1").unwrap_err().to_string().starts_with("Endereço inválido"));
    }

    fn ips(v: &[&str]) -> Vec<IpAddr> {
        v.iter().map(|s| s.parse().unwrap()).collect()
    }

    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn rank_prefers_same_subnet_v4_then_same_v6_prefix() {
        let local = ips(&["192.168.0.5", "2001:db8:1:2::5"]);
        let out = rank_addrs(
            &local,
            strs(&["[2001:db8:9::1]:1", "10.0.0.7:1", "[2001:db8:1:2::9]:1", "192.168.0.20:1"]),
        );
        assert_eq!(out, ["192.168.0.20:1", "[2001:db8:1:2::9]:1", "10.0.0.7:1", "[2001:db8:9::1]:1"]);
    }

    #[test]
    fn rank_drops_link_local_v6_and_bare_ips() {
        let out = rank_addrs(&[], strs(&["[fe80::1]:1", "192.168.0.20", "[::1]:1", "10.0.0.7:1"]));
        assert_eq!(out, ["10.0.0.7:1", "[::1]:1"]);
    }

    #[test]
    fn rank_is_stable_within_class() {
        let out = rank_addrs(&[], strs(&["10.0.0.9:1", "10.0.0.3:1", "[2001:db8::2]:1", "[2001:db8::1]:1"]));
        assert_eq!(out, ["10.0.0.9:1", "10.0.0.3:1", "[2001:db8::2]:1", "[2001:db8::1]:1"]);
    }

    #[test]
    fn shareable_excludes_link_local_loopback_multicast() {
        for bad in ["fe80::1", "febf::1", "::1", "::", "ff02::1", "127.0.0.1", "169.254.1.1", "0.0.0.0"] {
            assert!(!is_shareable(&bad.parse().unwrap()), "{bad}");
        }
        for ok in ["192.168.0.5", "2001:db8::1", "fd12:3456::1"] {
            assert!(is_shareable(&ok.parse().unwrap()), "{ok}");
        }
    }

    #[test]
    fn display_addrs_brackets_v6_and_hides_link_local() {
        let l = ips(&["192.168.0.109", "fe80::1", "2001:db8::1"]);
        assert_eq!(display_addrs(&l, PREFERRED_PORT), ["192.168.0.109", "2001:db8::1"]);
        assert_eq!(display_addrs(&l, 47821), ["192.168.0.109:47821", "[2001:db8::1]:47821"]);
    }

    #[test]
    fn display_addrs_hides_default_port() {
        let ips: Vec<IpAddr> = vec!["192.168.0.109".parse().unwrap()];
        assert_eq!(display_addrs(&ips, PREFERRED_PORT), ["192.168.0.109"]);
        assert_eq!(display_addrs(&ips, 47821), ["192.168.0.109:47821"]);
    }
}
