//! Ponte JNI da transferência de arquivos para as APIs Android que só existem
//! em Kotlin: SAF/ContentResolver (`FileBridge`), MediaStore, espaço livre e o
//! Foreground Service temporário (`TransferBridge`/`TransferService`).
//! Nada de protocolo aqui — só chamadas de plataforma.
#![cfg(target_os = "android")]
use crate::android_jni::{jstring_to_string, with_class};
use crate::error::{AppError, Result};
use jni::objects::JValue;

const FILE_BRIDGE: &str = "app.clipsync.desktop.transfer.FileBridge";
const TRANSFER_BRIDGE: &str = "app.clipsync.desktop.transfer.TransferBridge";

#[derive(Debug, Clone, serde::Deserialize)]
pub struct UriInfo {
    pub name: String,
    /// -1 = desconhecido.
    pub size: i64,
    pub mime: Option<String>,
}

fn call_string_uri(method: &str, arg: &str) -> Result<String> {
    with_class(FILE_BRIDGE, |env, class, ctx| {
        let js = env.new_string(arg).map_err(|e| AppError::Other(e.to_string()))?;
        let out = env
            .call_static_method(class, method, "(Landroid/content/Context;Ljava/lang/String;)Ljava/lang/String;", &[JValue::Object(ctx), JValue::Object(&js)])
            .and_then(|v| v.l())
            .map_err(|e| AppError::Other(format!("FileBridge.{method}: {e}")))?;
        jstring_to_string(env, out)
    })
}

pub fn describe_uri(uri: &str) -> Result<UriInfo> {
    let json = call_string_uri("describe", uri)?;
    Ok(serde_json::from_str(&json)?)
}

/// `(fd, size)`; o fd já é nosso (`detachFd`).
pub fn open_uri(uri: &str) -> Result<(i32, i64)> {
    let pair: Vec<i64> = with_class(FILE_BRIDGE, |env, class, ctx| {
        let js = env.new_string(uri).map_err(|e| AppError::Other(e.to_string()))?;
        let out = env
            .call_static_method(class, "openRead", "(Landroid/content/Context;Ljava/lang/String;)[J", &[JValue::Object(ctx), JValue::Object(&js)])
            .and_then(|v| v.l())
            .map_err(|e| AppError::Other(format!("FileBridge.openRead: {e}")))?;
        let arr = jni::objects::JLongArray::from(out);
        let mut buf = [0i64; 2];
        env.get_long_array_region(&arr, 0, &mut buf).map_err(|e| AppError::Other(e.to_string()))?;
        Ok(buf.to_vec())
    })?;
    if pair[0] < 0 {
        return Err(AppError::Other("source unavailable: URI sem permissão ou arquivo removido".into()));
    }
    Ok((pair[0] as i32, pair[1]))
}

pub fn free_space(path: &str) -> Option<u64> {
    with_class(FILE_BRIDGE, |env, class, ctx| {
        let js = env.new_string(path).map_err(|e| AppError::Other(e.to_string()))?;
        let v = env
            .call_static_method(class, "freeSpace", "(Landroid/content/Context;Ljava/lang/String;)J", &[JValue::Object(ctx), JValue::Object(&js)])
            .and_then(|v| v.j())
            .map_err(|e| AppError::Other(format!("FileBridge.freeSpace: {e}")))?;
        Ok(v)
    })
    .ok()
    .filter(|v| *v >= 0)
    .map(|v| v as u64)
}

/// Publica o `.part` verificado em Downloads/ClipSync. Retorna `(uri, nome final)`.
pub fn publish_downloads(part: &str, name: &str, mime: Option<&str>) -> Result<(String, String)> {
    let json = with_class(FILE_BRIDGE, |env, class, ctx| {
        let jp = env.new_string(part).map_err(|e| AppError::Other(e.to_string()))?;
        let jn = env.new_string(name).map_err(|e| AppError::Other(e.to_string()))?;
        let jm = match mime {
            Some(m) => env.new_string(m).map_err(|e| AppError::Other(e.to_string()))?,
            None => jni::objects::JString::from(jni::objects::JObject::null()),
        };
        let out = env
            .call_static_method(
                class,
                "publishToDownloads",
                "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(ctx), JValue::Object(&jp), JValue::Object(&jn), JValue::Object(&jm)],
            )
            .and_then(|v| v.l())
            .map_err(|e| AppError::Other(format!("FileBridge.publishToDownloads: {e}")))?;
        jstring_to_string(env, out)
    })?;
    #[derive(serde::Deserialize)]
    struct Out {
        uri: String,
        name: String,
    }
    let o: Out = serde_json::from_str(&json)?;
    Ok((o.uri, o.name))
}

/// Remove linhas pendentes nossas no MediaStore (morte durante a publicação).
pub fn cleanup_pending() -> Result<i32> {
    with_class(FILE_BRIDGE, |env, class, ctx| {
        env.call_static_method(class, "cleanupPending", "(Landroid/content/Context;)I", &[JValue::Object(ctx)])
            .and_then(|v| v.i())
            .map_err(|e| AppError::Other(format!("FileBridge.cleanupPending: {e}")))
    })
}

/// Compartilhamento pendente (`ShareBridge`), consumido na leitura.
pub fn share_take() -> Result<Option<String>> {
    with_class("app.clipsync.desktop.share.ShareBridge", |env, class, _ctx| {
        let out = env
            .call_static_method(class, "take", "()Ljava/lang/String;", &[])
            .and_then(|v| v.l())
            .map_err(|e| AppError::Other(format!("ShareBridge.take: {e}")))?;
        if out.is_null() {
            return Ok(None);
        }
        jstring_to_string(env, out).map(Some)
    })
}

const NOTIFY_PERMISSION: &str = "app.clipsync.desktop.transfer.NotifyPermission";

/// `granted` | `prompt` | `blocked` | `unsupported` (ver `NotifyPermission.kt`).
pub fn notify_status() -> Result<String> {
    with_class(NOTIFY_PERMISSION, |env, class, ctx| {
        let out = env
            .call_static_method(class, "status", "(Landroid/content/Context;)Ljava/lang/String;", &[JValue::Object(ctx)])
            .and_then(|v| v.l())
            .map_err(|e| AppError::Other(format!("NotifyPermission.status: {e}")))?;
        jstring_to_string(env, out)
    })
}

pub fn notify_request() -> Result<bool> {
    with_class(NOTIFY_PERMISSION, |env, class, ctx| {
        env.call_static_method(class, "request", "(Landroid/content/Context;)Z", &[JValue::Object(ctx)])
            .and_then(|v| v.z())
            .map_err(|e| AppError::Other(format!("NotifyPermission.request: {e}")))
    })
}

pub fn notify_open_settings() -> Result<()> {
    with_class(NOTIFY_PERMISSION, |env, class, ctx| {
        env.call_static_method(class, "openSettings", "(Landroid/content/Context;)V", &[JValue::Object(ctx)])
            .map(|_| ())
            .map_err(|e| AppError::Other(format!("NotifyPermission.openSettings: {e}")))
    })
}

/// "Arquivo recebido" com Abrir (`ReceivedNotifier`). `false` = não notificou (app na frente
/// ou sem permissão).
pub fn notify_received(id: &str, title: &str, text: &str, uri: &str, mime: Option<&str>) -> Result<bool> {
    with_class("app.clipsync.desktop.transfer.ReceivedNotifier", |env, class, ctx| {
        let (ji, jt, jx, ju) = (jstr(env, id)?, jstr(env, title)?, jstr(env, text)?, jstr(env, uri)?);
        let jm = match mime {
            Some(m) => jstr(env, m)?,
            None => jni::objects::JString::from(jni::objects::JObject::null()),
        };
        env.call_static_method(
            class,
            "notify",
            "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Z",
            &[JValue::Object(ctx), JValue::Object(&ji), JValue::Object(&jt), JValue::Object(&jx), JValue::Object(&ju), JValue::Object(&jm)],
        )
        .and_then(|v| v.z())
        .map_err(|e| AppError::Other(format!("ReceivedNotifier.notify: {e}")))
    })
}

/// Abre o seletor SAF (precisa da Activity). O resultado vem por [`take_pick_result`].
pub fn pick_file() -> Result<bool> {
    with_class(FILE_BRIDGE, |env, class, ctx| {
        env.call_static_method(class, "pickFile", "(Landroid/content/Context;)Z", &[JValue::Object(ctx)])
            .and_then(|v| v.z())
            .map_err(|e| AppError::Other(format!("FileBridge.pickFile: {e}")))
    })
}

pub fn take_pick_result() -> Result<Option<String>> {
    with_class(FILE_BRIDGE, |env, class, _ctx| {
        let out = env
            .call_static_method(class, "takePickResult", "()Ljava/lang/String;", &[])
            .and_then(|v| v.l())
            .map_err(|e| AppError::Other(format!("FileBridge.takePickResult: {e}")))?;
        if out.is_null() {
            return Ok(None);
        }
        jstring_to_string(env, out).map(Some)
    })
}

// Foreground Service: só durante transferência ativa.

fn jstr<'a>(env: &mut jni::JNIEnv<'a>, s: &str) -> Result<jni::objects::JString<'a>> {
    env.new_string(s).map_err(|e| AppError::Other(e.to_string()))
}

/// `startForegroundService` (único caminho que exige `startForeground` depois). `false` =
/// o sistema recusou (app em segundo plano); a transferência segue sem FGS e
/// [`fgs_retry_if_needed`] tenta de novo quando a Activity volta.
pub fn fgs_start(title: &str, text: &str, percent: i32, id: &str) -> bool {
    let r = with_class(TRANSFER_BRIDGE, |env, class, ctx| {
        let (jt, jx, ji) = (jstr(env, title)?, jstr(env, text)?, jstr(env, id)?);
        env.call_static_method(
            class,
            "fgsStart",
            "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;ILjava/lang/String;)Z",
            &[JValue::Object(ctx), JValue::Object(&jt), JValue::Object(&jx), JValue::Int(percent), JValue::Object(&ji)],
        )
        .and_then(|v| v.z())
        .map_err(|e| AppError::Other(format!("TransferBridge.fgsStart: {e}")))
    });
    match r {
        Ok(ok) => ok,
        Err(e) => {
            log::warn!("[FILE] FGS não iniciou: {e}");
            false
        }
    }
}

/// Só a notificação (fala direto com a instância do serviço; nunca `startService`).
pub fn fgs_update(title: &str, text: &str, percent: i32, id: &str) {
    let _ = with_class(TRANSFER_BRIDGE, |env, class, _ctx| {
        let (jt, jx, ji) = (jstr(env, title)?, jstr(env, text)?, jstr(env, id)?);
        env.call_static_method(
            class,
            "fgsUpdate",
            "(Ljava/lang/String;Ljava/lang/String;ILjava/lang/String;)V",
            &[JValue::Object(&jt), JValue::Object(&jx), JValue::Int(percent), JValue::Object(&ji)],
        )
        .map_err(|e| AppError::Other(format!("TransferBridge.fgsUpdate: {e}")))?;
        Ok(())
    });
}

pub fn fgs_stop() {
    let _ = with_class(TRANSFER_BRIDGE, |env, class, _ctx| {
        env.call_static_method(class, "fgsStop", "()V", &[]).map_err(|e| AppError::Other(format!("TransferBridge.fgsStop: {e}")))?;
        Ok(())
    });
}

/// `startForeground()` aceito e em vigor.
pub fn fgs_running() -> bool {
    with_class(TRANSFER_BRIDGE, |env, class, _ctx| {
        env.call_static_method(class, "fgsRunning", "()Z", &[]).and_then(|v| v.z()).map_err(|e| AppError::Other(format!("TransferBridge.fgsRunning: {e}")))
    })
    .unwrap_or(false)
}

/// `MainActivity.onResume`: transferência ativa sem FGS → tenta subir agora (permitido em primeiro plano).
#[no_mangle]
pub extern "system" fn Java_app_clipsync_desktop_transfer_TransferBridge_nativeActivityResumed(_env: jni::JNIEnv, _class: jni::objects::JClass) {
    let Some(app) = crate::ime::app_handle() else { return };
    super::manager::fgs_retry_if_needed(app);
    crate::keepalive::apply(app);
}

/// "Compartilhar → ClipSync" chegou: a UI chama `share_take`.
#[no_mangle]
pub extern "system" fn Java_app_clipsync_desktop_share_ShareBridge_nativeShareReceived(_env: jni::JNIEnv, _class: jni::objects::JClass) {
    let Some(app) = crate::ime::app_handle() else { return };
    log::info!("[SHARE] Share received from another app");
    crate::events::emit(app, crate::events::SHARE_RECEIVED, serde_json::json!({}));
}

/// Botão "Cancelar" da notificação.
#[no_mangle]
pub extern "system" fn Java_app_clipsync_desktop_transfer_TransferBridge_nativeCancel(
    mut env: jni::JNIEnv,
    _class: jni::objects::JClass,
    id: jni::objects::JString,
) {
    let Ok(id) = env.get_string(&id) else { return };
    let id: String = id.into();
    let Some(app) = crate::ime::app_handle() else { return };
    if let Err(e) = super::manager::cancel(app, &id) {
        log::warn!("[FILE] cancel pela notificação falhou id={id}: {e}");
    }
}
