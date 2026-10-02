//! Local file provenance. Validation is derived, never an observation or a cache of truth.
use crate::{MemoryRecord, memory::now_ms, source};
use anyhow::{Result, ensure};
use rusqlite::Transaction;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{ErrorKind, Read};
use std::path::{Component, Path};

pub const MAX_SOURCE_REFS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceRole {
    Origin,
    Dependency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    File,
}

/// Immutable capture of a local file. Repository identity is the canonical local root,
/// not a Git remote or HEAD. content_hash is algorithm-qualified (blake3:<hex>).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileSourceReference {
    pub kind: SourceKind,
    pub role: SourceRole,
    pub repository: String,
    pub path: String,
    pub content_hash: String,
    pub observed_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FreshnessStatus {
    Current,
    Stale,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FreshnessReason {
    NoDependencies,
    DependenciesCurrent,
    HashMatches,
    ContentChanged,
    FileMissing,
    SourceUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDependencyCheck {
    pub source_index: usize,
    pub expected_hash: String,
    pub observed_hash: Option<String>,
    pub status: FreshnessStatus,
    pub reason: FreshnessReason,
}

/// Evaluated now against an immutable observation. No timestamp for dependency-free
/// notes: current means no external validity claim, not verified factual correctness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Freshness {
    pub status: FreshnessStatus,
    pub reason: FreshnessReason,
    pub checked_at_ms: Option<u64>,
    pub checks: Vec<FileDependencyCheck>,
}

pub(crate) fn migrate(tx: &Transaction<'_>) -> Result<()> {
    let version: i64 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version < 4 {
        // No guesses from free-text source, no rewrite of immutable snapshots or
        // legacy normalized inputs. The key ignores capture timestamps for dedup.
        tx.execute_batch(
            "ALTER TABLE memories ADD COLUMN source_refs_key TEXT NOT NULL DEFAULT '[]';",
        )?;
    }
    Ok(())
}

fn validate_path(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty() && path.len() <= 4096 && !path.contains(['\0', '\\']),
        "invalid file locator"
    );
    ensure!(
        path.split('/')
            .all(|part| !part.is_empty() && part != "." && part != ".."),
        "file locator must be normalized and relative"
    );
    ensure!(
        Path::new(path)
            .components()
            .all(|part| matches!(part, Component::Normal(_))),
        "file locator must be relative"
    );
    Ok(())
}

impl FileSourceReference {
    pub fn capture(root: impl AsRef<Path>, path: &str, role: SourceRole) -> Result<Self> {
        validate_path(path)?;
        let root = source::repository_root(root.as_ref())?;
        let content_hash = read_hash(&root, path)
            .map_err(|reason| anyhow::anyhow!("file capture failed: {reason:?}"))?;
        Ok(Self {
            kind: SourceKind::File,
            role,
            repository: root.to_str().unwrap().to_owned(),
            path: path.to_owned(),
            content_hash,
            observed_at_ms: now_ms()?,
        })
    }

    fn validate(&self) -> Result<()> {
        validate_path(&self.path)?;
        ensure!(
            self.repository.len() <= 4096
                && !self.repository.contains('\0')
                && Path::new(&self.repository).is_absolute(),
            "invalid repository identity"
        );
        let hash = self.content_hash.strip_prefix("blake3:").unwrap_or("");
        ensure!(
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
            "invalid file digest"
        );
        Ok(())
    }

    fn check(&self, source_index: usize) -> FileDependencyCheck {
        let result = if self.validate().is_err() {
            Err(FreshnessReason::SourceUnavailable)
        } else {
            read_hash(Path::new(&self.repository), &self.path)
        };
        let (status, reason, observed_hash) = match result {
            Ok(hash) if hash == self.content_hash => (
                FreshnessStatus::Current,
                FreshnessReason::HashMatches,
                Some(hash),
            ),
            Ok(hash) => (
                FreshnessStatus::Stale,
                FreshnessReason::ContentChanged,
                Some(hash),
            ),
            Err(FreshnessReason::FileMissing) => {
                (FreshnessStatus::Stale, FreshnessReason::FileMissing, None)
            }
            Err(_) => (
                FreshnessStatus::Unknown,
                FreshnessReason::SourceUnavailable,
                None,
            ),
        };
        FileDependencyCheck {
            source_index,
            expected_hash: self.content_hash.clone(),
            observed_hash,
            status,
            reason,
        }
    }
}

/// Called once before optional provider work, then again under the writer lock.
/// Only explicit captures are checked on insertion; updates preserve provenance.
pub(crate) fn validate_capture(refs: &[FileSourceReference]) -> Result<()> {
    ensure!(refs.len() <= MAX_SOURCE_REFS, "too many source references");
    for reference in refs {
        reference.validate()?;
        ensure!(
            reference.check(0).status == FreshnessStatus::Current,
            "source changed or unavailable since capture"
        );
    }
    Ok(())
}

pub(crate) fn refs_key(refs: &[FileSourceReference]) -> Result<String> {
    // The order is part of the explicit input; observation time is not.
    Ok(serde_json::to_string(
        &refs
            .iter()
            .map(|r| (&r.kind, &r.role, &r.repository, &r.path, &r.content_hash))
            .collect::<Vec<_>>(),
    )?)
}

impl MemoryRecord {
    pub fn freshness(&self) -> Result<Freshness> {
        let mut result = Freshness {
            status: FreshnessStatus::Current,
            reason: FreshnessReason::NoDependencies,
            checked_at_ms: None,
            checks: Vec::new(),
        };
        for (index, reference) in self.source_refs.iter().enumerate() {
            if reference.role == SourceRole::Dependency {
                result.checks.push(reference.check(index));
            }
        }
        if !result.checks.is_empty() {
            result.checked_at_ms = Some(now_ms()?);
            result.reason = FreshnessReason::DependenciesCurrent;
            // Known stale wins over unknown; every dependency still has its own result.
            if let Some(check) = result
                .checks
                .iter()
                .find(|c| c.status == FreshnessStatus::Stale)
                .or_else(|| {
                    result
                        .checks
                        .iter()
                        .find(|c| c.status == FreshnessStatus::Unknown)
                })
            {
                result.status = check.status;
                result.reason = check.reason;
            }
        }
        Ok(result)
    }

    pub(crate) fn is_current(&self) -> Result<bool> {
        Ok(self.freshness()?.status == FreshnessStatus::Current)
    }
}

/// Bounded point-in-time reads, independent of index language/ignore policy. Explicit
/// references may name ignored files. Refuse symlinks, directories and special files.
/// This is not a sandbox or an atomic filesystem snapshot against hostile changes.
fn read_hash(root: &Path, relative: &str) -> std::result::Result<String, FreshnessReason> {
    use FreshnessReason::{FileMissing, SourceUnavailable};
    if root.canonicalize().ok().as_deref() != Some(root) || !root.is_dir() {
        return Err(SourceUnavailable);
    }
    let mut path = root.to_path_buf();
    let parts: Vec<_> = relative.split('/').collect();
    for (index, part) in parts.iter().enumerate() {
        path.push(part);
        let metadata = fs::symlink_metadata(&path).map_err(|e| {
            if e.kind() == ErrorKind::NotFound {
                FileMissing
            } else {
                SourceUnavailable
            }
        })?;
        if metadata.file_type().is_symlink() || (index + 1 < parts.len() && !metadata.is_dir()) {
            return Err(SourceUnavailable);
        }
        if index + 1 == parts.len()
            && (!metadata.is_file() || metadata.len() > source::MAX_FILE_BYTES)
        {
            return Err(SourceUnavailable);
        }
    }
    let mut file = File::open(&path).map_err(|_| SourceUnavailable)?;
    let before = file.metadata().map_err(|_| SourceUnavailable)?;
    let mut bytes = Vec::new();
    (&mut file)
        .take(source::MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| SourceUnavailable)?;
    let after = file.metadata().map_err(|_| SourceUnavailable)?;
    let at_path = fs::symlink_metadata(&path).map_err(|_| SourceUnavailable)?;
    if bytes.len() as u64 > source::MAX_FILE_BYTES
        || !before.is_file()
        || !at_path.is_file()
        || before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || after.len() != at_path.len()
        || after.modified().ok() != at_path.modified().ok()
        || path.canonicalize().ok().as_deref() != Some(path.as_path())
    {
        return Err(SourceUnavailable);
    }
    Ok(format!("blake3:{}", blake3::hash(&bytes).to_hex()))
}
