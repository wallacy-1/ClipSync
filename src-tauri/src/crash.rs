//! Panic → `crash.log` no diretório de dados do app.
//!
//! O release usa `panic = "abort"`: qualquer panic derruba o processo na hora e,
//! no Windows (sem console) ou num Android sem logcat à mão, o erro sumiria.
//! O hook grava mensagem, local e thread antes do abort.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const FILE: &str = "crash.log";
/// Acima disso o arquivo atual vira `crash.log.1` (só uma geração).
const MAX_BYTES: u64 = 512 * 1024;

static DIR: OnceLock<PathBuf> = OnceLock::new();

/// Instala o hook o quanto antes; até [`set_dir`] o arquivo vai para o temp do sistema.
pub fn install() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let entry = format!(
            "[{}] ClipSync {} ({}) thread={}\n{info}\n{}\n",
            chrono::Local::now().to_rfc3339(),
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            thread.name().unwrap_or("?"),
            std::backtrace::Backtrace::force_capture(),
        );
        log::error!("[CRASH] {info}");
        let _ = append(&entry);
        previous(info);
    }));
}

/// Diretório de dados do app (perfil incluído), conhecido só no `setup`.
pub fn set_dir(dir: &Path) {
    let _ = DIR.set(dir.to_path_buf());
}

fn append(entry: &str) -> std::io::Result<()> {
    let dir = DIR.get().cloned().unwrap_or_else(std::env::temp_dir);
    let path = dir.join(FILE);
    if std::fs::metadata(&path).map(|m| m.len() > MAX_BYTES).unwrap_or(false) {
        let _ = std::fs::rename(&path, dir.join(format!("{FILE}.1")));
    }
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
    f.write_all(entry.as_bytes())?;
    f.sync_all()
}

#[cfg(test)]
mod tests {
    #[test]
    fn panic_is_written_to_crash_log() {
        let dir = std::env::temp_dir().join(format!("clipsync-crash-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        super::set_dir(&dir);
        super::install();
        let r = std::thread::Builder::new().name("boom-thread".into()).spawn(|| panic!("boom de teste")).unwrap().join();
        assert!(r.is_err());
        let log = std::fs::read_to_string(dir.join(super::FILE)).unwrap();
        assert!(log.contains("boom de teste") && log.contains("thread=boom-thread"), "{log}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
