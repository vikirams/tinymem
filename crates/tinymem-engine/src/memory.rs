//! Explicit, scope-isolated memory. Reads never update evidence or access statistics.

use crate::{Engine, FileSourceReference, Freshness, SearchOptions};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_RESULTS: usize = 100;
const MAX_RESULT_BYTES: usize = 1024 * 1024;
const MAX_CONTENT_BYTES: usize = 64 * 1024;
const MAX_METADATA_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryScope {
    pub user_id: String,
    pub agent_id: Option<String>,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    #[default]
    Fact,
    Preference,
    Decision,
    Plan,
    Relationship,
    ArchitecturalDecision,
    IssueSolution,
}

fn default_metadata() -> Value {
    Value::Object(Default::default())
}

fn default_source() -> String {
    "explicit".to_owned()
}

/// Legacy full-replacement input. Only content is normalized; identifiers are exact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryInput {
    pub content: String,
    #[serde(default)]
    pub kind: MemoryKind,
    pub key: Option<String>,
    #[serde(default = "default_metadata")]
    pub metadata: Value,
    pub source: Option<String>,
    pub expires_at_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewMemory {
    pub content: String,
    #[serde(default)]
    pub kind: MemoryKind,
    #[serde(default = "default_source")]
    pub source: String,
    pub expires_at_ms: Option<u64>,
    #[serde(default)]
    pub source_refs: Vec<FileSourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryUpdate {
    pub content: String,
    pub expected_version: Option<u64>,
}

/// Explicitly distinguishes legacy replacement from optimistic, content-only updates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryMutation {
    Replace(MemoryInput),
    Update(MemoryUpdate),
}

impl From<MemoryInput> for MemoryMutation {
    fn from(input: MemoryInput) -> Self {
        Self::Replace(input)
    }
}

impl From<MemoryUpdate> for MemoryMutation {
    fn from(input: MemoryUpdate) -> Self {
        Self::Update(input)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: String,
    pub scope: MemoryScope,
    pub content: String,
    pub kind: MemoryKind,
    pub key: Option<String>,
    pub metadata: Value,
    pub source: Option<String>,
    pub expires_at_ms: Option<u64>,
    #[serde(default)]
    pub source_refs: Vec<FileSourceReference>,
    /// The modern version and legacy revision always denote the same observation.
    pub version: u64,
    pub revision: u64,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

impl MemoryRecord {
    fn input(&self) -> MemoryInput {
        MemoryInput {
            content: self.content.clone(),
            kind: self.kind,
            key: self.key.clone(),
            metadata: self.metadata.clone(),
            source: self.source.clone(),
            expires_at_ms: self.expires_at_ms,
        }
    }

    fn replace_input(&mut self, input: MemoryInput) {
        self.content = input.content;
        self.kind = input.kind;
        self.key = input.key;
        self.metadata = input.metadata;
        self.source = input.source;
        self.expires_at_ms = input.expires_at_ms;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryEvent {
    /// One of "added", "updated", or "deleted".
    pub event: String,
    pub version: u64,
    pub at_ms: u64,
    pub memory: MemoryRecord,
    /// Derived at inspection time; not part of the immutable observation.
    pub freshness: Freshness,
}

/// Legacy classification and envelope; intentionally has no `dynamic` field.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryProfile {
    pub stable: Vec<MemoryRecord>,
    pub recent: Vec<MemoryRecord>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeMemoryProfile {
    pub stable: Vec<MemoryRecord>,
    pub dynamic: Vec<MemoryRecord>,
}

/// Called inside Engine::open's immediate schema transaction (including v1 -> v2).
pub(crate) fn migrate(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS memories (
            rowid INTEGER PRIMARY KEY,
            id TEXT NOT NULL UNIQUE,
            scope TEXT NOT NULL,
            memory_key TEXT,
            input TEXT NOT NULL,
            content TEXT NOT NULL,
            record TEXT NOT NULL,
            expires_at_ms TEXT,
            updated_at_ms INTEGER NOT NULL,
            deleted INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1))
         );
         CREATE INDEX IF NOT EXISTS memories_scope_updated
            ON memories(scope, updated_at_ms DESC);
         CREATE UNIQUE INDEX IF NOT EXISTS memories_scope_key
            ON memories(scope, memory_key) WHERE memory_key IS NOT NULL AND deleted=0;
         CREATE TABLE IF NOT EXISTS memory_events (
            memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
            version INTEGER NOT NULL,
            event TEXT NOT NULL CHECK (event IN ('added', 'updated', 'deleted')),
            at_ms INTEGER NOT NULL,
            snapshot TEXT NOT NULL,
            PRIMARY KEY(memory_id, version)
         );
         CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
            content, content='memories', content_rowid='rowid'
         );
         CREATE TRIGGER IF NOT EXISTS memories_insert AFTER INSERT ON memories
         WHEN new.deleted=0 BEGIN
            INSERT INTO memories_fts(rowid, content) VALUES (new.rowid, new.content);
         END;
         CREATE TRIGGER IF NOT EXISTS memories_delete AFTER DELETE ON memories
         WHEN old.deleted=0 BEGIN
            INSERT INTO memories_fts(memories_fts, rowid, content)
                VALUES ('delete', old.rowid, old.content);
         END;
         CREATE TRIGGER IF NOT EXISTS memories_update AFTER UPDATE ON memories BEGIN
            INSERT INTO memories_fts(memories_fts, rowid, content)
                SELECT 'delete', old.rowid, old.content WHERE old.deleted=0;
            INSERT INTO memories_fts(rowid, content)
                SELECT new.rowid, new.content WHERE new.deleted=0;
         END;",
    )?;
    Ok(())
}

impl Engine {
    /// Keyed upsert or unkeyed full-input deduplication, including expired evidence.
    pub fn remember(&mut self, scope: &MemoryScope, input: MemoryInput) -> Result<MemoryRecord> {
        self.remember_with_sources(scope, input, Vec::new())
    }

    fn remember_with_sources(
        &mut self,
        scope: &MemoryScope,
        input: MemoryInput,
        source_refs: Vec<FileSourceReference>,
    ) -> Result<MemoryRecord> {
        let scope_key = validate_scope(scope)?;
        let input = normalize_input(input)?;
        let input_json = serde_json::to_string(&input)?;
        crate::freshness::validate_capture(&source_refs)?;
        let refs_key = crate::freshness::refs_key(&source_refs)?;
        let embedding = self.embedding_for(&input.content)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        // Provider work and waiting for SQLite may race a file edit. Never commit
        // a new capture checked only before obtaining the writer lock.
        crate::freshness::validate_capture(&source_refs)?;
        let existing = if let Some(key) = &input.key {
            tx.query_row(
                "SELECT record FROM memories
                 WHERE scope=?1 AND memory_key=?2 AND deleted=0",
                params![scope_key, key],
                |row| read_record(row, 0),
            )
            .optional()?
        } else {
            tx.query_row(
                "SELECT record FROM memories
                 WHERE scope=?1 AND memory_key IS NULL AND input=?2 AND source_refs_key=?3 AND deleted=0
                 ORDER BY updated_at_ms DESC, id LIMIT 1",
                params![scope_key, input_json, refs_key],
                |row| read_record(row, 0),
            )
            .optional()?
        };
        let record = match existing {
            Some(record) if record.input() == input => record,
            Some(mut record) => {
                record.replace_input(input);
                advance_record(&tx, &scope_key, &mut record)?;
                save_record(&tx, &scope_key, &record, "updated", false)?;
                record
            }
            None => {
                let at_ms = next_timestamp(&tx, &scope_key)?;
                let id = tx.query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))?;
                let mut record = MemoryRecord {
                    id,
                    scope: scope.clone(),
                    content: String::new(),
                    kind: MemoryKind::Fact,
                    key: None,
                    metadata: default_metadata(),
                    source: None,
                    expires_at_ms: None,
                    source_refs,
                    version: 1,
                    revision: 1,
                    created_at_ms: at_ms,
                    updated_at_ms: at_ms,
                };
                record.replace_input(input);
                save_record(&tx, &scope_key, &record, "added", false)?;
                record
            }
        };
        crate::retrieval::save_embedding(&tx, &record, &embedding)?;
        tx.commit()?;
        Ok(record)
    }

    pub fn add_memory(&mut self, scope: &MemoryScope, input: NewMemory) -> Result<MemoryRecord> {
        self.remember_with_sources(
            scope,
            MemoryInput {
                content: input.content,
                kind: input.kind,
                key: None,
                metadata: default_metadata(),
                source: Some(input.source),
                expires_at_ms: input.expires_at_ms,
            },
            input.source_refs,
        )
    }

    /// Only current, undeleted, unexpired evidence is visible here.
    pub fn get_memory(&self, scope: &MemoryScope, id: &str) -> Result<Option<MemoryRecord>> {
        let scope_key = validate_scope(scope)?;
        validate_token(id, "memory id")?;
        let record = find_record(&self.conn, &scope_key, id)?;
        let now = now_ms()?;
        match record {
            Some(record)
                if record.expires_at_ms.is_none_or(|expiry| expiry > now)
                    && record.is_current()? =>
            {
                Ok(Some(record))
            }
            _ => Ok(None),
        }
    }

    /// Legacy replacement may revive expired evidence; modern updates preserve all other fields.
    pub fn update_memory(
        &mut self,
        scope: &MemoryScope,
        id: &str,
        input: impl Into<MemoryMutation>,
    ) -> Result<MemoryRecord> {
        let scope_key = validate_scope(scope)?;
        validate_token(id, "memory id")?;
        let mutation = match input.into() {
            MemoryMutation::Replace(input) => MemoryMutation::Replace(normalize_input(input)?),
            MemoryMutation::Update(mut input) => {
                input.content = normalize_content(&input.content)?;
                MemoryMutation::Update(input)
            }
        };
        let content = match &mutation {
            MemoryMutation::Replace(input) => &input.content,
            MemoryMutation::Update(input) => &input.content,
        };
        let embedding = self.embedding_for(content)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut record = find_record(&tx, &scope_key, id)?.context("memory not found")?;
        let replacement = match mutation {
            MemoryMutation::Replace(input) => input,
            MemoryMutation::Update(input) => {
                check_version(record.version, input.expected_version)?;
                MemoryInput {
                    content: input.content,
                    ..record.input()
                }
            }
        };
        if record.input() != replacement {
            record.replace_input(replacement);
            advance_record(&tx, &scope_key, &mut record)?;
            // The scope-local unique key constraint rejects conflicts before an event is appended.
            save_record(&tx, &scope_key, &record, "updated", false)?;
        }
        crate::retrieval::save_embedding(&tx, &record, &embedding)?;
        tx.commit()?;
        Ok(record)
    }

    /// Soft deletion appends an observation and is not secure erasure.
    pub fn delete_memory(
        &mut self,
        scope: &MemoryScope,
        id: &str,
        expected_version: Option<u64>,
    ) -> Result<bool> {
        let scope_key = validate_scope(scope)?;
        validate_token(id, "memory id")?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(mut record) = find_record(&tx, &scope_key, id)? else {
            tx.commit()?;
            return Ok(false);
        };
        check_version(record.version, expected_version)?;
        advance_record(&tx, &scope_key, &mut record)?;
        save_record(&tx, &scope_key, &record, "deleted", true)?;
        tx.commit()?;
        Ok(true)
    }

    /// Legacy logical hard purge removes current state and all scoped audit history.
    /// SQLite pages, WAL files, and backups are not securely erased.
    pub fn forget_memory(&mut self, scope: &MemoryScope, id: &str) -> Result<bool> {
        let scope_key = validate_scope(scope)?;
        validate_token(id, "memory id")?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let removed = tx.execute(
            "DELETE FROM memories WHERE scope=?1 AND id=?2",
            params![scope_key, id],
        )?;
        tx.commit()?;
        Ok(removed != 0)
    }

    /// Literal AND search of content only; BM25 wins over deterministic recency ties.
    pub fn recall(
        &self,
        scope: &MemoryScope,
        query: &str,
        options: impl Into<SearchOptions>,
    ) -> Result<Vec<MemoryRecord>> {
        let scope_key = validate_scope(scope)?;
        ensure!(query.len() <= 4096, "query exceeds 4096 bytes");
        ensure!(!query.contains('\0'), "query contains NUL");
        let words: Vec<_> = query
            .split(|ch: char| !ch.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .take(33)
            .collect();
        ensure!(words.len() <= 32, "query exceeds 32 words");
        let options = bounded_options(options.into());
        if words.is_empty() || options.limit == 0 || options.max_bytes < 2 {
            return Ok(Vec::new());
        }
        let query = words
            .iter()
            .map(|word| format!("\"{word}\""))
            .collect::<Vec<_>>()
            .join(" AND ");
        let mut statement = self.conn.prepare(
            "SELECT m.record FROM memories_fts JOIN memories m ON m.rowid=memories_fts.rowid
             WHERE memories_fts MATCH ?1 AND m.scope=?2 AND m.deleted=0
               AND (m.expires_at_ms IS NULL OR m.expires_at_ms > ?3)
             ORDER BY bm25(memories_fts), m.updated_at_ms DESC, m.id",
        )?;
        let rows = statement
            .query_map(params![query, scope_key, expiry_text(now_ms()?)], |row| {
                read_record(row, 0)
            })?;
        let mut records = Vec::new();
        let mut budget = JsonBudget::new(2, options.max_bytes);
        for record in rows {
            let record = record?;
            if !record.is_current()? {
                continue;
            }
            if budget.include(&record, !records.is_empty())? {
                records.push(record);
                if records.len() == options.limit {
                    break;
                }
            }
        }
        Ok(records)
    }

    /// Latest 100 added/updated snapshots, returned in ascending revision order.
    pub fn memory_history(&self, scope: &MemoryScope, id: &str) -> Result<Vec<MemoryRecord>> {
        let scope_key = validate_scope(scope)?;
        validate_token(id, "memory id")?;
        let mut statement = self.conn.prepare(
            "SELECT e.snapshot FROM memory_events e JOIN memories m ON m.id=e.memory_id
             WHERE m.scope=?1 AND m.id=?2 AND e.event!='deleted'
             ORDER BY e.version DESC LIMIT 100",
        )?;
        let mut records = statement
            .query_map(params![scope_key, id], |row| read_record(row, 0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        records.reverse();
        Ok(records)
    }

    /// Latest bounded audit events, including deletion, in ascending version order.
    pub fn memory_events(
        &self,
        scope: &MemoryScope,
        id: &str,
        options: impl Into<SearchOptions>,
    ) -> Result<Vec<MemoryEvent>> {
        let scope_key = validate_scope(scope)?;
        validate_token(id, "memory id")?;
        let options = bounded_options(options.into());
        if options.limit == 0 || options.max_bytes < 2 {
            return Ok(Vec::new());
        }
        let mut statement = self.conn.prepare(
            "SELECT e.event,e.version,e.at_ms,e.snapshot
             FROM memory_events e JOIN memories m ON m.id=e.memory_id
             WHERE m.scope=?1 AND m.id=?2 ORDER BY e.version DESC LIMIT 100",
        )?;
        let rows = statement.query_map(params![scope_key, id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u64>(1)?,
                row.get::<_, u64>(2)?,
                read_record(row, 3)?,
            ))
        })?;
        let mut events = Vec::new();
        let mut budget = JsonBudget::new(2, options.max_bytes);
        for event in rows {
            let (event, version, at_ms, memory) = event?;
            let freshness = memory.freshness()?;
            let event = MemoryEvent {
                event,
                version,
                at_ms,
                memory,
                freshness,
            };
            if budget.include(&event, !events.is_empty())? {
                events.push(event);
                if events.len() == options.limit {
                    break;
                }
            }
        }
        events.reverse();
        Ok(events)
    }

    pub fn profile(
        &self,
        scope: &MemoryScope,
        options: impl Into<SearchOptions>,
    ) -> Result<MemoryProfile> {
        let (stable, recent) = self.profile_records(scope, options.into(), false)?;
        Ok(MemoryProfile { stable, recent })
    }

    pub fn memory_profile(
        &self,
        scope: &MemoryScope,
        options: impl Into<SearchOptions>,
    ) -> Result<NativeMemoryProfile> {
        let (stable, dynamic) = self.profile_records(scope, options.into(), true)?;
        Ok(NativeMemoryProfile { stable, dynamic })
    }

    fn profile_records(
        &self,
        scope: &MemoryScope,
        options: SearchOptions,
        native: bool,
    ) -> Result<(Vec<MemoryRecord>, Vec<MemoryRecord>)> {
        let scope_key = validate_scope(scope)?;
        let options = bounded_options(options);
        let envelope_bytes = if native {
            serde_json::to_vec(&NativeMemoryProfile::default())?.len()
        } else {
            serde_json::to_vec(&MemoryProfile::default())?.len()
        };
        let mut stable = Vec::new();
        let mut other = Vec::new();
        if options.limit == 0 || options.max_bytes < envelope_bytes {
            return Ok((stable, other));
        }
        let mut statement = self.conn.prepare(
            "SELECT record FROM memories WHERE scope=?1 AND deleted=0
               AND (expires_at_ms IS NULL OR expires_at_ms > ?2)
             ORDER BY updated_at_ms DESC, id",
        )?;
        let rows = statement.query_map(params![scope_key, expiry_text(now_ms()?)], |row| {
            read_record(row, 0)
        })?;
        let mut budget = JsonBudget::new(envelope_bytes, options.max_bytes);
        for record in rows {
            let record = record?;
            if !record.is_current()? {
                continue;
            }
            let is_stable = if native {
                matches!(
                    record.kind,
                    MemoryKind::Fact | MemoryKind::Preference | MemoryKind::Relationship
                )
            } else {
                matches!(
                    record.kind,
                    MemoryKind::Fact
                        | MemoryKind::Preference
                        | MemoryKind::Decision
                        | MemoryKind::ArchitecturalDecision
                )
            };
            let group = if is_stable { &mut stable } else { &mut other };
            if budget.include(&record, !group.is_empty())? {
                group.push(record);
                if stable.len() + other.len() == options.limit {
                    break;
                }
            }
        }
        Ok((stable, other))
    }
}

fn validate_token(value: &str, name: &str) -> Result<()> {
    ensure!(!value.trim().is_empty(), "{name} must not be blank");
    ensure!(value.len() <= 256, "{name} exceeds 256 bytes");
    ensure!(!value.contains('\0'), "{name} contains NUL");
    Ok(())
}

pub(crate) fn validate_scope(scope: &MemoryScope) -> Result<String> {
    validate_token(&scope.user_id, "user id")?;
    if let Some(agent) = &scope.agent_id {
        validate_token(agent, "agent id")?;
    }
    if let Some(project) = &scope.project_id {
        validate_token(project, "project id")?;
    }
    // JSON encodes the exact tuple without ambiguous separators or NULL wildcards.
    Ok(serde_json::to_string(scope)?)
}

fn normalize_content(content: &str) -> Result<String> {
    let content = content.trim();
    ensure!(!content.is_empty(), "memory content must not be blank");
    ensure!(
        content.len() <= MAX_CONTENT_BYTES,
        "memory content exceeds 65536 bytes"
    );
    ensure!(!content.contains('\0'), "memory content contains NUL");
    Ok(content.to_owned())
}

fn normalize_input(mut input: MemoryInput) -> Result<MemoryInput> {
    input.content = normalize_content(&input.content)?;
    if let Some(key) = &input.key {
        validate_token(key, "memory key")?;
    }
    if let Some(source) = &input.source {
        ensure!(source.len() <= 4096, "memory source exceeds 4096 bytes");
        ensure!(!source.contains('\0'), "memory source contains NUL");
    }
    ensure!(
        input.metadata.is_object(),
        "memory metadata must be an object"
    );
    let mut pending = vec![&input.metadata];
    while let Some(value) = pending.pop() {
        match value {
            Value::String(value) => {
                ensure!(!value.contains('\0'), "memory metadata contains NUL");
            }
            Value::Array(values) => pending.extend(values),
            Value::Object(values) => {
                for (key, value) in values {
                    ensure!(!key.contains('\0'), "memory metadata contains NUL");
                    pending.push(value);
                }
            }
            _ => {}
        }
    }
    ensure!(
        serde_json::to_vec(&input.metadata)?.len() <= MAX_METADATA_BYTES,
        "memory metadata exceeds 16384 serialized bytes"
    );
    Ok(input)
}

pub(crate) fn now_ms() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis()
        .try_into()?)
}

fn next_timestamp(tx: &Transaction<'_>, scope_key: &str) -> Result<u64> {
    let latest: Option<u64> = tx.query_row(
        "SELECT max(updated_at_ms) FROM memories WHERE scope=?1",
        [scope_key],
        |row| row.get(0),
    )?;
    let next = match latest {
        Some(latest) => latest.checked_add(1).context("memory timestamp overflow")?,
        None => 0,
    };
    Ok(now_ms()?.max(next))
}

fn advance_record(tx: &Transaction<'_>, scope_key: &str, record: &mut MemoryRecord) -> Result<()> {
    record.version = record
        .version
        .checked_add(1)
        .context("memory version overflow")?;
    record.revision = record.version;
    record.updated_at_ms = next_timestamp(tx, scope_key)?;
    Ok(())
}

fn check_version(version: u64, expected: Option<u64>) -> Result<()> {
    ensure!(
        expected.is_none_or(|expected| expected == version),
        "memory version conflict"
    );
    Ok(())
}

/// Fixed-width decimal TEXT sorts correctly and preserves the entire u64 range.
pub(crate) fn expiry_text(expiry: u64) -> String {
    format!("{expiry:020}")
}

pub(crate) fn read_record(row: &Row<'_>, index: usize) -> rusqlite::Result<MemoryRecord> {
    let json: String = row.get(index)?;
    serde_json::from_str(&json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn find_record(conn: &Connection, scope_key: &str, id: &str) -> Result<Option<MemoryRecord>> {
    Ok(conn
        .query_row(
            "SELECT record FROM memories WHERE scope=?1 AND id=?2 AND deleted=0",
            params![scope_key, id],
            |row| read_record(row, 0),
        )
        .optional()?)
}

fn save_record(
    tx: &Transaction<'_>,
    scope_key: &str,
    record: &MemoryRecord,
    event: &str,
    deleted: bool,
) -> Result<()> {
    let snapshot = serde_json::to_string(record)?;
    let input = serde_json::to_string(&record.input())?;
    tx.execute(
        "INSERT INTO memories(id,scope,memory_key,input,content,record,expires_at_ms,updated_at_ms,deleted,source_refs_key)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
         ON CONFLICT(id) DO UPDATE SET memory_key=excluded.memory_key,input=excluded.input,
           content=excluded.content,record=excluded.record,expires_at_ms=excluded.expires_at_ms,
           updated_at_ms=excluded.updated_at_ms,deleted=excluded.deleted,source_refs_key=excluded.source_refs_key",
        params![
            record.id,
            scope_key,
            record.key,
            input,
            record.content,
            snapshot,
            record.expires_at_ms.map(expiry_text),
            record.updated_at_ms,
            deleted,
            crate::freshness::refs_key(&record.source_refs)?,
        ],
    )?;
    tx.execute(
        "INSERT INTO memory_events(memory_id,version,event,at_ms,snapshot) VALUES (?1,?2,?3,?4,?5)",
        params![
            record.id,
            record.version,
            event,
            record.updated_at_ms,
            snapshot
        ],
    )?;
    Ok(())
}

pub(crate) fn bounded_options(options: SearchOptions) -> SearchOptions {
    SearchOptions {
        limit: options.limit.min(MAX_RESULTS),
        max_bytes: options.max_bytes.min(MAX_RESULT_BYTES),
    }
}

pub(crate) struct JsonBudget {
    used: usize,
    max: usize,
}

impl JsonBudget {
    pub(crate) fn new(envelope_bytes: usize, max: usize) -> Self {
        Self {
            used: envelope_bytes,
            max,
        }
    }

    pub(crate) fn include(&mut self, value: &impl Serialize, comma: bool) -> Result<bool> {
        let bytes = serde_json::to_vec(value)?.len() + usize::from(comma);
        if self.used.saturating_add(bytes) > self.max {
            return Ok(false);
        }
        self.used += bytes;
        Ok(true)
    }
}
