use crate::db::Db;
use crate::identity::LocalIdentity;
use crate::keystore::Keystore;
use crate::models::{now_ms, Device, DeviceState, Settings, SyncStatus};
use crate::transport::pairing::PairingSession;
use crate::transport::protocol::{DataFrame, Message};
use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Mutex, RwLock};
use tokio::sync::mpsc::UnboundedSender;

/// Dispositivo visto na rede (mDNS), pareado ou não.
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub device_id: String,
    pub name: String,
    pub platform: String,
    pub public_key: String,
    pub addrs: Vec<IpAddr>,
    pub port: u16,
    pub pairing: bool,
    pub first_seen: i64,
    pub last_seen: i64,
    pub fullname: String,
}

/// Conexão ativa (cifrada) com um dispositivo confiável.
pub struct ConnHandle {
    /// Controle (clipboard, acks, `File*`): sem limite, tem prioridade no loop.
    pub tx: UnboundedSender<Message>,
    /// Dados de arquivo: **bounded** — o emissor bloqueia quando a conexão não drenou.
    pub data_tx: tokio::sync::mpsc::Sender<DataFrame>,
    pub address: String,
    /// Capabilities anunciadas pelo peer no `Hello` (ex.: `file-v1`).
    pub caps: Vec<String>,
    /// Identifica esta conexão; tasks de transferência ficam amarradas a ela.
    pub conn_id: u64,
}

pub struct AppState {
    pub app_dir: PathBuf,
    pub identity: RwLock<LocalIdentity>,
    pub keystore: Keystore,
    pub db: Db,
    pub settings: RwLock<Settings>,
    /// `Some(0)` = pausado sem prazo; `Some(ts)` = até ts; `None` = ativo
    pub paused_until: Mutex<Option<i64>>,
    /// Anti-loop e dedupe do clipboard, compartilhado por watcher, poll e IME.
    pub clip_dedupe: Mutex<crate::clipboard::dedupe::DedupeState>,
    pub peers: Mutex<HashMap<String, PeerInfo>>,
    pub connections: Mutex<HashMap<String, ConnHandle>>,
    pub connecting: Mutex<std::collections::HashSet<String>>,
    pub pairing: Mutex<Option<PairingSession>>,
    pub listen_port: AtomicU16,
    pub last_error: Mutex<Option<String>>,
    /// Transferências de arquivo ativas (tasks) e throttle de progresso.
    pub transfers: crate::transfer::manager::TransferManager,
}

impl AppState {
    pub fn self_id(&self) -> String {
        self.identity.read().unwrap().device_id.clone()
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().unwrap().clone()
    }

    /// Nome exibido e anunciado: o dos Ajustes vale sobre o da instalação.
    pub fn display_name(&self) -> String {
        let custom = self.settings.read().unwrap().device_name.trim().to_string();
        if custom.is_empty() {
            self.identity.read().unwrap().name.clone()
        } else {
            custom
        }
    }

    pub fn is_paused(&self) -> bool {
        let mut p = self.paused_until.lock().unwrap();
        match *p {
            None => false,
            Some(0) => true,
            Some(ts) => {
                if now_ms() >= ts {
                    *p = None;
                    false
                } else {
                    true
                }
            }
        }
    }

    pub fn port(&self) -> u16 {
        self.listen_port.load(Ordering::Relaxed)
    }

    pub fn is_connected(&self, device_id: &str) -> bool {
        self.connections.lock().unwrap().contains_key(device_id)
    }

    pub fn send_to(&self, device_id: &str, msg: Message) -> bool {
        let conns = self.connections.lock().unwrap();
        match conns.get(device_id) {
            Some(c) => c.tx.send(msg).is_ok(),
            None => false,
        }
    }

    pub fn sync_status(&self) -> SyncStatus {
        let paused_until = *self.paused_until.lock().unwrap();
        let connected = self.connections.lock().unwrap().len() as u32;
        let queued = self.db.outbox_total().unwrap_or(0);
        SyncStatus {
            paused: self.is_paused(),
            paused_until: paused_until.filter(|v| *v > 0),
            online: self.port() > 0,
            connected_count: connected,
            queued_count: queued,
            last_error: self.last_error.lock().unwrap().clone(),
        }
    }

    /// Lista completa: este dispositivo + pareados (com estado ao vivo) + descobertos não pareados.
    pub fn devices(&self) -> Vec<Device> {
        let me = self.identity.read().unwrap().clone();
        let settings = self.settings();
        let mut out = vec![Device {
            device_id: me.device_id.clone(),
            name: self.display_name(),
            platform: me.platform.clone(),
            public_key: me.public_key_hex(),
            state: DeviceState::Connected,
            is_self: true,
            auto_send: settings.auto_send,
            auto_receive: settings.auto_receive,
            last_seen: Some(now_ms()),
            address: None,
            paired_at: None,
            last_error: None,
            queued: 0,
        }];
        let peers = self.peers.lock().unwrap().clone();
        let conns = self.connections.lock().unwrap();
        let connecting = self.connecting.lock().unwrap();
        let queued = self.db.outbox_counts().unwrap_or_default();
        let stored = self.db.list_devices().unwrap_or_default();
        for d in &stored {
            let peer = peers.get(&d.device_id);
            let (state, address) = if let Some(c) = conns.get(&d.device_id) {
                (DeviceState::Connected, Some(c.address.clone()))
            } else if connecting.contains(&d.device_id) {
                (DeviceState::Connecting, peer.and_then(|p| p.addrs.first().map(|a| a.to_string())))
            } else if let Some(p) = peer {
                (DeviceState::Disconnected, p.addrs.first().map(|a| a.to_string()))
            } else {
                (DeviceState::Disconnected, None)
            };
            let mut dev = d.to_device(state, address, *queued.get(&d.device_id).unwrap_or(&0));
            if let Some(p) = peer {
                dev.last_seen = Some(p.last_seen.max(d.last_seen.unwrap_or(0)));
            }
            out.push(dev);
        }
        for (id, p) in peers.iter() {
            if stored.iter().any(|d| &d.device_id == id) {
                continue;
            }
            out.push(Device {
                device_id: id.clone(),
                name: p.name.clone(),
                platform: p.platform.clone(),
                public_key: p.public_key.clone(),
                state: DeviceState::Discovered,
                is_self: false,
                auto_send: false,
                auto_receive: false,
                last_seen: Some(p.last_seen),
                address: p.addrs.first().map(|a| a.to_string()),
                paired_at: None,
                last_error: None,
                queued: 0,
            });
        }
        out
    }
}
