//! Persistent history storage for husk — the `husk.db` analog of opencode.db.
//!
//! A single SQLite file holds sessions, messages, full-fidelity tool parts
//! (never truncated here: the DB is the source of truth that the in-RAM
//! context window compresses *from*), per-request usage records, counters,
//! and an FTS5 index for index-backed search.
//!
//! The connection is synchronous and wrapped in a mutex: one writer, tiny
//! footprint, WAL mode for cheap reads while streaming.

use crate::tokenutil::{estimate_tokens, TokenLedger, TokenUsage};
use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    pub role: Role,
    pub content: String,
    pub tokens: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => "system",
            Role::Tool => "tool",
        }
    }
    fn from_str(s: &str) -> Role {
        match s {
            "assistant" => Role::Assistant,
            "system" => Role::System,
            "tool" => Role::Tool,
            _ => Role::User,
        }
    }
}

/// A high-fidelity artifact attached to a message: a tool call, its full
/// output, or a worker-produced summary. Full content lives here, not in RAM.
#[derive(Debug, Clone)]
pub struct Part {
    pub id: String,
    pub message_id: String,
    pub kind: PartKind,
    pub name: String,
    pub content: String,
    pub tokens: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartKind {
    ToolCall,
    ToolOutput,
    Summary,
}

impl PartKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            PartKind::ToolCall => "tool_call",
            PartKind::ToolOutput => "tool_output",
            PartKind::Summary => "summary",
        }
    }
    fn from_str(s: &str) -> PartKind {
        match s {
            "tool_output" => PartKind::ToolOutput,
            "summary" => PartKind::Summary,
            _ => PartKind::ToolCall,
        }
    }
}

const SCHEMA_VERSION: i64 = 1;

const MIGRATIONS: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id         TEXT PRIMARY KEY,
    title      TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    provider   TEXT NOT NULL DEFAULT '',
    model      TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS messages (
    id         TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    role       TEXT NOT NULL,
    content    TEXT NOT NULL,
    tokens     INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id, created_at);
CREATE TABLE IF NOT EXISTS parts (
    id         TEXT PRIMARY KEY,
    message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    kind       TEXT NOT NULL,
    name       TEXT NOT NULL DEFAULT '',
    content    TEXT NOT NULL,
    tokens     INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_parts_message ON parts(message_id);
CREATE TABLE IF NOT EXISTS usage_records (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id        TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    model             TEXT NOT NULL,
    prompt_tokens     INTEGER NOT NULL,
    cached_tokens     INTEGER NOT NULL,
    completion_tokens INTEGER NOT NULL,
    created_at        TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_usage_session ON usage_records(session_id);
CREATE TABLE IF NOT EXISTS session_counters (
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    key        TEXT NOT NULL,
    value      INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (session_id, key)
);
CREATE TRIGGER IF NOT EXISTS parts_ad AFTER DELETE ON parts BEGIN
    DELETE FROM search_index WHERE ref_id = old.id;
END;
"#;

pub struct Database {
    conn: Mutex<Connection>,
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create data dir {}", parent.display()))?;
            }
        }
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open database at {}", path.display()))?;
        let db = Self { conn: Mutex::new(conn) };
        db.migrate()?;
        Ok(db)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn: Mutex::new(conn) };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version < SCHEMA_VERSION {
            conn.execute_batch(MIGRATIONS)?;
            // FTS5 may be unavailable in odd SQLite builds; degrade to no index
            // instead of failing to open the database at all.
            let _ = conn.execute_batch(
                "CREATE VIRTUAL TABLE IF NOT EXISTS search_index USING fts5(
                     ref_id UNINDEXED, ref_kind UNINDEXED, title, body,
                     tokenize = 'porter unicode61'
                 );",
            );
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        Ok(())
    }

    // ---- sessions ----

    pub fn create_session(&self, provider: &str, model: &str, title: Option<&str>) -> Result<Session> {
        let now = now_rfc3339();
        let session = Session {
            id: uuid::Uuid::new_v4().to_string(),
            title: title
                .map(str::to_string)
                .unwrap_or_else(|| format!("session {}", &now[..19])),
            created_at: now.clone(),
            updated_at: now,
            provider: provider.to_string(),
            model: model.to_string(),
        };
        self.conn.lock().unwrap().execute(
            "INSERT INTO sessions (id, title, created_at, updated_at, provider, model)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                session.id,
                session.title,
                session.created_at,
                session.updated_at,
                session.provider,
                session.model
            ],
        )?;
        Ok(session)
    }

    pub fn get_session(&self, id: &str) -> Result<Option<Session>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, created_at, updated_at, provider, model FROM sessions WHERE id = ?1",
        )?;
        let mut rows = stmt.query([id])?;
        if let Some(row) = rows.next()? {
            return Ok(Some(Session {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                provider: row.get(4)?,
                model: row.get(5)?,
            }));
        }
        Ok(None)
    }

    pub fn list_sessions(&self, limit: usize) -> Result<Vec<Session>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, created_at, updated_at, provider, model
             FROM sessions ORDER BY updated_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit as i64], |row| {
            Ok(Session {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                provider: row.get(4)?,
                model: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn rename_session(&self, id: &str, title: &str) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE sessions SET title = ?2, updated_at = ?3 WHERE id = ?1",
            rusqlite::params![id, title, now_rfc3339()],
        )?;
        Ok(())
    }

    pub fn touch_session(&self, id: &str) -> Result<()> {
        self.rename_session_keep_title(id)
    }

    fn rename_session_keep_title(&self, id: &str) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE sessions SET updated_at = ?2 WHERE id = ?1",
            rusqlite::params![id, now_rfc3339()],
        )?;
        Ok(())
    }

    pub fn delete_session(&self, id: &str) -> Result<()> {
        self.conn.lock().unwrap().execute("DELETE FROM sessions WHERE id = ?1", [id])?;
        Ok(())
    }

    // ---- messages ----

    pub fn append_message(&self, session_id: &str, role: Role, content: &str) -> Result<Message> {
        let now = now_rfc3339();
        let message = Message {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            role,
            content: content.to_string(),
            tokens: estimate_tokens(content) as u64,
            created_at: now,
        };
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO messages (id, session_id, role, content, tokens, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                message.id,
                message.session_id,
                message.role.as_str(),
                message.content,
                message.tokens as i64,
                message.created_at
            ],
        )?;
        conn.execute(
            "INSERT INTO search_index (ref_id, ref_kind, title, body)
             VALUES (?1, 'message', ?2, ?3)",
            rusqlite::params![message.id, format!("{} message", role.as_str()), message.content],
        )?;
        conn.execute(
            "UPDATE sessions SET updated_at = ?2 WHERE id = ?1",
            rusqlite::params![session_id, message.created_at],
        )?;
        Ok(message)
    }

    /// Recent messages in chronological order, keeping only the newest
    /// `limit` rows in memory (low-RAM discipline for long sessions).
    pub fn recent_messages(&self, session_id: &str, limit: usize) -> Result<Vec<Message>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, session_id, role, content, tokens, created_at FROM (
                 SELECT * FROM messages WHERE session_id = ?1
                 ORDER BY created_at DESC, id DESC LIMIT ?2
             ) ORDER BY created_at ASC, id ASC",
        )?;
        let rows = stmt.query_map(
            rusqlite::params![session_id, limit as i64],
            |row| {
                Ok(Message {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    role: Role::from_str(&row.get::<_, String>(2)?),
                    content: row.get(3)?,
                    tokens: row.get::<_, i64>(4)?.max(0) as u64,
                    created_at: row.get(5)?,
                })
            },
        )?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    // ---- parts ----

    pub fn append_part(
        &self,
        message_id: &str,
        kind: PartKind,
        name: &str,
        content: &str,
    ) -> Result<Part> {
        let now = now_rfc3339();
        let part = Part {
            id: uuid::Uuid::new_v4().to_string(),
            message_id: message_id.to_string(),
            kind,
            name: name.to_string(),
            content: content.to_string(),
            tokens: estimate_tokens(content) as u64,
            created_at: now,
        };
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO parts (id, message_id, kind, name, content, tokens, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                part.id,
                part.message_id,
                part.kind.as_str(),
                part.name,
                part.content,
                part.tokens as i64,
                part.created_at
            ],
        )?;
        conn.execute(
            "INSERT INTO search_index (ref_id, ref_kind, title, body)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![part.id, part.kind.as_str(), part.name, part.content],
        )?;
        Ok(part)
    }

    pub fn get_part(&self, id: &str) -> Result<Option<Part>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, message_id, kind, name, content, tokens, created_at
             FROM parts WHERE id = ?1",
        )?;
        let mut rows = stmt.query([id])?;
        if let Some(row) = rows.next()? {
            return Ok(Some(Part {
                id: row.get(0)?,
                message_id: row.get(1)?,
                kind: PartKind::from_str(&row.get::<_, String>(2)?),
                name: row.get(3)?,
                content: row.get(4)?,
                tokens: row.get::<_, i64>(5)?.max(0) as u64,
                created_at: row.get(6)?,
            }));
        }
        Ok(None)
    }

    /// Index-backed search (FTS5 + porter stemming). Falls back to LIKE when
    /// FTS5 syntax errors occur. Returns (ref_kind, title, snippet-ish body head).
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<(String, String, String)>> {
        let conn = self.conn.lock().unwrap();
        let fts = conn
            .prepare(
                "SELECT ref_kind, title, substr(body, 1, 240) FROM search_index
                 WHERE search_index MATCH ?1 ORDER BY rank LIMIT ?2",
            )
            .and_then(|mut stmt| collect_search(&mut stmt, rusqlite::params![query, limit as i64]))
            .map_err(anyhow::Error::from);
        match fts {
            Ok(rows) => Ok(rows),
            Err(_) => {
                let mut stmt = conn.prepare(
                    "SELECT ref_kind, title, substr(body, 1, 240) FROM search_index
                     WHERE body LIKE '%' || ?1 || '%' LIMIT ?2",
                )?;
                Ok(collect_search(&mut stmt, rusqlite::params![query, limit as i64])?)
            }
        }
    }

    // ---- accounting ----

    pub fn record_usage(&self, session_id: &str, model: &str, usage: TokenUsage) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO usage_records (session_id, model, prompt_tokens, cached_tokens, completion_tokens, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                session_id,
                model,
                usage.prompt_tokens as i64,
                usage.cached_tokens as i64,
                usage.completion_tokens as i64,
                now_rfc3339()
            ],
        )?;
        Ok(())
    }

    pub fn bump_counter(&self, session_id: &str, key: &str, delta: u64) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO session_counters (session_id, key, value) VALUES (?1, ?2, ?3)
             ON CONFLICT(session_id, key) DO UPDATE SET value = value + excluded.value",
            rusqlite::params![session_id, key, delta as i64],
        )?;
        Ok(())
    }

    /// Rebuild the session's token ledger from persisted usage + counters.
    pub fn session_ledger(&self, session_id: &str) -> Result<TokenLedger> {
        let conn = self.conn.lock().unwrap();
        let mut ledger = TokenLedger::default();
        {
            let mut stmt = conn.prepare(
                "SELECT prompt_tokens, cached_tokens, completion_tokens
                 FROM usage_records WHERE session_id = ?1",
            )?;
            let rows = stmt.query_map([session_id], |row| {
                Ok(TokenUsage::new(
                    row.get::<_, i64>(0)?.max(0) as u64,
                    row.get::<_, i64>(1)?.max(0) as u64,
                    row.get::<_, i64>(2)?.max(0) as u64,
                ))
            })?;
            for usage in rows {
                ledger.record_supervisor(usage?);
            }
        }
        let mut stmt = conn.prepare("SELECT key, value FROM session_counters WHERE session_id = ?1")?;
        let rows = stmt.query_map([session_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?.max(0) as u64))
        })?;
        for row in rows {
            let (key, value) = row?;
            match key.as_str() {
                "worker_input" => ledger.worker_input += value,
                "worker_output" => ledger.worker_output += value,
                "context_raw" => ledger.context_raw += value,
                "context_compressed" => ledger.context_compressed += value,
                _ => {}
            }
        }
        Ok(ledger)
    }
}

fn collect_search(
    stmt: &mut rusqlite::Statement<'_>,
    params: impl rusqlite::Params,
) -> std::result::Result<Vec<(String, String, String)>, rusqlite::Error> {
    let rows = stmt.query_map(params, |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Database {
        Database::open_in_memory().expect("in-memory db")
    }

    #[test]
    fn schema_and_fts5_available() {
        let db = db();
        let conn = db.conn.lock().unwrap();
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        // FTS5 must exist in the bundled build; if this fails the search
        // fallback silently kicks in, so assert capability explicitly here.
        let fts: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name = 'search_index'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(fts, 1, "FTS5 index table missing");
    }

    #[test]
    fn session_crud_and_recency_order() {
        let db = db();
        let s1 = db.create_session("deepseek", "deepseek/deepseek-v4-flash-0731", Some("first")).unwrap();
        let s2 = db.create_session("deepseek", "deepseek/deepseek-v4-flash-0731", Some("second")).unwrap();
        db.rename_session(&s1.id, "first-renamed").unwrap();

        let fetched = db.get_session(&s1.id).unwrap().unwrap();
        assert_eq!(fetched.title, "first-renamed");
        assert_eq!(fetched.model, "deepseek/deepseek-v4-flash-0731");

        // Touch s1 so it becomes most-recent.
        std::thread::sleep(std::time::Duration::from_millis(5));
        db.touch_session(&s1.id).unwrap();
        let sessions = db.list_sessions(10).unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].id, s1.id);
        assert_eq!(sessions[1].id, s2.id);

        db.delete_session(&s2.id).unwrap();
        assert!(db.get_session(&s2.id).unwrap().is_none());
    }

    #[test]
    fn messages_store_tokens_and_order_chronologically() {
        let db = db();
        let s = db.create_session("deepseek", "m", None).unwrap();
        for i in 0..30 {
            db.append_message(&s.id, Role::User, &format!("msg {i:02}")).unwrap();
        }
        // Low-RAM window: only the newest 10 come back, but in chronological order.
        let recent = db.recent_messages(&s.id, 10).unwrap();
        assert_eq!(recent.len(), 10);
        assert_eq!(recent[0].content, "msg 20");
        assert_eq!(recent[9].content, "msg 29");
        assert!(recent[0].tokens > 0);
    }

    #[test]
    fn parts_full_fidelity_and_search() {
        let db = db();
        let s = db.create_session("deepseek", "m", None).unwrap();
        let msg = db.append_message(&s.id, Role::Assistant, "ran tests").unwrap();
        let body = "cargo test output:\nwarning: unused variable `x`\ntest result: FAILED. 1 failed;";
        let part = db.append_part(&msg.id, PartKind::ToolOutput, "cargo", body).unwrap();
        assert_eq!(part.tokens, estimate_tokens(body) as u64);

        let fetched = db.get_part(&part.id).unwrap().unwrap();
        assert_eq!(fetched.content, body);

        let hits = db.search("FAILED", 5).unwrap();
        assert!(!hits.is_empty(), "FTS should find FAILED in part body");
        assert_eq!(hits[0].0, "tool_output");

        // Deleting the part removes its index entry (trigger).
        {
            let conn = db.conn.lock().unwrap();
            conn.execute("DELETE FROM parts WHERE id = ?1", [&part.id]).unwrap();
        }
        let hits = db.search("FAILED", 5).unwrap();
        assert!(hits.is_empty(), "index must not leak deleted parts");
    }

    #[test]
    fn ledger_roundtrip_from_usage_and_counters() {
        let db = db();
        let s = db.create_session("deepseek", "m", None).unwrap();
        db.record_usage(&s.id, "deepseek/deepseek-v4-flash-0731", TokenUsage::new(500, 100, 80)).unwrap();
        db.bump_counter(&s.id, "worker_input", 40_000).unwrap();
        db.bump_counter(&s.id, "worker_output", 1_200).unwrap();
        db.bump_counter(&s.id, "context_raw", 40_000).unwrap();
        db.bump_counter(&s.id, "context_compressed", 1_200).unwrap();

        let ledger = db.session_ledger(&s.id).unwrap();
        assert_eq!(ledger.supervisor, TokenUsage::new(500, 100, 80));
        assert_eq!(ledger.worker_input, 40_000);
        assert_eq!(ledger.worker_output, 1_200);
        assert_eq!(ledger.saved_tokens(), 38_800);
    }

    #[test]
    fn deleting_session_cascades() {
        let db = db();
        let s = db.create_session("deepseek", "m", None).unwrap();
        let msg = db.append_message(&s.id, Role::User, "hello").unwrap();
        db.append_part(&msg.id, PartKind::ToolCall, "bash", "ls").unwrap();
        db.record_usage(&s.id, "m", TokenUsage::new(10, 0, 5)).unwrap();
        db.delete_session(&s.id).unwrap();

        let conn = db.conn.lock().unwrap();
        let messages: i64 = conn.query_row("SELECT count(*) FROM messages", [], |r| r.get(0)).unwrap();
        let parts: i64 = conn.query_row("SELECT count(*) FROM parts", [], |r| r.get(0)).unwrap();
        let usage: i64 = conn.query_row("SELECT count(*) FROM usage_records", [], |r| r.get(0)).unwrap();
        assert_eq!((messages, parts, usage), (0, 0, 0));
    }
}
