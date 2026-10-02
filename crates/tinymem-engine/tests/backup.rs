use blake3::Hash;
use rusqlite::Connection;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use tempfile::{Builder, TempDir};
use tinymem_engine::{Engine, MemoryKind, MemoryScope, MemoryUpdate, NewMemory};

const APPLICATION_ID: i64 = 0x544d454d;

fn tempdir() -> TempDir {
    let canonical_parent = env::temp_dir().canonicalize().unwrap();
    Builder::new()
        .prefix("tinymem-backup-test-")
        .tempdir_in(canonical_parent)
        .unwrap()
}

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tinymem"))
        .args(args)
        .output()
        .expect("run compiled tinymem executable")
}

fn hash(path: &Path) -> Hash {
    blake3::hash(&fs::read(path).expect("read file for digest"))
}

fn wal_path(db: &Path) -> PathBuf {
    PathBuf::from(format!("{}-wal", db.display()))
}

fn scope() -> MemoryScope {
    MemoryScope {
        user_id: "backup-test-user".into(),
        agent_id: Some("backup-test-agent".into()),
        project_id: Some("backup-test-project".into()),
    }
}

fn committed_records(engine: &mut Engine) -> (String, Vec<String>) {
    let scope = scope();
    let added = engine
        .add_memory(
            &scope,
            NewMemory {
                content: "backup keeps the first committed observation".into(),
                kind: MemoryKind::Decision,
                source: "backup-cli-test".into(),
                expires_at_ms: None,
                source_refs: Vec::new(),
            },
        )
        .unwrap();
    engine
        .update_memory(
            &scope,
            &added.id,
            MemoryUpdate {
                content: "backup keeps the corrected committed observation".into(),
                expected_version: Some(1),
            },
        )
        .unwrap();
    let events = engine
        .memory_events(&scope, &added.id, usize::MAX)
        .unwrap()
        .into_iter()
        .map(|event| event.event)
        .collect();
    (added.id, events)
}

#[test]
fn backup_includes_committed_wal_data_and_does_not_checkpoint_or_mutate_source() {
    let directory = tempdir();
    let db = directory.path().join("memory.db");
    let output = directory.path().join("memory.backup.db");
    let mut writer = Engine::open(&db).unwrap();
    let (id, events) = committed_records(&mut writer);
    assert_eq!(events, ["added", "updated"]);

    let wal = wal_path(&db);
    assert!(
        wal.is_file(),
        "committed changes should still have an open WAL sidecar"
    );
    assert!(fs::metadata(&wal).unwrap().len() > 0);
    let source_db_before = hash(&db);
    let source_wal_before = hash(&wal);

    let result = cli(&[
        "--db",
        db.to_str().unwrap(),
        "backup",
        "--output",
        output.to_str().unwrap(),
    ]);
    assert!(
        result.status.success(),
        "backup failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stderr.is_empty());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&result.stdout).unwrap()["backup"],
        output.to_str().unwrap()
    );
    assert!(output.is_file());
    assert_eq!(
        hash(&db),
        source_db_before,
        "backup modified the source database file"
    );
    assert_eq!(
        hash(&wal),
        source_wal_before,
        "backup checkpointed or modified the source WAL"
    );

    // Restore through the public CLI, then use the Engine API to verify current
    // data and every immutable audit event copied from the still-open writer.
    let restored_db = directory.path().join("restored.db");
    let result = cli(&[
        "--db",
        restored_db.to_str().unwrap(),
        "restore",
        "--input",
        output.to_str().unwrap(),
    ]);
    assert!(
        result.status.success(),
        "restore failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let restored = Engine::open(&restored_db).unwrap();
    let restored_memory = restored.get_memory(&scope(), &id).unwrap().unwrap();
    assert_eq!(restored_memory.version, 2);
    assert_eq!(
        restored_memory.content,
        "backup keeps the corrected committed observation"
    );
    let restored_events = restored.memory_events(&scope(), &id, usize::MAX).unwrap();
    assert_eq!(
        restored_events
            .iter()
            .map(|event| event.event.as_str())
            .collect::<Vec<_>>(),
        ["added", "updated"]
    );
    assert_eq!(
        restored_events[0].memory.content,
        "backup keeps the first committed observation"
    );

    // Keep the writer alive through all operations above: backup must use the WAL
    // snapshot without requiring the active application's connection to close.
    drop(writer);
}

fn failed_backup_does_not_publish_output(source: &Path, output: &Path) {
    let result = cli(&[
        "--db",
        source.to_str().unwrap(),
        "backup",
        "--output",
        output.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert!(
        !result.stderr.is_empty(),
        "failed backup should explain its refusal"
    );
}

#[test]
fn backup_rejects_corrupt_foreign_and_future_schema_without_rewriting_sources() {
    let directory = tempdir();
    let name = "corrupt.db";
    let source = directory.path().join(name);
    let output = directory.path().join(format!("{name}.backup"));
    fs::write(&source, b"not a sqlite database").unwrap();
    let source_hash = hash(&source);
    failed_backup_does_not_publish_output(&source, &output);
    assert_eq!(hash(&source), source_hash);
    assert!(!output.exists());

    let foreign = directory.path().join("foreign.db");
    let connection = Connection::open(&foreign).unwrap();
    connection
        .execute_batch("CREATE TABLE foreign_data(value TEXT);")
        .unwrap();
    drop(connection);
    let foreign_hash = hash(&foreign);
    let foreign_output = directory.path().join("foreign.backup");
    failed_backup_does_not_publish_output(&foreign, &foreign_output);
    assert_eq!(hash(&foreign), foreign_hash);
    assert!(!foreign_output.exists());

    let future = directory.path().join("future.db");
    let connection = Connection::open(&future).unwrap();
    connection
        .pragma_update(None, "application_id", APPLICATION_ID)
        .unwrap();
    connection
        .pragma_update(None, "user_version", 5_i64)
        .unwrap();
    drop(connection);
    let future_hash = hash(&future);
    let future_output = directory.path().join("future.backup");
    failed_backup_does_not_publish_output(&future, &future_output);
    assert_eq!(hash(&future), future_hash);
    assert!(!future_output.exists());
}

#[test]
fn backup_and_restore_refuse_existing_destinations_and_sqlite_sidecars() {
    let directory = tempdir();
    let db = directory.path().join("memory.db");
    let mut writer = Engine::open(&db).unwrap();
    committed_records(&mut writer);

    let existing = directory.path().join("existing.backup");
    fs::write(&existing, b"keep this destination").unwrap();
    let before = hash(&existing);
    failed_backup_does_not_publish_output(&db, &existing);
    assert_eq!(hash(&existing), before);

    let blocked = directory.path().join("blocked.backup");
    let blocked_wal = wal_path(&blocked);
    fs::write(&blocked_wal, b"keep this sidecar").unwrap();
    let wal_before = hash(&blocked_wal);
    failed_backup_does_not_publish_output(&db, &blocked);
    assert!(!blocked.exists());
    assert_eq!(hash(&blocked_wal), wal_before);

    let good_backup = directory.path().join("valid.backup");
    let result = cli(&[
        "--db",
        db.to_str().unwrap(),
        "backup",
        "--output",
        good_backup.to_str().unwrap(),
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );

    let existing_restore = directory.path().join("existing-restore.db");
    fs::write(&existing_restore, b"keep this restore destination").unwrap();
    let restore_before = hash(&existing_restore);
    let result = cli(&[
        "--db",
        existing_restore.to_str().unwrap(),
        "restore",
        "--input",
        good_backup.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    assert_eq!(hash(&existing_restore), restore_before);

    let blocked_restore = directory.path().join("blocked-restore.db");
    let restore_wal = wal_path(&blocked_restore);
    fs::write(&restore_wal, b"keep restore sidecar").unwrap();
    let restore_wal_before = hash(&restore_wal);
    let result = cli(&[
        "--db",
        blocked_restore.to_str().unwrap(),
        "restore",
        "--input",
        good_backup.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    assert!(!blocked_restore.exists());
    assert_eq!(hash(&restore_wal), restore_wal_before);
}

#[test]
fn backup_restore_require_explicit_absolute_paths_and_existing_parents() {
    let directory = tempdir();
    let db = directory.path().join("memory.db");
    let mut writer = Engine::open(&db).unwrap();
    committed_records(&mut writer);
    let valid_backup = directory.path().join("valid.backup");
    let result = cli(&[
        "--db",
        db.to_str().unwrap(),
        "backup",
        "--output",
        valid_backup.to_str().unwrap(),
    ]);
    assert!(result.status.success());

    let no_db = cli(&[
        "backup",
        "--output",
        directory.path().join("no-db.backup").to_str().unwrap(),
    ]);
    assert!(!no_db.status.success());
    assert!(String::from_utf8_lossy(&no_db.stderr).contains("explicit --db"));
    let no_restore_db = cli(&["restore", "--input", valid_backup.to_str().unwrap()]);
    assert!(!no_restore_db.status.success());
    assert!(String::from_utf8_lossy(&no_restore_db.stderr).contains("explicit --db"));

    let relative_output = "tinymem-relative-backup.test";
    let relative = cli(&[
        "--db",
        db.to_str().unwrap(),
        "backup",
        "--output",
        relative_output,
    ]);
    assert!(!relative.status.success());
    assert!(String::from_utf8_lossy(&relative.stderr).contains("absolute paths"));
    assert!(!Path::new(relative_output).exists());

    let relative_restore = cli(&[
        "--db",
        "tinymem-relative-restore.test",
        "restore",
        "--input",
        valid_backup.to_str().unwrap(),
    ]);
    assert!(!relative_restore.status.success());
    assert!(String::from_utf8_lossy(&relative_restore.stderr).contains("absolute paths"));
    assert!(!Path::new("tinymem-relative-restore.test").exists());

    let missing_source = directory.path().join("missing.db");
    let missing_output = directory.path().join("missing.backup");
    let missing = cli(&[
        "--db",
        missing_source.to_str().unwrap(),
        "backup",
        "--output",
        missing_output.to_str().unwrap(),
    ]);
    assert!(!missing.status.success());
    assert!(!missing_output.exists());

    let missing_parent_output = directory.path().join("absent-parent/output.db");
    let missing_parent = cli(&[
        "--db",
        db.to_str().unwrap(),
        "backup",
        "--output",
        missing_parent_output.to_str().unwrap(),
    ]);
    assert!(!missing_parent.status.success());
    assert!(!missing_parent_output.parent().unwrap().exists());
}

#[cfg(unix)]
#[test]
fn backup_and_restore_refuse_symlink_source_and_destination_paths() {
    use std::os::unix::fs::symlink;

    let directory = tempdir();
    let db = directory.path().join("memory.db");
    let mut writer = Engine::open(&db).unwrap();
    committed_records(&mut writer);
    let backup = directory.path().join("valid.backup");
    let result = cli(&[
        "--db",
        db.to_str().unwrap(),
        "backup",
        "--output",
        backup.to_str().unwrap(),
    ]);
    assert!(result.status.success());

    let source_link = directory.path().join("source-link.db");
    symlink(&db, &source_link).unwrap();
    let source_link_output = directory.path().join("source-link.backup");
    failed_backup_does_not_publish_output(&source_link, &source_link_output);
    assert!(!source_link_output.exists());

    let destination_target = directory.path().join("destination-target.db");
    fs::write(&destination_target, b"leave target untouched").unwrap();
    let target_hash = hash(&destination_target);
    let destination_link = directory.path().join("destination-link.db");
    symlink(&destination_target, &destination_link).unwrap();
    failed_backup_does_not_publish_output(&db, &destination_link);
    assert_eq!(hash(&destination_target), target_hash);

    let restore_target = directory.path().join("restore-target.db");
    fs::write(&restore_target, b"leave restore target untouched").unwrap();
    let restore_target_hash = hash(&restore_target);
    let restore_link = directory.path().join("restore-link.db");
    symlink(&restore_target, &restore_link).unwrap();
    let result = cli(&[
        "--db",
        restore_link.to_str().unwrap(),
        "restore",
        "--input",
        backup.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("symlink path refused"));
    assert_eq!(hash(&restore_target), restore_target_hash);
}
