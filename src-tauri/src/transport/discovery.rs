//! Descoberta automática na LAN via mDNS / DNS-SD (`_clip-sync._tcp.local`).
//! Cada instância anuncia id, nome, plataforma e chave pública; ninguém
//! precisa digitar IP.
use crate::events;
use crate::models::now_ms;
use crate::state::{AppState, PeerInfo};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

pub const SERVICE_TYPE: &str = "_clip-sync._tcp.local.";

pub struct Discovery {
    daemon: ServiceDaemon,
    registered: Mutex<Option<String>>,
}

impl Discovery {
    pub fn start(app: &AppHandle) -> anyhow::Result<Self> {
        let daemon = ServiceDaemon::new()?;
        let d = Self { daemon, registered: Mutex::new(None) };
        d.spawn_browser(app.clone())?;
        Ok(d)
    }

    /// (Re)anuncia este dispositivo. `pairing` marca que há uma sessão de
    /// pareamento aberta (usado pelo código digitado manualmente).
    pub fn announce(&self, state: &AppState, pairing: bool) -> anyhow::Result<()> {
        let id = state.identity.read().unwrap().clone();
        let name = state.display_name();
        let port = state.port();
        if port == 0 {
            return Ok(());
        }
        let mut props: HashMap<String, String> = HashMap::new();
        props.insert("v".into(), "1".into());
        props.insert("id".into(), id.device_id.clone());
        props.insert("name".into(), name);
        props.insert("platform".into(), id.platform.clone());
        props.insert("pk".into(), id.public_key_hex());
        props.insert("pairing".into(), if pairing { "1".into() } else { "0".into() });

        let host = format!("clipsync-{}.local.", &id.device_id[..8]);
        let info = ServiceInfo::new(SERVICE_TYPE, &id.device_id, &host, "", port, props)?
            .enable_addr_auto();
        let fullname = info.get_fullname().to_string();

        let mut reg = self.registered.lock().unwrap();
        if let Some(old) = reg.take() {
            let _ = self.daemon.unregister(&old);
        }
        self.daemon.register(info)?;
        *reg = Some(fullname);
        Ok(())
    }

    fn spawn_browser(&self, app: AppHandle) -> anyhow::Result<()> {
        let rx = self.daemon.browse(SERVICE_TYPE)?;
        std::thread::Builder::new()
            .name("mdns-browse".into())
            .spawn(move || {
                while let Ok(ev) = rx.recv() {
                    match ev {
                        ServiceEvent::ServiceResolved(info) => on_resolved(&app, &info),
                        ServiceEvent::ServiceRemoved(_, fullname) => on_removed(&app, &fullname),
                        _ => {}
                    }
                }
            })?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn shutdown(&self) {
        if let Some(old) = self.registered.lock().unwrap().take() {
            let _ = self.daemon.unregister(&old);
        }
        let _ = self.daemon.shutdown();
    }
}

fn on_resolved(app: &AppHandle, info: &ServiceInfo) {
    let state = app.state::<AppState>();
    let get = |k: &str| info.get_property_val_str(k).map(|s| s.to_string());
    let Some(id) = get("id") else { return };
    if id == state.self_id() {
        return;
    }
    let addrs: Vec<IpAddr> = info
        .get_addresses()
        .iter()
        .copied()
        .filter(super::is_usable_peer_addr)
        .collect();
    if addrs.is_empty() {
        return;
    }
    let now = now_ms();
    let (peer, changed) = {
        let mut peers = state.peers.lock().unwrap();
        let prev = peers.get(&id);
        let first_seen = prev.map(|p| p.first_seen).unwrap_or(now);
        let changed = prev.map(|p| p.addrs != addrs || p.port != info.get_port() || p.pairing != (get("pairing").as_deref() == Some("1"))).unwrap_or(true);
        let p = PeerInfo {
            device_id: id.clone(),
            name: get("name").unwrap_or_else(|| "Dispositivo".into()),
            platform: get("platform").unwrap_or_default(),
            public_key: get("pk").unwrap_or_default(),
            addrs,
            port: info.get_port(),
            pairing: get("pairing").as_deref() == Some("1"),
            first_seen,
            last_seen: now,
            fullname: info.get_fullname().to_string(),
        };
        peers.insert(id.clone(), p.clone());
        (p, changed)
    };
    if !changed {
        return;
    }
    log::info!("[DISCOVERY] Device discovered id={} name={:?} platform={} addrs={:?} port={} pairing={}", peer.device_id, peer.name, peer.platform, peer.addrs, peer.port, peer.pairing);
    let _ = state.db.touch_device(&id, peer.addrs.first().map(|a| a.to_string()).as_deref());
    events::emit(app, events::DEVICE_DISCOVERED, serde_json::json!({ "deviceId": id }));
    events::emit(app, events::DEVICE_UPDATED, state.devices());
    super::maybe_connect(app.clone(), id);
}

fn on_removed(app: &AppHandle, fullname: &str) {
    let state = app.state::<AppState>();
    let removed = {
        let mut peers = state.peers.lock().unwrap();
        let id = peers.iter().find(|(_, p)| p.fullname == fullname).map(|(id, _)| id.clone());
        if let Some(id) = &id {
            peers.remove(id);
        }
        id
    };
    if let Some(id) = &removed {
        log::info!("[DISCOVERY] Device removed id={id}");
    }
    if removed.is_some() {
        events::emit(app, events::DEVICE_UPDATED, state.devices());
    }
}
