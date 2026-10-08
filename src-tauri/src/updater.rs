//! Atualização automática (só desktop). Endpoint e chave pública ficam em
//! `tauri.conf.json` → `plugins.updater`; o pacote baixado só é instalado se a
//! assinatura bater com essa chave. Sem chave pública configurada, nada é feito.
use crate::error::{AppError, Result};
use crate::events;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

const FIRST_CHECK: Duration = Duration::from_secs(15);
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    pub notes: Option<String>,
}

/// Última versão encontrada, para a UI que abrir depois do evento.
static PENDING: Mutex<Option<UpdateInfo>> = Mutex::new(None);
static INSTALLING: AtomicBool = AtomicBool::new(false);

fn configured(app: &AppHandle) -> bool {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|u| u.get("pubkey"))
        .and_then(|k| k.as_str())
        .is_some_and(|k| !k.trim().is_empty())
}

/// Verificação periódica em segundo plano; avisa a UI com `update:available`.
pub fn spawn(app: &AppHandle) {
    if !configured(app) {
        log::info!("[UPDATE] Sem chave pública em plugins.updater: atualização automática desligada");
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK).await;
        loop {
            if let Err(e) = check(&app).await {
                log::warn!("[UPDATE] Verificação falhou: {e}");
            }
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

pub async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>> {
    if !configured(app) {
        return Ok(None);
    }
    let update = app.updater().map_err(other)?.check().await.map_err(other)?;
    let info = update.map(|u| UpdateInfo { version: u.version, current_version: u.current_version, notes: u.body });
    let changed = {
        let mut p = PENDING.lock().unwrap();
        let changed = p.as_ref().map(|x| &x.version) != info.as_ref().map(|x| &x.version);
        p.clone_from(&info);
        changed
    };
    if let Some(i) = &info {
        if changed {
            log::info!("[UPDATE] Nova versão {} (atual {})", i.version, i.current_version);
            events::emit(app, events::UPDATE_AVAILABLE, i);
        }
    }
    Ok(info)
}

pub fn pending() -> Option<UpdateInfo> {
    PENDING.lock().unwrap().clone()
}

/// Baixa, confere a assinatura, instala e reinicia. No Windows o instalador fecha o app.
pub async fn install(app: &AppHandle) -> Result<()> {
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return Err(AppError::Other("atualização já em andamento".into()));
    }
    let res = async {
        let update = app
            .updater()
            .map_err(other)?
            .check()
            .await
            .map_err(other)?
            .ok_or_else(|| AppError::Other("nenhuma atualização disponível".into()))?;
        log::info!("[UPDATE] Instalando {}", update.version);
        update.download_and_install(|_, _| {}, || {}).await.map_err(other)
    }
    .await;
    INSTALLING.store(false, Ordering::SeqCst);
    res?;
    app.restart();
}

fn other(e: tauri_plugin_updater::Error) -> AppError {
    AppError::Other(e.to_string())
}
