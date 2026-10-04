//! Onde os arquivos recebidos moram: diretório de destino, `.part` temporário
//! (no mesmo volume, para o `rename` final ser atômico), espaço livre e
//! finalização. Só caminhos construídos localmente; o nome do peer entra aqui
//! já sanitizado e ainda passa por [`names::resolve_in`].
use super::names::{dedupe_name, resolve_in};
use crate::error::{AppError, Result};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

#[cfg_attr(target_os = "android", allow(dead_code))]
pub const DEST_SUBDIR: &str = "ClipSync";
pub const PART_SUBDIR: &str = ".part";
pub const PART_EXT: &str = "part";

/// Windows: `<Downloads>/ClipSync` (destino final). Android: diretório **privado**
/// do app (`<app_data>/transfers`) — o destino final é o MediaStore, não um caminho.
pub fn dest_dir(app: &AppHandle) -> Result<PathBuf> {
    #[cfg(not(target_os = "android"))]
    let dir = app
        .path()
        .download_dir()
        .map_err(|e| AppError::Other(format!("pasta de downloads indisponível: {e}")))?
        .join(DEST_SUBDIR);
    #[cfg(target_os = "android")]
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("diretório privado indisponível: {e}")))?
        .join("transfers");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Onde uma origem não-seekável é copiada antes do envio (Android).
pub fn outgoing_tmp_dir(app: &AppHandle) -> Result<PathBuf> {
    dest_dir(app)
}

/// `<Downloads>/ClipSync/.part` — mesmo volume do destino.
pub fn part_dir(app: &AppHandle) -> Result<PathBuf> {
    let dir = dest_dir(app)?.join(PART_SUBDIR);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Caminho do `.part` de uma transferência: só o id entra no nome.
pub fn part_path(app: &AppHandle, transfer_id: &str) -> Result<PathBuf> {
    Ok(part_dir(app)?.join(format!("{transfer_id}.{PART_EXT}")))
}

/// Bytes livres para este processo no volume de `path`. `None` = não sabemos
/// (plataforma sem implementação): o chamador decide se aceita mesmo assim.
#[cfg(windows)]
pub fn free_space(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut avail: u64 = 0;
    let mut total: u64 = 0;
    let mut free: u64 = 0;
    // SAFETY: ponteiros válidos para a duração da chamada; string terminada em NUL.
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, &mut total, &mut free) };
    if ok == 0 {
        None
    } else {
        Some(avail)
    }
}

#[cfg(target_os = "android")]
pub fn free_space(path: &Path) -> Option<u64> {
    super::android::free_space(&path.to_string_lossy())
}

#[cfg(not(any(windows, target_os = "android")))]
pub fn free_space(_path: &Path) -> Option<u64> {
    None
}

/// Espaço exigido para aceitar um arquivo de `size` bytes. No Android a publicação
/// no MediaStore é uma **cópia** (o `.part` privado ainda existe durante ela): 2×.
pub fn required_space(size: u64) -> u64 {
    let copies: u64 = if cfg!(target_os = "android") { 2 } else { 1 };
    size * copies + size / 20 + super::FREE_SPACE_MARGIN
}

/// Move o `.part` verificado para `dest_dir/<safe_name>` sem sobrescrever nada:
/// colisão vira `nome (2).ext`. `rename` no mesmo volume é atômico; se falhar
/// (outro volume), copia e apaga. Retorna o caminho final.
pub fn finalize(part: &Path, dest_dir: &Path, safe_name: &str) -> Result<PathBuf> {
    let name = dedupe_name(safe_name, |candidate| {
        resolve_in(dest_dir, candidate).map(|p| p.exists()).unwrap_or(true)
    });
    let target = resolve_in(dest_dir, &name).ok_or_else(|| AppError::Other("nome de destino inválido".into()))?;
    if target.exists() {
        return Err(AppError::Other("destino já existe".into()));
    }
    match std::fs::rename(part, &target) {
        Ok(()) => Ok(target),
        Err(rename_err) => {
            // Volume diferente (Downloads redirecionado): copia e remove.
            std::fs::copy(part, &target).map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("{rename_err}; copy: {e}"))))?;
            let _ = std::fs::remove_file(part);
            Ok(target)
        }
    }
}

/// Finalização por plataforma. Retorna `(destino, nome final)`: no Windows um caminho
/// (rename atômico em `dest_dir`); no Android uma `content://` URI do MediaStore
/// (Downloads/ClipSync, `IS_PENDING` até o fim da cópia).
pub fn finalize_platform(part: &Path, dest_dir: &Path, safe_name: &str, mime: Option<&str>) -> Result<(String, String)> {
    #[cfg(not(target_os = "android"))]
    {
        let _ = mime;
        let final_path = finalize(part, dest_dir, safe_name)?;
        let name = final_path.file_name().and_then(|n| n.to_str()).unwrap_or(safe_name).to_string();
        Ok((final_path.to_string_lossy().into_owned(), name))
    }
    #[cfg(target_os = "android")]
    {
        let _ = dest_dir;
        super::android::publish_downloads(&part.to_string_lossy(), safe_name, mime)
    }
}

/// Apaga `.part` sem linha correspondente (crash entre criar o arquivo e a linha,
/// ou linha já apagada pela retenção).
pub fn remove_orphans(app: &AppHandle, known_ids: &[String]) -> usize {
    let Ok(dir) = part_dir(app) else { return 0 };
    remove_orphans_in(&dir, known_ids)
}

fn remove_orphans_in(dir: &Path, known_ids: &[String]) -> usize {
    let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
    let mut n = 0;
    for entry in rd.flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some(PART_EXT) {
            continue;
        }
        let id = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        if !known_ids.contains(&id) && std::fs::remove_file(&p).is_ok() {
            n += 1;
        }
    }
    n
}

/// Cópias temporárias de origens não-seekáveis (`out-<uuid>.tmp`, ver `source.rs`)
/// deixadas por um processo morto no meio do envio: na subida nenhuma task existe,
/// logo todas são órfãs. Só o padrão exato é tocado.
pub fn remove_outgoing_tmp(dir: &Path) -> usize {
    let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
    let mut n = 0;
    for entry in rd.flatten() {
        let p = entry.path();
        let Some(name) = p.file_name().and_then(|s| s.to_str()) else { continue };
        let is_tmp = name.strip_prefix("out-").and_then(|r| r.strip_suffix(".tmp")).map(|id| uuid::Uuid::parse_str(id).is_ok()).unwrap_or(false);
        if is_tmp && p.is_file() && std::fs::remove_file(&p).is_ok() {
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outgoing_tmp_cleanup_only_touches_the_exact_pattern() {
        let dir = tmp();
        let orphan = dir.join(format!("out-{}.tmp", uuid::Uuid::new_v4()));
        std::fs::write(&orphan, b"x").unwrap();
        std::fs::write(dir.join("out-notauuid.tmp"), b"x").unwrap();
        std::fs::write(dir.join("outro.tmp"), b"x").unwrap();
        std::fs::write(dir.join("foto.jpg"), b"x").unwrap();
        assert_eq!(remove_outgoing_tmp(&dir), 1);
        assert!(!orphan.exists());
        assert!(dir.join("out-notauuid.tmp").exists());
        assert!(dir.join("outro.tmp").exists());
        assert!(dir.join("foto.jpg").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn orphans_keep_known_and_remove_unknown() {
        let dir = tmp();
        let pd = dir.join(PART_SUBDIR);
        std::fs::create_dir_all(&pd).unwrap();
        std::fs::write(pd.join("keep.part"), b"x").unwrap();
        std::fs::write(pd.join("gone.part"), b"x").unwrap();
        std::fs::write(pd.join("other.txt"), b"x").unwrap();
        let n = remove_orphans_in(&pd, &["keep".to_string()]);
        assert_eq!(n, 1);
        assert!(pd.join("keep.part").exists());
        assert!(!pd.join("gone.part").exists());
        assert!(pd.join("other.txt").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("clipsync-dest-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn finalize_renames_and_dedupes() {
        let dir = tmp();
        let part_dir = dir.join(PART_SUBDIR);
        std::fs::create_dir_all(&part_dir).unwrap();
        std::fs::write(dir.join("foto.jpg"), b"old").unwrap();

        let part = part_dir.join("t1.part");
        std::fs::write(&part, b"new").unwrap();
        let out = finalize(&part, &dir, "foto.jpg").unwrap();
        assert_eq!(out, dir.join("foto (2).jpg"));
        assert_eq!(std::fs::read(&out).unwrap(), b"new");
        assert_eq!(std::fs::read(dir.join("foto.jpg")).unwrap(), b"old");
        assert!(!part.exists());

        // Nome já sanitizado mas com separador não pode sair do diretório.
        let part2 = part_dir.join("t2.part");
        std::fs::write(&part2, b"x").unwrap();
        assert!(finalize(&part2, &dir, "../x.txt").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn required_space_has_margin() {
        assert_eq!(required_space(0), super::super::FREE_SPACE_MARGIN);
        assert!(required_space(1_000_000_000) > 1_050_000_000);
    }

    #[cfg(windows)]
    #[test]
    fn free_space_works_on_temp() {
        assert!(free_space(&std::env::temp_dir()).unwrap() > 0);
    }
}
