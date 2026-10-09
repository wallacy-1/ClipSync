//! Origem de uma transferência de saída: caminho ou `content://`. O emissor só vê um `File`
//! seekável + tamanho. No Android, uma URI
//! não-seekável (provider que devolve pipe) cai no **fallback em disco**: cópia
//! única para `<privado>/transfers/out-<id>.tmp` — nunca para a memória.
use crate::error::{AppError, Result};
#[cfg(target_os = "android")]
use std::io::{Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferSource {
    Path(PathBuf),
    AndroidUri(String),
}

impl TransferSource {
    pub fn parse(s: &str) -> Self {
        if s.starts_with("content://") {
            TransferSource::AndroidUri(s.to_string())
        } else {
            TransferSource::Path(PathBuf::from(s))
        }
    }

    pub fn as_str(&self) -> String {
        match self {
            TransferSource::Path(p) => p.to_string_lossy().into_owned(),
            TransferSource::AndroidUri(u) => u.clone(),
        }
    }
}

/// Metadados conhecidos antes de abrir (para registrar a transferência).
#[derive(Debug, Clone)]
pub struct SourceInfo {
    pub name: String,
    pub size: u64,
    pub mime: Option<String>,
}

pub fn describe(src: &TransferSource) -> Result<SourceInfo> {
    match src {
        TransferSource::Path(p) => {
            let meta = std::fs::metadata(p).map_err(|e| AppError::Other(crate::i18n::trf("err.file_unreadable", &[("e", &e.to_string())])))?;
            if !meta.is_file() {
                return Err(AppError::Other(crate::i18n::tr("err.not_a_file").into()));
            }
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("arquivo").to_string();
            Ok(SourceInfo { name, size: meta.len(), mime: None })
        }
        #[cfg(target_os = "android")]
        TransferSource::AndroidUri(u) => {
            let info = super::android::describe_uri(u)?;
            if info.size < 0 {
                // Tamanho desconhecido: abre para descobrir (statSize) sem ler o conteúdo.
                let opened = open(src, None)?;
                return Ok(SourceInfo { name: info.name, size: opened.size, mime: info.mime });
            }
            Ok(SourceInfo { name: info.name, size: info.size as u64, mime: info.mime })
        }
        #[cfg(not(target_os = "android"))]
        TransferSource::AndroidUri(_) => Err(AppError::Other("content:// só existe no Android".into())),
    }
}

pub struct Opened {
    pub file: std::fs::File,
    pub size: u64,
    /// Caminho de uma cópia temporária (fonte não-seekável); o chamador apaga ao terminar.
    pub temp_copy: Option<PathBuf>,
}

/// Abre a origem para leitura sequencial + `seek`. `fallback_dir` é onde uma
/// fonte não-seekável é copiada (obrigatório no Android; ignorado para caminhos).
pub fn open(src: &TransferSource, fallback_dir: Option<&Path>) -> Result<Opened> {
    match src {
        TransferSource::Path(p) => {
            let file = std::fs::File::open(p).map_err(|e| AppError::Other(format!("source unavailable: {e}")))?;
            let size = file.metadata()?.len();
            Ok(Opened { file, size, temp_copy: None })
        }
        #[cfg(target_os = "android")]
        TransferSource::AndroidUri(u) => open_android_uri(u, fallback_dir),
        #[cfg(not(target_os = "android"))]
        TransferSource::AndroidUri(_) => {
            let _ = fallback_dir;
            Err(AppError::Other("content:// só existe no Android".into()))
        }
    }
}

#[cfg(target_os = "android")]
fn open_android_uri(uri: &str, fallback_dir: Option<&Path>) -> Result<Opened> {
    use std::os::fd::FromRawFd;
    let (fd, size) = super::android::open_uri(uri)?; // fd já desanexado: somos os donos
    // SAFETY: `detachFd()` transferiu a posse do descritor para nós; ninguém mais o fecha.
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    let seekable = file.seek(SeekFrom::Start(0)).is_ok();
    if seekable {
        let size = if size >= 0 { size as u64 } else { file.seek(SeekFrom::End(0)).and_then(|n| file.seek(SeekFrom::Start(0)).map(|_| n))? };
        return Ok(Opened { file, size, temp_copy: None });
    }
    // Pipe/stream: única forma de garantir seek + hash + retomada é uma cópia em disco privado.
    let dir = fallback_dir.ok_or_else(|| AppError::Other("source not seekable".into()))?;
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("out-{}.tmp", uuid::Uuid::new_v4()));
    let mut out = std::fs::File::create(&tmp)?;
    std::io::copy(&mut file, &mut out)?;
    drop(file);
    let mut f = std::fs::File::open(&tmp)?;
    let size = f.seek(SeekFrom::End(0))?;
    f.seek(SeekFrom::Start(0))?;
    log::info!("[FILE] source não-seekável copiada para disco privado ({size} B)");
    Ok(Opened { file: f, size, temp_copy: Some(tmp) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_describe_path() {
        assert_eq!(TransferSource::parse("content://x/y"), TransferSource::AndroidUri("content://x/y".into()));
        assert!(matches!(TransferSource::parse("C:\\a\\b.txt"), TransferSource::Path(_)));
        let p = std::env::temp_dir().join(format!("clipsync-src-{}.bin", uuid::Uuid::new_v4()));
        std::fs::write(&p, b"hello").unwrap();
        let src = TransferSource::Path(p.clone());
        let info = describe(&src).unwrap();
        assert_eq!(info.size, 5);
        assert!(info.name.ends_with(".bin"));
        let mut o = open(&src, None).unwrap();
        assert_eq!(o.size, 5);
        std::io::Seek::seek(&mut o.file, std::io::SeekFrom::Start(3)).unwrap();
        let _ = std::fs::remove_file(p);
        assert!(describe(&TransferSource::Path(PathBuf::from("Z:/nope/nope"))).is_err());
    }
}
