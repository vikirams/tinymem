use serde_json::json;
use tempfile::tempdir;
use tinymem_engine::{
    Engine, MemoryInput, MemoryKind, MemoryProfile, MemoryRecord, MemoryScope, SearchOptions,
};

fn scope(user: &str) -> MemoryScope {
    MemoryScope {
        user_id: user.into(),
        agent_id: None,
        project_id: None,
    }
}

fn input(content: &str) -> MemoryInput {
    serde_json::from_value(json!({ "content": content })).unwrap()
}

#[test]
fn personal_facts_are_retrievable_and_in_the_stable_profile() {
    let database = tempdir().unwrap();
    let mut engine = Engine::open(database.path().join("memory.db")).unwrap();
    let scope = scope("alice");
    let record = engine
        .remember(&scope, input("  Alice lives in Kyoto.\n"))
        .unwrap();
    assert_eq!(record.content, "Alice lives in Kyoto.");
    assert_eq!(record.kind, MemoryKind::Fact);
    assert_eq!(record.metadata, json!({}));
    assert_eq!(record.revision, 1);
    assert_eq!(record.scope, scope);
    assert_eq!(record.id.len(), 32);
    assert!(
        record
            .id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    );
    assert_eq!(
        engine.get_memory(&scope, &record.id).unwrap(),
        Some(record.clone())
    );
    assert_eq!(
        engine.recall(&scope, "Alice Kyoto", 5).unwrap(),
        vec![record.clone()]
    );
    let profile = engine.profile(&scope, 5).unwrap();
    assert_eq!(profile.stable, vec![record]);
    assert!(profile.recent.is_empty());
}

#[test]
fn same_key_changes_keep_the_id_and_append_only_changed_revisions() {
    let database = tempdir().unwrap();
    let mut engine = Engine::open(database.path().join("memory.db")).unwrap();
    let scope = scope("alice");
    let mut original = input("Alice prefers coffee");
    original.key = Some("drink".into());
    original.kind = MemoryKind::Preference;
    let first = engine.remember(&scope, original.clone()).unwrap();
    original.content = "  Alice prefers coffee\n".into();
    assert_eq!(engine.remember(&scope, original.clone()).unwrap(), first);
    original.content = "Alice prefers tea".into();
    let second = engine.remember(&scope, original.clone()).unwrap();
    assert_eq!(second.id, first.id);
    assert_eq!(second.revision, 2);
    assert_eq!(second.created_at_ms, first.created_at_ms);
    assert!(second.updated_at_ms >= first.updated_at_ms);
    assert_eq!(
        engine.memory_history(&scope, &first.id).unwrap(),
        vec![first, second.clone()]
    );
    assert_eq!(engine.remember(&scope, original).unwrap(), second);
    assert_eq!(engine.memory_history(&scope, &second.id).unwrap().len(), 2);
    assert!(engine.recall(&scope, "coffee", 5).unwrap().is_empty());
    assert_eq!(engine.recall(&scope, "tea", 5).unwrap(), vec![second]);
}

#[test]
fn forgetting_removes_current_and_historical_memory_only_in_its_scope() {
    let database = tempdir().unwrap();
    let mut engine = Engine::open(database.path().join("memory.db")).unwrap();
    let alice = scope("alice");
    let bob = scope("bob");
    let mut original = input("obsolete password preference");
    original.key = Some("preference".into());
    let first = engine.remember(&alice, original.clone()).unwrap();
    original.content = "current passphrase preference".into();
    let current = engine.remember(&alice, original.clone()).unwrap();
    assert!(!engine.forget_memory(&bob, &first.id).unwrap());
    assert_eq!(engine.get_memory(&alice, &first.id).unwrap(), Some(current));
    assert_eq!(engine.memory_history(&alice, &first.id).unwrap().len(), 2);
    assert!(engine.forget_memory(&alice, &first.id).unwrap());
    assert!(!engine.forget_memory(&alice, &first.id).unwrap());
    assert!(engine.get_memory(&alice, &first.id).unwrap().is_none());
    assert!(engine.memory_history(&alice, &first.id).unwrap().is_empty());
    assert!(engine.recall(&alice, "obsolete", 5).unwrap().is_empty());
    assert!(engine.recall(&alice, "passphrase", 5).unwrap().is_empty());
    assert!(engine.profile(&alice, 5).unwrap().stable.is_empty());
    let replacement = engine.remember(&alice, original).unwrap();
    assert_ne!(replacement.id, first.id);
    assert_eq!(replacement.revision, 1);
    assert_eq!(
        engine.recall(&alice, "passphrase", 5).unwrap(),
        vec![replacement]
    );
    assert!(engine.memory_history(&alice, &first.id).unwrap().is_empty());
}

#[test]
fn every_operation_uses_the_exact_scope_tuple_without_none_wildcards() {
    let database = tempdir().unwrap();
    let mut engine = Engine::open(database.path().join("memory.db")).unwrap();
    let scopes = [
        scope("alice"),
        scope("bob"),
        MemoryScope {
            agent_id: Some("alpha".into()),
            ..scope("alice")
        },
        MemoryScope {
            agent_id: Some("beta".into()),
            ..scope("alice")
        },
        MemoryScope {
            project_id: Some("red".into()),
            ..scope("alice")
        },
        MemoryScope {
            agent_id: Some("alpha".into()),
            project_id: Some("red".into()),
            ..scope("alice")
        },
        MemoryScope {
            agent_id: Some("alpha".into()),
            project_id: Some("blue".into()),
            ..scope("alice")
        },
    ];
    let mut keyed = input("scoped shared evidence");
    keyed.key = Some("same-key".into());
    let records: Vec<_> = scopes
        .iter()
        .map(|scope| engine.remember(scope, keyed.clone()).unwrap())
        .collect();
    for (index, local) in scopes.iter().enumerate() {
        assert_eq!(
            engine.remember(local, keyed.clone()).unwrap(),
            records[index]
        );
        assert_eq!(
            engine.recall(local, "shared", 100).unwrap(),
            vec![records[index].clone()]
        );
        assert_eq!(
            engine.profile(local, 100).unwrap().stable,
            vec![records[index].clone()]
        );
        for (foreign_index, foreign) in records.iter().enumerate() {
            if index == foreign_index {
                continue;
            }
            assert_ne!(foreign.id, records[index].id);
            assert!(engine.get_memory(local, &foreign.id).unwrap().is_none());
            assert!(
                engine
                    .memory_history(local, &foreign.id)
                    .unwrap()
                    .is_empty()
            );
            assert!(
                engine
                    .update_memory(local, &foreign.id, keyed.clone())
                    .is_err()
            );
            assert!(!engine.forget_memory(local, &foreign.id).unwrap());
        }
        assert_eq!(
            engine.get_memory(local, &records[index].id).unwrap(),
            Some(records[index].clone())
        );
    }
    // Unkeyed deduplication is scoped too, not a global content hash.
    let mut ids = Vec::new();
    for local in &scopes {
        let first = engine
            .remember(local, input("unkeyed shared evidence"))
            .unwrap();
        assert_eq!(
            engine
                .remember(local, input("  unkeyed shared evidence  "))
                .unwrap(),
            first
        );
        assert!(!ids.contains(&first.id));
        ids.push(first.id);
    }
}

#[test]
fn assistants_can_share_the_same_user_with_default_global_agent_scope() {
    let database = tempdir().unwrap();
    let path = database.path().join("memory.db");
    let mut first_assistant = Engine::open(&path).unwrap();
    let mut second_assistant = Engine::open(&path).unwrap();
    let scope: MemoryScope = serde_json::from_value(json!({"user_id": "alice"})).unwrap();
    assert_eq!(scope.agent_id, None);
    assert_eq!(scope.project_id, None);
    let first = first_assistant
        .remember(&scope, input("Alice prefers concise replies"))
        .unwrap();
    assert_eq!(
        second_assistant.get_memory(&scope, &first.id).unwrap(),
        Some(first.clone())
    );
    assert_eq!(
        second_assistant
            .remember(&scope, input("Alice prefers concise replies"))
            .unwrap(),
        first
    );
}

#[test]
fn unkeyed_dedup_compares_the_entire_normalized_input() {
    let database = tempdir().unwrap();
    let mut engine = Engine::open(database.path().join("memory.db")).unwrap();
    let scope = scope("alice");
    let base = MemoryInput {
        content: "Alice prefers metric units".into(),
        kind: MemoryKind::Preference,
        key: None,
        metadata: json!({"b": [1, {"nested": true}], "a": "SI"}),
        source: Some("conversation".into()),
        expires_at_ms: Some(u64::MAX),
    };
    let first = engine.remember(&scope, base.clone()).unwrap();
    let mut reordered = base.clone();
    reordered.content = " \nAlice prefers metric units\t ".into();
    reordered.metadata = serde_json::from_str(r#"{"a":"SI","b":[1,{"nested":true}]}"#).unwrap();
    assert_eq!(engine.remember(&scope, reordered).unwrap(), first);
    let variants = [
        MemoryInput {
            content: "Alice prefers imperial units".into(),
            ..base.clone()
        },
        MemoryInput {
            kind: MemoryKind::Fact,
            ..base.clone()
        },
        MemoryInput {
            metadata: json!({"a": "SI", "b": [2]}),
            ..base.clone()
        },
        MemoryInput {
            source: None,
            ..base.clone()
        },
        MemoryInput {
            expires_at_ms: None,
            ..base.clone()
        },
        MemoryInput {
            key: Some("units".into()),
            ..base.clone()
        },
    ];
    let mut ids = vec![first.id.clone()];
    for variant in variants {
        let record = engine.remember(&scope, variant.clone()).unwrap();
        assert!(!ids.contains(&record.id));
        assert_eq!(engine.remember(&scope, variant).unwrap(), record);
        assert_eq!(
            engine.memory_history(&scope, &record.id).unwrap(),
            vec![record.clone()]
        );
        ids.push(record.id);
    }
    assert_eq!(engine.get_memory(&scope, &first.id).unwrap(), Some(first));
}

#[test]
fn explicit_updates_replace_fields_but_key_conflicts_roll_back_everything() {
    let database = tempdir().unwrap();
    let path = database.path().join("memory.db");
    let mut engine = Engine::open(&path).unwrap();
    let scope = scope("alice");
    let first = engine
        .remember(
            &scope,
            MemoryInput {
                key: Some("first".into()),
                ..input("amber preference")
            },
        )
        .unwrap();
    let second = engine
        .remember(
            &scope,
            MemoryInput {
                key: Some("second".into()),
                ..input("violet preference")
            },
        )
        .unwrap();
    let conflicting = MemoryInput {
        key: second.key.clone(),
        ..input("corrupt overwrite")
    };
    assert!(
        engine
            .update_memory(&scope, &first.id, conflicting)
            .is_err()
    );
    assert_eq!(
        engine.get_memory(&scope, &first.id).unwrap(),
        Some(first.clone())
    );
    assert_eq!(
        engine.get_memory(&scope, &second.id).unwrap(),
        Some(second.clone())
    );
    assert_eq!(
        engine.memory_history(&scope, &first.id).unwrap(),
        vec![first.clone()]
    );
    assert_eq!(
        engine.memory_history(&scope, &second.id).unwrap(),
        vec![second.clone()]
    );
    assert!(engine.recall(&scope, "corrupt", 5).unwrap().is_empty());
    assert_eq!(
        engine.recall(&scope, "amber", 5).unwrap(),
        vec![first.clone()]
    );
    assert_eq!(
        engine.recall(&scope, "violet", 5).unwrap(),
        vec![second.clone()]
    );
    let changed = MemoryInput {
        content: "  resolved preference  ".into(),
        kind: MemoryKind::IssueSolution,
        key: Some("renamed".into()),
        metadata: json!({"ticket": 7}),
        source: Some("issue tracker".into()),
        expires_at_ms: Some(u64::MAX),
    };
    let updated = engine
        .update_memory(&scope, &first.id, changed.clone())
        .unwrap();
    assert_eq!(updated.id, first.id);
    assert_eq!(updated.revision, 2);
    assert_eq!(updated.created_at_ms, first.created_at_ms);
    assert!(updated.updated_at_ms > first.updated_at_ms);
    assert_eq!(updated.content, "resolved preference");
    assert_eq!(updated.kind, MemoryKind::IssueSolution);
    assert_eq!(updated.key.as_deref(), Some("renamed"));
    assert_eq!(updated.metadata, json!({"ticket": 7}));
    assert_eq!(updated.source.as_deref(), Some("issue tracker"));
    assert_eq!(updated.expires_at_ms, Some(u64::MAX));
    assert_eq!(
        engine.update_memory(&scope, &updated.id, changed).unwrap(),
        updated
    );
    assert_eq!(
        engine.memory_history(&scope, &updated.id).unwrap(),
        vec![first.clone(), updated.clone()]
    );
    assert!(engine.recall(&scope, "amber", 5).unwrap().is_empty());
    assert_eq!(
        engine.recall(&scope, "resolved", 5).unwrap(),
        vec![updated.clone()]
    );
    drop(engine);
    let engine = Engine::open(&path).unwrap();
    assert_eq!(
        engine.get_memory(&scope, &updated.id).unwrap(),
        Some(updated.clone())
    );
    assert_eq!(engine.get_memory(&scope, &second.id).unwrap(), Some(second));
    assert_eq!(
        engine.memory_history(&scope, &updated.id).unwrap(),
        vec![first, updated]
    );
}

#[test]
fn expired_keyed_memories_keep_history_and_identity_and_can_be_revived() {
    let database = tempdir().unwrap();
    let mut engine = Engine::open(database.path().join("memory.db")).unwrap();
    let scope = scope("alice");
    let mut expired = MemoryInput {
        key: Some("plan".into()),
        kind: MemoryKind::Plan,
        expires_at_ms: Some(0),
        ..input("visit Kyoto")
    };
    let first = engine.remember(&scope, expired.clone()).unwrap();
    assert!(engine.get_memory(&scope, &first.id).unwrap().is_none());
    assert!(engine.recall(&scope, "Kyoto", 5).unwrap().is_empty());
    assert_eq!(engine.profile(&scope, 5).unwrap(), MemoryProfile::default());
    assert_eq!(
        engine.memory_history(&scope, &first.id).unwrap(),
        vec![first.clone()]
    );
    assert_eq!(engine.remember(&scope, expired.clone()).unwrap(), first);
    expired.expires_at_ms = None;
    let active = engine.remember(&scope, expired.clone()).unwrap();
    assert_eq!(active.id, first.id);
    assert_eq!(active.revision, 2);
    assert_eq!(
        engine.get_memory(&scope, &first.id).unwrap(),
        Some(active.clone())
    );
    assert_eq!(
        engine.recall(&scope, "Kyoto", 5).unwrap(),
        vec![active.clone()]
    );
    assert_eq!(
        engine.profile(&scope, 5).unwrap().recent,
        vec![active.clone()]
    );
    expired.expires_at_ms = Some(0);
    let expired_again = engine.update_memory(&scope, &first.id, expired).unwrap();
    assert!(engine.get_memory(&scope, &first.id).unwrap().is_none());
    assert!(engine.recall(&scope, "Kyoto", 5).unwrap().is_empty());
    assert_eq!(engine.profile(&scope, 5).unwrap(), MemoryProfile::default());
    assert_eq!(
        engine.memory_history(&scope, &first.id).unwrap(),
        vec![first.clone(), active, expired_again]
    );
    assert!(engine.forget_memory(&scope, &first.id).unwrap());
    assert!(engine.memory_history(&scope, &first.id).unwrap().is_empty());
}

#[test]
fn expired_unkeyed_duplicates_remain_expired_until_the_input_changes() {
    let database = tempdir().unwrap();
    let mut engine = Engine::open(database.path().join("memory.db")).unwrap();
    let scope = scope("alice");
    let expired = MemoryInput {
        expires_at_ms: Some(0),
        ..input("old evidence")
    };
    let old = engine.remember(&scope, expired.clone()).unwrap();
    assert_eq!(engine.remember(&scope, expired).unwrap(), old);
    assert!(engine.get_memory(&scope, &old.id).unwrap().is_none());
    let active = engine.remember(&scope, input("old evidence")).unwrap();
    assert_ne!(old.id, active.id);
    assert_eq!(engine.recall(&scope, "evidence", 5).unwrap(), vec![active]);
    assert_eq!(engine.memory_history(&scope, &old.id).unwrap(), vec![old]);
}

#[test]
fn profile_groups_all_kinds_and_serde_uses_strict_snake_case_inputs() {
    let database = tempdir().unwrap();
    let mut engine = Engine::open(database.path().join("memory.db")).unwrap();
    let scope = scope("alice");
    let kinds = [
        (MemoryKind::Fact, "fact"),
        (MemoryKind::Preference, "preference"),
        (MemoryKind::Decision, "decision"),
        (MemoryKind::ArchitecturalDecision, "architectural_decision"),
        (MemoryKind::Plan, "plan"),
        (MemoryKind::Relationship, "relationship"),
        (MemoryKind::IssueSolution, "issue_solution"),
    ];
    let mut records = Vec::new();
    for (kind, name) in kinds {
        assert_eq!(serde_json::to_value(kind).unwrap(), json!(name));
        assert_eq!(
            serde_json::from_value::<MemoryKind>(json!(name)).unwrap(),
            kind
        );
        records.push(
            engine
                .remember(
                    &scope,
                    MemoryInput {
                        kind,
                        ..input(name)
                    },
                )
                .unwrap(),
        );
    }
    let profile = engine.profile(&scope, 100).unwrap();
    assert_eq!(
        profile.stable,
        records[..4].iter().rev().cloned().collect::<Vec<_>>()
    );
    assert_eq!(
        profile.recent,
        records[4..].iter().rev().cloned().collect::<Vec<_>>()
    );
    assert_eq!(
        serde_json::from_value::<MemoryProfile>(serde_json::to_value(&profile).unwrap()).unwrap(),
        profile
    );
    assert_eq!(
        serde_json::from_value::<MemoryRecord>(serde_json::to_value(&records[0]).unwrap()).unwrap(),
        records[0]
    );
    assert!(serde_json::from_value::<MemoryInput>(json!({"content": "fact", "typo": 1})).is_err());
    assert!(
        serde_json::from_value::<MemoryScope>(json!({"user_id": "alice", "tenant": "other"}))
            .is_err()
    );
    assert!(serde_json::from_value::<MemoryKind>(json!("ArchitecturalDecision")).is_err());
    assert!(
        serde_json::from_value::<MemoryInput>(json!({"content": "fact", "kind": "unknown"}))
            .is_err()
    );
    let defaults = input("fact");
    assert_eq!(defaults.kind, MemoryKind::Fact);
    assert_eq!(defaults.metadata, json!({}));
    assert_eq!(defaults.key, None);
    assert_eq!(defaults.source, None);
    assert_eq!(defaults.expires_at_ms, None);
}

#[test]
fn recall_is_literal_lexical_and_search_of_content_not_metadata() {
    let database = tempdir().unwrap();
    let mut engine = Engine::open(database.path().join("memory.db")).unwrap();
    let scope = scope("alice");
    let both = engine.remember(&scope, input("zephyr quartz")).unwrap();
    let operator = engine.remember(&scope, input("zephyr OR quartz")).unwrap();
    engine.remember(&scope, input("zephyr alone")).unwrap();
    engine.remember(&scope, input("quartz alone")).unwrap();
    engine
        .remember(
            &scope,
            MemoryInput {
                key: Some("zephyr quartz".into()),
                metadata: json!({"note": "zephyr quartz"}),
                source: Some("zephyr quartz".into()),
                ..input("unrelated content")
            },
        )
        .unwrap();
    let hits = engine.recall(&scope, "zephyr-quartz", 100).unwrap();
    assert_eq!(hits.len(), 2);
    assert!(hits.contains(&both));
    assert!(hits.contains(&operator));
    assert_eq!(
        engine.recall(&scope, "zephyr OR quartz", 100).unwrap(),
        vec![operator]
    );
    assert!(engine.recall(&scope, "\"()**\"", 100).unwrap().is_empty());
    assert!(engine.recall(&scope, "", 100).unwrap().is_empty());
    assert!(
        engine
            .recall(&scope, "zephyr NOT quartz", 100)
            .unwrap()
            .is_empty()
    );
    assert!(engine.recall(&scope, &"x".repeat(4097), 100).is_err());
    assert!(
        engine
            .recall(&scope, &vec!["zephyr"; 33].join(" "), 100)
            .is_err()
    );
    assert!(
        !engine
            .recall(&scope, &vec!["zephyr"; 32].join(" "), 100)
            .unwrap()
            .is_empty()
    );
    assert!(
        engine
            .recall(&scope, &"!".repeat(4096), 100)
            .unwrap()
            .is_empty()
    );
    assert!(engine.recall(&scope, "zephyr\0", 100).is_err());
}

#[test]
fn recall_ranks_bm25_before_newest_with_deterministic_recency_ties() {
    let database = tempdir().unwrap();
    let mut engine = Engine::open(database.path().join("memory.db")).unwrap();
    let scope = scope("alice");
    let strongest = engine
        .remember(&scope, input("needle needle needle filler"))
        .unwrap();
    let older = engine
        .remember(&scope, input("needle filler filler filler"))
        .unwrap();
    let newest = engine
        .remember(&scope, input("needle filler filler again"))
        .unwrap();
    assert_eq!(
        engine.recall(&scope, "needle", 100).unwrap(),
        vec![strongest, newest, older]
    );
}

#[test]
fn budgets_count_complete_serialized_arrays_and_profile_envelopes() {
    let database = tempdir().unwrap();
    let mut engine = Engine::open(database.path().join("memory.db")).unwrap();
    let scope = scope("alice");
    let stable = engine
        .remember(
            &scope,
            MemoryInput {
                metadata: json!({"quote": "\"\\\n", "unicode": "京都"}),
                ..input("budget contains \"quotes\" and \\slashes")
            },
        )
        .unwrap();
    let array_bytes = serde_json::to_vec(&vec![stable.clone()]).unwrap().len();
    let options = |max_bytes| SearchOptions {
        limit: 100,
        max_bytes,
    };
    assert_eq!(
        engine
            .recall(&scope, "budget", options(array_bytes))
            .unwrap(),
        vec![stable.clone()]
    );
    assert!(
        engine
            .recall(&scope, "budget", options(array_bytes - 1))
            .unwrap()
            .is_empty()
    );
    let single_profile = MemoryProfile {
        stable: vec![stable.clone()],
        recent: Vec::new(),
    };
    let profile_bytes = serde_json::to_vec(&single_profile).unwrap().len();
    assert_eq!(
        engine.profile(&scope, options(profile_bytes)).unwrap(),
        single_profile
    );
    assert_eq!(
        engine.profile(&scope, options(profile_bytes - 1)).unwrap(),
        MemoryProfile::default()
    );
    let recent = engine
        .remember(
            &scope,
            MemoryInput {
                kind: MemoryKind::Plan,
                ..input("budget plan")
            },
        )
        .unwrap();
    let full = MemoryProfile {
        stable: vec![stable],
        recent: vec![recent],
    };
    let full_bytes = serde_json::to_vec(&full).unwrap().len();
    assert_eq!(engine.profile(&scope, options(full_bytes)).unwrap(), full);
    let reduced = engine.profile(&scope, options(full_bytes - 1)).unwrap();
    assert_eq!(reduced.stable.len() + reduced.recent.len(), 1);
    assert!(serde_json::to_vec(&reduced).unwrap().len() < full_bytes);
    for budget in [0, 1, 2, 24, 128, 512, 1024] {
        let recall = engine.recall(&scope, "budget", options(budget)).unwrap();
        assert!(recall.is_empty() || serde_json::to_vec(&recall).unwrap().len() <= budget);
        let profile = engine.profile(&scope, options(budget)).unwrap();
        assert!(
            profile == MemoryProfile::default()
                || serde_json::to_vec(&profile).unwrap().len() <= budget
        );
        if budget == 0 {
            assert!(recall.is_empty());
            assert_eq!(profile, MemoryProfile::default());
        }
    }
    let zero_limit = SearchOptions {
        limit: 0,
        max_bytes: usize::MAX,
    };
    assert!(
        engine
            .recall(&scope, "budget", zero_limit)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        engine.profile(&scope, zero_limit).unwrap(),
        MemoryProfile::default()
    );
    let one = engine.profile(&scope, 1).unwrap();
    assert_eq!(one.stable.len() + one.recent.len(), 1);
}

#[test]
fn oversized_candidates_do_not_hide_smaller_memories_that_fit_the_budget() {
    let database = tempdir().unwrap();
    let mut engine = Engine::open(database.path().join("memory.db")).unwrap();
    let scope = scope("alice");
    let small = engine.remember(&scope, input("budget")).unwrap();
    engine
        .remember(
            &scope,
            MemoryInput {
                kind: MemoryKind::Plan,
                ..input(&format!("budget {}", "x".repeat(8192)))
            },
        )
        .unwrap();
    let options = SearchOptions {
        limit: 100,
        max_bytes: 1024,
    };
    assert_eq!(
        engine.recall(&scope, "budget", options).unwrap(),
        vec![small.clone()]
    );
    assert_eq!(engine.profile(&scope, options).unwrap().stable, vec![small]);
    assert!(engine.profile(&scope, options).unwrap().recent.is_empty());
}

#[test]
fn limits_and_maximum_byte_budgets_are_capped_and_defaults_are_bounded() {
    let database = tempdir().unwrap();
    let mut engine = Engine::open(database.path().join("memory.db")).unwrap();
    let scope = scope("alice");
    for index in 0..110 {
        engine
            .remember(&scope, input(&format!("capacity entry {index}")))
            .unwrap();
    }
    let unbounded = SearchOptions {
        limit: usize::MAX,
        max_bytes: usize::MAX,
    };
    assert_eq!(
        engine.recall(&scope, "capacity", unbounded).unwrap().len(),
        100
    );
    let profile = engine.profile(&scope, unbounded).unwrap();
    assert_eq!(profile.stable.len() + profile.recent.len(), 100);
    let defaults = engine
        .recall(&scope, "capacity", SearchOptions::default())
        .unwrap();
    assert_eq!(defaults.len(), 5);
    assert!(serde_json::to_vec(&defaults).unwrap().len() <= 16_000);
    let profile = engine.profile(&scope, SearchOptions::default()).unwrap();
    assert_eq!(profile.stable.len() + profile.recent.len(), 5);
    assert!(serde_json::to_vec(&profile).unwrap().len() <= 16_000);
    let large_scope = MemoryScope {
        project_id: Some("large".into()),
        ..scope.clone()
    };
    for index in 0..20 {
        engine
            .remember(
                &large_scope,
                input(&format!("{} {index}", "capacity ".repeat(7200))),
            )
            .unwrap();
    }
    let recall = engine.recall(&large_scope, "capacity", unbounded).unwrap();
    assert!(!recall.is_empty());
    assert!(recall.len() < 20);
    assert!(serde_json::to_vec(&recall).unwrap().len() <= 1024 * 1024);
    let profile = engine.profile(&large_scope, unbounded).unwrap();
    assert!(!profile.stable.is_empty());
    assert!(profile.stable.len() < 20);
    assert!(serde_json::to_vec(&profile).unwrap().len() <= 1024 * 1024);
    assert!(
        engine
            .recall(&large_scope, "capacity", SearchOptions::default())
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        engine
            .profile(&large_scope, SearchOptions::default())
            .unwrap(),
        MemoryProfile::default()
    );
}

#[test]
fn malformed_inputs_cannot_change_records_history_or_search() {
    let database = tempdir().unwrap();
    let mut engine = Engine::open(database.path().join("memory.db")).unwrap();
    let scope = scope("alice");
    let original = engine
        .remember(&scope, input("valid original evidence"))
        .unwrap();
    let invalid = [
        json!({"content": ""}),
        json!({"content": " \n\t "}),
        json!({"content": "x".repeat(64 * 1024 + 1)}),
        json!({"content": "é".repeat(32 * 1024 + 1)}),
        json!({"content": "nul\0content"}),
        json!({"content": "bad", "key": " "}),
        json!({"content": "bad", "key": "x".repeat(257)}),
        json!({"content": "bad", "key": "nul\0key"}),
        json!({"content": "bad", "source": "x".repeat(4097)}),
        json!({"content": "bad", "source": "nul\0source"}),
        json!({"content": "bad", "metadata": null}),
        json!({"content": "bad", "metadata": []}),
        json!({"content": "bad", "metadata": {"blob": "x".repeat(16 * 1024)}}),
        json!({"content": "bad", "metadata": {"blob": "\"".repeat(9000)}}),
        json!({"content": "bad", "metadata": {"nested": [{"key": "nul\0value"}]}}),
        json!({"content": "bad", "metadata": {"nul\0key": "value"}}),
    ];
    for invalid in invalid {
        let input: MemoryInput = serde_json::from_value(invalid).unwrap();
        assert!(engine.remember(&scope, input.clone()).is_err());
        assert!(engine.update_memory(&scope, &original.id, input).is_err());
    }
    assert_eq!(
        engine.get_memory(&scope, &original.id).unwrap(),
        Some(original.clone())
    );
    assert_eq!(
        engine.memory_history(&scope, &original.id).unwrap(),
        vec![original.clone()]
    );
    assert_eq!(
        engine.recall(&scope, "evidence", 100).unwrap(),
        vec![original.clone()]
    );
    assert_eq!(engine.profile(&scope, 100).unwrap().stable, vec![original]);
}

#[test]
fn malformed_scopes_and_ids_are_rejected_by_every_operation() {
    let database = tempdir().unwrap();
    let mut engine = Engine::open(database.path().join("memory.db")).unwrap();
    let valid_scope = scope("alice");
    let original = engine
        .remember(&valid_scope, input("valid evidence"))
        .unwrap();
    let invalid_scopes = [
        scope(""),
        scope(" \n"),
        scope(&"x".repeat(257)),
        scope(&"é".repeat(129)),
        scope("nul\0user"),
        MemoryScope {
            agent_id: Some("".into()),
            ..valid_scope.clone()
        },
        MemoryScope {
            agent_id: Some(" \n".into()),
            ..valid_scope.clone()
        },
        MemoryScope {
            agent_id: Some("x".repeat(257)),
            ..valid_scope.clone()
        },
        MemoryScope {
            agent_id: Some("nul\0agent".into()),
            ..valid_scope.clone()
        },
        MemoryScope {
            project_id: Some("".into()),
            ..valid_scope.clone()
        },
        MemoryScope {
            project_id: Some(" \n".into()),
            ..valid_scope.clone()
        },
        MemoryScope {
            project_id: Some("x".repeat(257)),
            ..valid_scope.clone()
        },
        MemoryScope {
            project_id: Some("nul\0project".into()),
            ..valid_scope.clone()
        },
    ];
    let zero_budget = SearchOptions {
        limit: 0,
        max_bytes: 0,
    };
    for invalid in invalid_scopes {
        assert!(engine.remember(&invalid, input("bad evidence")).is_err());
        assert!(engine.get_memory(&invalid, &original.id).is_err());
        assert!(
            engine
                .update_memory(&invalid, &original.id, input("bad evidence"))
                .is_err()
        );
        assert!(engine.recall(&invalid, "", zero_budget).is_err());
        assert!(engine.profile(&invalid, zero_budget).is_err());
        assert!(engine.memory_history(&invalid, &original.id).is_err());
        assert!(engine.forget_memory(&invalid, &original.id).is_err());
    }
    for id in [
        "".to_owned(),
        " \n".into(),
        "x".repeat(257),
        "nul\0id".into(),
    ] {
        assert!(engine.get_memory(&valid_scope, &id).is_err());
        assert!(
            engine
                .update_memory(&valid_scope, &id, input("bad evidence"))
                .is_err()
        );
        assert!(engine.memory_history(&valid_scope, &id).is_err());
        assert!(engine.forget_memory(&valid_scope, &id).is_err());
    }
    assert!(engine.get_memory(&valid_scope, "absent").unwrap().is_none());
    assert!(
        engine
            .update_memory(&valid_scope, "absent", input("bad evidence"))
            .is_err()
    );
    assert!(
        engine
            .memory_history(&valid_scope, "absent")
            .unwrap()
            .is_empty()
    );
    assert!(!engine.forget_memory(&valid_scope, "absent").unwrap());
    assert_eq!(
        engine.get_memory(&valid_scope, &original.id).unwrap(),
        Some(original)
    );
}

#[test]
fn exact_byte_boundaries_and_full_u64_expiry_are_supported() {
    let database = tempdir().unwrap();
    let mut engine = Engine::open(database.path().join("memory.db")).unwrap();
    let scope = MemoryScope {
        user_id: "u".repeat(256),
        agent_id: Some("a".repeat(256)),
        project_id: Some("p".repeat(256)),
    };
    let input = MemoryInput {
        content: format!("  {}\n", "é".repeat(32 * 1024)),
        key: Some("k".repeat(256)),
        source: Some("s".repeat(4096)),
        metadata: json!({"blob": "x".repeat(16 * 1024 - 11)}),
        expires_at_ms: Some(u64::MAX),
        kind: MemoryKind::Fact,
    };
    let record = engine.remember(&scope, input.clone()).unwrap();
    assert_eq!(record.content.len(), 64 * 1024);
    assert_eq!(
        serde_json::to_vec(&record.metadata).unwrap().len(),
        16 * 1024
    );
    assert_eq!(record.expires_at_ms, Some(u64::MAX));
    assert_eq!(
        engine.get_memory(&scope, &record.id).unwrap(),
        Some(record.clone())
    );
    assert_eq!(engine.remember(&scope, input).unwrap(), record);
    assert_eq!(
        engine.memory_history(&scope, &record.id).unwrap(),
        vec![record]
    );
}

#[test]
fn keys_and_scope_ids_are_exact_not_trimmed_or_prefix_matched() {
    let database = tempdir().unwrap();
    let mut engine = Engine::open(database.path().join("memory.db")).unwrap();
    let normal_scope = scope("alice");
    let spaced_scope = scope(" alice ");
    let first = engine
        .remember(
            &normal_scope,
            MemoryInput {
                key: Some("key".into()),
                ..input("exact evidence")
            },
        )
        .unwrap();
    let spaced_key = engine
        .remember(
            &normal_scope,
            MemoryInput {
                key: Some(" key ".into()),
                ..input("exact evidence")
            },
        )
        .unwrap();
    let spaced_user = engine
        .remember(
            &spaced_scope,
            MemoryInput {
                key: Some("key".into()),
                ..input("exact evidence")
            },
        )
        .unwrap();
    assert_ne!(first.id, spaced_key.id);
    assert_ne!(first.id, spaced_user.id);
    assert_eq!(spaced_key.key.as_deref(), Some(" key "));
    assert_eq!(spaced_user.scope.user_id, " alice ");
    assert_eq!(engine.recall(&normal_scope, "exact", 100).unwrap().len(), 2);
    assert_eq!(
        engine.recall(&spaced_scope, "exact", 100).unwrap(),
        vec![spaced_user]
    );
}

#[test]
fn memories_and_history_survive_restart_and_forget_survives_another_restart() {
    let database = tempdir().unwrap();
    let path = database.path().join("memory.db");
    let scope = scope("alice");
    let (first, current, unkeyed) = {
        let mut engine = Engine::open(&path).unwrap();
        let first = engine
            .remember(
                &scope,
                MemoryInput {
                    key: Some("location".into()),
                    ..input("Alice lives in Osaka")
                },
            )
            .unwrap();
        let current = engine
            .remember(
                &scope,
                MemoryInput {
                    key: Some("location".into()),
                    ..input("Alice lives in Kyoto")
                },
            )
            .unwrap();
        let unkeyed = engine.remember(&scope, input("Alice prefers tea")).unwrap();
        (first, current, unkeyed)
    };
    {
        let mut engine = Engine::open(&path).unwrap();
        assert_eq!(
            engine.get_memory(&scope, &current.id).unwrap(),
            Some(current.clone())
        );
        assert_eq!(
            engine.memory_history(&scope, &current.id).unwrap(),
            vec![first, current.clone()]
        );
        assert_eq!(
            engine.remember(&scope, input("Alice prefers tea")).unwrap(),
            unkeyed
        );
        assert_eq!(
            engine.recall(&scope, "Kyoto", 5).unwrap(),
            vec![current.clone()]
        );
        assert!(engine.recall(&scope, "Osaka", 5).unwrap().is_empty());
        assert!(engine.forget_memory(&scope, &current.id).unwrap());
    }
    let engine = Engine::open(&path).unwrap();
    assert!(engine.get_memory(&scope, &current.id).unwrap().is_none());
    assert!(
        engine
            .memory_history(&scope, &current.id)
            .unwrap()
            .is_empty()
    );
    assert!(engine.recall(&scope, "Kyoto", 5).unwrap().is_empty());
    assert_eq!(
        engine.get_memory(&scope, &unkeyed.id).unwrap(),
        Some(unkeyed)
    );
}

#[test]
fn history_returns_the_latest_100_revisions_in_ascending_order() {
    let database = tempdir().unwrap();
    let mut engine = Engine::open(database.path().join("memory.db")).unwrap();
    let scope = scope("alice");
    let mut latest = None;
    for revision in 1..=105 {
        latest = Some(
            engine
                .remember(
                    &scope,
                    MemoryInput {
                        key: Some("versioned".into()),
                        ..input(&format!("revision {revision}"))
                    },
                )
                .unwrap(),
        );
    }
    let latest = latest.unwrap();
    let history = engine.memory_history(&scope, &latest.id).unwrap();
    assert_eq!(history.len(), 100);
    assert_eq!(history[0].revision, 6);
    assert_eq!(history[0].content, "revision 6");
    assert_eq!(history[99], latest);
    assert!(
        history
            .windows(2)
            .all(|pair| pair[1].revision == pair[0].revision + 1)
    );
    assert_eq!(engine.get_memory(&scope, &latest.id).unwrap(), Some(latest));
}

#[test]
fn concurrent_same_key_and_unkeyed_remembers_are_serialized_without_duplicates() {
    use std::sync::{Arc, Barrier};
    use std::thread;
    let database = tempdir().unwrap();
    let path = database.path().join("memory.db");
    for key in [None, Some("shared-key".to_owned())] {
        let engines: Vec<_> = (0..6).map(|_| Engine::open(&path).unwrap()).collect();
        let barrier = Arc::new(Barrier::new(engines.len()));
        let workers: Vec<_> = engines
            .into_iter()
            .map(|mut engine| {
                let barrier = barrier.clone();
                let input = MemoryInput {
                    key: key.clone(),
                    ..input("simultaneous shared evidence")
                };
                thread::spawn(move || {
                    barrier.wait();
                    engine.remember(&scope("alice"), input).unwrap()
                })
            })
            .collect();
        let records: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect();
        assert!(records.iter().all(|record| record == &records[0]));
        assert_eq!(records[0].revision, 1);
        let engine = Engine::open(&path).unwrap();
        assert_eq!(
            engine
                .memory_history(&scope("alice"), &records[0].id)
                .unwrap(),
            vec![records[0].clone()]
        );
    }
    let engine = Engine::open(&path).unwrap();
    assert_eq!(
        engine
            .recall(&scope("alice"), "simultaneous", 100)
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn concurrent_keyed_changes_keep_every_revision_under_one_id() {
    use std::sync::{Arc, Barrier};
    use std::thread;
    let database = tempdir().unwrap();
    let path = database.path().join("memory.db");
    let engines: Vec<_> = (0..6).map(|_| Engine::open(&path).unwrap()).collect();
    let barrier = Arc::new(Barrier::new(engines.len()));
    let workers: Vec<_> = engines
        .into_iter()
        .enumerate()
        .map(|(index, mut engine)| {
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                engine
                    .remember(
                        &scope("alice"),
                        MemoryInput {
                            key: Some("shared-key".into()),
                            ..input(&format!("concurrent state {index}"))
                        },
                    )
                    .unwrap()
            })
        })
        .collect();
    let mut records: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    records.sort_by_key(|record| record.revision);
    assert!(records.iter().all(|record| record.id == records[0].id));
    assert_eq!(
        records
            .iter()
            .map(|record| record.revision)
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5, 6]
    );
    let engine = Engine::open(&path).unwrap();
    assert_eq!(
        engine
            .memory_history(&scope("alice"), &records[0].id)
            .unwrap(),
        records
    );
    assert_eq!(
        engine.get_memory(&scope("alice"), &records[0].id).unwrap(),
        Some(records[5].clone())
    );
}
