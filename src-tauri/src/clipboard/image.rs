//! Imagem copiada vira PNG em disco privado + item de histórico (`ContentType::Image`) e segue
//! pela transferência de arquivos (`FileOffer.purpose = "clipboard"`), nunca por `Message::Clip`.
//! Quem recebe registra o item e, se as regras deixam, põe no clipboard.
//! O item guarda `content = "img:<sha256 do PNG>"` (igual nos dois lados, dá o dedupe) e
//! `metadata = {path, thumb, width, height, bytes, mime}`. A UI só lê a imagem por `item_image`.
//!
//! Anti-loop: Windows ignora o número de sequência da própria escrita; Android
//! marca a própria escrita com o rótulo "ClipSync" (`ClipImageBridge`).
use crate::content::ContentType;
use crate::error::{AppError, Result};
use crate::events;
use crate::models::{now_ms, ClipboardItem, SyncState};
use crate::state::AppState;
use crate::transport::protocol::CAP_CLIP_IMAGE_V1;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Manager};

pub const PURPOSE: &str = "clipboard";
/// PNG maior que isso não é enviado (print 4K fica em ~5–15 MB).
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;
/// Arquivo sem item de histórico (transferência antiga) fica em disco este tempo (retomada, reabrir).
const KEEP_FOR: Duration = Duration::from_secs(3 * 24 * 3600);
const SUBDIR: &str = "clip-images";
/// Imagens não fixadas no histórico; ao passar disso a mais antiga sai (item e arquivos).
pub const MAX_IMAGE_ITEMS: usize = 50;
const THUMB_MAX: u32 = 320;
/// A UI só pede a imagem inteira abaixo disso (data URL pelo IPC); acima, usa a miniatura.
const FULL_MAX_BYTES: u64 = 4 * 1024 * 1024;

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
    Ok(dir(app)?.join(format!("{}-{ts}-{short}.png", crate::i18n::tr("file.image"))))
}

/// Chave de dedupe/anti-loop no mesmo espaço dos hashes de texto (Android: carimbo do clip).
#[cfg(target_os = "android")]
fn key(raw: &str) -> String {
    format!("img:{}", crate::db::content_hash(raw))
}

/// Watcher subiu: o que já estava no clipboard não vira item nem é enviado.
pub fn prime(app: &AppHandle) {
    #[cfg(target_os = "android")]
    if let Some(p) = android::peek() {
        app.state::<AppState>().clip_dedupe.lock().unwrap().prime(&key(&p.stamp));
    }
    #[cfg(windows)]
    if let Some(img) = windows::read(app) {
        app.state::<AppState>().clip_dedupe.lock().unwrap().prime(&key_rgba(&img.rgba));
    }
    #[cfg(not(any(windows, target_os = "android")))]
    let _ = app;
}

/// Chamado pelo polling quando o clipboard não tem texto. Barato quando nada mudou;
/// a conversão para PNG roda fora da thread do watcher.
pub fn poll(app: &AppHandle) {
    // Sem checar `send_wanted`: a captura no histórico acontece sempre; o envio é que obedece às regras.
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

/// Imagem que o teclado guardou em PNG enquanto o núcleo não existia (`ImeInbox`). `stamp` é o
/// mesmo carimbo de `peek`, então a cópia que o polling também vir é descartada pelo dedupe.
/// Sem `poll_changed` como filtro: o `prime` da subida já marcou esse carimbo como visto.
/// Bloqueante (hash e miniatura); chamar fora da thread principal.
#[cfg(target_os = "android")]
pub fn ingest_inbox_png(app: &AppHandle, src: &Path, stamp: &str) {
    let k = key(stamp);
    let fresh = {
        let state = app.state::<AppState>();
        let mut d = state.clip_dedupe.lock().unwrap();
        d.poll_changed(&k);
        matches!(d.decide(&k, now_ms()), super::dedupe::Decision::New)
    };
    if !fresh {
        return;
    }
    let Ok(path) = new_path(app) else { return };
    if std::fs::rename(src, &path).is_err() && std::fs::copy(src, &path).is_err() {
        log::warn!("[CLIPBOARD] imagem do inbox do teclado ilegível");
        return;
    }
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    on_local_image(app, &path, size);
}

#[cfg(windows)]
fn key_rgba(rgba: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("img:{}", hex::encode(Sha256::digest(rgba)))
}

fn send_wanted(app: &AppHandle) -> bool {
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
    let Some(item) = register(app, path, size, None) else { return };
    events::emit(app, events::CLIPBOARD_CHANGED, &item);
    let targets: Vec<String> = if send_wanted(app) {
        state
            .db
            .list_devices()
            .unwrap_or_default()
            .into_iter()
            .filter(|d| d.auto_send && !d.blocked)
            .map(|d| d.device_id)
            .collect()
    } else {
        Vec::new()
    };
    log::info!("[CLIPBOARD] Image detected id={} size={size} targets={}", item.id, targets.len());
    let src = path.to_string_lossy();
    for dev in targets {
        supersede(app, &dev);
        if let Err(e) = crate::transfer::manager::send_file_with(app, &dev, &src, Some(PURPOSE)) {
            log::warn!("[CLIPBOARD] imagem não enfileirada para {dev}: {e}");
        }
    }
}

/// PNG já em [`dir`] vira item de histórico: miniatura, hash do PNG como `content`, dedupe e
/// limite de [`MAX_IMAGE_ITEMS`]. `from`: dispositivo remetente (`None` = copiada aqui).
/// Bloqueante (hash e miniatura); chamar fora da thread principal.
fn register(app: &AppHandle, path: &Path, size: u64, from: Option<&str>) -> Option<ClipboardItem> {
    let state = app.state::<AppState>();
    let sha = match crate::transfer::hash::sha256_prefix_blocking(path, None) {
        Ok(h) => crate::transfer::hash::hex(h),
        Err(e) => {
            log::warn!("[CLIPBOARD] hash da imagem falhou: {e}");
            return None;
        }
    };
    let thumb = thumb_path(path);
    let (w, h) = thumbnail(path, &thumb).or_else(|| png_dimensions(path)).unwrap_or((0, 0));
    let mut meta = serde_json::json!({ "path": path.to_string_lossy(), "width": w, "height": h, "bytes": size, "mime": "image/png" });
    if thumb.is_file() {
        meta["thumb"] = thumb.to_string_lossy().into();
    }
    let now = now_ms();
    let item = ClipboardItem {
        id: uuid::Uuid::new_v4().to_string(),
        content: format!("img:{sha}"),
        content_type: ContentType::Image,
        source_device_id: from.map(str::to_string).unwrap_or_else(|| state.self_id()),
        created_at: now,
        expires_at: crate::sync::expiry_for(&state.settings(), false, now),
        is_pinned: false,
        is_sensitive: false,
        sync_state: if from.is_some() { SyncState::Synced } else { SyncState::Local },
        metadata: meta,
    };
    if !state.db.insert_item(&item).unwrap_or(false) {
        return None;
    }
    // Mesma imagem de antes (outro arquivo): o item novo substitui o velho e os arquivos dele saem.
    for old in state.db.dedupe_content(&crate::db::content_hash(&item.content), &item.id).unwrap_or_default() {
        remove_files(app, &old);
    }
    let evicted = state.db.evict_images(MAX_IMAGE_ITEMS).unwrap_or_default();
    for old in &evicted {
        remove_files(app, old);
    }
    if !evicted.is_empty() {
        let ids: Vec<&str> = evicted.iter().map(|i| i.id.as_str()).collect();
        events::emit(app, events::CLIPBOARD_EXPIRED, serde_json::json!({ "ids": ids }));
    }
    Some(item)
}

fn thumb_path(png: &Path) -> PathBuf {
    let stem = png.file_stem().and_then(|s| s.to_str()).unwrap_or_else(|| crate::i18n::tr("file.image"));
    png.with_file_name(format!("{stem}.thumb.png"))
}

/// Largura e altura lidas do cabeçalho IHDR, sem decodificar.
fn png_dimensions(path: &Path) -> Option<(u32, u32)> {
    use std::io::Read;
    let mut head = [0u8; 24];
    std::fs::File::open(path).ok()?.read_exact(&mut head).ok()?;
    if head[..8] != [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        return None;
    }
    Some((u32::from_be_bytes(head[16..20].try_into().ok()?), u32::from_be_bytes(head[20..24].try_into().ok()?)))
}

fn meta_path(item: &ClipboardItem, key: &str) -> Option<PathBuf> {
    item.metadata.get(key).and_then(|v| v.as_str()).map(PathBuf::from)
}

/// Só arquivos dentro de [`dir`] são lidos ou apagados por item de histórico.
fn in_dir(app: &AppHandle, p: &Path) -> bool {
    dir(app).map(|d| p.parent() == Some(d.as_path())).unwrap_or(false)
}

/// Origens de transferências ainda abertas: esses PNG não podem sumir debaixo do envio.
fn open_sources(app: &AppHandle) -> Vec<String> {
    app.state::<AppState>()
        .db
        .transfer_list(10_000)
        .unwrap_or_default()
        .into_iter()
        .filter(|t| !t.state.is_terminal())
        .filter_map(|t| t.source)
        .collect()
}

/// Apaga PNG e miniatura de um item já removido do banco, a menos que uma transferência aberta
/// use o PNG ou outro item ainda aponte para o arquivo.
pub fn remove_files(app: &AppHandle, item: &ClipboardItem) {
    if item.content_type != ContentType::Image {
        return;
    }
    let open = open_sources(app);
    let others = app.state::<AppState>().db.image_items(false).unwrap_or_default();
    for key in ["path", "thumb"] {
        let Some(p) = meta_path(item, key).filter(|p| in_dir(app, p)) else { continue };
        let shared = others.iter().any(|o| o.id != item.id && [meta_path(o, "path"), meta_path(o, "thumb")].contains(&Some(p.clone())));
        if !shared && !open.iter().any(|s| Path::new(s) == p) {
            let _ = std::fs::remove_file(&p);
        }
    }
}

/// PNG do item, se ainda existe. Sumiu (apagado por fora): o item não serve mais e sai do histórico.
fn existing_file(app: &AppHandle, item: &ClipboardItem) -> Result<PathBuf> {
    if let Some(p) = meta_path(item, "path").filter(|p| p.is_file() && in_dir(app, p)) {
        return Ok(p);
    }
    let _ = app.state::<AppState>().db.delete_item(&item.id);
    remove_files(app, item);
    events::emit(app, events::CLIPBOARD_EXPIRED, serde_json::json!({ "ids": [item.id] }));
    Err(AppError::Other(crate::i18n::tr("err.image_gone").into()))
}

pub fn file_of(app: &AppHandle, item: &ClipboardItem) -> Result<PathBuf> {
    existing_file(app, item)
}

pub fn copy_item(app: &AppHandle, item: &ClipboardItem) -> Result<()> {
    write_local(app, &existing_file(app, item)?)
}

/// Peer sem `clip-image-v1` trataria o PNG como arquivo comum.
pub fn send_item(app: &AppHandle, item: &ClipboardItem, device_ids: &[String]) -> Result<()> {
    if device_ids.is_empty() {
        return Ok(());
    }
    let path = existing_file(app, item)?;
    let src = path.to_string_lossy();
    let state = app.state::<AppState>();
    let mut first_err = None;
    for dev in device_ids {
        if state.is_connected(dev) && !crate::transfer::manager::peer_has_cap(app, dev, CAP_CLIP_IMAGE_V1) {
            first_err.get_or_insert(AppError::Other(crate::i18n::tr("err.image_old_peer").into()));
            continue;
        }
        supersede(app, dev);
        if let Err(e) = crate::transfer::manager::send_file_with(app, dev, &src, Some(PURPOSE)) {
            log::warn!("[CLIPBOARD] imagem não enfileirada para {dev}: {e}");
            first_err.get_or_insert(e);
        }
    }
    first_err.map_or(Ok(()), Err)
}

/// Imagem do item como data URL para a UI: a miniatura, ou a imagem inteira (`full`) se for pequena.
pub fn data_url(app: &AppHandle, item: &ClipboardItem, full: bool) -> Result<String> {
    use base64::Engine;
    let usable = |key: &str| meta_path(item, key).filter(|p| p.is_file() && in_dir(app, p));
    let small = |p: &PathBuf| std::fs::metadata(p).map(|m| m.len() <= FULL_MAX_BYTES).unwrap_or(false);
    let path = [if full { usable("path") } else { None }, usable("thumb"), usable("path")]
        .into_iter()
        .flatten()
        .find(small)
        .ok_or_else(|| AppError::Other(crate::i18n::tr("err.image_gone").into()))?;
    let bytes = std::fs::read(path)?;
    Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
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

/// Imagem recebida e verificada (já em [`dir`]): entra no histórico com o remetente como origem e
/// vai para o clipboard se as regras deixam.
pub fn on_received(app: &AppHandle, device_id: &str, path: &Path) {
    let state = app.state::<AppState>();
    let auto_dev = state.db.get_device(device_id).ok().flatten().map(|d| d.auto_receive && !d.blocked).unwrap_or(false);
    let apply = auto_dev && state.settings().auto_receive && !state.is_paused();
    log::info!("[CLIPBOARD] Image received from={device_id} apply_clipboard={apply}");
    let path = path.to_path_buf();
    let from = device_id.to_string();
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let Some(item) = register(&app, &path, size, Some(&from)) else { return };
        let applied = apply
            && match write_local(&app, &path) {
                Ok(()) => true,
                Err(e) => {
                    log::warn!("[CLIPBOARD] falha ao pôr a imagem no clipboard: {e}");
                    false
                }
            };
        events::emit(&app, events::CLIPBOARD_RECEIVED, serde_json::json!({ "item": item, "fromDeviceId": from, "applied": applied }));
    });
}

pub fn write_local(app: &AppHandle, path: &Path) -> Result<()> {
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
        Err(AppError::Other(crate::i18n::tr("err.image_unsupported").into()))
    }
}

/// Subida: apaga arquivos antigos que nenhum item de histórico nem transferência aberta usa.
/// Arquivo de item vive tanto quanto o item (a expiração do histórico cuida dele).
pub fn purge_old(app: &AppHandle) {
    let Ok(dir) = dir(app) else { return };
    let open = open_sources(app);
    let mut keep: Vec<PathBuf> = Vec::new();
    for it in app.state::<AppState>().db.image_items(false).unwrap_or_default() {
        keep.extend(["path", "thumb"].iter().filter_map(|k| meta_path(&it, k)));
    }
    let Ok(rd) = std::fs::read_dir(&dir) else { return };
    let mut n = 0;
    for e in rd.flatten() {
        let p = e.path();
        let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).map(|age| age > KEEP_FOR).unwrap_or(false);
        if old && !keep.contains(&p) && !open.iter().any(|s| Path::new(s) == p) && std::fs::remove_file(&p).is_ok() {
            n += 1;
        }
    }
    if n > 0 {
        log::info!("[CLIPBOARD] Removed {n} old clipboard image(s)");
    }
}

/// Miniatura de até [`THUMB_MAX`] px (lado maior) em `dest`. Devolve as dimensões do original.
fn thumbnail(src: &Path, dest: &Path) -> Option<(u32, u32)> {
    #[cfg(windows)]
    {
        let img = windows::decode_png(src).ok()?;
        let (rgba, width, height) = downscale_rgba(&img.rgba, img.width, img.height, THUMB_MAX);
        windows::encode_png(&windows::Rgba { rgba, width, height }, dest).ok()?;
        Some((img.width, img.height))
    }
    #[cfg(target_os = "android")]
    {
        android::thumbnail(src, dest, THUMB_MAX)
    }
    #[cfg(not(any(windows, target_os = "android")))]
    {
        let _ = (src, dest);
        None
    }
}

/// Reduz RGBA 8 bits para caber em `max` px no lado maior (média de caixa, ponderada pelo alfa);
/// já pequena, volta igual. Devolve `(pixels, largura, altura)`.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn downscale_rgba(rgba: &[u8], w: u32, h: u32, max: u32) -> (Vec<u8>, u32, u32) {
    let longest = w.max(h);
    if w == 0 || h == 0 || max == 0 || longest <= max || rgba.len() != w as usize * h as usize * 4 {
        return (rgba.to_vec(), w, h);
    }
    let scaled = |side: u32| ((side as u64 * max as u64 + longest as u64 / 2) / longest as u64).max(1) as usize;
    let (nw, nh) = (scaled(w), scaled(h));
    let (w, h) = (w as usize, h as usize);
    let mut out = Vec::with_capacity(nw * nh * 4);
    for y in 0..nh {
        let (y0, y1) = (y * h / nh, ((y + 1) * h / nh).max(y * h / nh + 1).min(h));
        for x in 0..nw {
            let (x0, x1) = (x * w / nw, ((x + 1) * w / nw).max(x * w / nw + 1).min(w));
            let (mut r, mut g, mut b, mut a, mut n) = (0u64, 0u64, 0u64, 0u64, 0u64);
            for sy in y0..y1 {
                for px in rgba[(sy * w + x0) * 4..(sy * w + x1) * 4].chunks_exact(4) {
                    let al = px[3] as u64;
                    r += px[0] as u64 * al;
                    g += px[1] as u64 * al;
                    b += px[2] as u64 * al;
                    a += al;
                    n += 1;
                }
            }
            // Tudo transparente (a == 0) cai em 0 por `checked_div`.
            let avg = |sum: u64, d: u64| (sum + d / 2).checked_div(d).unwrap_or(0) as u8;
            out.extend_from_slice(&[avg(r, a), avg(g, a), avg(b, a), avg(a, n)]);
        }
    }
    (out, nw as u32, nh as u32)
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
        // Outro processo com o clipboard aberto (gerenciadores, outro ClipSync) faz a escrita falhar
        // por instantes (os error 1418): tenta de novo antes de desistir. Chamado fora da thread principal.
        let mut attempt = 0;
        loop {
            match app.clipboard().write_image(&image) {
                Ok(()) => break,
                Err(e) if attempt >= 4 => return Err(AppError::Other(e.to_string())),
                Err(_) => {
                    attempt += 1;
                    std::thread::sleep(std::time::Duration::from_millis(60 * attempt));
                }
            }
        }
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
        let mut reader = dec.read_info().map_err(|e| AppError::Other(crate::i18n::trf("err.png_invalid", &[("e", &e.to_string())])))?;
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).map_err(|e| AppError::Other(crate::i18n::trf("err.png_invalid", &[("e", &e.to_string())])))?;
        buf.truncate(info.buffer_size());
        let (w, h) = (info.width, info.height);
        let rgba = match info.color_type {
            png::ColorType::Rgba => buf,
            png::ColorType::GrayscaleAlpha => buf.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
            other => return Err(AppError::Other(crate::i18n::trf("err.png_color", &[("e", &format!("{other:?}"))]))),
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
    use jni::objects::{JIntArray, JValue};
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

    /// `ClipImageBridge.thumbnail`: grava a miniatura em `dest` e devolve as dimensões do original.
    pub fn thumbnail(src: &Path, dest: &Path, max: u32) -> Option<(u32, u32)> {
        with_class(BRIDGE, |env, class, _ctx| {
            let js = env.new_string(src.to_string_lossy()).map_err(|e| AppError::Other(e.to_string()))?;
            let jd = env.new_string(dest.to_string_lossy()).map_err(|e| AppError::Other(e.to_string()))?;
            let out = env
                .call_static_method(
                    class,
                    "thumbnail",
                    "(Ljava/lang/String;Ljava/lang/String;I)[I",
                    &[JValue::Object(&js), JValue::Object(&jd), JValue::Int(max as i32)],
                )
                .and_then(|v| v.l())
                .map_err(|e| AppError::Other(format!("ClipImageBridge.thumbnail: {e}")))?;
            if out.is_null() {
                return Ok(None);
            }
            // [largura, altura, larguraMiniatura, alturaMiniatura]
            let mut dims = [0i32; 4];
            env.get_int_array_region(&JIntArray::from(out), 0, &mut dims).map_err(|e| AppError::Other(e.to_string()))?;
            Ok(Some((dims[0].max(0) as u32, dims[1].max(0) as u32)))
        })
        .ok()?
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

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, px: [u8; 4]) -> Vec<u8> {
        px.iter().copied().cycle().take(w as usize * h as usize * 4).collect()
    }

    #[test]
    fn small_image_is_returned_unchanged() {
        let src = solid(10, 6, [1, 2, 3, 255]);
        assert_eq!(downscale_rgba(&src, 10, 6, 320), (src, 10, 6));
    }

    #[test]
    fn downscale_keeps_aspect_and_solid_color() {
        let (out, w, h) = downscale_rgba(&solid(1000, 500, [10, 200, 30, 255]), 1000, 500, 320);
        assert_eq!((w, h), (320, 160));
        assert_eq!(out.len(), 320 * 160 * 4);
        assert!(out.chunks_exact(4).all(|p| p == [10, 200, 30, 255]));
        // Retrato: o lado maior é a altura.
        let (_, w, h) = downscale_rgba(&solid(30, 900, [0, 0, 0, 255]), 30, 900, 300);
        assert_eq!((w, h), (10, 300));
    }

    #[test]
    fn box_filter_averages_and_ignores_transparent_pixels() {
        // Xadrez preto/branco 2×2 → 1 pixel cinza médio.
        let checker = [0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 255];
        let (out, w, h) = downscale_rgba(&checker, 2, 2, 1);
        assert_eq!((w, h), (1, 1));
        assert_eq!(out, [128, 128, 128, 255]);
        // Pixel transparente (vermelho) não tinge o vizinho opaco azul.
        let (out, ..) = downscale_rgba(&[255, 0, 0, 0, 0, 0, 255, 255], 2, 1, 1);
        assert_eq!(out[..3], [0, 0, 255]);
    }

    #[test]
    fn png_header_gives_dimensions() {
        let mut head = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R'];
        head.extend_from_slice(&1920u32.to_be_bytes());
        head.extend_from_slice(&1080u32.to_be_bytes());
        let p = std::env::temp_dir().join(format!("clipsync-ihdr-{}.png", uuid::Uuid::new_v4()));
        std::fs::write(&p, &head).unwrap();
        assert_eq!(png_dimensions(&p), Some((1920, 1080)));
        std::fs::write(&p, b"not a png at all, just text........").unwrap();
        assert_eq!(png_dimensions(&p), None);
        let _ = std::fs::remove_file(&p);
        assert_eq!(thumb_path(Path::new("C:/x/imagem-1.png")), Path::new("C:/x/imagem-1.thumb.png"));
    }
}
