//! Explicit file evidence through public engine APIs; no model or repository index required.
use std::fs;
use tempfile::tempdir;
use tinymem_engine::{
    ContextScope, Engine, FileSourceReference, FreshnessStatus, MemoryScope, NewMemory, SourceRole,
};

fn scope() -> MemoryScope {
    MemoryScope {
        user_id: "alice".into(),
        agent_id: None,
        project_id: Some("app".into()),
    }
}

fn input(content: &str, source_refs: Vec<FileSourceReference>) -> NewMemory {
    serde_json::from_value(serde_json::json!({
        "content": content, "source": "explicit correction", "source_refs": source_refs
    }))
    .unwrap()
}

#[test]
fn file_dependency_is_recalled_until_changed_but_its_origin_and_history_survive() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let db = root.join("memory.db");
    fs::write(root.join("clock.rs"), "use UTC\n").unwrap();
    let dependency =
        FileSourceReference::capture(root, "clock.rs", SourceRole::Dependency).unwrap();
    let origin = FileSourceReference::capture(root, "clock.rs", SourceRole::Origin).unwrap();
    let mut engine = Engine::open(&db).unwrap();
    let saved = engine
        .add_memory(
            &scope(),
            input("clock correction use UTC", vec![dependency.clone()]),
        )
        .unwrap();
    let recorded = engine
        .add_memory(
            &scope(),
            input("clock decision recorded here", vec![origin]),
        )
        .unwrap();
    assert_eq!(saved.source_refs, vec![dependency]);
    assert_eq!(
        engine.get_memory(&scope(), &saved.id).unwrap(),
        Some(saved.clone())
    );
    assert_eq!(
        engine.recall(&scope(), "correction", 5).unwrap(),
        vec![saved.clone()]
    );
    assert_eq!(
        engine.memory_events(&scope(), &saved.id, 5).unwrap()[0]
            .freshness
            .status,
        FreshnessStatus::Current
    );
    drop(engine);

    fs::write(root.join("clock.rs"), "use monotonic clock\n").unwrap();
    let mut engine = Engine::open(&db).unwrap();
    assert!(engine.get_memory(&scope(), &saved.id).unwrap().is_none());
    assert!(engine.recall(&scope(), "correction", 5).unwrap().is_empty());
    assert!(
        engine
            .search_memories(&scope(), "correction", 5)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        engine.memory_profile(&scope(), 5).unwrap().stable,
        vec![recorded.clone()]
    );
    assert_eq!(
        engine.profile(&scope(), 5).unwrap().stable,
        vec![recorded.clone()]
    );
    let context = engine
        .assemble_context(
            "correction",
            &ContextScope {
                memory: scope(),
                repository: None,
            },
            16000,
        )
        .unwrap();
    assert!(context.entries.is_empty());
    assert_eq!(
        engine.get_memory(&scope(), &recorded.id).unwrap(),
        Some(recorded)
    );
    let history = engine.memory_events(&scope(), &saved.id, 5).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].memory, saved);
    assert_eq!(history[0].freshness.status, FreshnessStatus::Stale);
    assert_eq!(
        serde_json::to_value(&history[0].freshness).unwrap()["reason"],
        "content_changed"
    );
    assert_eq!(
        engine.memory_history(&scope(), &saved.id).unwrap(),
        vec![saved]
    );
}
