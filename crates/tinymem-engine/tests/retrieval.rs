//! Retrieval mechanics, not a benchmark of any model's semantic quality.
use anyhow::{Result, bail};
use rusqlite::Connection;
use std::sync::{Arc, Barrier};
use std::thread;
use tempfile::tempdir;
use tinymem_engine::{
    Embedder, Engine, MemoryKind, MemoryScope, MemorySearchOptions, MemoryUpdate, NewMemory,
    SearchMode, SearchOptions,
};

fn scope() -> MemoryScope {
    MemoryScope {
        user_id: "alice".into(),
        agent_id: None,
        project_id: None,
    }
}
fn memory(content: &str) -> NewMemory {
    NewMemory {
        content: content.into(),
        kind: MemoryKind::Fact,
        source: "test".into(),
        expires_at_ms: None,
        source_refs: Vec::new(),
    }
}
fn options(mode: SearchMode) -> MemorySearchOptions {
    MemorySearchOptions {
        mode,
        search: SearchOptions {
            limit: 100,
            max_bytes: 100_000,
        },
        ..Default::default()
    }
}
struct Fixture {
    model: &'static str,
}
impl Embedder for Fixture {
    fn model(&self) -> &str {
        self.model
    }
    fn dimensions(&self) -> usize {
        3
    }
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        Ok(
            if text.contains("espresso") || text.contains("morning drink") || text == "hot beverage"
            {
                vec![20.0, 0.0, 0.0]
            } else if text.contains("compiler") {
                vec![0.0, 10.0, 0.0]
            } else {
                vec![0.0, 0.0, 1.0]
            },
        )
    }
}

#[test]
fn opt_in_backfill_persistence_semantic_matches_and_hybrid_union_preserve_evidence() {
    let directory = tempdir().unwrap();
    let db = directory.path().join("memory.db");
    let mut engine = Engine::open(&db).unwrap();
    let exact = engine.add_memory(&scope(), memory("espresso")).unwrap();
    let meaning = engine
        .add_memory(&scope(), memory("morning drink"))
        .unwrap();
    let keyword = engine.add_memory(&scope(), memory("compiler")).unwrap();
    let audit = engine.memory_events(&scope(), &meaning.id, 100).unwrap();
    assert!(
        engine
            .search_memories(&scope(), "hot beverage", 5)
            .unwrap()
            .is_empty()
    );
    assert!(
        engine
            .search_memories(&scope(), "hot beverage", options(SearchMode::Semantic))
            .is_err()
    );
    assert!(engine.index_memories(&scope(), 100).is_err());
    let mut engine = engine
        .with_embedder(Fixture {
            model: "fixture-v1",
        })
        .unwrap();
    assert!(
        engine
            .search_memories(&scope(), "hot beverage", options(SearchMode::Semantic))
            .unwrap()
            .is_empty()
    );
    assert_eq!(engine.index_memories(&scope(), 0).unwrap().remaining, 3);
    let partial = engine.index_memories(&scope(), 1).unwrap();
    assert_eq!((partial.indexed, partial.remaining), (1, 2));
    let completed = engine.index_memories(&scope(), 1000).unwrap();
    assert_eq!((completed.indexed, completed.remaining), (2, 0));
    assert_eq!(engine.index_memories(&scope(), 100).unwrap().indexed, 0);
    let semantic = engine
        .search_memories(&scope(), "hot beverage", options(SearchMode::Semantic))
        .unwrap();
    assert_eq!(semantic.len(), 2);
    assert!(semantic.contains(&exact) && semantic.contains(&meaning));
    assert!(!semantic.contains(&keyword));
    // Hybrid OR finds a lexical-only candidate and boosts the shared candidate.
    let hybrid = engine
        .search_memories(&scope(), "espresso compiler", options(SearchMode::Hybrid))
        .unwrap();
    assert_eq!(hybrid[0], exact);
    assert_eq!(hybrid.len(), 3);
    assert!(hybrid.contains(&meaning) && hybrid.contains(&keyword));
    assert!(
        engine
            .search_memories(&scope(), "espresso compiler", 5)
            .unwrap()
            .is_empty(),
        "default remains literal AND"
    );
    assert_eq!(
        engine.memory_events(&scope(), &meaning.id, 100).unwrap(),
        audit
    );
    drop(engine);
    let engine = Engine::open(&db)
        .unwrap()
        .with_embedder(Fixture {
            model: "fixture-v1",
        })
        .unwrap();
    assert_eq!(
        engine
            .search_memories(&scope(), "hot beverage", options(SearchMode::Semantic))
            .unwrap(),
        semantic
    );
    let mut other_model = Engine::open(&db)
        .unwrap()
        .with_embedder(Fixture {
            model: "fixture-v2",
        })
        .unwrap();
    assert!(
        other_model
            .search_memories(&scope(), "hot beverage", options(SearchMode::Semantic))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        other_model.index_memories(&scope(), 100).unwrap().indexed,
        3
    );
    assert_eq!(
        other_model
            .search_memories(&scope(), "hot beverage", options(SearchMode::Semantic))
            .unwrap(),
        semantic
    );
    // No current-evidence or derived-index writes occur during semantic/hybrid reads.
    let conn = Connection::open(&db).unwrap();
    let before: i64 = conn
        .query_row("PRAGMA data_version", [], |row| row.get(0))
        .unwrap();
    engine
        .search_memories(&scope(), "espresso", options(SearchMode::Hybrid))
        .unwrap();
    let after: i64 = conn
        .query_row("PRAGMA data_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(before, after);
}

#[test]
fn scope_expiration_updates_deletes_and_unconfigured_writers_cannot_leak_stale_vectors() {
    let directory = tempdir().unwrap();
    let db = directory.path().join("memory.db");
    let mut engine = Engine::open(&db)
        .unwrap()
        .with_embedder(Fixture { model: "test" })
        .unwrap();
    let original = engine.add_memory(&scope(), memory("espresso")).unwrap();
    let expired = engine
        .add_memory(
            &scope(),
            NewMemory {
                expires_at_ms: Some(0),
                ..memory("espresso expired")
            },
        )
        .unwrap();
    for change in [
        MemoryScope {
            user_id: "bob".into(),
            ..scope()
        },
        MemoryScope {
            agent_id: Some("other".into()),
            ..scope()
        },
        MemoryScope {
            project_id: Some("other".into()),
            ..scope()
        },
    ] {
        assert!(
            engine
                .search_memories(&change, "hot beverage", options(SearchMode::Semantic))
                .unwrap()
                .is_empty()
        );
        let other = engine.add_memory(&change, memory("espresso")).unwrap();
        assert_ne!(other.id, original.id);
        assert_eq!(
            engine
                .search_memories(&change, "hot beverage", options(SearchMode::Hybrid))
                .unwrap(),
            vec![other]
        );
    }
    assert_eq!(
        engine
            .search_memories(&scope(), "hot beverage", options(SearchMode::Semantic))
            .unwrap(),
        vec![original.clone()]
    );
    assert!(
        engine
            .memory_events(&scope(), &expired.id, 100)
            .unwrap()
            .len()
            == 1
    );
    let updated = engine
        .update_memory(
            &scope(),
            &original.id,
            MemoryUpdate {
                content: "compiler".into(),
                expected_version: Some(1),
            },
        )
        .unwrap();
    assert!(
        engine
            .search_memories(&scope(), "hot beverage", options(SearchMode::Semantic))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        engine
            .search_memories(&scope(), "compiler", options(SearchMode::Semantic))
            .unwrap(),
        vec![updated.clone()]
    );
    // A lexical-only process invalidates every cached model, transactionally.
    let mut writer = Engine::open(&db).unwrap();
    let newer = writer
        .update_memory(
            &scope(),
            &original.id,
            MemoryUpdate {
                content: "morning drink".into(),
                expected_version: Some(2),
            },
        )
        .unwrap();
    assert!(
        engine
            .search_memories(&scope(), "compiler", options(SearchMode::Semantic))
            .unwrap()
            .is_empty()
    );
    assert!(
        engine
            .search_memories(&scope(), "hot beverage", options(SearchMode::Semantic))
            .unwrap()
            .is_empty()
    );
    assert_eq!(engine.index_memories(&scope(), 100).unwrap().indexed, 1);
    assert_eq!(
        engine
            .search_memories(&scope(), "hot beverage", options(SearchMode::Semantic))
            .unwrap(),
        vec![newer]
    );
    assert!(
        writer
            .delete_memory(&scope(), &original.id, Some(3))
            .unwrap()
    );
    assert!(
        engine
            .search_memories(&scope(), "hot beverage", options(SearchMode::Semantic))
            .unwrap()
            .is_empty()
    );
    assert_eq!(engine.index_memories(&scope(), 100).unwrap().remaining, 0);
    assert!(writer.forget_memory(&scope(), &expired.id).unwrap());
}

#[test]
fn semantic_and_hybrid_enforce_query_threshold_and_exact_serialized_budgets() {
    let directory = tempdir().unwrap();
    let mut engine = Engine::open(directory.path().join("memory.db"))
        .unwrap()
        .with_embedder(Fixture { model: "test" })
        .unwrap();
    engine
        .add_memory(&scope(), memory("espresso large\n\"quoted\""))
        .unwrap();
    engine
        .add_memory(&scope(), memory("morning drink"))
        .unwrap();
    for mode in [SearchMode::Semantic, SearchMode::Hybrid] {
        let all = engine
            .search_memories(&scope(), "hot beverage", options(mode))
            .unwrap();
        let bytes = serde_json::to_vec(&all).unwrap().len();
        let at_budget = MemorySearchOptions {
            search: SearchOptions {
                max_bytes: bytes,
                ..options(mode).search
            },
            ..options(mode)
        };
        assert_eq!(
            engine
                .search_memories(&scope(), "hot beverage", at_budget)
                .unwrap(),
            all
        );
        let smaller = MemorySearchOptions {
            search: SearchOptions {
                max_bytes: bytes - 1,
                ..at_budget.search
            },
            ..at_budget
        };
        let selected = engine
            .search_memories(&scope(), "hot beverage", smaller)
            .unwrap();
        assert!(selected.len() < all.len());
        assert!(serde_json::to_vec(&selected).unwrap().len() < bytes);
        for search in [
            SearchOptions {
                limit: 0,
                max_bytes: 10000,
            },
            SearchOptions {
                limit: 100,
                max_bytes: 1,
            },
        ] {
            assert!(
                engine
                    .search_memories(
                        &scope(),
                        "hot beverage",
                        MemorySearchOptions {
                            search,
                            ..options(mode)
                        }
                    )
                    .unwrap()
                    .is_empty()
            );
        }
        for query in [
            "bad\0query".to_owned(),
            "x".repeat(4097),
            "word ".repeat(33),
        ] {
            assert!(
                engine
                    .search_memories(&scope(), &query, options(mode))
                    .is_err()
            );
        }
        assert!(
            engine
                .search_memories(&scope(), "!", options(mode))
                .unwrap()
                .is_empty()
        );
        for min_similarity in [f64::NAN, f64::INFINITY, 1.01, -1.01] {
            assert!(
                engine
                    .search_memories(
                        &scope(),
                        "hot beverage",
                        MemorySearchOptions {
                            min_similarity,
                            ..options(mode)
                        }
                    )
                    .is_err()
            );
        }
        assert_eq!(
            engine
                .search_memories(
                    &scope(),
                    "hot beverage",
                    MemorySearchOptions {
                        min_similarity: 1.0,
                        ..options(mode)
                    }
                )
                .unwrap()
                .len(),
            2
        );
    }
}

struct Invalid {
    vector: Vec<f32>,
    dimensions: usize,
}
impl Embedder for Invalid {
    fn model(&self) -> &str {
        "test"
    }
    fn dimensions(&self) -> usize {
        self.dimensions
    }
    fn embed(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(self.vector.clone())
    }
}
#[test]
fn invalid_vectors_and_provider_failures_never_partially_commit_memory_or_backfill() {
    let directory = tempdir().unwrap();
    let db = directory.path().join("memory.db");
    for vector in [
        vec![],
        vec![0.0, 0.0, 0.0],
        vec![f32::NAN, 0.0, 0.0],
        vec![f32::INFINITY, 0.0, 0.0],
    ] {
        let mut engine = Engine::open(&db)
            .unwrap()
            .with_embedder(Invalid {
                vector,
                dimensions: 3,
            })
            .unwrap();
        assert!(engine.add_memory(&scope(), memory("invalid")).is_err());
        assert!(
            engine
                .memory_profile(&scope(), 100)
                .unwrap()
                .stable
                .is_empty()
        );
    }
    assert!(
        Engine::open(&db)
            .unwrap()
            .with_embedder(Invalid {
                vector: vec![],
                dimensions: 0
            })
            .is_err()
    );
    assert!(
        Engine::open(&db)
            .unwrap()
            .with_embedder(Invalid {
                vector: vec![],
                dimensions: 4097
            })
            .is_err()
    );
    let mut engine = Engine::open(&db).unwrap();
    let original = engine.add_memory(&scope(), memory("espresso")).unwrap();
    engine.add_memory(&scope(), memory("fail")).unwrap();
    struct Fails;
    impl Embedder for Fails {
        fn model(&self) -> &str {
            "test"
        }
        fn dimensions(&self) -> usize {
            3
        }
        fn embed(&self, text: &str) -> Result<Vec<f32>> {
            if text == "fail" {
                bail!("intentional provider failure")
            }
            Ok(vec![1.0, 0.0, 0.0])
        }
    }
    let mut engine = engine.with_embedder(Fails).unwrap();
    assert!(engine.index_memories(&scope(), 100).is_err());
    assert!(
        engine
            .search_memories(&scope(), "hot beverage", options(SearchMode::Semantic))
            .unwrap()
            .is_empty()
    );
    assert!(
        engine
            .update_memory(
                &scope(),
                &original.id,
                MemoryUpdate {
                    content: "fail".into(),
                    expected_version: Some(1)
                }
            )
            .is_err()
    );
    assert_eq!(
        engine.get_memory(&scope(), &original.id).unwrap(),
        Some(original.clone())
    );
    assert_eq!(
        engine
            .memory_events(&scope(), &original.id, 100)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn concurrent_backfill_rejects_a_vector_generated_for_an_obsolete_version() {
    let directory = tempdir().unwrap();
    let db = directory.path().join("memory.db");
    let mut writer = Engine::open(&db).unwrap();
    let original = writer.add_memory(&scope(), memory("espresso")).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    struct Blocked(Arc<Barrier>);
    impl Embedder for Blocked {
        fn model(&self) -> &str {
            "test"
        }
        fn dimensions(&self) -> usize {
            3
        }
        fn embed(&self, _text: &str) -> Result<Vec<f32>> {
            self.0.wait();
            self.0.wait();
            Ok(vec![1.0, 0.0, 0.0])
        }
    }
    let engine = Engine::open(&db)
        .unwrap()
        .with_embedder(Blocked(barrier.clone()))
        .unwrap();
    let backfill = thread::spawn(move || {
        let mut engine = engine;
        engine.index_memories(&scope(), 100).unwrap()
    });
    barrier.wait();
    writer
        .update_memory(
            &scope(),
            &original.id,
            MemoryUpdate {
                content: "compiler".into(),
                expected_version: Some(1),
            },
        )
        .unwrap();
    barrier.wait();
    let report = backfill.join().unwrap();
    assert_eq!((report.indexed, report.stale, report.remaining), (0, 1, 1));
    let engine = writer.with_embedder(Fixture { model: "test" }).unwrap();
    assert!(
        engine
            .search_memories(&scope(), "hot beverage", options(SearchMode::Semantic))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn corrupt_vectors_fail_explicitly_instead_of_silently_falling_back() {
    let directory = tempdir().unwrap();
    let db = directory.path().join("memory.db");
    let mut engine = Engine::open(&db)
        .unwrap()
        .with_embedder(Fixture { model: "test" })
        .unwrap();
    engine.add_memory(&scope(), memory("espresso")).unwrap();
    Connection::open(&db)
        .unwrap()
        .execute("UPDATE memory_embeddings SET vector=x'00'", [])
        .unwrap();
    assert!(
        engine
            .search_memories(&scope(), "espresso", options(SearchMode::Hybrid))
            .is_err()
    );
    assert_eq!(
        engine
            .search_memories(&scope(), "espresso", 100)
            .unwrap()
            .len(),
        1
    );
}
