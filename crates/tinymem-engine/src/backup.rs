//! Explicit SQLite snapshots, separate from Engine::open (never migrates a source).
use anyhow::{Context, Result, ensure};
use rusqlite::{
    Connection, OpenFlags,
    backup::{Backup, StepResult},
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn no_symlinks(path: &Path) -> Result<()> {
    for parent in path.ancestors() {
        if let Ok(meta) = fs::symlink_metadata(parent) {
            ensure!(
                !meta.file_type().is_symlink(),
                "symlink path refused: {}",
                parent.display()
            );
        }
    }
    Ok(())
}
fn sidecars(path: &Path) -> Vec<PathBuf> {
    ["-wal", "-shm", "-journal"]
        .map(|suffix| {
            let mut value = path.as_os_str().to_os_string();
            value.push(suffix);
            PathBuf::from(value)
        })
        .to_vec()
}
fn absent(path: &Path) -> Result<()> {
    ensure!(
        fs::symlink_metadata(path).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
        "destination already exists or cannot be inspected: {}",
        path.display()
    );
    for sidecar in sidecars(path) {
        ensure!(
            fs::symlink_metadata(&sidecar).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
            "destination SQLite sidecar exists or cannot be inspected: {}",
            sidecar.display()
        );
    }
    Ok(())
}
fn validate_snapshot(conn: &Connection) -> Result<()> {
    let id: i64 = conn.pragma_query_value(None, "application_id", |row| row.get(0))?;
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    ensure!(
        id == super::APPLICATION_ID && (1..=super::SCHEMA_VERSION).contains(&version),
        "not a compatible TinyMem native database"
    );
    let integrity: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    ensure!(
        integrity == "ok",
        "SQLite integrity check failed: {integrity}"
    );
    let mut tables = vec!["repositories", "files", "chunks", "chunks_fts"];
    if version >= 2 {
        tables.extend(["memories", "memory_events", "memories_fts"]);
    }
    if version >= 3 {
        tables.push("memory_embeddings");
    }
    for table in tables {
        let count: i64 = conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?",
            [table],
            |row| row.get(0),
        )?;
        ensure!(count == 1, "TinyMem schema table missing: {table}");
    }
    // Preparation validates required column names without changing data/schema.
    for query in [
        "SELECT root,indexed_at_ms FROM repositories LIMIT 0",
        "SELECT root,path,hash,language,indexed_at_ms,symbols,syntax_errors FROM files LIMIT 0",
        "SELECT id,root,path,start_line,end_line,content FROM chunks LIMIT 0",
    ] {
        conn.prepare(query)?;
    }
    if version >= 2 {
        conn.prepare("SELECT rowid,id,scope,memory_key,input,content,record,expires_at_ms,updated_at_ms,deleted FROM memories LIMIT 0")?;
        conn.prepare("SELECT memory_id,version,event,at_ms,snapshot FROM memory_events LIMIT 0")?;
    }
    if version >= 3 {
        conn.prepare(
            "SELECT memory_id,model,dimensions,version,vector FROM memory_embeddings LIMIT 0",
        )?;
    }
    if version >= 4 {
        conn.prepare("SELECT source_refs_key FROM memories LIMIT 0")?;
    }
    let mut foreign = conn.prepare("PRAGMA foreign_key_check")?;
    ensure!(
        foreign.query([])?.next()?.is_none(),
        "SQLite foreign key check failed"
    );
    // FTS external-content checks run only on the private snapshot. SQLite's
    // ordinary integrity_check does not validate FTS index/content consistency.
    conn.execute(
        "INSERT INTO chunks_fts(chunks_fts, rank) VALUES ('integrity-check', 1)",
        [],
    )?;
    if version >= 2 {
        // Deleted personal records are intentionally absent from this index, so
        // external-content rank=1 would compare against the wrong active subset.
        conn.execute(
            "INSERT INTO memories_fts(memories_fts) VALUES ('integrity-check')",
            [],
        )?;
    }
    Ok(())
}

/// Publish a consistent validated snapshot to an absent path. Both backup and
/// restore use this operation; callers must stop all users of a restore target.
/// The source is read-only. Publication uses a no-replace hard link, not rename.
pub fn snapshot_database(source: &Path, destination: &Path) -> Result<()> {
    ensure!(
        source.is_absolute() && destination.is_absolute(),
        "backup/restore require absolute paths"
    );
    no_symlinks(source)?;
    no_symlinks(destination)?;
    ensure!(
        fs::metadata(source)?.is_file(),
        "source must be a regular database file"
    );
    for sidecar in sidecars(source) {
        no_symlinks(&sidecar)?;
    }
    absent(destination)?;
    let parent = destination.parent().context("destination has no parent")?;
    ensure!(parent.is_dir(), "destination parent must already exist");
    let temporary = parent.join(format!(
        ".tinymem-snapshot-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let created = options.open(&temporary)?;
    drop(created);
    let result = (|| -> Result<()> {
        let source = Connection::open_with_flags(
            source,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        source.busy_timeout(Duration::from_secs(5))?;
        let mut target = Connection::open(&temporary)?;
        {
            let backup = Backup::new(&source, &mut target)?;
            let deadline = Instant::now() + Duration::from_secs(30);
            loop {
                match backup.step(256)? {
                    StepResult::Done => break,
                    StepResult::More | StepResult::Busy | StepResult::Locked => {}
                    _ => anyhow::bail!("unsupported SQLite backup status"),
                }
                ensure!(
                    Instant::now() < deadline,
                    "SQLite snapshot exceeded 30 seconds"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        // Validate the finished consistent image rather than racing source reads.
        target.pragma_update(None, "journal_mode", "DELETE")?;
        validate_snapshot(&target)?;
        drop(target);
        fs::File::open(&temporary)?.sync_all()?;
        absent(destination)?;
        no_symlinks(destination)?;
        fs::hard_link(&temporary, destination)
            .context("cannot publish snapshot without overwriting destination")?;
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    for sidecar in sidecars(&temporary) {
        let _ = fs::remove_file(sidecar);
    }
    result
}
