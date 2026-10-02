//! Scope-local semantic ranking and hybrid reciprocal-rank fusion.

use crate::embedding::{decode, encode};
use crate::memory::{
    JsonBudget, bounded_options, expiry_text, now_ms, read_record, validate_scope,
};
use crate::{Engine, MemoryRecord, MemoryScope, SearchOptions};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const CANDIDATES: usize = 100;
const RRF_K: f64 = 60.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    #[default]
    Lexical,
    Semantic,
    Hybrid,
}

#[derive(Debug, Clone, Copy)]
pub struct MemorySearchOptions {
    pub search: SearchOptions,
    pub mode: SearchMode,
    /// Inclusive cosine floor for the semantic branch, in [-1, 1].
    pub min_similarity: f64,
}

impl Default for MemorySearchOptions {
    fn default() -> Self {
        Self {
            search: SearchOptions::default(),
            mode: SearchMode::Lexical,
            min_similarity: 0.25,
        }
    }
}

impl From<SearchOptions> for MemorySearchOptions {
    fn from(search: SearchOptions) -> Self {
        Self {
            search,
            ..Self::default()
        }
    }
}

impl From<usize> for MemorySearchOptions {
    fn from(limit: usize) -> Self {
        SearchOptions::from(limit).into()
    }
}

#[derive(Debug, Serialize)]
pub struct EmbeddingIndexReport {
    pub indexed: usize,
    /// A concurrent update/deletion made a generated vector obsolete; retry indexing.
    pub stale: usize,
    pub remaining: usize,
    pub model: String,
    pub dimensions: usize,
}

pub(crate) struct DocumentEmbedding {
    model: String,
    dimensions: usize,
    bytes: Vec<u8>,
}

pub(crate) fn migrate(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS memory_embeddings (
            memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
            model TEXT NOT NULL,
            dimensions INTEGER NOT NULL CHECK (dimensions BETWEEN 1 AND 4096),
            version INTEGER NOT NULL,
            vector BLOB NOT NULL,
            PRIMARY KEY(memory_id, model, dimensions)
         );
         CREATE TRIGGER IF NOT EXISTS memories_invalidate_embeddings AFTER UPDATE ON memories BEGIN
            DELETE FROM memory_embeddings WHERE memory_id=new.id;
         END;",
    )?;
    Ok(())
}

pub(crate) fn save_embedding(
    tx: &Transaction<'_>,
    record: &MemoryRecord,
    embedding: &Option<DocumentEmbedding>,
) -> Result<()> {
    if let Some(embedding) = embedding {
        tx.execute(
            "INSERT INTO memory_embeddings(memory_id,model,dimensions,version,vector)
             VALUES (?1,?2,?3,?4,?5)
             ON CONFLICT(memory_id,model,dimensions) DO UPDATE SET version=excluded.version,vector=excluded.vector",
            params![record.id, embedding.model, embedding.dimensions, record.version, embedding.bytes],
        )?;
    }
    Ok(())
}

impl Engine {
    /// Opt in to semantic retrieval. Identity/dimension metadata is frozen at configuration.
    /// Existing evidence is not embedded until an explicit index_memories call or write.
    pub fn with_embedder(mut self, adapter: impl crate::Embedder + 'static) -> Result<Self> {
        self.embedding = Some(crate::embedding::Provider::new(adapter)?);
        Ok(self)
    }

    pub(crate) fn embedding_for(&self, content: &str) -> Result<Option<DocumentEmbedding>> {
        self.embedding
            .as_ref()
            .map(|provider| {
                Ok(DocumentEmbedding {
                    model: provider.model.clone(),
                    dimensions: provider.dimensions,
                    bytes: encode(&provider.embed(content)?),
                })
            })
            .transpose()
    }

    /// Explicit cache backfill, at most 100 active memories per call. Provider work is
    /// outside the writer lock; version checks prevent storing stale generated vectors.
    /// A provider failure commits no vectors. This never appends personal-memory events.
    pub fn index_memories(
        &mut self,
        scope: &MemoryScope,
        limit: usize,
    ) -> Result<EmbeddingIndexReport> {
        let scope_key = validate_scope(scope)?;
        let provider = self
            .embedding
            .as_ref()
            .context("semantic retrieval requires an embedder")?;
        let model = provider.model.clone();
        let dimensions = provider.dimensions;
        let now = expiry_text(now_ms()?);
        let pending = {
            let mut statement = self.conn.prepare(
                "SELECT m.record FROM memories m
                 LEFT JOIN memory_embeddings e ON e.memory_id=m.id AND e.model=?2 AND e.dimensions=?3
                 WHERE m.scope=?1 AND m.deleted=0 AND (m.expires_at_ms IS NULL OR m.expires_at_ms > ?4)
                   AND (e.memory_id IS NULL OR e.version != json_extract(m.record,'$.version'))
                 ORDER BY m.id LIMIT ?5",
            )?;
            statement
                .query_map(
                    params![scope_key, model, dimensions, now, limit.min(100)],
                    |row| read_record(row, 0),
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let generated = pending
            .into_iter()
            .map(|record| {
                let embedding = self.embedding_for(&record.content)?;
                Ok((record, embedding))
            })
            .collect::<Result<Vec<_>>>()?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut indexed = 0;
        let mut stale = 0;
        let now = expiry_text(now_ms()?);
        for (record, embedding) in generated {
            let version: Option<u64> = tx.query_row(
                "SELECT json_extract(record,'$.version') FROM memories
                 WHERE scope=?1 AND id=?2 AND deleted=0 AND (expires_at_ms IS NULL OR expires_at_ms > ?3)",
                params![scope_key, record.id, now], |row| row.get(0),
            ).optional()?;
            if version == Some(record.version) {
                save_embedding(&tx, &record, &embedding)?;
                indexed += 1;
            } else {
                stale += 1;
            }
        }
        let remaining = tx.query_row(
            "SELECT count(*) FROM memories m
             LEFT JOIN memory_embeddings e ON e.memory_id=m.id AND e.model=?2 AND e.dimensions=?3
             WHERE m.scope=?1 AND m.deleted=0 AND (m.expires_at_ms IS NULL OR m.expires_at_ms > ?4)
               AND (e.memory_id IS NULL OR e.version != json_extract(m.record,'$.version'))",
            params![scope_key, model, dimensions, now],
            |row| row.get(0),
        )?;
        tx.commit()?;
        Ok(EmbeddingIndexReport {
            indexed,
            stale,
            remaining,
            model,
            dimensions,
        })
    }

    /// Lexical compatibility by default; semantic is exact cosine search and hybrid
    /// fuses top-100 lexical-OR and semantic rankings using RRF (k=60).
    /// Query embedding is read-only: no indexing or personal-memory writes occur.
    pub fn search_memories(
        &self,
        scope: &MemoryScope,
        query: &str,
        options: impl Into<MemorySearchOptions>,
    ) -> Result<Vec<MemoryRecord>> {
        let options = options.into();
        ensure!(
            options.min_similarity.is_finite() && (-1.0..=1.0).contains(&options.min_similarity),
            "min_similarity must be finite and between -1 and 1"
        );
        if options.mode == SearchMode::Lexical {
            return self.recall(scope, query, options.search);
        }
        let scope_key = validate_scope(scope)?;
        ensure!(query.len() <= 4096, "query exceeds 4096 bytes");
        ensure!(!query.contains('\0'), "query contains NUL");
        let words: Vec<_> = query
            .split(|ch: char| !ch.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .take(33)
            .collect();
        ensure!(words.len() <= 32, "query exceeds 32 words");
        let provider = self
            .embedding
            .as_ref()
            .context("semantic retrieval requires an embedder")?;
        let budget_options = bounded_options(options.search);
        if words.is_empty() || budget_options.limit == 0 || budget_options.max_bytes < 2 {
            return Ok(Vec::new());
        }
        let query_vector = provider.embed(query)?;
        let query_norm_squared = query_vector
            .iter()
            .map(|value| f64::from(*value).powi(2))
            .sum::<f64>();
        // All ranking/hydration reads share a snapshot, including concurrent updates.
        let tx = self.conn.unchecked_transaction()?;
        let now = expiry_text(now_ms()?);
        let mut statement = tx.prepare(
            "SELECT m.record,e.vector FROM memory_embeddings e JOIN memories m ON m.id=e.memory_id
             WHERE m.scope=?1 AND m.deleted=0 AND (m.expires_at_ms IS NULL OR m.expires_at_ms > ?2)
               AND e.model=?3 AND e.dimensions=?4 AND e.version=json_extract(m.record,'$.version')",
        )?;
        let rows = statement.query_map(
            params![scope_key, now, provider.model, provider.dimensions],
            |row| Ok((read_record(row, 0)?, row.get::<_, Vec<u8>>(1)?)),
        )?;
        let mut semantic: Vec<(String, f64)> = Vec::new();
        for row in rows {
            let (record, bytes) = row?;
            if !record.is_current()? {
                continue;
            }
            let id = record.id;
            let vector = decode(&bytes, provider.dimensions)?;
            let dot = vector
                .iter()
                .zip(&query_vector)
                .map(|(left, right)| f64::from(*left) * f64::from(*right))
                .sum::<f64>();
            let norm_squared = vector
                .iter()
                .map(|value| f64::from(*value).powi(2))
                .sum::<f64>();
            let score = (dot / (norm_squared * query_norm_squared).sqrt()).clamp(-1.0, 1.0);
            if score < options.min_similarity {
                continue;
            }
            semantic.push((id, score));
            semantic.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            semantic.truncate(CANDIDATES);
        }
        drop(statement);
        let mut ranked = if options.mode == SearchMode::Hybrid {
            let lexical_query = words
                .iter()
                .map(|word| format!("\"{word}\""))
                .collect::<Vec<_>>()
                .join(" OR ");
            let mut statement = tx.prepare(
                "SELECT m.record FROM memories_fts JOIN memories m ON m.rowid=memories_fts.rowid
                 WHERE memories_fts MATCH ?1 AND m.scope=?2 AND m.deleted=0
                   AND (m.expires_at_ms IS NULL OR m.expires_at_ms > ?3)
                 ORDER BY bm25(memories_fts),m.updated_at_ms DESC,m.id",
            )?;
            let rows = statement.query_map(params![lexical_query, scope_key, now], |row| {
                read_record(row, 0)
            })?;
            let mut lexical = Vec::new();
            for record in rows {
                let record = record?;
                if record.is_current()? {
                    lexical.push(record.id);
                    if lexical.len() == CANDIDATES {
                        break;
                    }
                }
            }
            let mut fused = HashMap::<String, f64>::new();
            for ranking in [lexical, semantic.into_iter().map(|(id, _)| id).collect()] {
                for (rank, id) in ranking.into_iter().enumerate() {
                    *fused.entry(id).or_default() += 1.0 / (RRF_K + rank as f64 + 1.0);
                }
            }
            fused.into_iter().collect::<Vec<_>>()
        } else {
            semantic
        };
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let mut records = Vec::new();
        let mut budget = JsonBudget::new(2, budget_options.max_bytes);
        for (id, _) in ranked {
            let record = tx.query_row(
                "SELECT record FROM memories WHERE scope=?1 AND id=?2",
                params![scope_key, id],
                |row| read_record(row, 0),
            )?;
            if !record.is_current()? {
                continue;
            }
            if budget.include(&record, !records.is_empty())? {
                records.push(record);
                if records.len() == budget_options.limit {
                    break;
                }
            }
        }
        tx.commit()?;
        Ok(records)
    }
}
