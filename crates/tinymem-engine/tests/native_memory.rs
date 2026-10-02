//! Contract tests at the modern public Engine seam, with real persistent SQLite.
use std::sync::{Arc, Barrier};
use std::thread;
use tempfile::tempdir;
use tinymem_engine::{Engine, MemoryKind, MemoryScope, MemoryUpdate, NewMemory, SearchOptions};

fn scope() -> MemoryScope {
    MemoryScope {
        user_id: "alice".into(),
        agent_id: None,
        project_id: None,
    }
}

fn memory(content: &str, kind: MemoryKind) -> NewMemory {
    NewMemory {
        content: content.into(),
        kind,
        source: "explicit user request".into(),
        expires_at_ms: None,
        source_refs: Vec::new(),
    }
}

#[test]
fn modern_crud_keeps_identity_immutable_audit_and_noop_versions_across_restarts() {
    let directory = tempdir().unwrap();
    let db = directory.path().join("memory.db");
    let scope = scope();
    let mut engine = Engine::open(&db).unwrap();
    let original = engine
        .add_memory(&scope, memory("Prefer tea", MemoryKind::Preference))
        .unwrap();
    assert_eq!(original.version, 1);
    let initial_events = engine.memory_events(&scope, &original.id, 100).unwrap();
    assert_eq!(initial_events.len(), 1);
    assert_eq!(initial_events[0].event, "added");
    assert_eq!(initial_events[0].memory, original);

    let unchanged = engine
        .update_memory(
            &scope,
            &original.id,
            MemoryUpdate {
                content: "  Prefer tea  ".into(),
                expected_version: Some(1),
            },
        )
        .unwrap();
    assert_eq!(unchanged, original);
    assert_eq!(
        engine.memory_events(&scope, &original.id, 100).unwrap(),
        initial_events
    );

    let updated = engine
        .update_memory(
            &scope,
            &original.id,
            MemoryUpdate {
                content: "Prefer coffee".into(),
                expected_version: Some(1),
            },
        )
        .unwrap();
    assert_eq!(updated.id, original.id);
    assert_eq!(updated.created_at_ms, original.created_at_ms);
    assert_eq!(updated.kind, original.kind);
    assert_eq!(updated.source, original.source);
    assert_eq!(updated.version, 2);
    // Even a no-op cannot bypass a stale optimistic version.
    assert!(
        engine
            .update_memory(
                &scope,
                &original.id,
                MemoryUpdate {
                    content: updated.content.clone(),
                    expected_version: Some(1),
                }
            )
            .is_err()
    );
    assert!(engine.delete_memory(&scope, &original.id, Some(1)).is_err());
    let events = engine.memory_events(&scope, &original.id, 100).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0], initial_events[0]);
    assert_eq!(events[1].event, "updated");
    assert_eq!(events[1].memory, updated);
    assert!(engine.search_memories(&scope, "tea", 5).unwrap().is_empty());
    assert_eq!(
        engine.search_memories(&scope, "coffee", 5).unwrap(),
        vec![updated.clone()]
    );
    assert_eq!(
        engine.memory_profile(&scope, 5).unwrap().stable,
        vec![updated.clone()]
    );
    assert_eq!(
        engine.memory_events(&scope, &original.id, 100).unwrap(),
        events
    );
    drop(engine);

    let mut engine = Engine::open(&db).unwrap();
    assert_eq!(
        engine.get_memory(&scope, &original.id).unwrap(),
        Some(updated.clone())
    );
    assert!(engine.delete_memory(&scope, &original.id, Some(2)).unwrap());
    assert!(!engine.delete_memory(&scope, &original.id, None).unwrap());
    assert!(
        engine
            .update_memory(
                &scope,
                &original.id,
                MemoryUpdate {
                    content: "do not resurrect".into(),
                    expected_version: None,
                }
            )
            .is_err()
    );
    drop(engine);

    let engine = Engine::open(&db).unwrap();
    assert!(engine.get_memory(&scope, &original.id).unwrap().is_none());
    assert!(
        engine
            .search_memories(&scope, "coffee", 5)
            .unwrap()
            .is_empty()
    );
    assert!(engine.memory_profile(&scope, 5).unwrap().stable.is_empty());
    let audit = engine.memory_events(&scope, &original.id, 100).unwrap();
    assert_eq!(&audit[..2], events.as_slice());
    assert_eq!(audit[2].event, "deleted");
    assert_eq!(audit[2].version, 3);
    assert_eq!(audit[2].memory.id, original.id);
    assert_eq!(audit[2].memory.content, updated.content);
}

#[test]
fn native_profile_classifies_every_kind_and_excludes_expired_evidence() {
    let directory = tempdir().unwrap();
    let mut engine = Engine::open(directory.path().join("memory.db")).unwrap();
    let scope = scope();
    let mut stable = Vec::new();
    let mut dynamic = Vec::new();
    for (kind, is_stable) in [
        (MemoryKind::Fact, true),
        (MemoryKind::Preference, true),
        (MemoryKind::Relationship, true),
        (MemoryKind::Decision, false),
        (MemoryKind::Plan, false),
        (MemoryKind::ArchitecturalDecision, false),
        (MemoryKind::IssueSolution, false),
    ] {
        let record = engine
            .add_memory(&scope, memory("shared evidence", kind))
            .unwrap();
        if is_stable {
            stable.push(record);
        } else {
            dynamic.push(record);
        }
    }
    stable.reverse();
    dynamic.reverse();
    let expired = engine
        .add_memory(
            &scope,
            NewMemory {
                expires_at_ms: Some(0),
                ..memory("expired evidence", MemoryKind::Fact)
            },
        )
        .unwrap();
    let profile = engine.memory_profile(&scope, 100).unwrap();
    assert_eq!(profile.stable, stable);
    assert_eq!(profile.dynamic, dynamic);
    assert!(engine.get_memory(&scope, &expired.id).unwrap().is_none());
    assert!(
        engine
            .search_memories(&scope, "expired", 100)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        engine.memory_events(&scope, &expired.id, 100).unwrap()[0].memory,
        expired
    );
    let budget = serde_json::to_vec(&profile).unwrap().len();
    assert_eq!(
        engine
            .memory_profile(
                &scope,
                SearchOptions {
                    limit: 100,
                    max_bytes: budget
                }
            )
            .unwrap(),
        profile
    );
    let smaller = engine
        .memory_profile(
            &scope,
            SearchOptions {
                limit: 100,
                max_bytes: budget - 1,
            },
        )
        .unwrap();
    assert!(smaller.stable.len() + smaller.dynamic.len() < 7);
    assert!(serde_json::to_vec(&smaller).unwrap().len() < budget);
}

#[test]
fn concurrent_optimistic_updates_commit_exactly_one_new_observation() {
    let directory = tempdir().unwrap();
    let db = directory.path().join("memory.db");
    let mut engine = Engine::open(&db).unwrap();
    let original = engine
        .add_memory(&scope(), memory("original", MemoryKind::Fact))
        .unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let writers: Vec<_> = ["first writer", "second writer"]
        .into_iter()
        .map(|content| {
            let db = db.clone();
            let id = original.id.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                let mut engine = Engine::open(db).unwrap();
                barrier.wait();
                engine.update_memory(
                    &scope(),
                    &id,
                    MemoryUpdate {
                        content: content.into(),
                        expected_version: Some(1),
                    },
                )
            })
        })
        .collect();
    let results: Vec<_> = writers
        .into_iter()
        .map(|writer| writer.join().unwrap())
        .collect();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    let conflict = results
        .iter()
        .find_map(|result| result.as_ref().err())
        .unwrap();
    assert!(conflict.to_string().contains("version conflict"));
    let winner = results.into_iter().find_map(Result::ok).unwrap();
    assert_eq!(winner.version, 2);
    assert_eq!(winner.id, original.id);
    assert_eq!(
        engine.get_memory(&scope(), &original.id).unwrap(),
        Some(winner.clone())
    );
    let events = engine.memory_events(&scope(), &original.id, 100).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].memory, original);
    assert_eq!(events[1].memory, winner);
}
