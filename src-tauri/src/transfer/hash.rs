//! SHA-256 de arquivos, em streaming (nunca o arquivo inteiro em memória).
use crate::error::Result;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

const BUF: usize = 1024 * 1024;

/// Hash dos primeiros `limit` bytes de `r` (ou tudo com `None`). Bloqueante.
pub fn sha256_reader_blocking(mut r: impl Read, limit: Option<u64>) -> Result<Sha256> {
    let mut h = Sha256::new();
    let mut buf = vec![0u8; BUF];
    let mut remaining = limit.unwrap_or(u64::MAX);
    while remaining > 0 {
        let want = buf.len().min(remaining.min(usize::MAX as u64) as usize);
        let n = r.read(&mut buf[..want])?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        remaining -= n as u64;
    }
    Ok(h)
}

/// Hash dos primeiros `limit` bytes (ou do arquivo todo com `None`). Bloqueante.
pub fn sha256_prefix_blocking(path: &Path, limit: Option<u64>) -> Result<Sha256> {
    sha256_reader_blocking(std::fs::File::open(path)?, limit)
}

pub fn hex(h: Sha256) -> String {
    hex::encode(h.finalize())
}

/// Hash completo de um caminho, fora do runtime async.
#[allow(dead_code)]
pub async fn sha256_file(path: PathBuf) -> Result<String> {
    tokio::task::spawn_blocking(move || sha256_prefix_blocking(&path, None).map(hex))
        .await
        .map_err(|e| crate::error::AppError::Other(e.to_string()))?
}

/// Hash completo de um arquivo já aberto (posição é rebobinada antes), fora do runtime.
pub async fn sha256_open_file(mut file: std::fs::File) -> Result<String> {
    tokio::task::spawn_blocking(move || {
        use std::io::Seek;
        file.seek(std::io::SeekFrom::Start(0))?;
        sha256_reader_blocking(&file, None).map(hex)
    })
    .await
    .map_err(|e| crate::error::AppError::Other(e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vectors_and_prefix() {
        let p = std::env::temp_dir().join(format!("clipsync-hash-{}", uuid::Uuid::new_v4()));
        std::fs::write(&p, b"").unwrap();
        assert_eq!(hex(sha256_prefix_blocking(&p, None).unwrap()), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(hex(sha256_prefix_blocking(&p, None).unwrap()), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        std::fs::write(&p, b"abcXYZ").unwrap();
        assert_eq!(hex(sha256_prefix_blocking(&p, Some(3)).unwrap()), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let _ = std::fs::remove_file(p);
    }
}
