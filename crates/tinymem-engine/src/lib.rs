//! Local-first memory and repository context. Embeddings are optional; source snapshots
//! are isolated by canonical worktree root.

mod backup;
pub use backup::snapshot_database;
mod context;
pub use context::{
    AssembledContext, ContextCategory, ContextEntry, ContextOptions, ContextProvenance,
    ContextScope, ContextSource, MAX_CONTEXT_TOKENS,
};
mod embedding;
mod freshness;
pub use freshness::{
    FileDependencyCheck, FileSourceReference, Freshness, FreshnessReason, FreshnessStatus,
    SourceKind, SourceRole,
};
pub mod mcp;
mod memory;
mod retrieval;
pub use embedding::{CommandEmbedder, Embedder};
pub use retrieval::{EmbeddingIndexReport, MemorySearchOptions, SearchMode};
mod parser;
mod source;
pub use memory::{
    MemoryEvent, MemoryInput, MemoryKind, MemoryMutation, MemoryProfile, MemoryRecord, MemoryScope,
    MemoryUpdate, NativeMemoryProfile, NewMemory,
};
pub use parser::Symbol;

use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, TransactionBehavior, params};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const APPLICATION_ID: i64 = 0x544d454d;
const SCHEMA_VERSION: i64 = 4;

fn validate_database(conn: &Connection) -> Result<()> {
    let id: i64 = conn.pragma_query_value(None, "application_id", |row| row.get(0))?;
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let schema_objects: i64 = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE name NOT GLOB 'sqlite_*'",
        [],
        |row| row.get(0),
    )?;
    ensure!(
        id == APPLICATION_ID || (id == 0 && version == 0 && schema_objects == 0),
        "not a TinyMem Rust engine database; use a separate database from Python"
    );
    ensure!(
        version <= SCHEMA_VERSION,
        "database schema is newer than this engine"
    );
    Ok(())
}

pub struct Engine {
    conn: Connection,
    embedding: Option<embedding::Provider>,
}

#[derive(Debug, Clone, Copy)]
pub struct SearchOptions {
    pub limit: usize,
    /// Maximum compact JSON bytes for the returned hits array (not model tokens).
    pub max_bytes: usize,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            limit: 5,
            max_bytes: 16_000,
        }
    }
}

impl From<usize> for SearchOptions {
    fn from(limit: usize) -> Self {
        Self {
            limit,
            ..Self::default()
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RefreshReport {
    pub repository: String,
    pub indexed: usize,
    pub unchanged: usize,
    pub removed: usize,
    pub skipped: usize,
    pub indexed_at_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct ContextHit {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
    pub content_hash: String,
    pub language: String,
    pub symbols: Vec<Symbol>,
    pub syntax_errors: bool,
    pub indexed_at_ms: u64,
}

impl Engine {
    /// Uses a separate database from the legacy Python memory store.
    pub fn open(db_path: impl AsRef<Path>) -> Result<Self> {
        let mut conn = Connection::open(db_path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        validate_database(&conn)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        // The writer lock may have waited for another process to change the schema.
        // Never migrate using compatibility markers read before acquiring that lock.
        validate_database(&tx)?;
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS repositories (
                root TEXT PRIMARY KEY, indexed_at_ms INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS files (
                root TEXT NOT NULL, path TEXT NOT NULL, hash TEXT NOT NULL,
                language TEXT NOT NULL, indexed_at_ms INTEGER NOT NULL,
                symbols TEXT NOT NULL, syntax_errors INTEGER NOT NULL,
                PRIMARY KEY(root, path),
                FOREIGN KEY(root) REFERENCES repositories(root) ON DELETE CASCADE
             );
             CREATE TABLE IF NOT EXISTS chunks (
                id INTEGER PRIMARY KEY, root TEXT NOT NULL, path TEXT NOT NULL,
                start_line INTEGER NOT NULL, end_line INTEGER NOT NULL, content TEXT NOT NULL,
                FOREIGN KEY(root, path) REFERENCES files(root, path) ON DELETE CASCADE
             );
             CREATE INDEX IF NOT EXISTS chunks_scope ON chunks(root, path);
             CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
                content, path, content='chunks', content_rowid='id'
             );
             CREATE TRIGGER IF NOT EXISTS chunks_insert AFTER INSERT ON chunks BEGIN
                INSERT INTO chunks_fts(rowid, content, path) VALUES (new.id, new.content, new.path);
             END;
             CREATE TRIGGER IF NOT EXISTS chunks_delete AFTER DELETE ON chunks BEGIN
                INSERT INTO chunks_fts(chunks_fts, rowid, content, path)
                VALUES ('delete', old.id, old.content, old.path);
             END;",
        )?;
        memory::migrate(&tx)?;
        retrieval::migrate(&tx)?;
        freshness::migrate(&tx)?;
        tx.pragma_update(None, "application_id", APPLICATION_ID)?;
        tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        tx.commit()?;
        Ok(Self {
            conn,
            embedding: None,
        })
    }

    /// Reconciles the working tree atomically; scan/read errors roll back the refresh.
    pub fn refresh(&mut self, root: impl AsRef<Path>) -> Result<RefreshReport> {
        let root = source::repository_root(root.as_ref())?;
        let root_id = root.to_str().unwrap();
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT INTO repositories(root, indexed_at_ms) VALUES (?1, ?2)
             ON CONFLICT(root) DO UPDATE SET indexed_at_ms=excluded.indexed_at_ms",
            params![root_id, now],
        )?;
        let mut previous: HashMap<String, String> = {
            let mut statement = tx.prepare("SELECT path,hash FROM files WHERE root=?1")?;
            statement
                .query_map([root_id], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?
        };
        let mut report = RefreshReport {
            repository: root_id.to_owned(),
            indexed: 0,
            unchanged: 0,
            removed: 0,
            skipped: 0,
            indexed_at_ms: now,
        };
        for entry in source::walk(&root) {
            let entry = entry.context("repository scan failed; previous index preserved")?;
            if let Some(error) = entry.error() {
                anyhow::bail!("invalid ignore rules; previous index preserved: {error}");
            }
            if entry.file_type().is_some_and(|kind| kind.is_dir()) {
                source::validate_ignores(entry.path())?;
            }
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                continue;
            }
            let Some(file) = source::read_source(&root, entry.path())? else {
                report.skipped += 1;
                continue;
            };
            if previous.remove(&file.path).as_deref() == Some(file.hash.as_str()) {
                report.unchanged += 1;
                continue;
            }
            tx.execute(
                "DELETE FROM files WHERE root=?1 AND path=?2",
                params![root_id, file.path],
            )?;
            let parsed = parser::parse(&file.content, file.language)?;
            tx.execute(
                "INSERT INTO files(root,path,hash,language,indexed_at_ms,symbols,syntax_errors)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![
                    root_id,
                    file.path,
                    file.hash,
                    file.language,
                    now,
                    serde_json::to_string(&parsed.symbols)?,
                    parsed.syntax_errors
                ],
            )?;
            let lines: Vec<_> = file.content.lines().collect();
            for (ordinal, chunk) in lines.chunks(source::CHUNK_LINES).enumerate() {
                let start = ordinal * source::CHUNK_LINES + 1;
                tx.execute(
                    "INSERT INTO chunks(root,path,start_line,end_line,content) VALUES (?1,?2,?3,?4,?5)",
                    params![root_id, file.path, start, start + chunk.len() - 1, chunk.join("\n")],
                )?;
            }
            report.indexed += 1;
        }
        for path in previous.keys() {
            tx.execute(
                "DELETE FROM files WHERE root=?1 AND path=?2",
                params![root_id, path],
            )?;
            report.removed += 1;
        }
        tx.commit()?;
        Ok(report)
    }

    /// Literal lexical search, scoped to one worktree. No model or provider is required.
    pub fn search(
        &self,
        root: impl AsRef<Path>,
        query: &str,
        options: impl Into<SearchOptions>,
    ) -> Result<Vec<ContextHit>> {
        self.search_repository_candidates(root, query, options.into(), false, None)
    }

    pub(crate) fn search_repository_candidates(
        &self,
        root: impl AsRef<Path>,
        query: &str,
        options: SearchOptions,
        any_terms: bool,
        exclusions: Option<&globset::GlobSet>,
    ) -> Result<Vec<ContextHit>> {
        let limit = options.limit.min(100);
        let max_bytes = options.max_bytes.min(1024 * 1024);
        let root = source::repository_root(root.as_ref())?;
        ensure!(query.len() <= 4096, "query exceeds 4096 bytes");
        let tokens: Vec<_> = query
            .split(|ch: char| !ch.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .take(32)
            .collect();
        if tokens.is_empty() || limit == 0 || max_bytes < 2 {
            return Ok(Vec::new());
        }
        let query = tokens
            .iter()
            .map(|word| format!("\"{word}\""))
            .collect::<Vec<_>>()
            .join(if any_terms { " OR " } else { " AND " });
        let mut statement = self.conn.prepare(
            "SELECT c.path,c.start_line,c.end_line,c.content,f.hash,f.language,f.indexed_at_ms,f.symbols,f.syntax_errors
             FROM chunks_fts JOIN chunks c ON c.id=chunks_fts.rowid
             JOIN files f ON f.root=c.root AND f.path=c.path
             WHERE chunks_fts MATCH ?1 AND c.root=?2
             ORDER BY bm25(chunks_fts), c.path, c.start_line LIMIT ?3"
        )?;
        // Excluded paths must not consume even the oversampling limit. Stream
        // past them before hydrating content, checking freshness, or byte packing.
        let scan_limit = limit.saturating_mul(8);
        let filtered = exclusions.is_some_and(|set| !set.is_empty());
        let mut rows = statement.query(params![
            query,
            root.to_str().unwrap(),
            if filtered { -1i64 } else { scan_limit as i64 },
        ])?;
        let mut current = Vec::new();
        let mut used_bytes = 2; // JSON array brackets.
        let mut hashes: HashMap<String, Option<String>> = HashMap::new();
        let mut scanned = 0;
        while scanned < scan_limit {
            let Some(row) = rows.next()? else { break };
            let path: String = row.get(0)?;
            if exclusions.is_some_and(|set| set.is_match(&path)) {
                continue;
            }
            scanned += 1;
            let mut hit = ContextHit {
                path,
                start_line: row.get(1)?,
                end_line: row.get(2)?,
                content: row.get(3)?,
                content_hash: row.get(4)?,
                language: row.get(5)?,
                indexed_at_ms: row.get(6)?,
                symbols: serde_json::from_str(&row.get::<_, String>(7)?).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        7,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?,
                syntax_errors: row.get(8)?,
            };
            let hash = hashes.entry(hit.path.clone()).or_insert_with(|| {
                source::read_source(&root, &root.join(&hit.path))
                    .ok()
                    .flatten()
                    .map(|file| file.hash)
            });
            if hash.as_deref() != Some(hit.content_hash.as_str()) {
                continue;
            }
            hit.symbols.retain(|symbol| {
                symbol.start_line <= hit.end_line && symbol.end_line >= hit.start_line
            });
            let bytes = serde_json::to_vec(&hit)?.len() + usize::from(!current.is_empty());
            if used_bytes + bytes > max_bytes {
                continue;
            }
            used_bytes += bytes;
            current.push(hit);
            if current.len() == limit {
                break;
            }
        }
        Ok(current)
    }
}
