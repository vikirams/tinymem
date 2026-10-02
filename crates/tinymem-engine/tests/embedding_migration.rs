//! Genuine v2-layout fixture, independently created without Engine::open.
use anyhow::Result;
use rusqlite::{Connection, params};
use serde_json::json;
use tempfile::tempdir;
use tinymem_engine::{
    Embedder, Engine, MemoryRecord, MemoryScope, MemorySearchOptions, SearchMode,
};

struct Constant;
impl Embedder for Constant {
    fn model(&self) -> &str {
        "fixture"
    }
    fn dimensions(&self) -> usize {
        2
    }
    fn embed(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(vec![1.0, 0.0])
    }
}

#[test]
fn v2_migration_preserves_memory_and_audit_then_allows_explicit_backfill() {
    let directory = tempdir().unwrap();
    let db = directory.path().join("native.db");
    let conn = Connection::open(&db).unwrap();
    conn.execute_batch(
        "PRAGMA application_id=0x544d454d;
         PRAGMA user_version=2;
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
         CREATE TRIGGER memories_delete AFTER DELETE ON memories WHEN old.deleted=0 BEGIN
            INSERT INTO memories_fts(memories_fts,rowid,content) VALUES('delete',old.rowid,old.content);
         END;
         CREATE TRIGGER memories_update AFTER UPDATE ON memories BEGIN
            INSERT INTO memories_fts(memories_fts,rowid,content)
                SELECT 'delete',old.rowid,old.content WHERE old.deleted=0;
            INSERT INTO memories_fts(rowid,content) SELECT new.rowid,new.content WHERE new.deleted=0;
         END;",
    ).unwrap();
    let scope = MemoryScope {
        user_id: "alice".into(),
        agent_id: None,
        project_id: None,
    };
    let record: MemoryRecord = serde_json::from_value(json!({
        "id":"v2-memory", "scope":scope, "content":"preserved preference", "kind":"preference",
        "key":null, "metadata":{}, "source":"explicit", "expires_at_ms":null,
        "version":1, "revision":1, "created_at_ms":42, "updated_at_ms":42,
    }))
    .unwrap();
    let snapshot = serde_json::to_string(&record).unwrap();
    let input = json!({"content":record.content,"kind":record.kind,"key":null,"metadata":{},"source":"explicit","expires_at_ms":null}).to_string();
    conn.execute("INSERT INTO memories(id,scope,input,content,record,updated_at_ms) VALUES(?1,?2,?3,?4,?5,42)",
        params![record.id, serde_json::to_string(&scope).unwrap(), input, record.content, snapshot]).unwrap();
    conn.execute(
        "INSERT INTO memory_events VALUES(?1,1,'added',42,?2)",
        params![record.id, snapshot],
    )
    .unwrap();
    drop(conn);
    let engine = Engine::open(&db).unwrap();
    assert_eq!(
        engine.get_memory(&scope, &record.id).unwrap(),
        Some(record.clone())
    );
    assert_eq!(
        engine.search_memories(&scope, "preference", 5).unwrap(),
        vec![record.clone()]
    );
    let audit = engine.memory_events(&scope, &record.id, 100).unwrap();
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].memory, record);
    let version: i64 = Connection::open(&db)
        .unwrap()
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 4);
    let mut engine = engine.with_embedder(Constant).unwrap();
    assert_eq!(engine.index_memories(&scope, 100).unwrap().indexed, 1);
    assert_eq!(
        engine
            .search_memories(
                &scope,
                "unrelated wording",
                MemorySearchOptions {
                    mode: SearchMode::Semantic,
                    ..Default::default()
                }
            )
            .unwrap(),
        vec![record]
    );
    assert_eq!(
        engine.memory_events(&scope, "v2-memory", 100).unwrap(),
        audit
    );
}
