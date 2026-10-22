//! Atalhos de envio do Android: bloco de Ajustes rápidos, widget e "Selecionar texto → ClipSync"
//! (`ClipSendActivity`). Aqui ficam o envio explícito (`CoreBridge.nativeSendNow`), "Copiar último"
//! e o resumo de status que o Rust empurra para o Kotlin (`StatusStore`). Fora do Android o status é no-op.
use crate::clipboard::{self, MAX_TEXT_BYTES};
use crate::content::ContentType;
use crate::db::content_hash;
use crate::error::{AppError, Result};
use crate::events;
use crate::models::{now_ms, ClipboardItem};
use crate::state::AppState;
use serde_json::{json, Value};
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use tauri::{AppHandle, Manager};

/// Rajadas de eventos (cópia + sync + dispositivo) viram uma atualização só.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
const DEBOUNCE: Duration = Duration::from_millis(500);
/// Caracteres da prévia do último item no widget.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
const PREVIEW_CHARS: usize = 120;
/// Tempo máximo esperando uma reconexão que o `wake` acabou de disparar, antes de enfileirar.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
const CONNECT_WAIT: Duration = Duration::from_millis(1500);

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
static PENDING: AtomicBool = AtomicBool::new(false);

/// Chamado por `events::emit`: só os eventos que mudam o que o bloco/widget mostram.
pub fn on_event(app: &AppHandle, name: &str) {
    if matches!(
        name,
        events::CLIPBOARD_CHANGED
            | events::CLIPBOARD_RECEIVED
            | events::CLIPBOARD_EXPIRED
            | events::DEVICE_UPDATED
            | events::SYNC_STATE
            | events::SETTINGS_CHANGED
    ) {
        refresh(app);
    }
}

/// Agenda o envio do resumo de status ao Kotlin (debounce de 500 ms). No-op fora do Android.
pub fn refresh(app: &AppHandle) {
    #[cfg(target_os = "android")]
    {
        use std::sync::atomic::Ordering;
        if PENDING.swap(true, Ordering::AcqRel) {
            return;
        }
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(DEBOUNCE).await;
            PENDING.store(false, Ordering::Release);
            let _ = tauri::async_runtime::spawn_blocking(move || {
                if let Err(e) = platform::push(&status_json(&app)) {
                    log::debug!("[QUICK] status não enviado ao bloco/widget: {e}");
                }
            })
            .await;
        });
    }
    #[cfg(not(target_os = "android"))]
    let _ = app;
}

/// Primeira linha útil, numa linha só, limitada a `PREVIEW_CHARS`.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn one_line_preview(content: &str) -> String {
    let joined = content.split_whitespace().collect::<Vec<_>>().join(" ");
    joined.chars().take(PREVIEW_CHARS).collect()
}

/// Prévia do item: imagem não mostra o hash; sensível com "ocultar" liga vira vazio (o Kotlin avisa).
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn preview_of(item: &ClipboardItem, hide_sensitive: bool) -> String {
    if item.content_type == ContentType::Image {
        // O Kotlin mostra "Imagem"/"Image" pelo `type` (strings.xml), no idioma do sistema.
        String::new()
    } else if item.is_sensitive && hide_sensitive {
        String::new()
    } else {
        one_line_preview(&item.content)
    }
}

/// `{ paired, connected, paused, pausedUntil, last, updatedAt }`.
/// `last.local` diz se o item nasceu neste aparelho (o Kotlin escreve "de PC" só quando não).
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn status_json(app: &AppHandle) -> Value {
    let state = app.state::<AppState>();
    let settings = state.settings();
    let paired: Vec<_> = state.db.list_devices().unwrap_or_default().into_iter().filter(|d| !d.blocked).collect();
    let connected = paired.iter().filter(|d| state.is_connected(&d.device_id)).count();
    let status = state.sync_status();
    let last = state.db.latest_item().ok().flatten().map(|it| {
        let local = it.source_device_id == state.self_id();
        let from = paired.iter().find(|d| d.device_id == it.source_device_id).map(|d| d.name.clone()).unwrap_or_default();
        json!({
            "preview": preview_of(&it, settings.hide_sensitive),
            "type": it.content_type.as_str(),
            "sensitive": it.is_sensitive,
            "from": from,
            "local": local,
            "at": it.created_at,
        })
    });
    json!({
        "paired": paired.len(),
        "connected": connected,
        "paused": status.paused,
        "pausedUntil": status.paused_until,
        "last": last,
        "updatedAt": now_ms(),
    })
}

/// Envio explícito do texto para todos os pareados não bloqueados (ignora auto_send, tipos e pausa).
/// Retorna `{ok, sent, queued, names, queuedNames}` ou `{ok:false, error}`.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub fn send_now(app: &AppHandle, text: String, sensitive: bool) -> Value {
    match try_send_now(app, text, sensitive) {
        Ok(v) => v,
        Err(e) => json!({ "ok": false, "error": e.to_string() }),
    }
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn try_send_now(app: &AppHandle, text: String, sensitive: bool) -> Result<Value> {
    let state = app.state::<AppState>();
    let targets: Vec<_> = state.db.list_devices()?.into_iter().filter(|d| !d.blocked).collect();
    if targets.is_empty() {
        return Ok(json!({ "ok": true, "sent": 0, "queued": 0, "names": [], "queuedNames": [] }));
    }
    if text.trim().is_empty() || text.len() > MAX_TEXT_BYTES {
        return Err(AppError::Other(crate::i18n::tr("err.text_too_big_1mb").into()));
    }
    // O Android pode ter derrubado a rede em segundo plano: reconecta e espera um pouco
    // por quem tem endereço conhecido (no pior caso, PC desligado, o aviso de "na fila" atrasa 1,5 s).
    crate::transport::wake(app);
    let deadline = std::time::Instant::now() + CONNECT_WAIT;
    while std::time::Instant::now() < deadline {
        // Logo após o `wake` o peer pode ainda não estar em `connecting`: endereço conhecido também conta.
        let reachable = |d: &crate::db::StoredDevice| {
            d.last_address.is_some()
                || state.peers.lock().unwrap().contains_key(&d.device_id)
                || state.connecting.lock().unwrap().contains(&d.device_id)
        };
        if !targets.iter().any(|d| !state.is_connected(&d.device_id) && reachable(d)) {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    // Marca o hash como visto antes de criar o item: o poll da MainActivity/IME não duplica.
    let hash = content_hash(&text);
    let _ = state.clip_dedupe.lock().unwrap().decide(&hash, now_ms());
    let latest = state.db.latest_item()?.filter(|i| i.content_type != ContentType::Image && content_hash(&i.content) == hash);
    let item_id = match latest {
        Some(i) => i.id,
        None => clipboard::ingest_shared_text(app, text, sensitive)?.id,
    };

    let (online, offline): (Vec<_>, Vec<_>) = targets.iter().partition(|d| state.is_connected(&d.device_id));
    let ids: Vec<String> = targets.iter().map(|d| d.device_id.clone()).collect();
    crate::sync::send_item(app, &item_id, &ids)?;
    let names = |v: &[&crate::db::StoredDevice]| v.iter().map(|d| d.name.clone()).collect::<Vec<_>>();
    Ok(json!({
        "ok": true,
        "sent": online.len(),
        "queued": offline.len(),
        "names": names(&online),
        "queuedNames": names(&offline),
    }))
}

/// "Copiar último" do widget: põe o item mais recente do histórico no clipboard (sensível também).
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub fn copy_latest(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    match state.db.latest_item() {
        Ok(Some(item)) => clipboard::copy_item(app, &item).is_ok(),
        _ => false,
    }
}

/// Pede ao Android para adicionar o bloco "Enviar clipboard" (API 33+): `requested` | `unsupported`.
#[tauri::command]
pub fn quick_add_tile() -> Result<String> {
    platform::add("addTile")
}

/// Pede ao launcher para fixar o widget na tela inicial: `requested` | `unsupported`.
#[tauri::command]
pub fn quick_add_widget() -> Result<String> {
    platform::add("addWidget")
}

#[cfg(target_os = "android")]
mod platform {
    use crate::android_jni::{jstring_to_string, with_class};
    use crate::error::{AppError, Result};
    use jni::objects::JValue;

    pub fn push(status: &serde_json::Value) -> Result<()> {
        with_class("app.clipsync.desktop.quick.StatusStore", |env, class, ctx| {
            let js = env.new_string(status.to_string()).map_err(|e| AppError::Other(e.to_string()))?;
            env.call_static_method(
                class,
                "update",
                "(Landroid/content/Context;Ljava/lang/String;)V",
                &[JValue::Object(ctx), JValue::Object(&js)],
            )
            .map_err(|e| AppError::Other(format!("StatusStore.update: {e}")))?;
            Ok(())
        })
    }

    pub fn add(method: &str) -> Result<String> {
        with_class("app.clipsync.desktop.quick.QuickAdd", |env, class, ctx| {
            let out = env
                .call_static_method(class, method, "(Landroid/content/Context;)Ljava/lang/String;", &[JValue::Object(ctx)])
                .and_then(|v| v.l())
                .map_err(|e| AppError::Other(format!("QuickAdd.{method}: {e}")))?;
            jstring_to_string(env, out)
        })
    }
}

#[cfg(not(target_os = "android"))]
mod platform {
    use crate::error::Result;

    pub fn add(_method: &str) -> Result<String> {
        Ok("unsupported".into())
    }
}

#[cfg(target_os = "android")]
mod bridge {
    use crate::ime::app_handle;
    use jni::objects::{JClass, JString};
    use jni::sys::{jboolean, jstring, JNI_FALSE, JNI_TRUE};
    use jni::JNIEnv;
    use serde_json::json;

    fn to_jstring(env: &mut JNIEnv, v: &serde_json::Value) -> jstring {
        env.new_string(v.to_string()).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
    }

    /// `CoreBridge.nativeSendNow(text, sensitive): String` — JSON de `send_now`. Chamado fora da thread principal.
    #[no_mangle]
    pub extern "system" fn Java_app_clipsync_desktop_ime_CoreBridge_nativeSendNow(
        mut env: JNIEnv,
        _class: JClass,
        text: JString,
        sensitive: jboolean,
    ) -> jstring {
        let out = match (app_handle(), env.get_string(&text)) {
            (Some(app), Ok(text)) => {
                let text: String = text.into();
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| super::send_now(app, text, sensitive != JNI_FALSE))).unwrap_or_else(|_| {
                    log::error!("[QUICK] send_now entrou em pânico");
                    json!({ "ok": false, "error": crate::i18n::tr("err.internal") })
                })
            }
            (None, _) => json!({ "ok": false, "error": crate::i18n::tr("err.core_not_started") }),
            (_, Err(_)) => json!({ "ok": false, "error": crate::i18n::tr("err.bad_text") }),
        };
        to_jstring(&mut env, &out)
    }

    /// `CoreBridge.nativeCopyLatest(): Boolean` — `false` = nada copiado.
    #[no_mangle]
    pub extern "system" fn Java_app_clipsync_desktop_ime_CoreBridge_nativeCopyLatest(_env: JNIEnv, _class: JClass) -> jboolean {
        let Some(app) = app_handle() else { return JNI_FALSE };
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| super::copy_latest(app))) {
            Ok(true) => JNI_TRUE,
            Ok(false) => JNI_FALSE,
            Err(_) => {
                log::error!("[QUICK] copy_latest entrou em pânico");
                JNI_FALSE
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(content: &str, ty: ContentType, sensitive: bool) -> ClipboardItem {
        ClipboardItem {
            id: "1".into(),
            content: content.into(),
            content_type: ty,
            source_device_id: "d".into(),
            created_at: 0,
            expires_at: None,
            is_pinned: false,
            is_sensitive: sensitive,
            sync_state: crate::models::SyncState::Local,
            metadata: json!({}),
        }
    }

    #[test]
    fn preview_is_one_line_and_capped() {
        let long = format!("a\n\n  b\t{}", "x".repeat(300));
        let p = preview_of(&item(&long, ContentType::Text, false), true);
        assert!(p.starts_with("a b x"));
        assert_eq!(p.chars().count(), PREVIEW_CHARS);
    }

    #[test]
    fn preview_hides_sensitive_and_images() {
        assert_eq!(preview_of(&item("senha123", ContentType::Token, true), true), "");
        assert_eq!(preview_of(&item("senha123", ContentType::Token, true), false), "senha123");
        assert_eq!(preview_of(&item(&format!("img:{}", "0".repeat(64)), ContentType::Image, false), true), "");
    }
}
