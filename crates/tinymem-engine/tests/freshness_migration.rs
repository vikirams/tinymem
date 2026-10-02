//! Independently constructed native-v3 layout; migration is exercised via Engine::open.
use rusqlite::{Connection, params};
use serde_json::json;
use tempfile::tempdir;
use tinymem_engine::{
    Embedder, Engine, MemoryRecord, MemoryScope, MemorySearchOptions, NewMemory, SearchMode,
};

fn scope() -> MemoryScope {
    MemoryScope {
        user_id: "alice".into(),
        agent_id: None,
        project_id: None,
    }
}

fn v3_fixture(path: &std::path::Path) -> (Connection, MemoryRecord) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        "PRAGMA application_id=0x544d454d;
         PRAGMA user_version=3;
         CREATE TABLE memories (
            rowid INTEGER PRIMARY KEY, id TEXT NOT NULL UNIQUE, scope TEXT NOT NULL,
            memory_key TEXT, input TEXT NOT NULL, content TEXT NOT NULL, record TEXT NOT NULL,
            expires_at_ms TEXT, updated_at_ms INTEGER NOT NULL,
            deleted INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0,1))
         );
         CREATE TABLE memory_events (
            memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
            version INTEGER NOT NULL, event TEXT NOT NULL CHECK (event IN ('added','updated','deleted')),
            at_ms INTEGER NOT NULL, snapshot TEXT NOT NULL, PRIMARY KEY(memory_id,version)
         );
         CREATE VIRTUAL TABLE memories_fts USING fts5(content,content='memories',content_rowid='rowid');
         CREATE TRIGGER memories_insert AFTER INSERT ON memories WHEN new.deleted=0 BEGIN
            INSERT INTO memories_fts(rowid,content) VALUES(new.rowid,new.content);
         END;
         CREATE TABLE memory_embeddings (
            memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
            model TEXT NOT NULL, dimensions INTEGER NOT NULL CHECK (dimensions BETWEEN 1 AND 4096),
            version INTEGER NOT NULL, vector BLOB NOT NULL, PRIMARY KEY(memory_id, model, dimensions)
         );"
    ).unwrap();
    // Genuine old JSON, not current MemoryRecord serialization with its default fields.
    let snapshot = json!({
        "id":"0123456789abcdef0123456789abcdef", "scope":scope(), "content":"clock correction", "kind":"fact",
        "key":null, "metadata":{}, "source":"clock.rs", "expires_at_ms":null,
        "version":1, "revision":1, "created_at_ms":42, "updated_at_ms":42
    }).to_string();
    let input = r#"{"content":"clock correction","kind":"fact","key":null,"metadata":{},"source":"clock.rs","expires_at_ms":null}"#;
    let record: MemoryRecord = serde_json::from_str(&snapshot).unwrap();
    conn.execute("INSERT INTO memories(id,scope,input,content,record,updated_at_ms) VALUES(?1,?2,?3,?4,?5,42)",
        params![record.id, serde_json::to_string(&scope()).unwrap(), input, record.content, snapshot]).unwrap();
    conn.execute(
        "INSERT INTO memory_events VALUES(?1,1,'added',42,?2)",
        params![record.id, snapshot],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO memory_embeddings VALUES(?1,'fixture',2,1,?2)",
        params![record.id, [1f32.to_le_bytes(), 0f32.to_le_bytes()].concat()],
    )
    .unwrap();
    (conn, record)
}

struct Constant;
impl Embedder for Constant {
    fn model(&self) -> &str {
        "fixture"
    }
    fn dimensions(&self) -> usize {
        2
    }
    fn embed(&self, _: &str) -> anyhow::Result<Vec<f32>> {
        Ok(vec![1., 0.])
    }
}

#[test]
fn v3_migration_restart_retains_legacy_sources_ids_dedup_vectors_and_audit_without_dependencies() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let (conn, old) = v3_fixture(&db);
    drop(conn);
    for _ in 0..2 {
        let mut engine = Engine::open(&db).unwrap().with_embedder(Constant).unwrap();
        assert_eq!(
            engine.get_memory(&scope(), &old.id).unwrap(),
            Some(old.clone())
        );
        assert!(old.source_refs.is_empty());
        assert_eq!(
            engine
                .search_memories(
                    &scope(),
                    "query",
                    MemorySearchOptions {
                        mode: SearchMode::Semantic,
                        ..Default::default()
                    }
                )
                .unwrap(),
            vec![old.clone()]
        );
        let same: NewMemory =
            serde_json::from_value(json!({"content":"clock correction", "source":"clock.rs"}))
                .unwrap();
        assert_eq!(engine.add_memory(&scope(), same).unwrap(), old);
        let audit = engine.memory_events(&scope(), &old.id, 100).unwrap();
        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].memory, old);
        assert_eq!(
            serde_json::to_value(&audit[0].freshness).unwrap(),
            json!({"status":"current","reason":"no_dependencies","checked_at_ms":null,"checks":[]})
        );
    }
}

#[test]
fn failed_v3_migration_rolls_back_all_schema_changes_and_preserves_old_data() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let (conn, old) = v3_fixture(&db);
    // A malformed/interrupted external schema has a conflicting new column. Failure
    // occurs at the v4 step after earlier CREATEs; none of those may be committed.
    conn.execute_batch("ALTER TABLE memories ADD COLUMN source_refs_key TEXT")
        .unwrap();
    let before: String = conn
        .query_row(
            "SELECT group_concat(sql,';') FROM sqlite_master ORDER BY rowid",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(Engine::open(&db).is_err());
    let after: String = conn
        .query_row(
            "SELECT group_concat(sql,';') FROM sqlite_master ORDER BY rowid",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(
        conn.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        3
    );
    // Repair only the deliberate fixture fault and retry, without losing evidence.
    conn.execute_batch("ALTER TABLE memories DROP COLUMN source_refs_key")
        .unwrap();
    drop(conn);
    let engine = Engine::open(&db).unwrap();
    assert_eq!(
        engine.get_memory(&scope(), &old.id).unwrap(),
        Some(old.clone())
    );
    assert_eq!(
        engine.memory_events(&scope(), &old.id, 100).unwrap()[0].memory,
        old
    );
}
