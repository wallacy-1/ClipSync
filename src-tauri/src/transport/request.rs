//! Pedido de pareamento: um dispositivo avisa outro, pela rede, que quer parear.
//!
//! O pedido vai em texto puro, antes do Noise, e não prova nada: só faz o outro lado
//! mostrar um aviso. Aceitar abre uma sessão de pareamento normal, e o código ainda é
//! lido na tela de um e digitado no outro; um pedido forjado não pareia nada sozinho.
use crate::error::{AppError, Result};
use crate::events;
use crate::identity::DeviceIdentity;
use crate::models::now_ms;
use crate::state::AppState;
use crate::transport::protocol::{read_frame, write_frame, PreHello, PROTOCOL_VERSION};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::net::TcpStream;

pub const MODE: &str = "request";
const TIMEOUT: Duration = Duration::from_secs(6);
/// Rodadas do pedido quando nenhum endereço responde (rede recém-conectada, firewall).
const ATTEMPTS: u32 = 3;
/// Pedidos do mesmo IP mais próximos que isso são recusados (evita enxurrada de avisos).
const MIN_INTERVAL_MS: i64 = 10_000;
const MAX_NAME: usize = 64;

#[derive(Debug, Serialize, Deserialize)]
struct Reply {
    ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

fn last_by_ip() -> &'static Mutex<HashMap<String, i64>> {
    static LAST: OnceLock<Mutex<HashMap<String, i64>>> = OnceLock::new();
    LAST.get_or_init(Default::default)
}

/// Lado que recebe: avisa a UI e responde na mesma conexão, que então fecha.
pub async fn on_request(app: &AppHandle, stream: &mut TcpStream, from: Option<DeviceIdentity>, peer_addr: &str) -> Result<()> {
    let state = app.state::<AppState>();
    let ip = peer_addr.rsplit_once(':').map(|(ip, _)| ip).unwrap_or(peer_addr).to_string();
    let now = now_ms();
    let throttled = {
        let mut last = last_by_ip().lock().unwrap();
        last.retain(|_, t| now - *t < MIN_INTERVAL_MS);
        match last.entry(ip) {
            std::collections::hash_map::Entry::Occupied(_) => true,
            std::collections::hash_map::Entry::Vacant(v) => {
                v.insert(now);
                false
            }
        }
    };
    let reply = match from {
        None => Reply { ok: false, reason: Some("pedido sem remetente".into()) },
        Some(f) if f.device_id == state.self_id() => Reply { ok: false, reason: Some("este é o próprio dispositivo".into()) },
        Some(_) if throttled => Reply { ok: false, reason: Some("aguarde alguns segundos e peça de novo".into()) },
        Some(f) => {
            let name: String = f.name.chars().take(MAX_NAME).collect();
            log::info!("[PAIRING] Pairing requested by id={} name={name:?} from {peer_addr}", f.device_id);
            events::emit(
                app,
                events::PAIRING_REQUEST,
                serde_json::json!({ "deviceId": f.device_id, "name": name, "platform": f.platform }),
            );
            Reply { ok: true, reason: None }
        }
    };
    write_frame(stream, &serde_json::to_vec(&reply)?).await
}

/// Motivo da recusa recebido do outro lado. O fio leva o texto em pt (compatível com versões antigas);
/// os conhecidos são traduzidos aqui, o resto aparece como veio.
fn refusal_text(reason: Option<String>) -> String {
    use crate::i18n::tr;
    match reason.as_deref() {
        None => tr("err.pair_declined").into(),
        Some("pedido sem remetente") => tr("err.pair_no_sender").into(),
        Some("este é o próprio dispositivo") => tr("err.pair_self").into(),
        Some("aguarde alguns segundos e peça de novo") => tr("err.pair_throttled").into(),
        Some(other) => other.to_string(),
    }
}

/// Lado que pede: manda o pedido a um dispositivo visto na rede e espera o "recebido".
pub async fn send(app: &AppHandle, device_id: &str) -> Result<()> {
    let state = app.state::<AppState>();
    let addrs: Vec<SocketAddr> = state
        .peers
        .lock()
        .unwrap()
        .get(device_id)
        .map(|p| p.addrs.iter().map(|a| SocketAddr::new(*a, p.port)).collect())
        .unwrap_or_default();
    if addrs.is_empty() {
        return Err(AppError::Pairing(crate::i18n::tr("err.pair_left_network").into()));
    }
    let me = DeviceIdentity { name: state.display_name(), ..state.identity.read().unwrap().summary() };
    let hello = PreHello { v: PROTOCOL_VERSION, mode: MODE.into(), pairing_id: None, auth: None, from: Some(me) };
    let bytes = serde_json::to_vec(&hello)?;

    // Só repete se nenhum endereço aceitou o TCP: se o pedido chegou, o outro lado
    // recusa o mesmo IP por `MIN_INTERVAL_MS` e a nova rodada viraria "aguarde".
    let mut last = AppError::Pairing(crate::i18n::tr("err.pair_unreachable").into());
    for attempt in 1..=ATTEMPTS {
        if attempt > 1 {
            log::warn!("[PAIRING] Request attempt {}/{ATTEMPTS} failed: {last}", attempt - 1);
            tokio::time::sleep(Duration::from_millis(1500 * u64::from(attempt - 1))).await;
        }
        for addr in &addrs {
            let mut s = match tokio::time::timeout(TIMEOUT, TcpStream::connect(addr)).await {
                Ok(Ok(s)) => s,
                Ok(Err(e)) => {
                    last = e.into();
                    continue;
                }
                Err(_) => {
                    last = AppError::Pairing(crate::i18n::tr("err.pair_unreachable").into());
                    continue;
                }
            };
            let res = tokio::time::timeout(TIMEOUT, async {
                write_frame(&mut s, &bytes).await?;
                read_frame(&mut s).await
            })
            .await;
            return match res {
                Ok(Ok(raw)) => {
                    let r: Reply = serde_json::from_slice(&raw).map_err(|_| AppError::Pairing(crate::i18n::tr("err.pair_bad_reply").into()))?;
                    if r.ok { Ok(()) } else { Err(AppError::Pairing(refusal_text(r.reason))) }
                }
                Ok(Err(e)) => Err(e),
                // Versões antigas tratam o pedido como sessão e ficam esperando o handshake.
                Err(_) => Err(AppError::Pairing(crate::i18n::tr("err.pair_no_reply_update").into())),
            };
        }
    }
    Err(last)
}
