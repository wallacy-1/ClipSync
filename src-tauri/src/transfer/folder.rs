//! Pasta → `.zip` em `<app_dir>/folder-zips/<uuid>/<pasta>.zip`, enviado como arquivo
//! comum (o outro lado recebe o `.zip`). Só no PC: o seletor do Android não entrega pastas.
//! O zip some ao concluir ou cancelar; falha mantém (para "Tentar de novo") até a subida
//! seguinte ou "Limpar concluídas", quando sai tudo que nenhuma transferência viva usa.
use crate::error::{AppError, Result};
use crate::state::AppState;
use crate::transfer::state::TransferState;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

const SUBDIR: &str = "folder-zips";

fn root(app: &AppHandle) -> PathBuf {
    app.state::<AppState>().app_dir.join(SUBDIR)
}

/// Caminho a enviar: o próprio arquivo ou, se for pasta, o zip dela. Bloqueante.
pub fn prepare(app: &AppHandle, path: &str) -> Result<String> {
    let src = Path::new(path);
    if !src.is_dir() {
        return Ok(path.to_string());
    }
    let name = src.file_name().and_then(|n| n.to_str()).filter(|n| !n.is_empty()).unwrap_or("pasta");
    let dir = root(app).join(uuid::Uuid::new_v4().simple().to_string());
    std::fs::create_dir_all(&dir)?;
    let out = dir.join(format!("{name}.zip"));
    if let Err(e) = zip_dir(src, name, &out) {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(e);
    }
    log::info!("[FILE] Folder zipped {:?} -> {:?}", src, out);
    Ok(out.to_string_lossy().into_owned())
}

/// Apaga o zip (e o diretório dele) se `source` for um zip de pasta nosso.
pub fn release(app: &AppHandle, source: &str) {
    let root = root(app);
    let Some(parent) = Path::new(source).parent() else { return };
    if parent.parent() == Some(root.as_path()) {
        let _ = std::fs::remove_dir_all(parent);
    }
}

/// Zip vivo = transferência não terminal ou falha (retomável).
pub fn purge(app: &AppHandle) {
    let root = root(app);
    let Ok(rd) = std::fs::read_dir(&root) else { return };
    let keep: Vec<PathBuf> = app
        .state::<AppState>()
        .db
        .transfer_list(10_000)
        .unwrap_or_default()
        .into_iter()
        .filter(|t| !t.state.is_terminal() || matches!(t.state, TransferState::Failed { .. }))
        .filter_map(|t| t.source.and_then(|s| Path::new(&s).parent().map(Path::to_path_buf)))
        .collect();
    let mut n = 0;
    for e in rd.flatten() {
        let p = e.path();
        if !keep.contains(&p) && std::fs::remove_dir_all(&p).is_ok() {
            n += 1;
        }
    }
    if n > 0 {
        log::info!("[FILE] Removed {n} folder zip(s)");
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn zip_dir(src: &Path, name: &str, out: &Path) -> Result<()> {
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    let file = std::fs::File::create(out)?;
    let mut zip = zip::ZipWriter::new(std::io::BufWriter::new(file));
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).large_file(true);
    let zerr = |e: zip::result::ZipError| AppError::Other(format!("zip: {e}"));
    // Pilha em vez de recursão; links simbólicos ficam de fora (podem apontar para fora da pasta).
    let mut stack = vec![(src.to_path_buf(), name.to_string())];
    while let Some((dir, prefix)) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir)?.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        if entries.is_empty() {
            zip.add_directory(format!("{prefix}/"), opts).map_err(zerr)?;
        }
        for e in entries {
            let ft = e.file_type()?;
            let rel = format!("{prefix}/{}", e.file_name().to_string_lossy());
            if ft.is_dir() {
                stack.push((e.path(), rel));
            } else if ft.is_file() {
                let file_opts = match zip_time(&e.metadata()?) {
                    Some(t) => opts.last_modified_time(t),
                    None => opts,
                };
                zip.start_file(rel, file_opts).map_err(zerr)?;
                let mut f = std::fs::File::open(e.path())?;
                std::io::copy(&mut f, &mut zip)?;
            }
        }
    }
    zip.finish().map_err(zerr)?.flush()?;
    Ok(())
}

/// Data de modificação para a entrada do zip; sem ela os arquivos extraídos saem com 1980-01-01.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn zip_time(meta: &std::fs::Metadata) -> Option<zip::DateTime> {
    use chrono::{Datelike, Timelike};
    let t: chrono::DateTime<chrono::Local> = meta.modified().ok()?.into();
    zip::DateTime::from_date_and_time(
        u16::try_from(t.year()).ok()?,
        t.month() as u8,
        t.day() as u8,
        t.hour() as u8,
        t.minute() as u8,
        t.second() as u8,
    )
    .ok()
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn zip_dir(_src: &Path, _name: &str, _out: &Path) -> Result<()> {
    Err(AppError::Other("envio de pasta indisponível nesta plataforma".into()))
}

#[cfg(all(test, not(any(target_os = "android", target_os = "ios"))))]
mod tests {
    use super::*;

    #[test]
    fn zips_nested_folder_with_relative_names() {
        let base = std::env::temp_dir().join(format!("clipsync-zip-{}", uuid::Uuid::new_v4()));
        let src = base.join("Fotos");
        std::fs::create_dir_all(src.join("sub/vazia")).unwrap();
        std::fs::write(src.join("a.txt"), b"ola").unwrap();
        std::fs::write(src.join("sub/b.bin"), vec![7u8; 5000]).unwrap();
        let out = base.join("Fotos.zip");
        zip_dir(&src, "Fotos", &out).unwrap();

        let mut z = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
        let mut names: Vec<String> = (0..z.len()).map(|i| z.by_index(i).unwrap().name().to_string()).collect();
        names.sort();
        assert_eq!(names, ["Fotos/a.txt", "Fotos/sub/b.bin", "Fotos/sub/vazia/"]);
        let mut s = String::new();
        std::io::Read::read_to_string(&mut z.by_name("Fotos/a.txt").unwrap(), &mut s).unwrap();
        assert_eq!(s, "ola");
        // A data de modificação vai junto (não o padrão 1980-01-01 do zip).
        assert!(z.by_name("Fotos/a.txt").unwrap().last_modified().map(|t| t.year() > 2000).unwrap_or(false));
        let _ = std::fs::remove_dir_all(&base);
    }
}
