//! Histórico local em SQLite. Cada dispositivo tem o seu; nada é central.
use crate::content::ContentType;
use crate::error::Result;
use crate::models::{now_ms, ClipboardItem, Device, DeviceState, Settings, SyncState};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::Path;
use std::sync::Mutex;

pub struct Db {
    pub(crate) conn: Mutex<Connection>,
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS clipboard_items (
  id TEXT PRIMARY KEY,
  content TEXT NOT NULL,
  content_type TEXT NOT NULL,
  content_hash TEXT NOT NULL,
  source_device_id TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  expires_at INTEGER,
  is_pinned INTEGER NOT NULL DEFAULT 0,
  is_sensitive INTEGER NOT NULL DEFAULT 0,
  sync_state TEXT NOT NULL DEFAULT 'local',
  metadata TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS idx_items_created ON clipboard_items(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_items_hash ON clipboard_items(content_hash);

CREATE TABLE IF NOT EXISTS devices (
  device_id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  platform TEXT NOT NULL,
  public_key TEXT NOT NULL,
  auto_send INTEGER NOT NULL DEFAULT 1,
  auto_receive INTEGER NOT NULL DEFAULT 1,
  blocked INTEGER NOT NULL DEFAULT 0,
  last_seen INTEGER,
  last_address TEXT,
  paired_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS outbox (
  item_id TEXT NOT NULL,
  device_id TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (item_id, device_id)
);

CREATE TABLE IF NOT EXISTS settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

-- Transferências de arquivo (ver transfer/store.rs). O conteúdo nunca passa por aqui:
-- só metadados, estado e o offset durável; bytes vivem no filesystem (.part).
CREATE TABLE IF NOT EXISTS file_transfers (
  id TEXT PRIMARY KEY,
  direction TEXT NOT NULL,
  device_id TEXT NOT NULL,
  original_name TEXT NOT NULL,
  safe_name TEXT NOT NULL,
  mime TEXT,
  size INTEGER NOT NULL,
  chunk_size INTEGER NOT NULL,
  sha256_expected TEXT,
  sha256_actual TEXT,
  state TEXT NOT NULL,
  state_reason TEXT,
  bytes_done INTEGER NOT NULL DEFAULT 0,
  source TEXT,
  temp_path TEXT,
  final_path TEXT,
  attempts INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  completed_at INTEGER,
  purpose TEXT
);
CREATE INDEX IF NOT EXISTS idx_ft_device_state ON file_transfers(device_id, state);
CREATE INDEX IF NOT EXISTS idx_ft_created ON file_transfers(created_at DESC);
"#;

fn row_to_item(r: &Row) -> rusqlite::Result<ClipboardItem> {
    let ct: String = r.get("content_type")?;
    let meta: String = r.get("metadata")?;
    Ok(ClipboardItem {
        id: r.get("id")?,
        content: r.get("content")?,
        content_type: ContentType::parse(&ct),
        source_device_id: r.get("source_device_id")?,
        created_at: r.get("created_at")?,
        expires_at: r.get("expires_at")?,
        is_pinned: r.get::<_, i64>("is_pinned")? != 0,
        is_sensitive: r.get::<_, i64>("is_sensitive")? != 0,
        sync_state: SyncState::parse(&r.get::<_, String>("sync_state")?),
        metadata: serde_json::from_str(&meta).unwrap_or(serde_json::json!({})),
    })
}

#[derive(Debug, Clone)]
pub struct StoredDevice {
    pub device_id: String,
    pub name: String,
    pub platform: String,
    pub public_key: String,
    pub auto_send: bool,
    pub auto_receive: bool,
    pub blocked: bool,
    pub last_seen: Option<i64>,
    pub last_address: Option<String>,
    pub paired_at: i64,
}

impl StoredDevice {
    pub fn to_device(&self, state: DeviceState, address: Option<String>, queued: u32) -> Device {
        Device {
            device_id: self.device_id.clone(),
            name: self.name.clone(),
            platform: self.platform.clone(),
            public_key: self.public_key.clone(),
            state: if self.blocked { DeviceState::Blocked } else { state },
            is_self: false,
            auto_send: self.auto_send,
            auto_receive: self.auto_receive,
            last_seen: self.last_seen,
            address: address.or_else(|| self.last_address.clone()),
            paired_at: Some(self.paired_at),
            last_error: None,
            queued,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ItemFilter {
    pub query: Option<String>,
    /// "all" | "text" | "code" | "link" | "email" | "image" | "sensitive" | "pinned"
    pub category: Option<String>,
    pub limit: u32,
}

pub fn content_hash(s: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(s.as_bytes()))
}

/// Colunas adicionadas depois da primeira versão do schema (bancos antigos não as têm).
fn migrate(conn: &Connection) -> Result<()> {
    let has_purpose: bool = conn
        .prepare("SELECT 1 FROM pragma_table_info('file_transfers') WHERE name = 'purpose'")?
        .exists([])?;
    if !has_purpose {
        conn.execute_batch("ALTER TABLE file_transfers ADD COLUMN purpose TEXT")?;
    }
    Ok(())
}

impl Db {
    pub fn open(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let conn = Connection::open(dir.join("clipsync.db"))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Insere se o id ainda não existe. Retorna `true` quando inseriu.
    pub fn insert_item(&self, it: &ClipboardItem) -> Result<bool> {
        let c = self.conn.lock().unwrap();
        let n = c.execute(
            "INSERT OR IGNORE INTO clipboard_items
             (id, content, content_type, content_hash, source_device_id, created_at, expires_at,
              is_pinned, is_sensitive, sync_state, metadata)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                it.id,
                it.content,
                it.content_type.as_str(),
                content_hash(&it.content),
                it.source_device_id,
                it.created_at,
                it.expires_at,
                it.is_pinned as i64,
                it.is_sensitive as i64,
                it.sync_state.as_str(),
                it.metadata.to_string(),
            ],
        )?;
        Ok(n > 0)
    }

    /// Remove duplicata recente do mesmo conteúdo (mantém o mais novo; preserva fixados).
    /// Devolve o que saiu: imagem removida deixa arquivos para o chamador apagar.
    pub fn dedupe_content(&self, hash: &str, keep_id: &str) -> Result<Vec<ClipboardItem>> {
        let c = self.conn.lock().unwrap();
        let gone = {
            let mut st = c.prepare("SELECT * FROM clipboard_items WHERE content_hash = ?1 AND id != ?2 AND is_pinned = 0")?;
            let v: Vec<ClipboardItem> = st.query_map(params![hash, keep_id], row_to_item)?.filter_map(|r| r.ok()).collect();
            v
        };
        c.execute(
            "DELETE FROM clipboard_items WHERE content_hash = ?1 AND id != ?2 AND is_pinned = 0",
            params![hash, keep_id],
        )?;
        c.execute("DELETE FROM outbox WHERE item_id NOT IN (SELECT id FROM clipboard_items)", [])?;
        Ok(gone)
    }

    pub fn get_item(&self, id: &str) -> Result<Option<ClipboardItem>> {
        let c = self.conn.lock().unwrap();
        Ok(c.query_row("SELECT * FROM clipboard_items WHERE id = ?1", [id], row_to_item)
            .optional()?)
    }

    pub fn latest_item(&self) -> Result<Option<ClipboardItem>> {
        let c = self.conn.lock().unwrap();
        Ok(c.query_row(
            "SELECT * FROM clipboard_items ORDER BY created_at DESC LIMIT 1",
            [],
            row_to_item,
        )
        .optional()?)
    }

    pub fn list_items(&self, f: &ItemFilter) -> Result<Vec<ClipboardItem>> {
        let c = self.conn.lock().unwrap();
        let mut sql = String::from("SELECT * FROM clipboard_items WHERE 1=1");
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(q) = f.query.as_ref().filter(|q| !q.trim().is_empty()) {
            // Imagem guarda só o hash em `content`: a busca casa pelo tipo ("imagem"/"image"), não por ele.
            sql.push_str(" AND ((content_type != 'image' AND content LIKE ?1) OR content_type LIKE ?1 OR source_device_id LIKE ?1)");
            args.push(Box::new(format!("%{}%", q.trim())));
        }
        match f.category.as_deref() {
            Some("pinned") => sql.push_str(" AND is_pinned = 1"),
            Some("sensitive") => sql.push_str(" AND is_sensitive = 1"),
            Some("text") => sql.push_str(" AND content_type = 'text'"),
            Some("link") => sql.push_str(" AND content_type = 'link'"),
            Some("email") => sql.push_str(" AND content_type = 'email'"),
            Some("image") => sql.push_str(" AND content_type = 'image'"),
            Some("code") => sql.push_str(" AND content_type IN ('code','command')"),
            _ => {}
        }
        sql.push_str(" ORDER BY is_pinned DESC, created_at DESC LIMIT ");
        sql.push_str(&(if f.limit == 0 { 200 } else { f.limit }).to_string());
        let mut st = c.prepare(&sql)?;
        let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
        let rows = st.query_map(refs.as_slice(), row_to_item)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Itens de imagem, do mais antigo para o mais novo (`unpinned_only`: só os não fixados).
    pub fn image_items(&self, unpinned_only: bool) -> Result<Vec<ClipboardItem>> {
        let c = self.conn.lock().unwrap();
        let sql = format!(
            "SELECT * FROM clipboard_items WHERE content_type = 'image'{} ORDER BY created_at ASC",
            if unpinned_only { " AND is_pinned = 0" } else { "" }
        );
        let mut st = c.prepare(&sql)?;
        let rows = st.query_map([], row_to_item)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Mantém no máximo `max` imagens não fixadas: apaga as mais antigas e devolve o que saiu.
    pub fn evict_images(&self, max: usize) -> Result<Vec<ClipboardItem>> {
        let all = self.image_items(true)?;
        let excess = all.len().saturating_sub(max);
        let gone: Vec<ClipboardItem> = all.into_iter().take(excess).collect();
        for it in &gone {
            self.delete_item(&it.id)?;
        }
        Ok(gone)
    }

    pub fn count_items(&self) -> Result<(u32, u32)> {
        let c = self.conn.lock().unwrap();
        let total: u32 = c.query_row("SELECT COUNT(*) FROM clipboard_items", [], |r| r.get(0))?;
        let pinned: u32 =
            c.query_row("SELECT COUNT(*) FROM clipboard_items WHERE is_pinned = 1", [], |r| r.get(0))?;
        Ok((total, pinned))
    }

    pub fn set_pinned(&self, id: &str, pinned: bool) -> Result<()> {
        let c = self.conn.lock().unwrap();
        c.execute(
            "UPDATE clipboard_items SET is_pinned = ?2, expires_at = CASE WHEN ?2 = 1 THEN NULL ELSE expires_at END WHERE id = ?1",
            params![id, pinned as i64],
        )?;
        Ok(())
    }

    pub fn set_sync_state(&self, id: &str, state: SyncState) -> Result<()> {
        let c = self.conn.lock().unwrap();
        c.execute("UPDATE clipboard_items SET sync_state = ?2 WHERE id = ?1", params![id, state.as_str()])?;
        Ok(())
    }

    pub fn delete_item(&self, id: &str) -> Result<()> {
        let c = self.conn.lock().unwrap();
        c.execute("DELETE FROM clipboard_items WHERE id = ?1", [id])?;
        c.execute("DELETE FROM outbox WHERE item_id = ?1", [id])?;
        Ok(())
    }

    pub fn clear_items(&self, keep_pinned: bool) -> Result<u32> {
        let c = self.conn.lock().unwrap();
        let n = if keep_pinned {
            c.execute("DELETE FROM clipboard_items WHERE is_pinned = 0", [])?
        } else {
            c.execute("DELETE FROM clipboard_items", [])?
        };
        c.execute("DELETE FROM outbox", [])?;
        Ok(n as u32)
    }

    pub fn purge_expired(&self) -> Result<Vec<ClipboardItem>> {
        let c = self.conn.lock().unwrap();
        let now = now_ms();
        let ids: Vec<ClipboardItem> = {
            let mut st = c.prepare(
                "SELECT * FROM clipboard_items WHERE expires_at IS NOT NULL AND expires_at <= ?1 AND is_pinned = 0",
            )?;
            let v: Vec<ClipboardItem> = st.query_map([now], row_to_item)?.filter_map(|r| r.ok()).collect();
            v
        };
        if !ids.is_empty() {
            c.execute(
                "DELETE FROM clipboard_items WHERE expires_at IS NOT NULL AND expires_at <= ?1 AND is_pinned = 0",
                [now],
            )?;
        }
        // Fila de envio de item expirado nunca sai: sem isto o "N na fila" cresce para sempre.
        c.execute("DELETE FROM outbox WHERE item_id NOT IN (SELECT id FROM clipboard_items)", [])?;
        Ok(ids)
    }

    pub fn upsert_device(&self, d: &StoredDevice) -> Result<()> {
        let c = self.conn.lock().unwrap();
        c.execute(
            "INSERT INTO devices (device_id, name, platform, public_key, auto_send, auto_receive, blocked, last_seen, last_address, paired_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
             ON CONFLICT(device_id) DO UPDATE SET
               name = excluded.name, platform = excluded.platform, public_key = excluded.public_key,
               last_seen = COALESCE(excluded.last_seen, devices.last_seen),
               last_address = COALESCE(excluded.last_address, devices.last_address)",
            params![
                d.device_id,
                d.name,
                d.platform,
                d.public_key,
                d.auto_send as i64,
                d.auto_receive as i64,
                d.blocked as i64,
                d.last_seen,
                d.last_address,
                d.paired_at
            ],
        )?;
        Ok(())
    }

    pub fn touch_device(&self, id: &str, address: Option<&str>) -> Result<()> {
        let c = self.conn.lock().unwrap();
        c.execute(
            "UPDATE devices SET last_seen = ?2, last_address = COALESCE(?3, last_address) WHERE device_id = ?1",
            params![id, now_ms(), address],
        )?;
        Ok(())
    }

    /// Endereço informado pelo usuário; o mDNS sobrescreve depois, via `touch_device`.
    pub fn set_last_address(&self, id: &str, address: &str) -> Result<()> {
        let c = self.conn.lock().unwrap();
        c.execute("UPDATE devices SET last_address = ?2 WHERE device_id = ?1", params![id, address])?;
        Ok(())
    }

    pub fn list_devices(&self) -> Result<Vec<StoredDevice>> {
        let c = self.conn.lock().unwrap();
        let mut st = c.prepare("SELECT * FROM devices ORDER BY paired_at ASC")?;
        let rows = st.query_map([], |r| {
            Ok(StoredDevice {
                device_id: r.get("device_id")?,
                name: r.get("name")?,
                platform: r.get("platform")?,
                public_key: r.get("public_key")?,
                auto_send: r.get::<_, i64>("auto_send")? != 0,
                auto_receive: r.get::<_, i64>("auto_receive")? != 0,
                blocked: r.get::<_, i64>("blocked")? != 0,
                last_seen: r.get("last_seen")?,
                last_address: r.get("last_address")?,
                paired_at: r.get("paired_at")?,
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn get_device(&self, id: &str) -> Result<Option<StoredDevice>> {
        Ok(self.list_devices()?.into_iter().find(|d| d.device_id == id))
    }

    pub fn device_by_public_key(&self, pk_hex: &str) -> Result<Option<StoredDevice>> {
        Ok(self.list_devices()?.into_iter().find(|d| d.public_key == pk_hex))
    }

    pub fn set_device_flags(&self, id: &str, auto_send: Option<bool>, auto_receive: Option<bool>, blocked: Option<bool>) -> Result<()> {
        let c = self.conn.lock().unwrap();
        if let Some(v) = auto_send {
            c.execute("UPDATE devices SET auto_send = ?2 WHERE device_id = ?1", params![id, v as i64])?;
        }
        if let Some(v) = auto_receive {
            c.execute("UPDATE devices SET auto_receive = ?2 WHERE device_id = ?1", params![id, v as i64])?;
        }
        if let Some(v) = blocked {
            c.execute("UPDATE devices SET blocked = ?2 WHERE device_id = ?1", params![id, v as i64])?;
        }
        Ok(())
    }

    /// Remove o dispositivo e revoga a confiança (chave pública deixa de ser aceita).
    pub fn remove_device(&self, id: &str) -> Result<()> {
        let c = self.conn.lock().unwrap();
        c.execute("DELETE FROM devices WHERE device_id = ?1", [id])?;
        c.execute("DELETE FROM outbox WHERE device_id = ?1", [id])?;
        Ok(())
    }

    pub fn enqueue(&self, item_id: &str, device_id: &str) -> Result<()> {
        let c = self.conn.lock().unwrap();
        c.execute(
            "INSERT OR IGNORE INTO outbox (item_id, device_id, created_at) VALUES (?1, ?2, ?3)",
            params![item_id, device_id, now_ms()],
        )?;
        Ok(())
    }

    pub fn dequeue(&self, item_id: &str, device_id: &str) -> Result<()> {
        let c = self.conn.lock().unwrap();
        c.execute("DELETE FROM outbox WHERE item_id = ?1 AND device_id = ?2", params![item_id, device_id])?;
        Ok(())
    }

    pub fn outbox_for(&self, device_id: &str) -> Result<Vec<String>> {
        let c = self.conn.lock().unwrap();
        let mut st = c.prepare("SELECT item_id FROM outbox WHERE device_id = ?1 ORDER BY created_at ASC")?;
        let rows = st.query_map([device_id], |r| r.get(0))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// O item ainda espera Ack de algum dispositivo.
    pub fn outbox_has_item(&self, item_id: &str) -> Result<bool> {
        let c = self.conn.lock().unwrap();
        Ok(c.query_row("SELECT EXISTS(SELECT 1 FROM outbox WHERE item_id = ?1)", [item_id], |r| r.get(0))?)
    }

    pub fn outbox_counts(&self) -> Result<std::collections::HashMap<String, u32>> {
        let c = self.conn.lock().unwrap();
        let mut st = c.prepare("SELECT device_id, COUNT(*) FROM outbox GROUP BY device_id")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?)))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn outbox_total(&self) -> Result<u32> {
        let c = self.conn.lock().unwrap();
        Ok(c.query_row("SELECT COUNT(*) FROM outbox", [], |r| r.get(0))?)
    }

    pub fn load_settings(&self) -> Result<Settings> {
        let c = self.conn.lock().unwrap();
        let raw: Option<String> = c
            .query_row("SELECT value FROM settings WHERE key = 'settings'", [], |r| r.get(0))
            .optional()?;
        let mut s: Settings = raw.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        s.upgrade_types();
        Ok(s)
    }

    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        let c = self.conn.lock().unwrap();
        c.execute(
            "INSERT INTO settings (key, value) VALUES ('settings', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [serde_json::to_string(s)?],
        )?;
        Ok(())
    }

    pub fn get_flag(&self, key: &str) -> Result<Option<String>> {
        let c = self.conn.lock().unwrap();
        Ok(c.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional()?)
    }

    pub fn set_flag(&self, key: &str, value: &str) -> Result<()> {
        let c = self.conn.lock().unwrap();
        c.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::ContentType;
    use crate::models::now_ms;

    fn image(id: &str, created_at: i64, pinned: bool) -> ClipboardItem {
        ClipboardItem {
            content_type: ContentType::Image,
            created_at,
            is_pinned: pinned,
            ..item(id, &format!("img:{id}"), None)
        }
    }

    fn item(id: &str, content: &str, expires_at: Option<i64>) -> ClipboardItem {
        ClipboardItem {
            id: id.into(),
            content: content.into(),
            content_type: ContentType::Text,
            source_device_id: "me".into(),
            created_at: now_ms(),
            expires_at,
            is_pinned: false,
            is_sensitive: false,
            sync_state: SyncState::Queued,
            metadata: serde_json::json!({}),
        }
    }

    #[test]
    fn removed_items_leave_no_outbox_rows() {
        let dir = std::env::temp_dir().join(format!("clipsync-db-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let db = Db::open(&dir).unwrap();
        db.insert_item(&item("old", "a", Some(now_ms() - 1))).unwrap();
        db.insert_item(&item("dup", "b", None)).unwrap();
        db.insert_item(&item("new", "b", None)).unwrap();
        for id in ["old", "dup", "new"] {
            db.enqueue(id, "peer").unwrap();
        }
        assert_eq!(db.outbox_total().unwrap(), 3);
        db.purge_expired().unwrap();
        assert_eq!(db.outbox_total().unwrap(), 2);
        db.dedupe_content(&content_hash("b"), "new").unwrap();
        assert_eq!(db.outbox_for("peer").unwrap(), vec!["new".to_string()]);
        // Ack de um peer não conclui o item enquanto outro ainda espera.
        db.enqueue("new", "peer2").unwrap();
        db.dequeue("new", "peer").unwrap();
        assert!(db.outbox_has_item("new").unwrap());
        db.dequeue("new", "peer2").unwrap();
        assert!(!db.outbox_has_item("new").unwrap());
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn image_limit_evicts_oldest_unpinned() {
        let dir = std::env::temp_dir().join(format!("clipsync-db-img-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let db = Db::open(&dir).unwrap();
        // A mais antiga está fixada: fica, e não conta para o limite.
        db.insert_item(&image("pinned", 1, true)).unwrap();
        for i in 0..4 {
            db.insert_item(&image(&format!("i{i}"), 10 + i, false)).unwrap();
        }
        db.insert_item(&item("txt", "texto", None)).unwrap();
        let gone = db.evict_images(2).unwrap();
        assert_eq!(gone.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["i0", "i1"]);
        let left: Vec<String> = db.image_items(false).unwrap().into_iter().map(|i| i.id).collect();
        assert_eq!(left, ["pinned", "i2", "i3"]);
        assert!(db.get_item("txt").unwrap().is_some());
        // Busca textual não acha imagem pelo hash em `content`, só pelo tipo.
        let q = |s: &str| db.list_items(&ItemFilter { query: Some(s.into()), ..Default::default() }).unwrap().len();
        assert_eq!(q("img:i2"), 0);
        assert_eq!(q("image"), 3);
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
