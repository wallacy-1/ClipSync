//! Persistência de transferências (`file_transfers`). Só metadados e o offset
//! durável; nenhum byte do arquivo entra no SQLite. A tabela é também a fila:
//! `queued`/`waiting` por `device_id` é o que o gerenciador retoma ao conectar.
use super::state::{Direction, TransferState};
use crate::db::Db;
use crate::error::{AppError, Result};
use crate::models::now_ms;
use rusqlite::{params, OptionalExtension, Row};
use serde::Serialize;

/// Transferências terminadas ficam visíveis por este período; depois somem.
pub const RETENTION_MS: i64 = 30 * 24 * 60 * 60 * 1_000;
/// `waiting` sem progresso por mais que isto é abandonada.
pub const WAITING_EXPIRY_MS: i64 = 7 * 24 * 60 * 60 * 1_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferRecord {
    pub id: String,
    pub direction: Direction,
    pub device_id: String,
    /// Como veio da origem; só para exibir/logar, nunca para montar caminho.
    pub original_name: String,
    /// Nome sanitizado, efetivamente usado no disco.
    pub safe_name: String,
    pub mime: Option<String>,
    pub size: u64,
    pub chunk_size: u32,
    pub sha256_expected: Option<String>,
    pub sha256_actual: Option<String>,
    pub state: TransferState,
    /// Receptor: bytes contíguos duráveis. Emissor: último `FileAck`.
    pub bytes_done: u64,
    /// Emissor: caminho local ou `content://` (Android).
    pub source: Option<String>,
    /// Receptor: `.part`.
    pub temp_path: Option<String>,
    /// Receptor: destino final após verificação.
    pub final_path: Option<String>,
    pub attempts: u32,
    pub created_at: i64,
    pub updated_at: i64,
    pub completed_at: Option<i64>,
}

pub struct NewTransfer<'a> {
    pub id: &'a str,
    pub direction: Direction,
    pub device_id: &'a str,
    pub original_name: &'a str,
    pub safe_name: &'a str,
    pub mime: Option<&'a str>,
    pub size: u64,
    pub chunk_size: u32,
    pub sha256_expected: Option<&'a str>,
    pub state: &'a TransferState,
    pub source: Option<&'a str>,
    pub temp_path: Option<&'a str>,
}

fn row_to_record(r: &Row) -> rusqlite::Result<TransferRecord> {
    let dir: String = r.get("direction")?;
    let kind: String = r.get("state")?;
    let reason: Option<String> = r.get("state_reason")?;
    let size: i64 = r.get("size")?;
    let bytes_done: i64 = r.get("bytes_done")?;
    let chunk_size: i64 = r.get("chunk_size")?;
    let attempts: i64 = r.get("attempts")?;
    Ok(TransferRecord {
        id: r.get("id")?,
        direction: Direction::parse(&dir).unwrap_or(Direction::In),
        device_id: r.get("device_id")?,
        original_name: r.get("original_name")?,
        safe_name: r.get("safe_name")?,
        mime: r.get("mime")?,
        size: size.max(0) as u64,
        chunk_size: chunk_size.max(0) as u32,
        sha256_expected: r.get("sha256_expected")?,
        sha256_actual: r.get("sha256_actual")?,
        state: TransferState::from_db(&kind, reason.as_deref()).unwrap_or(TransferState::Failed { error: format!("estado desconhecido: {kind}") }),
        bytes_done: bytes_done.max(0) as u64,
        source: r.get("source")?,
        temp_path: r.get("temp_path")?,
        final_path: r.get("final_path")?,
        attempts: attempts.max(0) as u32,
        created_at: r.get("created_at")?,
        updated_at: r.get("updated_at")?,
        completed_at: r.get("completed_at")?,
    })
}

const COLS: &str = "id, direction, device_id, original_name, safe_name, mime, size, chunk_size, sha256_expected, sha256_actual, \
                    state, state_reason, bytes_done, source, temp_path, final_path, attempts, created_at, updated_at, completed_at";

impl Db {
    pub fn transfer_insert(&self, t: &NewTransfer) -> Result<()> {
        let now = now_ms();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO file_transfers (id, direction, device_id, original_name, safe_name, mime, size, chunk_size, sha256_expected, \
             state, state_reason, bytes_done, source, temp_path, attempts, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0, ?12, ?13, 0, ?14, ?14)",
            params![
                t.id,
                t.direction.as_str(),
                t.device_id,
                t.original_name,
                t.safe_name,
                t.mime,
                t.size as i64,
                t.chunk_size as i64,
                t.sha256_expected,
                t.state.kind(),
                t.state.reason(),
                t.source,
                t.temp_path,
                now,
            ],
        )?;
        Ok(())
    }

    pub fn transfer_get(&self, id: &str) -> Result<Option<TransferRecord>> {
        let conn = self.conn.lock().unwrap();
        Ok(conn
            .query_row(&format!("SELECT {COLS} FROM file_transfers WHERE id = ?1"), params![id], row_to_record)
            .optional()?)
    }

    /// Lookup sempre escopado pelo device da sessão: um peer não pode agir sobre
    /// transferências de outro só por conhecer o id.
    pub fn transfer_get_for_device(&self, device_id: &str, id: &str) -> Result<Option<TransferRecord>> {
        let conn = self.conn.lock().unwrap();
        Ok(conn
            .query_row(
                &format!("SELECT {COLS} FROM file_transfers WHERE id = ?1 AND device_id = ?2"),
                params![id, device_id],
                row_to_record,
            )
            .optional()?)
    }

    pub fn transfer_set_state(&self, id: &str, state: &TransferState) -> Result<()> {
        let now = now_ms();
        let completed: Option<i64> = if state.is_terminal() { Some(now) } else { None };
        let conn = self.conn.lock().unwrap();
        let n = conn.execute(
            "UPDATE file_transfers SET state = ?2, state_reason = ?3, updated_at = ?4, \
             completed_at = COALESCE(?5, completed_at) WHERE id = ?1",
            params![id, state.kind(), state.reason(), now, completed],
        )?;
        if n == 0 {
            return Err(AppError::Other(format!("transferência {id} não encontrada")));
        }
        Ok(())
    }

    /// Checkpoint durável (receptor: após fsync; emissor: após `FileAck`).
    pub fn transfer_set_progress(&self, id: &str, bytes_done: u64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE file_transfers SET bytes_done = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, bytes_done as i64, now_ms()],
        )?;
        Ok(())
    }

    pub fn transfer_set_hashes(&self, id: &str, expected: Option<&str>, actual: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE file_transfers SET sha256_expected = COALESCE(?2, sha256_expected), \
             sha256_actual = COALESCE(?3, sha256_actual), updated_at = ?4 WHERE id = ?1",
            params![id, expected, actual, now_ms()],
        )?;
        Ok(())
    }

    pub fn transfer_set_paths(&self, id: &str, temp_path: Option<&str>, final_path: Option<&str>, safe_name: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE file_transfers SET temp_path = COALESCE(?2, temp_path), final_path = COALESCE(?3, final_path), \
             safe_name = COALESCE(?4, safe_name), updated_at = ?5 WHERE id = ?1",
            params![id, temp_path, final_path, safe_name, now_ms()],
        )?;
        Ok(())
    }

    /// Cancelamento local decidido sem conexão: o peer ainda não sabe. Marcamos
    /// para avisar na próxima conexão (`state_reason`, sem coluna nova).
    pub const REASON_PENDING_NOTIFY: &'static str = "local-pending-notify";

    pub fn transfer_mark_pending_notify(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE file_transfers SET state_reason = ?2, updated_at = ?3 WHERE id = ?1 AND state = 'cancelled'",
            params![id, Self::REASON_PENDING_NOTIFY, now_ms()],
        )?;
        Ok(())
    }

    /// Ids cancelados localmente que o peer ainda não soube; limpa a marca.
    pub fn transfer_take_pending_notify(&self, device_id: &str) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut st = conn.prepare("SELECT id FROM file_transfers WHERE device_id = ?1 AND state = 'cancelled' AND state_reason = ?2")?;
        let ids: Vec<String> = st.query_map(params![device_id, Self::REASON_PENDING_NOTIFY], |r| r.get(0))?.filter_map(|r| r.ok()).collect();
        drop(st);
        for id in &ids {
            conn.execute("UPDATE file_transfers SET state_reason = 'local' WHERE id = ?1", params![id])?;
        }
        Ok(ids)
    }

    pub fn transfer_bump_attempts(&self, id: &str) -> Result<u32> {
        let conn = self.conn.lock().unwrap();
        conn.execute("UPDATE file_transfers SET attempts = attempts + 1, updated_at = ?2 WHERE id = ?1", params![id, now_ms()])?;
        let n: i64 = conn.query_row("SELECT attempts FROM file_transfers WHERE id = ?1", params![id], |r| r.get(0))?;
        Ok(n.max(0) as u32)
    }

    /// Mais recentes primeiro.
    pub fn transfer_list(&self, limit: u32) -> Result<Vec<TransferRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut st = conn.prepare(&format!("SELECT {COLS} FROM file_transfers ORDER BY created_at DESC LIMIT ?1"))?;
        let rows = st.query_map(params![limit as i64], row_to_record)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Fila de um device: o que precisa (re)começar quando a conexão existir,
    /// na ordem de criação.
    pub fn transfer_pending_for_device(&self, device_id: &str) -> Result<Vec<TransferRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut st = conn.prepare(&format!(
            "SELECT {COLS} FROM file_transfers WHERE device_id = ?1 AND state IN ('queued','waiting') ORDER BY created_at ASC"
        ))?;
        let rows = st.query_map(params![device_id], row_to_record)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Transferências não terminais de um device (ativas + fila).
    pub fn transfer_open_for_device(&self, device_id: &str) -> Result<Vec<TransferRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut st = conn.prepare(&format!(
            "SELECT {COLS} FROM file_transfers WHERE device_id = ?1 AND state NOT IN ('completed','failed','cancelled') ORDER BY created_at ASC"
        ))?;
        let rows = st.query_map(params![device_id], row_to_record)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Na subida do processo nada está em voo: o que ficou `offered/incoming/
    /// transferring/verifying` vira `waiting` (retomável). Retorna os ids afetados.
    pub fn transfer_sweep_on_startup(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut st = conn.prepare("SELECT id FROM file_transfers WHERE state IN ('offered','incoming','transferring','verifying')")?;
        let ids: Vec<String> = st.query_map([], |r| r.get(0))?.filter_map(|r| r.ok()).collect();
        drop(st);
        conn.execute(
            "UPDATE file_transfers SET state = 'waiting', state_reason = 'restart', updated_at = ?1 \
             WHERE state IN ('offered','incoming','transferring','verifying')",
            params![now_ms()],
        )?;
        Ok(ids)
    }

    /// Abandona `waiting` velhas e apaga terminadas fora da retenção. Retorna
    /// `(expiradas, apagadas)`; as expiradas ainda têm `.part` para o chamador limpar.
    pub fn transfer_purge(&self, now: i64) -> Result<(Vec<TransferRecord>, u32)> {
        let conn = self.conn.lock().unwrap();
        let mut st = conn.prepare(&format!("SELECT {COLS} FROM file_transfers WHERE state IN ('queued','waiting') AND updated_at < ?1"))?;
        let expired: Vec<TransferRecord> = st.query_map(params![now - WAITING_EXPIRY_MS], row_to_record)?.filter_map(|r| r.ok()).collect();
        drop(st);
        for t in &expired {
            conn.execute(
                "UPDATE file_transfers SET state = 'failed', state_reason = 'expired', updated_at = ?2, completed_at = ?2 WHERE id = ?1",
                params![t.id, now],
            )?;
        }
        let deleted = conn.execute(
            "DELETE FROM file_transfers WHERE state IN ('completed','failed','cancelled') AND COALESCE(completed_at, updated_at) < ?1",
            params![now - RETENTION_MS],
        )?;
        Ok((expired, deleted as u32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transfer::state::CancelledBy;

    fn temp_db() -> (Db, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("clipsync-test-{}", uuid::Uuid::new_v4()));
        let db = Db::open(&dir).unwrap();
        (db, dir)
    }

    fn new(id: &str, dir: Direction, device: &str, state: &TransferState) -> NewTransfer<'static> {
        NewTransfer {
            id: Box::leak(id.to_string().into_boxed_str()),
            direction: dir,
            device_id: Box::leak(device.to_string().into_boxed_str()),
            original_name: "../evil.txt",
            safe_name: "evil.txt",
            mime: Some("text/plain"),
            size: 1024,
            chunk_size: 256 * 1024,
            sha256_expected: None,
            state: Box::leak(Box::new(state.clone())),
            source: None,
            temp_path: None,
        }
    }

    #[test]
    fn insert_get_update_round_trip() {
        let (db, dir) = temp_db();
        db.transfer_insert(&new("t1", Direction::In, "devA", &TransferState::Incoming)).unwrap();
        let t = db.transfer_get("t1").unwrap().unwrap();
        assert_eq!(t.state, TransferState::Incoming);
        assert_eq!(t.direction, Direction::In);
        assert_eq!(t.original_name, "../evil.txt");
        assert_eq!(t.safe_name, "evil.txt");
        assert_eq!(t.bytes_done, 0);
        assert!(t.completed_at.is_none());

        db.transfer_set_state("t1", &TransferState::Transferring).unwrap();
        db.transfer_set_progress("t1", 512).unwrap();
        db.transfer_set_hashes("t1", Some("aa"), None).unwrap();
        db.transfer_set_paths("t1", Some("/tmp/t1.part"), None, None).unwrap();
        let t = db.transfer_get("t1").unwrap().unwrap();
        assert_eq!(t.state, TransferState::Transferring);
        assert_eq!(t.bytes_done, 512);
        assert_eq!(t.sha256_expected.as_deref(), Some("aa"));
        assert_eq!(t.temp_path.as_deref(), Some("/tmp/t1.part"));

        db.transfer_set_state("t1", &TransferState::Failed { error: "checksum:x".into() }).unwrap();
        let t = db.transfer_get("t1").unwrap().unwrap();
        assert_eq!(t.state, TransferState::Failed { error: "checksum:x".into() });
        assert!(t.completed_at.is_some());
        assert_eq!(db.transfer_bump_attempts("t1").unwrap(), 1);
        assert!(db.transfer_set_state("nope", &TransferState::Completed).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn device_scoping_and_queue_order() {
        let (db, dir) = temp_db();
        db.transfer_insert(&new("a1", Direction::Out, "devA", &TransferState::Queued)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        db.transfer_insert(&new("a2", Direction::Out, "devA", &TransferState::Waiting { reason: "disconnected".into() })).unwrap();
        db.transfer_insert(&new("b1", Direction::Out, "devB", &TransferState::Queued)).unwrap();
        db.transfer_insert(&new("a3", Direction::Out, "devA", &TransferState::Completed)).unwrap();

        assert!(db.transfer_get_for_device("devB", "a1").unwrap().is_none());
        assert!(db.transfer_get_for_device("devA", "a1").unwrap().is_some());

        let q: Vec<String> = db.transfer_pending_for_device("devA").unwrap().into_iter().map(|t| t.id).collect();
        assert_eq!(q, vec!["a1", "a2"]);
        assert_eq!(db.transfer_open_for_device("devA").unwrap().len(), 2);

        db.transfer_set_state("a1", &TransferState::Offered).unwrap();

        assert_eq!(db.transfer_list(10).unwrap().len(), 4);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pending_notify_round_trip() {
        let (db, dir) = temp_db();
        db.transfer_insert(&new("c1", Direction::Out, "d", &TransferState::Cancelled { by: CancelledBy::Local })).unwrap();
        db.transfer_insert(&new("c2", Direction::Out, "d", &TransferState::Cancelled { by: CancelledBy::Local })).unwrap();
        db.transfer_insert(&new("q1", Direction::Out, "d", &TransferState::Queued)).unwrap();
        db.transfer_mark_pending_notify("c1").unwrap();
        db.transfer_mark_pending_notify("q1").unwrap(); // não cancelada: ignorado
        assert_eq!(db.transfer_take_pending_notify("d").unwrap(), vec!["c1"]);
        assert!(db.transfer_take_pending_notify("d").unwrap().is_empty());
        // continua lendo como cancelada local
        assert_eq!(db.transfer_get("c1").unwrap().unwrap().state, TransferState::Cancelled { by: CancelledBy::Local });
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn startup_sweep_moves_in_flight_to_waiting() {
        let (db, dir) = temp_db();
        db.transfer_insert(&new("x1", Direction::In, "d", &TransferState::Transferring)).unwrap();
        db.transfer_insert(&new("x2", Direction::Out, "d", &TransferState::Offered)).unwrap();
        db.transfer_insert(&new("x3", Direction::Out, "d", &TransferState::Queued)).unwrap();
        db.transfer_insert(&new("x4", Direction::In, "d", &TransferState::Completed)).unwrap();
        let mut ids = db.transfer_sweep_on_startup().unwrap();
        ids.sort();
        assert_eq!(ids, vec!["x1", "x2"]);
        assert_eq!(db.transfer_get("x1").unwrap().unwrap().state, TransferState::Waiting { reason: "restart".into() });
        assert_eq!(db.transfer_get("x3").unwrap().unwrap().state, TransferState::Queued);
        assert_eq!(db.transfer_get("x4").unwrap().unwrap().state, TransferState::Completed);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn purge_expires_stale_waiting_and_deletes_old_terminal() {
        let (db, dir) = temp_db();
        db.transfer_insert(&new("w", Direction::Out, "d", &TransferState::Waiting { reason: "disconnected".into() })).unwrap();
        db.transfer_insert(&new("c", Direction::Out, "d", &TransferState::Cancelled { by: CancelledBy::Local })).unwrap();
        db.transfer_insert(&new("fresh", Direction::Out, "d", &TransferState::Queued)).unwrap();
        // Nada expira "agora".
        let (exp, del) = db.transfer_purge(now_ms()).unwrap();
        assert!(exp.is_empty());
        assert_eq!(del, 0);
        // 8 dias depois: waiting expira; cancelada (dentro de 30 dias) fica.
        let later = now_ms() + WAITING_EXPIRY_MS + 24 * 60 * 60 * 1_000;
        let (exp, del) = db.transfer_purge(later).unwrap();
        assert_eq!(exp.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), vec!["w", "fresh"]);
        assert_eq!(del, 0);
        assert_eq!(db.transfer_get("w").unwrap().unwrap().state, TransferState::Failed { error: "expired".into() });
        // 30 dias depois de virarem terminais: somem.
        let (_, del) = db.transfer_purge(later + RETENTION_MS + 60_000).unwrap();
        assert_eq!(del, 3);
        assert!(db.transfer_list(10).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
