//! One test owns the process-local SQLite VFS registration. Only the busy-wait
//! callback is wrapped; databases, file locking, and Engine operations are real.

use rusqlite::{Connection, ffi, params};
use std::cell::RefCell;
use std::fs;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;
use tempfile::tempdir;
use tinymem_engine::Engine;

type BusyBarrier = (Sender<()>, Receiver<()>);
thread_local! {
    static BUSY_BARRIER: RefCell<Option<BusyBarrier>> = const { RefCell::new(None) };
}

#[repr(C)]
struct BusyVfs {
    vfs: ffi::sqlite3_vfs,
    parent: *mut ffi::sqlite3_vfs,
}

unsafe extern "C" fn wait_for_commit(vfs: *mut ffi::sqlite3_vfs, micros: i32) -> i32 {
    if let Some((blocked, committed)) = BUSY_BARRIER.with(|slot| slot.borrow_mut().take()) {
        // Do not panic across the C ABI if the test fails or its channels close.
        if blocked.send(()).is_ok() {
            let _ = committed.recv_timeout(Duration::from_secs(10));
        }
    }
    // SAFETY: SQLite receives the first field of our repr(C) wrapper. Its Box
    // outlives every connection using it; SQLite owns the unchanged parent VFS.
    unsafe {
        let parent = (*vfs.cast::<BusyVfs>()).parent;
        ((*parent).xSleep.unwrap())(parent, micros)
    }
}

struct BusyWaitVfs(Box<BusyVfs>);

impl BusyWaitVfs {
    fn install() -> Self {
        // SAFETY: SQLite is initialized by the fixture connection. Copy its VFS
        // callbacks and keep the registered wrapper at a stable address until
        // the opener has joined and all its connections have closed.
        unsafe {
            let parent = ffi::sqlite3_vfs_find(std::ptr::null());
            assert!(!parent.is_null());
            let mut wrapper = Box::new(BusyVfs {
                vfs: *parent,
                parent,
            });
            wrapper.vfs.zName = c"tinymem-schema-lock-test".as_ptr();
            wrapper.vfs.pNext = std::ptr::null_mut();
            wrapper.vfs.xSleep = Some(wait_for_commit);
            assert_eq!(
                ffi::sqlite3_vfs_register(&mut wrapper.vfs, 1),
                ffi::SQLITE_OK
            );
            Self(wrapper)
        }
    }
}

impl Drop for BusyWaitVfs {
    fn drop(&mut self) {
        // SAFETY: The test has joined the opener; no connection still uses the
        // wrapper. Restore the original default before releasing the Box.
        unsafe {
            ffi::sqlite3_vfs_unregister(&mut self.0.vfs);
            ffi::sqlite3_vfs_register(self.0.parent, 1);
        }
    }
}

fn v1_fixture(db: &Path, root: &Path) -> Connection {
    let conn = Connection::open(db).unwrap();
    // An external native-v1 repository fixture, not Engine internals or a v2
    // database with its version marker edited. No personal-memory tables exist.
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA application_id=0x544d454d;
         PRAGMA user_version=1;
         CREATE TABLE repositories (root TEXT PRIMARY KEY, indexed_at_ms INTEGER NOT NULL);
         CREATE TABLE files (
            root TEXT NOT NULL, path TEXT NOT NULL, hash TEXT NOT NULL,
            language TEXT NOT NULL, indexed_at_ms INTEGER NOT NULL,
            symbols TEXT NOT NULL, syntax_errors INTEGER NOT NULL,
            PRIMARY KEY(root, path),
            FOREIGN KEY(root) REFERENCES repositories(root) ON DELETE CASCADE
         );
         CREATE TABLE chunks (
            id INTEGER PRIMARY KEY, root TEXT NOT NULL, path TEXT NOT NULL,
            start_line INTEGER NOT NULL, end_line INTEGER NOT NULL, content TEXT NOT NULL,
            FOREIGN KEY(root, path) REFERENCES files(root, path) ON DELETE CASCADE
         );
         CREATE INDEX chunks_scope ON chunks(root, path);
         CREATE VIRTUAL TABLE chunks_fts USING fts5(
            content, path, content='chunks', content_rowid='id'
         );
         CREATE TRIGGER chunks_insert AFTER INSERT ON chunks BEGIN
            INSERT INTO chunks_fts(rowid, content, path) VALUES (new.id, new.content, new.path);
         END;
         CREATE TRIGGER chunks_delete AFTER DELETE ON chunks BEGIN
            INSERT INTO chunks_fts(chunks_fts, rowid, content, path)
            VALUES ('delete', old.id, old.content, old.path);
         END;",
    )
    .unwrap();
    let content = "migrationneedle preserved repository evidence\n";
    fs::write(root.join("source.txt"), content).unwrap();
    let root = root.canonicalize().unwrap();
    let root = root.to_str().unwrap();
    conn.execute("INSERT INTO repositories VALUES (?1, 42)", [root])
        .unwrap();
    conn.execute(
        "INSERT INTO files VALUES (?1, 'source.txt', ?2, 'text', 42, '[]', 0)",
        params![root, blake3::hash(content.as_bytes()).to_hex().to_string()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO chunks(root,path,start_line,end_line,content)
         VALUES (?1, 'source.txt', 1, 1, ?2)",
        params![root, content.trim_end()],
    )
    .unwrap();
    conn
}

fn file_captures_are_revalidated_after_waiting_for_the_writer_lock() {
    use tinymem_engine::{FileSourceReference, MemoryScope, NewMemory, SourceRole};
    for change in ["edit", "delete", "unavailable"] {
        let directory = tempdir().unwrap();
        let db = directory.path().join("memory.db");
        let path = directory.path().join("source.txt");
        fs::write(&path, "original").unwrap();
        drop(Engine::open(&db).unwrap());
        let writer = Connection::open(&db).unwrap();
        let vfs = BusyWaitVfs::install();
        // This connection must be opened on the instrumented VFS before taking
        // the lock, so it is add_memory (not schema migration) that waits.
        let mut engine = Engine::open(&db).unwrap();
        let scope = MemoryScope {
            user_id: "alice".into(),
            agent_id: None,
            project_id: None,
        };
        let captured =
            FileSourceReference::capture(directory.path(), "source.txt", SourceRole::Dependency)
                .unwrap();
        let input: NewMemory = serde_json::from_value(
            serde_json::json!({"content":"captured correction", "source_refs":[captured]}),
        )
        .unwrap();
        writer.execute_batch("BEGIN IMMEDIATE").unwrap();
        let (blocked, wait_for_lock) = mpsc::channel();
        let (committed, wait_for_commit) = mpsc::channel();
        let writing_scope = scope.clone();
        let writing = thread::spawn(move || {
            BUSY_BARRIER.with(|slot| *slot.borrow_mut() = Some((blocked, wait_for_commit)));
            let result = engine.add_memory(&writing_scope, input);
            BUSY_BARRIER.with(|slot| slot.borrow_mut().take());
            result
        });
        let lock_observed = wait_for_lock.recv_timeout(Duration::from_secs(10));
        match change {
            "edit" => fs::write(&path, "modified").unwrap(),
            "delete" => fs::remove_file(&path).unwrap(),
            _ => {
                fs::remove_file(&path).unwrap();
                fs::create_dir(&path).unwrap();
            }
        }
        writer.execute_batch("COMMIT").unwrap();
        let _ = committed.send(());
        let result = writing.join().unwrap();
        drop(vfs);
        lock_observed.expect("add_memory must encounter the real writer lock");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("source changed or unavailable")
        );
        let engine = Engine::open(&db).unwrap();
        assert!(
            engine
                .memory_profile(&scope, 100)
                .unwrap()
                .stable
                .is_empty()
        );
        assert!(engine.recall(&scope, "correction", 100).unwrap().is_empty());
    }
}

#[test]
fn opening_revalidates_foreign_and_newer_schemas_after_waiting_for_the_writer_lock() {
    // One test owns the process-wide VFS; no parallel registration races.
    file_captures_are_revalidated_after_waiting_for_the_writer_lock();
    for (change, expected_error) in [
        ("PRAGMA user_version=5", Some("newer")),
        ("PRAGMA application_id=1234", Some("not a TinyMem")),
        ("PRAGMA user_version=1", None),
    ] {
        let database = tempdir().unwrap();
        let repo = tempdir().unwrap();
        let db = database.path().join("context.db");
        let writer = v1_fixture(&db, repo.path());
        writer.execute_batch("BEGIN IMMEDIATE").unwrap();
        writer.execute_batch(change).unwrap();
        let vfs = BusyWaitVfs::install();
        let (blocked, wait_for_lock) = mpsc::channel();
        let (committed, wait_for_commit) = mpsc::channel();
        let opening_db = db.clone();
        let opening_root = repo.path().to_owned();
        let opener = thread::spawn(move || {
            BUSY_BARRIER.with(|slot| *slot.borrow_mut() = Some((blocked, wait_for_commit)));
            let result = Engine::open(opening_db)
                .map(|engine| engine.search(opening_root, "migrationneedle", 5).unwrap());
            BUSY_BARRIER.with(|slot| slot.borrow_mut().take());
            result
        });
        let lock_observed = wait_for_lock.recv_timeout(Duration::from_secs(10));
        writer.execute_batch("COMMIT").unwrap();
        let before = (
            fs::read(&db).unwrap(),
            fs::read(database.path().join("context.db-wal")).unwrap(),
        );
        let _ = committed.send(());
        let result = opener.join().unwrap();
        drop(vfs);
        lock_observed.expect("opener must encounter the real SQLite writer lock");
        if let Some(message) = expected_error {
            let error = match result {
                Err(error) => error,
                Ok(_) => {
                    panic!("Engine accepted a concurrently committed incompatible schema: {change}")
                }
            };
            assert!(error.to_string().contains(message), "{error:#}");
            assert_eq!(fs::read(&db).unwrap(), before.0);
            assert_eq!(
                fs::read(database.path().join("context.db-wal")).unwrap(),
                before.1,
                "rejected schema must not be migrated"
            );
            assert!(Engine::open(&db).is_err(), "schema must stay incompatible");
        } else {
            let hits = result.unwrap();
            assert_eq!(hits.len(), 1);
            assert_eq!(hits[0].path, "source.txt");
            assert_eq!(
                hits[0].content,
                "migrationneedle preserved repository evidence"
            );
            assert_eq!(hits[0].indexed_at_ms, 42);
            let reopened = Engine::open(&db).unwrap();
            assert_eq!(
                reopened.search(repo.path(), "migrationneedle", 5).unwrap()[0].content,
                hits[0].content
            );
        }
    }
}
