//! Imagem copiada não vira item de histórico: vira PNG em disco privado e segue pela
//! transferência de arquivos (`FileOffer.purpose = "clipboard"`). Quem recebe põe no clipboard.
//!
//! ```text
//! cópia ─▶ poll (sem texto) ─▶ PNG em <app_data>/clip-images ─▶ send_file_with(purpose)
//! FileOffer{purpose} ─▶ receiver ─▶ <app_data>/clip-images ─▶ on_received ─▶ clipboard
//! ```
//!
//! Anti-loop: Windows ignora o número de sequência da própria escrita; Android
//! marca a própria escrita com o rótulo "ClipSync" (`ClipImageBridge`).
#[cfg(not(windows))]
use crate::error::AppError;
use crate::error::Result;
use crate::models::now_ms;
use crate::state::AppState;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Manager};

pub const PURPOSE: &str = "clipboard";
/// PNG maior que isso não é enviado (print 4K fica em ~5–15 MB).
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;
/// Imagens enviadas/recebidas ficam em disco este tempo (retomada, reabrir).
const KEEP_FOR: Duration = Duration::from_secs(3 * 24 * 3600);
const SUBDIR: &str = "clip-images";

pub fn is_clipboard(purpose: Option<&str>) -> bool {
    purpose == Some(PURPOSE)
}

/// Dentro do diretório do perfil (`AppState::app_dir`), não da raiz do app.
pub fn dir(app: &AppHandle) -> Result<PathBuf> {
    let dir = app.state::<AppState>().app_dir.join(SUBDIR);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn new_path(app: &AppHandle) -> Result<PathBuf> {
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let short = &uuid::Uuid::new_v4().simple().to_string()[..6];
    Ok(dir(app)?.join(format!("imagem-{ts}-{short}.png")))
}

/// Chave de dedupe/anti-loop no mesmo espaço dos hashes de texto (Android: carimbo do clip).
#[cfg(target_os = "android")]
fn key(raw: &str) -> String {
    format!("img:{}", crate::db::content_hash(raw))
}

/// Watcher subiu: o que já estava no clipboard não é enviado.
pub fn prime(app: &AppHandle) {
    #[cfg(target_os = "android")]
    if let Some(p) = android::peek() {
        app.state::<AppState>().clip_dedupe.lock().unwrap().prime(&key(&p.stamp));
    }
    #[cfg(not(target_os = "android"))]
    let _ = app;
}

/// Chamado pelo polling quando o clipboard não tem texto. Barato quando nada mudou;
/// a conversão para PNG roda fora da thread do watcher.
pub fn poll(app: &AppHandle) {
    if !wanted(app) {
        return;
    }
    #[cfg(target_os = "android")]
    {
        let Some(p) = android::peek() else { return };
        let k = key(&p.stamp);
        if !claim(app, &k) {
            return;
        }
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let Ok(path) = new_path(&app) else { return };
            match android::copy_as_png(&path) {
                Some(size) => on_local_image(&app, &path, size),
                None => log::warn!("[CLIPBOARD] imagem do clipboard ilegível (mime={:?})", p.mime),
            }
        });
    }
    #[cfg(windows)]
    {
        if windows::is_own_write() {
            return;
        }
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let Some(img) = windows::read(&app) else { return };
            let k = key_rgba(&img.rgba);
            if !claim(&app, &k) {
                return;
            }
            let Ok(path) = new_path(&app) else { return };
            match windows::encode_png(&img, &path) {
                Ok(size) => on_local_image(&app, &path, size),
                Err(e) => log::warn!("[CLIPBOARD] falha ao gerar PNG: {e}"),
            }
        });
    }
}

#[cfg(windows)]
fn key_rgba(rgba: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("img:{}", hex::encode(Sha256::digest(rgba)))
}

/// Envio automático ligado para imagens (mesmas regras do texto).
fn wanted(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    let s = state.settings();
    s.auto_send && !state.is_paused() && s.synced_types.iter().any(|t| t == "image")
}

/// Mesma memória do texto: só processa uma vez cada conteúdo e nunca o que o ClipSync escreveu.
fn claim(app: &AppHandle, k: &str) -> bool {
    let state = app.state::<AppState>();
    let mut d = state.clip_dedupe.lock().unwrap();
    if !d.poll_changed(k) {
        return false;
    }
    matches!(d.decide(k, now_ms()), super::dedupe::Decision::New)
}

fn on_local_image(app: &AppHandle, path: &Path, size: u64) {
    if size == 0 || size > MAX_BYTES {
        log::info!("[CLIPBOARD] imagem ignorada size={size} (limite {MAX_BYTES})");
        let _ = std::fs::remove_file(path);
        return;
    }
    let state = app.state::<AppState>();
    let targets: Vec<String> = state
        .db
        .list_devices()
        .unwrap_or_default()
        .into_iter()
        .filter(|d| d.auto_send && !d.blocked)
        .map(|d| d.device_id)
        .collect();
    log::info!("[CLIPBOARD] Image detected size={size} targets={}", targets.len());
    let src = path.to_string_lossy();
    for dev in targets {
        supersede(app, &dev);
        if let Err(e) = crate::transfer::manager::send_file_with(app, &dev, &src, Some(PURPOSE)) {
            log::warn!("[CLIPBOARD] imagem não enfileirada para {dev}: {e}");
        }
    }
}

/// Só a imagem copiada mais recente interessa: as anteriores ainda abertas para o
/// device (na fila de um offline, ou em envio) são canceladas. Sem isso um device
/// offline acumula imagens até o limite de transferências abertas.
fn supersede(app: &AppHandle, device_id: &str) {
    let open = app.state::<AppState>().db.transfer_open_for_device(device_id).unwrap_or_default();
    for t in open.iter().filter(|t| t.direction == crate::transfer::state::Direction::Out && is_clipboard(t.purpose.as_deref())) {
        log::info!("[CLIPBOARD] Superseded clipboard image id={} to={device_id}", t.id);
        let _ = crate::transfer::manager::cancel(app, &t.id);
    }
}

/// Imagem recebida e verificada (já em [`dir`]): aplica no clipboard se as regras deixam.
pub fn on_received(app: &AppHandle, device_id: &str, path: &Path) {
    let state = app.state::<AppState>();
    let auto_dev = state.db.get_device(device_id).ok().flatten().map(|d| d.auto_receive && !d.blocked).unwrap_or(false);
    let apply = auto_dev && state.settings().auto_receive && !state.is_paused();
    log::info!("[CLIPBOARD] Image received from={device_id} apply_clipboard={apply}");
    if !apply {
        return;
    }
    let path = path.to_path_buf();
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(e) = write_local(&app, &path) {
            log::warn!("[CLIPBOARD] falha ao pôr a imagem no clipboard: {e}");
        }
    });
}

fn write_local(app: &AppHandle, path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        let img = windows::decode_png(path)?;
        app.state::<AppState>().clip_dedupe.lock().unwrap().note_written(&key_rgba(&img.rgba));
        windows::write(app, &img)
    }
    #[cfg(target_os = "android")]
    {
        let _ = app;
        if android::set_image(path) {
            Ok(())
        } else {
            Err(AppError::Other("ClipImageBridge.setImage falhou".into()))
        }
    }
    #[cfg(not(any(windows, target_os = "android")))]
    {
        let _ = (app, path);
        Err(AppError::Other("imagem no clipboard indisponível nesta plataforma".into()))
    }
}

/// Subida: apaga imagens antigas que nenhuma transferência aberta usa mais.
pub fn purge_old(app: &AppHandle) {
    let Ok(dir) = dir(app) else { return };
    let open: Vec<String> = app
        .state::<AppState>()
        .db
        .transfer_list(10_000)
        .unwrap_or_default()
        .into_iter()
        .filter(|t| !t.state.is_terminal())
        .filter_map(|t| t.source)
        .collect();
    let Ok(rd) = std::fs::read_dir(&dir) else { return };
    let mut n = 0;
    for e in rd.flatten() {
        let p = e.path();
        let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).map(|age| age > KEEP_FOR).unwrap_or(false);
        if old && !open.iter().any(|s| Path::new(s) == p) && std::fs::remove_file(&p).is_ok() {
            n += 1;
        }
    }
    if n > 0 {
        log::info!("[CLIPBOARD] Removed {n} old clipboard image(s)");
    }
}

#[cfg(windows)]
mod windows {
    use crate::error::{AppError, Result};
    use std::path::Path;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tauri::AppHandle;
    use tauri_plugin_clipboard_manager::ClipboardExt;

    /// Número de sequência do clipboard logo após a nossa última escrita de imagem.
    static OWN_SEQ: AtomicU32 = AtomicU32::new(0);

    pub struct Rgba {
        pub rgba: Vec<u8>,
        pub width: u32,
        pub height: u32,
    }

    fn seq() -> u32 {
        // SAFETY: função sem argumentos que só lê um contador do sistema.
        unsafe { windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber() }
    }

    pub fn is_own_write() -> bool {
        let own = OWN_SEQ.load(Ordering::Relaxed);
        own != 0 && own == seq()
    }

    pub fn read(app: &AppHandle) -> Option<Rgba> {
        let img = app.clipboard().read_image().ok()?;
        Some(Rgba { rgba: img.rgba().to_vec(), width: img.width(), height: img.height() })
    }

    pub fn write(app: &AppHandle, img: &Rgba) -> Result<()> {
        let image = tauri::image::Image::new(&img.rgba, img.width, img.height);
        app.clipboard().write_image(&image).map_err(|e| AppError::Other(e.to_string()))?;
        OWN_SEQ.store(seq(), Ordering::Relaxed);
        Ok(())
    }

    pub fn encode_png(img: &Rgba, path: &Path) -> Result<u64> {
        let file = std::fs::File::create(path)?;
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), img.width, img.height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut w = enc.write_header().map_err(|e| AppError::Other(e.to_string()))?;
        w.write_image_data(&img.rgba).map_err(|e| AppError::Other(e.to_string()))?;
        w.finish().map_err(|e| AppError::Other(e.to_string()))?;
        Ok(std::fs::metadata(path)?.len())
    }

    /// Decodifica qualquer PNG (paleta, cinza, 16 bits) para RGBA 8 bits.
    pub fn decode_png(path: &Path) -> Result<Rgba> {
        let mut dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path)?));
        dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16 | png::Transformations::ALPHA);
        let mut reader = dec.read_info().map_err(|e| AppError::Other(format!("PNG inválido: {e}")))?;
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).map_err(|e| AppError::Other(format!("PNG inválido: {e}")))?;
        buf.truncate(info.buffer_size());
        let (w, h) = (info.width, info.height);
        let rgba = match info.color_type {
            png::ColorType::Rgba => buf,
            png::ColorType::GrayscaleAlpha => buf.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
            other => return Err(AppError::Other(format!("PNG com cor não suportada: {other:?}"))),
        };
        Ok(Rgba { rgba, width: w, height: h })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn png_round_trip() {
            let img = Rgba { rgba: (0..4 * 3 * 2).map(|i| i as u8).collect(), width: 3, height: 2 };
            let p = std::env::temp_dir().join(format!("clipsync-img-{}.png", uuid::Uuid::new_v4()));
            let size = encode_png(&img, &p).unwrap();
            assert!(size > 0);
            let back = decode_png(&p).unwrap();
            let _ = std::fs::remove_file(&p);
            assert_eq!((back.width, back.height), (3, 2));
            assert_eq!(back.rgba, img.rgba);
        }
    }
}

#[cfg(target_os = "android")]
mod android {
    use crate::android_jni::{jstring_to_string, with_class};
    use crate::error::AppError;
    use jni::objects::JValue;
    use std::path::Path;

    const BRIDGE: &str = "app.clipsync.desktop.clip.ClipImageBridge";

    #[derive(serde::Deserialize)]
    pub struct Peek {
        pub stamp: String,
        pub mime: Option<String>,
    }

    pub fn peek() -> Option<Peek> {
        let json = with_class(BRIDGE, |env, class, ctx| {
            let out = env
                .call_static_method(class, "peek", "(Landroid/content/Context;)Ljava/lang/String;", &[JValue::Object(ctx)])
                .and_then(|v| v.l())
                .map_err(|e| AppError::Other(format!("ClipImageBridge.peek: {e}")))?;
            if out.is_null() {
                return Ok(None);
            }
            jstring_to_string(env, out).map(Some)
        })
        .ok()??;
        serde_json::from_str(&json).ok()
    }

    pub fn copy_as_png(dest: &Path) -> Option<u64> {
        let n = with_class(BRIDGE, |env, class, ctx| {
            let jp = env.new_string(dest.to_string_lossy()).map_err(|e| AppError::Other(e.to_string()))?;
            env.call_static_method(class, "copyAsPng", "(Landroid/content/Context;Ljava/lang/String;)J", &[JValue::Object(ctx), JValue::Object(&jp)])
                .and_then(|v| v.j())
                .map_err(|e| AppError::Other(format!("ClipImageBridge.copyAsPng: {e}")))
        })
        .ok()?;
        (n >= 0).then_some(n as u64)
    }

    pub fn set_image(path: &Path) -> bool {
        with_class(BRIDGE, |env, class, ctx| {
            let jp = env.new_string(path.to_string_lossy()).map_err(|e| AppError::Other(e.to_string()))?;
            env.call_static_method(class, "setImage", "(Landroid/content/Context;Ljava/lang/String;)Z", &[JValue::Object(ctx), JValue::Object(&jp)])
                .and_then(|v| v.z())
                .map_err(|e| AppError::Other(format!("ClipImageBridge.setImage: {e}")))
        })
        .unwrap_or(false)
    }
}
