use std::fs;
use std::path::Path;
use std::sync::{Arc, Barrier};
use tempfile::tempdir;
use tinymem_engine::{
    ContextScope, Embedder, Engine, FileSourceReference, FreshnessReason, FreshnessStatus,
    MemoryRecord, MemoryScope, MemorySearchOptions, MemoryUpdate, NewMemory, SearchMode,
    SourceRole,
};

fn scope() -> MemoryScope {
    MemoryScope {
        user_id: "alice".into(),
        agent_id: Some("worker".into()),
        project_id: Some("app".into()),
    }
}
fn input(content: &str, refs: Vec<FileSourceReference>) -> NewMemory {
    serde_json::from_value(
        serde_json::json!({"content":content,"source":"clock.txt","source_refs":refs}),
    )
    .unwrap()
}
fn capture(root: &Path) -> FileSourceReference {
    FileSourceReference::capture(root, "clock.txt", SourceRole::Dependency).unwrap()
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
fn assert_hidden(engine: &mut Engine, saved: &MemoryRecord, reason: FreshnessReason) {
    assert!(engine.get_memory(&scope(), &saved.id).unwrap().is_none());
    assert!(engine.recall(&scope(), "clock", 100).unwrap().is_empty());
    for mode in [
        SearchMode::Lexical,
        SearchMode::Semantic,
        SearchMode::Hybrid,
    ] {
        assert!(
            engine
                .search_memories(
                    &scope(),
                    "clock",
                    MemorySearchOptions {
                        mode,
                        ..Default::default()
                    }
                )
                .unwrap()
                .is_empty()
        );
    }
    let profile = engine.profile(&scope(), 100).unwrap();
    assert!(profile.stable.is_empty() && profile.recent.is_empty());
    let profile = engine.memory_profile(&scope(), 100).unwrap();
    assert!(profile.stable.is_empty() && profile.dynamic.is_empty());
    assert!(
        engine
            .assemble_context(
                "clock",
                &ContextScope {
                    memory: scope(),
                    repository: None
                },
                16000
            )
            .unwrap()
            .entries
            .is_empty()
    );
    let audit = engine.memory_events(&scope(), &saved.id, 100).unwrap();
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].memory, *saved);
    assert_eq!(audit[0].freshness.reason, reason);
    assert!(audit[0].freshness.checked_at_ms.is_some());
}

#[test]
fn changed_deleted_renamed_unavailable_and_restored_files_apply_to_every_retrieval_mode() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("repo");
    fs::create_dir(&root).unwrap();
    let path = root.join("clock.txt");
    fs::write(&path, "UTC").unwrap();
    let db = dir.path().join("memory.db");
    let mut engine = Engine::open(&db).unwrap().with_embedder(Constant).unwrap();
    let saved = engine
        .add_memory(&scope(), input("clock correction", vec![capture(&root)]))
        .unwrap();
    assert_eq!(engine.index_memories(&scope(), 100).unwrap().remaining, 0);
    for mode in [
        SearchMode::Lexical,
        SearchMode::Semantic,
        SearchMode::Hybrid,
    ] {
        assert_eq!(
            engine
                .search_memories(
                    &scope(),
                    "clock",
                    MemorySearchOptions {
                        mode,
                        ..Default::default()
                    }
                )
                .unwrap(),
            vec![saved.clone()]
        );
    }
    fs::write(&path, "TAI").unwrap(); // same length; mtime alone is not freshness
    assert_hidden(&mut engine, &saved, FreshnessReason::ContentChanged);
    fs::rename(&path, root.join("renamed.txt")).unwrap();
    assert_hidden(&mut engine, &saved, FreshnessReason::FileMissing);
    fs::create_dir(&path).unwrap();
    assert_hidden(&mut engine, &saved, FreshnessReason::SourceUnavailable);
    assert_eq!(
        engine.memory_events(&scope(), &saved.id, 100).unwrap()[0]
            .freshness
            .status,
        FreshnessStatus::Unknown
    );
    fs::remove_dir(&path).unwrap();
    fs::write(&path, vec![0; 1024 * 1024 + 1]).unwrap();
    assert_hidden(&mut engine, &saved, FreshnessReason::SourceUnavailable);
    fs::write(&path, "UTC").unwrap();
    // Validation is point-in-time, not a latched invalidation: returning to the
    // exact captured content restores applicability without rewriting history.
    assert_eq!(
        engine.get_memory(&scope(), &saved.id).unwrap(),
        Some(saved.clone())
    );
    fs::write(root.join(".gitignore"), "clock.txt\n").unwrap();
    engine.refresh(&root).unwrap();
    assert_eq!(
        engine.get_memory(&scope(), &saved.id).unwrap(),
        Some(saved.clone())
    );
    // A missing/unmounted repository cannot prove a file was deleted.
    fs::rename(&root, dir.path().join("offline")).unwrap();
    assert_hidden(&mut engine, &saved, FreshnessReason::SourceUnavailable);
    drop(engine);
    let mut reopened = Engine::open(&db).unwrap().with_embedder(Constant).unwrap();
    assert_hidden(&mut reopened, &saved, FreshnessReason::SourceUnavailable);
}

#[test]
fn source_roles_hash_versions_dedup_and_exact_scopes_are_independent_of_legacy_labels() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("clock.txt"), "").unwrap();
    let mut reference = capture(dir.path());
    assert_eq!(
        reference.content_hash,
        "blake3:af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
    );
    let mut engine = Engine::open(dir.path().join("memory.db")).unwrap();
    let saved = engine
        .add_memory(&scope(), input("clock correction", vec![reference.clone()]))
        .unwrap();
    reference.observed_at_ms += 1; // retry observation time must not affect dedup
    assert_eq!(
        engine
            .add_memory(&scope(), input("clock correction", vec![reference.clone()]))
            .unwrap(),
        saved
    );
    let plain = engine
        .add_memory(&scope(), input("clock correction", vec![]))
        .unwrap();
    assert_ne!(plain.id, saved.id);
    reference.role = SourceRole::Origin;
    let origin = engine
        .add_memory(&scope(), input("clock correction", vec![reference]))
        .unwrap();
    assert_ne!(origin.id, saved.id);
    for other in [
        MemoryScope {
            user_id: "Alice".into(),
            ..scope()
        },
        MemoryScope {
            agent_id: None,
            ..scope()
        },
        MemoryScope {
            project_id: None,
            ..scope()
        },
        MemoryScope {
            project_id: Some(" app".into()),
            ..scope()
        },
    ] {
        assert!(engine.get_memory(&other, &saved.id).unwrap().is_none());
        assert!(
            engine
                .memory_events(&other, &saved.id, 100)
                .unwrap()
                .is_empty()
        );
        assert!(engine.memory_history(&other, &saved.id).unwrap().is_empty());
        assert!(
            engine
                .search_memories(&other, "clock", 100)
                .unwrap()
                .is_empty()
        );
        assert!(
            engine
                .memory_profile(&other, 100)
                .unwrap()
                .stable
                .is_empty()
        );
        assert!(
            engine
                .assemble_context(
                    "clock",
                    &ContextScope {
                        memory: other.clone(),
                        repository: None
                    },
                    8000
                )
                .unwrap()
                .entries
                .is_empty()
        );
        let foreign = engine
            .add_memory(&other, input("clock correction", vec![capture(dir.path())]))
            .unwrap();
        assert_ne!(foreign.id, saved.id);
        assert!(!engine.delete_memory(&scope(), &foreign.id, None).unwrap());
    }
    fs::write(dir.path().join("clock.txt"), "new branch contents").unwrap();
    let next = engine
        .add_memory(
            &scope(),
            input("clock correction", vec![capture(dir.path())]),
        )
        .unwrap();
    assert_ne!(next.id, saved.id);
    assert!(engine.get_memory(&scope(), &saved.id).unwrap().is_none());
    assert_eq!(engine.get_memory(&scope(), &plain.id).unwrap(), Some(plain));
    assert_eq!(
        engine.get_memory(&scope(), &origin.id).unwrap(),
        Some(origin)
    );
}

#[test]
fn invalid_captures_cannot_commit_and_updates_never_silently_rebase_provenance() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("clock.txt"), "UTC").unwrap();
    for path in [
        "../clock.txt",
        "./clock.txt",
        "/clock.txt",
        "a//b",
        "a/../clock.txt",
        "clock.txt/",
        "",
        "missing",
    ] {
        assert!(
            FileSourceReference::capture(root, path, SourceRole::Dependency).is_err(),
            "{path}"
        );
    }
    let db = root.join("memory.db");
    let mut engine = Engine::open(&db).unwrap();
    let reference = capture(root);
    let saved = engine
        .add_memory(&scope(), input("clock correction", vec![reference.clone()]))
        .unwrap();
    fs::write(root.join("clock.txt"), "changed").unwrap();
    assert!(
        engine
            .add_memory(&scope(), input("clock new", vec![reference]))
            .is_err()
    );
    let updated = engine
        .update_memory(
            &scope(),
            &saved.id,
            MemoryUpdate {
                content: "clock clarified".into(),
                expected_version: Some(1),
            },
        )
        .unwrap();
    assert_eq!(updated.id, saved.id);
    assert_eq!(updated.source_refs, saved.source_refs);
    assert_eq!(updated.version, 2);
    assert!(engine.get_memory(&scope(), &saved.id).unwrap().is_none());
    assert!(
        engine
            .update_memory(
                &scope(),
                &saved.id,
                MemoryUpdate {
                    content: updated.content.clone(),
                    expected_version: Some(1)
                }
            )
            .is_err()
    );
    let audit = engine.memory_events(&scope(), &saved.id, 100).unwrap();
    assert_eq!(audit.len(), 2);
    assert_eq!(audit[0].memory, saved);
    assert_eq!(audit[1].memory, updated);
    assert!(
        audit
            .iter()
            .all(|event| event.freshness.status == FreshnessStatus::Stale)
    );
}

#[test]
fn concurrent_optimistic_updates_preserve_reference_and_only_one_observation_commits() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("clock.txt"), "UTC").unwrap();
    let db = dir.path().join("memory.db");
    let mut engine = Engine::open(&db).unwrap();
    let saved = engine
        .add_memory(
            &scope(),
            input("clock correction", vec![capture(dir.path())]),
        )
        .unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let writers: Vec<_> = ["clock first", "clock second"]
        .into_iter()
        .map(|content| {
            let barrier = barrier.clone();
            let db = db.clone();
            let id = saved.id.clone();
            std::thread::spawn(move || {
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
    let results: Vec<_> = writers.into_iter().map(|w| w.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    fs::remove_file(dir.path().join("clock.txt")).unwrap();
    let audit = engine.memory_events(&scope(), &saved.id, 100).unwrap();
    assert_eq!(audit.len(), 2);
    assert_eq!(audit[0].memory, saved);
    assert_eq!(audit[1].memory.source_refs, saved.source_refs);
    assert!(
        audit
            .iter()
            .all(|e| e.freshness.reason == FreshnessReason::FileMissing)
    );
    assert!(engine.get_memory(&scope(), &saved.id).unwrap().is_none());
}

#[test]
fn multiple_dependencies_report_each_checked_version_and_keep_origins_out_of_validity() {
    let dir = tempdir().unwrap();
    for name in ["origin.txt", "first.txt", "second.txt"] {
        fs::write(dir.path().join(name), "original").unwrap();
    }
    let refs: Vec<_> = [
        ("origin.txt", SourceRole::Origin),
        ("first.txt", SourceRole::Dependency),
        ("second.txt", SourceRole::Dependency),
    ]
    .into_iter()
    .map(|(path, role)| FileSourceReference::capture(dir.path(), path, role).unwrap())
    .collect();
    let mut engine = Engine::open(dir.path().join("memory.db")).unwrap();
    let saved = engine
        .add_memory(&scope(), input("clock correction", refs.clone()))
        .unwrap();
    assert!(
        engine
            .add_memory(&scope(), input("too many", vec![refs[0].clone(); 17]))
            .is_err()
    );
    fs::remove_file(dir.path().join("origin.txt")).unwrap();
    assert_eq!(
        engine.get_memory(&scope(), &saved.id).unwrap(),
        Some(saved.clone())
    );
    fs::remove_file(dir.path().join("first.txt")).unwrap();
    fs::create_dir(dir.path().join("first.txt")).unwrap();
    fs::write(dir.path().join("second.txt"), "changed").unwrap();
    let event = &engine.memory_events(&scope(), &saved.id, 100).unwrap()[0];
    assert_eq!(event.memory, saved);
    assert_eq!(event.freshness.status, FreshnessStatus::Stale);
    assert_eq!(event.freshness.reason, FreshnessReason::ContentChanged);
    assert_eq!(event.freshness.checks.len(), 2);
    assert_eq!(event.freshness.checks[0].source_index, 1);
    assert_eq!(event.freshness.checks[0].status, FreshnessStatus::Unknown);
    assert_eq!(event.freshness.checks[1].source_index, 2);
    assert_eq!(
        event.freshness.checks[1].expected_hash,
        refs[2].content_hash
    );
    assert_ne!(
        event.freshness.checks[1].observed_hash.as_ref(),
        Some(&refs[2].content_hash)
    );
}

#[test]
fn source_changes_during_optional_query_work_are_checked_before_returning_cached_matches() {
    struct EditingQuery(std::path::PathBuf);
    impl Embedder for EditingQuery {
        fn model(&self) -> &str {
            "editing-query"
        }
        fn dimensions(&self) -> usize {
            2
        }
        fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
            if text == "clock query" {
                fs::write(&self.0, "changed during query")?;
            }
            Ok(vec![1., 0.])
        }
    }
    let dir = tempdir().unwrap();
    let path = dir.path().join("clock.txt");
    fs::write(&path, "UTC").unwrap();
    let mut engine = Engine::open(dir.path().join("memory.db"))
        .unwrap()
        .with_embedder(EditingQuery(path))
        .unwrap();
    let saved = engine
        .add_memory(
            &scope(),
            input("clock correction", vec![capture(dir.path())]),
        )
        .unwrap();
    assert!(
        engine
            .search_memories(
                &scope(),
                "clock query",
                MemorySearchOptions {
                    mode: SearchMode::Hybrid,
                    ..Default::default()
                }
            )
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        engine.memory_events(&scope(), &saved.id, 100).unwrap()[0]
            .freshness
            .reason,
        FreshnessReason::ContentChanged
    );
}

#[test]
fn stale_candidates_cannot_starve_current_context_or_semantic_ranking() {
    struct Ranked;
    impl Embedder for Ranked {
        fn model(&self) -> &str {
            "ranked"
        }
        fn dimensions(&self) -> usize {
            2
        }
        fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
            Ok(if text.contains("evergreen") {
                vec![1., 1.]
            } else {
                vec![1., 0.]
            })
        }
    }
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("clock.txt"), "UTC").unwrap();
    let db = dir.path().join("memory.db");
    let mut engine = Engine::open(&db).unwrap().with_embedder(Ranked).unwrap();
    let current = engine
        .add_memory(&scope(), input("clock evergreen note", vec![]))
        .unwrap();
    let reference = capture(dir.path());
    for index in 0..101 {
        engine
            .add_memory(
                &scope(),
                input(&format!("clock stale {index}"), vec![reference.clone()]),
            )
            .unwrap();
    }
    fs::remove_file(dir.path().join("clock.txt")).unwrap();
    for mode in [
        SearchMode::Lexical,
        SearchMode::Semantic,
        SearchMode::Hybrid,
    ] {
        assert_eq!(
            engine
                .search_memories(
                    &scope(),
                    "clock",
                    MemorySearchOptions {
                        mode,
                        ..Default::default()
                    }
                )
                .unwrap(),
            vec![current.clone()]
        );
    }
    let context_scope = ContextScope {
        memory: scope(),
        repository: None,
    };
    let mut lexical = Engine::open(&db).unwrap();
    assert_eq!(
        lexical
            .assemble_context("clock", &context_scope, 8000)
            .unwrap()
            .entries[0]
            .content,
        current.content
    );
    assert_eq!(
        engine
            .assemble_context("clock", &context_scope, 8000)
            .unwrap()
            .entries[0]
            .content,
        current.content
    );
}

#[cfg(unix)]
#[test]
fn permission_failures_and_symlinks_are_unknown_not_deletion_or_change() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let dir = tempdir().unwrap();
    let path = dir.path().join("clock.txt");
    fs::write(&path, "UTC").unwrap();
    let mut engine = Engine::open(dir.path().join("memory.db"))
        .unwrap()
        .with_embedder(Constant)
        .unwrap();
    let saved = engine
        .add_memory(
            &scope(),
            input("clock correction", vec![capture(dir.path())]),
        )
        .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read(&path).is_err() {
        // root bypasses permission bits; other unavailable cases always run
        assert_hidden(&mut engine, &saved, FreshnessReason::SourceUnavailable);
    }
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    fs::remove_file(&path).unwrap();
    for target in [dir.path().join("missing"), dir.path().join("target.txt")] {
        if target.ends_with("target.txt") {
            fs::write(&target, "UTC").unwrap();
        }
        symlink(&target, &path).unwrap();
        assert_hidden(&mut engine, &saved, FreshnessReason::SourceUnavailable);
        assert!(FileSourceReference::capture(dir.path(), "clock.txt", SourceRole::Origin).is_err());
        fs::remove_file(&path).unwrap();
    }
}
