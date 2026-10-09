//! Aviso/instalação de versão nova.
//! - Desktop: `tauri-plugin-updater`. Endpoint e chave pública ficam em `tauri.conf.json` →
//!   `plugins.updater`; o pacote baixado só é instalado se a assinatura bater com essa chave.
//!   Sem chave pública configurada, nada é feito.
//! - Android: só avisa. Lê a última release pública do GitHub (HTTP no Kotlin, `UpdateBridge`) e,
//!   havendo versão maior com `.apk`, a UI abre a página da release no navegador.
//!
//! A verificação automática só roda com o ajuste `check_updates` ligado; "Procurar agora" sempre funciona.
use crate::error::{AppError, Result};
use crate::events;
use crate::state::AppState;
use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager};

/// Repositório das releases (APK e `latest.json`). Mantenha igual ao endpoint em `tauri.conf.json`.
pub const GITHUB_REPO: &str = "wallacy-1/ClipSync";

const FIRST_CHECK: Duration = Duration::from_secs(15);
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    pub notes: Option<String>,
    /// Página da release (Android; no Windows o pacote vem do próprio updater).
    pub url: Option<String>,
}

/// Última versão encontrada, para a UI que abrir depois do evento.
static PENDING: Mutex<Option<UpdateInfo>> = Mutex::new(None);

/// Verificação periódica em segundo plano (só com `check_updates` ligado); avisa a UI com `update:available`.
pub fn spawn(app: &AppHandle) {
    if !backend::enabled(app) {
        log::info!("[UPDATE] Atualização automática indisponível nesta build");
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK).await;
        loop {
            if app.state::<AppState>().settings().check_updates {
                if let Err(e) = check(&app).await {
                    log::warn!("[UPDATE] Verificação falhou: {e}");
                }
            } else {
                log::debug!("[UPDATE] Verificação automática desligada nos Ajustes");
            }
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

pub async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>> {
    if !backend::enabled(app) {
        return Ok(None);
    }
    let info = backend::fetch(app).await?;
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

/// Página da release pendente (Android).
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub fn pending_url() -> Option<String> {
    pending().and_then(|u| u.url)
}

/// `1.2.3` ou `v1.2.3` (só números). Pré-release (`-beta`), metadados (`+x`) e qualquer outro formato: `None`.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let v = v.trim();
    let v = v.strip_prefix('v').unwrap_or(v);
    let mut it = v.split('.');
    let mut next = || {
        let p = it.next()?;
        if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        p.parse::<u64>().ok()
    };
    let out = (next()?, next()?, next()?);
    if it.next().is_some() {
        return None;
    }
    Some(out)
}

/// `latest` é estritamente maior que `current`? Tag fora do padrão ou pré-release: nunca.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

/// Interpreta `releases/latest` do GitHub. `None` = sem atualização para Android
/// (tag inválida, versão não maior ou release sem `.apk`).
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn parse_release(body: &str, current: &str) -> Result<Option<UpdateInfo>> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|e| AppError::Other(format!("resposta inválida do GitHub: {e}")))?;
    let Some(tag) = v.get("tag_name").and_then(|t| t.as_str()) else {
        return Ok(None);
    };
    if !is_newer(tag, current) {
        return Ok(None);
    }
    let has_apk = v.get("assets").and_then(|a| a.as_array()).is_some_and(|a| {
        a.iter().any(|x| x.get("name").and_then(|n| n.as_str()).is_some_and(|n| n.to_ascii_lowercase().ends_with(".apk")))
    });
    if !has_apk {
        return Ok(None);
    }
    // O link vai para o navegador: só aceita páginas do github.com.
    let url = v
        .get("html_url")
        .and_then(|u| u.as_str())
        .filter(|u| u.starts_with("https://github.com/"))
        .map(str::to_owned)
        .unwrap_or_else(|| format!("https://github.com/{GITHUB_REPO}/releases/latest"));
    Ok(Some(UpdateInfo {
        version: tag.trim().trim_start_matches('v').to_owned(),
        current_version: current.to_owned(),
        notes: v.get("body").and_then(|b| b.as_str()).filter(|b| !b.trim().is_empty()).map(str::to_owned),
        url: Some(url),
    }))
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use backend::install;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod backend {
    use super::UpdateInfo;
    use crate::error::{AppError, Result};
    use std::sync::atomic::{AtomicBool, Ordering};
    use tauri::AppHandle;
    use tauri_plugin_updater::UpdaterExt;

    static INSTALLING: AtomicBool = AtomicBool::new(false);

    /// Sem chave pública em `plugins.updater` o updater fica desligado.
    pub fn enabled(app: &AppHandle) -> bool {
        app.config()
            .plugins
            .0
            .get("updater")
            .and_then(|u| u.get("pubkey"))
            .and_then(|k| k.as_str())
            .is_some_and(|k| !k.trim().is_empty())
    }

    pub async fn fetch(app: &AppHandle) -> Result<Option<UpdateInfo>> {
        let update = app.updater().map_err(other)?.check().await.map_err(other)?;
        Ok(update.map(|u| UpdateInfo { version: u.version, current_version: u.current_version, notes: u.body, url: None }))
    }

    /// Baixa, confere a assinatura, instala e reinicia. No Windows o instalador fecha o app.
    pub async fn install(app: &AppHandle) -> Result<()> {
        if INSTALLING.swap(true, Ordering::SeqCst) {
            return Err(AppError::Other(crate::i18n::tr("err.update_running").into()));
        }
        let res = async {
            let update = app
                .updater()
                .map_err(other)?
                .check()
                .await
                .map_err(other)?
                .ok_or_else(|| AppError::Other(crate::i18n::tr("err.update_none").into()))?;
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
}

#[cfg(target_os = "android")]
mod backend {
    use super::{parse_release, UpdateInfo, GITHUB_REPO};
    use crate::android_jni::{jstring_to_string, with_class};
    use crate::error::{AppError, Result};
    use jni::objects::JValue;
    use tauri::AppHandle;

    pub fn enabled(_app: &AppHandle) -> bool {
        true
    }

    pub async fn fetch(app: &AppHandle) -> Result<Option<UpdateInfo>> {
        let current = app.package_info().version.to_string();
        let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
        let body = tauri::async_runtime::spawn_blocking(move || latest_release(&url))
            .await
            .map_err(|e| AppError::Other(e.to_string()))??;
        parse_release(&body, &current)
    }

    /// `UpdateBridge.fetch(ctx, url)`: corpo do JSON ou exceção Java (rede, HTTP != 200).
    fn latest_release(url: &str) -> Result<String> {
        with_class("app.clipsync.desktop.update.UpdateBridge", |env, class, ctx| {
            let js = env.new_string(url).map_err(|e| AppError::Other(e.to_string()))?;
            let out = env
                .call_static_method(
                    class,
                    "fetch",
                    "(Landroid/content/Context;Ljava/lang/String;)Ljava/lang/String;",
                    &[JValue::Object(ctx), JValue::Object(&js)],
                )
                .and_then(|v| v.l())
                .map_err(|e| AppError::Other(format!("UpdateBridge.fetch: {e}")))?;
            jstring_to_string(env, out)
        })
    }
}

#[cfg(target_os = "ios")]
mod backend {
    use super::UpdateInfo;
    use crate::error::Result;
    use tauri::AppHandle;

    pub fn enabled(_app: &AppHandle) -> bool {
        false
    }

    pub async fn fetch(_app: &AppHandle) -> Result<Option<UpdateInfo>> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_numerically_not_lexically() {
        assert!(is_newer("v1.0.10", "1.0.9"));
        assert!(is_newer("1.1.0", "1.0.99"));
        assert!(is_newer("2.0.0", "1.99.99"));
        assert!(!is_newer("1.0.9", "1.0.10"));
    }

    #[test]
    fn equal_or_older_is_not_an_update() {
        assert!(!is_newer("v1.0.0", "1.0.0"));
        assert!(!is_newer("v1.0.0", "1.2.0"));
    }

    #[test]
    fn prerelease_and_odd_tags_are_ignored() {
        assert!(!is_newer("1.1.0-beta", "1.0.0"));
        assert!(!is_newer("v1.1.0-rc.1", "1.0.0"));
        assert!(!is_newer("release-3", "1.0.0"));
        assert!(!is_newer("1.1", "1.0.0"));
        assert!(!is_newer("1.1.0.1", "1.0.0"));
        assert!(!is_newer("1.1.x", "1.0.0"));
        assert!(!is_newer("2.0.0", "garbage"));
    }

    fn release(tag: &str, assets: &[&str]) -> String {
        let assets: Vec<_> = assets.iter().map(|n| serde_json::json!({ "name": n })).collect();
        serde_json::json!({ "tag_name": tag, "html_url": format!("https://github.com/{GITHUB_REPO}/releases/tag/{tag}"), "body": "Notas", "assets": assets })
            .to_string()
    }

    #[test]
    fn release_with_apk_and_newer_version_is_offered() {
        let info = parse_release(&release("v1.0.1", &["ClipSync_1.0.1.apk", "latest.json"]), "1.0.0").unwrap().unwrap();
        assert_eq!(info.version, "1.0.1");
        assert_eq!(info.current_version, "1.0.0");
        assert_eq!(info.notes.as_deref(), Some("Notas"));
        assert_eq!(info.url.as_deref(), Some("https://github.com/wallacy-1/ClipSync/releases/tag/v1.0.1"));
    }

    #[test]
    fn release_without_apk_or_newer_version_is_ignored() {
        assert!(parse_release(&release("v1.0.1", &["setup.exe"]), "1.0.0").unwrap().is_none());
        assert!(parse_release(&release("v1.0.0", &["a.apk"]), "1.0.0").unwrap().is_none());
        assert!(parse_release(&release("v0.9.0", &["a.apk"]), "1.0.0").unwrap().is_none());
        assert!(parse_release("{}", "1.0.0").unwrap().is_none());
        assert!(parse_release("não é json", "1.0.0").is_err());
    }

    #[test]
    fn foreign_html_url_falls_back_to_releases_page() {
        let body = serde_json::json!({ "tag_name": "v2.0.0", "html_url": "https://evil.example/x", "assets": [{ "name": "a.apk" }] }).to_string();
        let info = parse_release(&body, "1.0.0").unwrap().unwrap();
        assert_eq!(info.url.as_deref(), Some("https://github.com/wallacy-1/ClipSync/releases/latest"));
    }
}
