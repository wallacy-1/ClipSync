//! Arquivos copiados no Explorer (Ctrl+C → `CF_HDROP`). Nunca saem sozinhos: Ctrl+C
//! de arquivo é quase sempre para colar em outra pasta. O núcleo só avisa a UI
//! (`clipboard:files`), que oferece "Enviar para…"; o envio usa `transfer_send`.
use crate::events;
use crate::models::now_ms;
use crate::state::AppState;
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// Acima disso a oferta lista só os primeiros (limite de transferências abertas por device).
pub const MAX_FILES: usize = 20;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopiedFile {
    pub path: String,
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopiedFiles {
    pub files: Vec<CopiedFile>,
    /// Pastas copiadas junto (não enviadas: transferência é por arquivo).
    pub skipped_dirs: u32,
    /// Arquivos além de [`MAX_FILES`].
    pub skipped_files: u32,
    pub copied_at: i64,
}

/// Retorna `true` se o clipboard tem uma lista de arquivos (tratada ou já vista),
/// para o chamador não tentar ler imagem.
pub fn poll(app: &AppHandle) -> bool {
    let Some(paths) = read() else { return false };
    let key = format!("files:{}", crate::db::content_hash(&paths.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>().join("\n")));
    {
        let state = app.state::<AppState>();
        let mut d = state.clip_dedupe.lock().unwrap();
        if !d.poll_changed(&key) {
            return true;
        }
    }
    let offer = describe(paths);
    log::info!(
        "[CLIPBOARD] Files copied count={} dirs={} skipped={}",
        offer.files.len(),
        offer.skipped_dirs,
        offer.skipped_files
    );
    if !offer.files.is_empty() {
        events::emit(app, events::CLIPBOARD_FILES, &offer);
    }
    true
}

/// Watcher subiu com arquivos já copiados: não oferece o que é de antes.
pub fn prime(app: &AppHandle) -> bool {
    let Some(paths) = read() else { return false };
    let key = format!("files:{}", crate::db::content_hash(&paths.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>().join("\n")));
    app.state::<AppState>().clip_dedupe.lock().unwrap().prime(&key);
    true
}

fn read() -> Option<Vec<PathBuf>> {
    let mut cb = arboard::Clipboard::new().ok()?;
    let paths = cb.get().file_list().ok()?;
    (!paths.is_empty()).then_some(paths)
}

fn describe(paths: Vec<PathBuf>) -> CopiedFiles {
    let mut files = Vec::new();
    let (mut skipped_dirs, mut skipped_files) = (0u32, 0u32);
    for p in paths {
        let Ok(meta) = std::fs::metadata(&p) else { continue };
        if meta.is_dir() {
            skipped_dirs += 1;
            continue;
        }
        if files.len() >= MAX_FILES {
            skipped_files += 1;
            continue;
        }
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("arquivo").to_string();
        files.push(CopiedFile { path: p.to_string_lossy().into_owned(), name, size: meta.len() });
    }
    CopiedFiles { files, skipped_dirs, skipped_files, copied_at: now_ms() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_skips_dirs_and_caps_count() {
        let dir = std::env::temp_dir().join(format!("clipsync-files-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let mut paths = vec![dir.join("sub"), dir.join("nao-existe.txt")];
        for i in 0..(MAX_FILES + 2) {
            let p = dir.join(format!("f{i}.bin"));
            std::fs::write(&p, vec![0u8; i]).unwrap();
            paths.push(p);
        }
        let d = describe(paths);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(d.files.len(), MAX_FILES);
        assert_eq!(d.skipped_dirs, 1);
        assert_eq!(d.skipped_files, 2);
        assert_eq!(d.files[3].size, 3);
        assert_eq!(d.files[3].name, "f3.bin");
    }
}
