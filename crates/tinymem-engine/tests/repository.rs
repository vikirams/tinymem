use std::fs;
use tempfile::tempdir;
use tinymem_engine::Engine;

#[test]
fn indexes_source_and_returns_grounded_context() {
    let repo = tempdir().unwrap();
    let database = tempdir().unwrap();
    fs::write(
        repo.path().join("auth.py"),
        "def authenticate(user):\n    return user\n",
    )
    .unwrap();
    let mut engine = Engine::open(database.path().join("context.db")).unwrap();
    let report = engine.refresh(repo.path()).unwrap();
    assert_eq!(report.indexed, 1);
    let hits = engine.search(repo.path(), "authenticate", 5).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, "auth.py");
    assert_eq!(hits[0].start_line, 1);
    assert!(hits[0].content.contains("def authenticate(user):"));
}

#[test]
fn refresh_only_reindexes_changed_files_and_removes_old_evidence() {
    let repo = tempdir().unwrap();
    let database = tempdir().unwrap();
    fs::write(
        repo.path().join("auth.py"),
        "def obsolete_login():\n    pass\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("stable.py"),
        "def stable_helper():\n    pass\n",
    )
    .unwrap();
    let mut engine = Engine::open(database.path().join("context.db")).unwrap();
    assert_eq!(engine.refresh(repo.path()).unwrap().indexed, 2);
    fs::write(
        repo.path().join("auth.py"),
        "def current_login():\n    pass\n",
    )
    .unwrap();
    assert_eq!(engine.refresh(repo.path()).unwrap().indexed, 1);
    assert!(
        engine
            .search(repo.path(), "obsolete_login", 5)
            .unwrap()
            .is_empty()
    );
    let hits = engine.search(repo.path(), "current_login", 5).unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].content.contains("current_login"));
    assert_eq!(engine.refresh(repo.path()).unwrap().indexed, 0);
}

#[test]
fn python_syntax_extraction_handles_async_nested_definitions_without_inventing_symbols() {
    let repo = tempdir().unwrap();
    let database = tempdir().unwrap();
    fs::write(repo.path().join("auth.py"),
        "# def fake():\nfrom .tokens import verify\nclass Auth:\n    async def authenticate(self):\n        def helper():\n            return verify()\n        return helper()\n").unwrap();
    let mut engine = Engine::open(database.path().join("context.db")).unwrap();
    engine.refresh(repo.path()).unwrap();
    let hits = engine.search(repo.path(), "authenticate", 5).unwrap();
    let symbols = &hits[0].symbols;
    assert!(
        symbols
            .iter()
            .any(|s| s.name == "authenticate" && s.start_line == 4 && s.end_line == 7)
    );
    assert!(
        symbols
            .iter()
            .any(|s| s.name == "Auth" && s.kind == "class")
    );
    assert!(
        symbols
            .iter()
            .any(|s| s.name == "helper" && s.kind == "function")
    );
    assert!(
        symbols
            .iter()
            .any(|s| s.kind == "import" && s.name.contains(".tokens"))
    );
    assert!(!symbols.iter().any(|s| s.name == "fake"));
    assert!(!hits[0].syntax_errors);
}

#[test]
fn search_never_returns_changed_or_deleted_source_as_current() {
    let repo = tempdir().unwrap();
    let database = tempdir().unwrap();
    fs::write(
        repo.path().join("auth.py"),
        "def old_authentication():\n    pass\n",
    )
    .unwrap();
    let mut engine = Engine::open(database.path().join("context.db")).unwrap();
    engine.refresh(repo.path()).unwrap();
    fs::write(
        repo.path().join("auth.py"),
        "def new_authentication():\n    pass\n",
    )
    .unwrap();
    assert!(
        engine
            .search(repo.path(), "old_authentication", 5)
            .unwrap()
            .is_empty()
    );
    engine.refresh(repo.path()).unwrap();
    assert_eq!(
        engine
            .search(repo.path(), "new_authentication", 5)
            .unwrap()
            .len(),
        1
    );
    fs::remove_file(repo.path().join("auth.py")).unwrap();
    assert!(
        engine
            .search(repo.path(), "new_authentication", 5)
            .unwrap()
            .is_empty()
    );
    engine.refresh(repo.path()).unwrap();
    assert!(
        engine
            .search(repo.path(), "new_authentication", 5)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn isolates_repositories_with_the_same_relative_paths() {
    let alpha = tempdir().unwrap();
    let beta = tempdir().unwrap();
    let database = tempdir().unwrap();
    fs::write(
        alpha.path().join("auth.py"),
        "def shared():\n    return 'alpha'\n",
    )
    .unwrap();
    fs::write(
        beta.path().join("auth.py"),
        "def shared():\n    return 'beta'\n",
    )
    .unwrap();
    let mut engine = Engine::open(database.path().join("context.db")).unwrap();
    engine.refresh(alpha.path()).unwrap();
    engine.refresh(beta.path()).unwrap();
    let alpha_hits = engine.search(alpha.path(), "shared", 5).unwrap();
    let beta_hits = engine.search(beta.path(), "shared", 5).unwrap();
    assert_eq!(alpha_hits.len(), 1);
    assert_eq!(beta_hits.len(), 1);
    assert!(alpha_hits[0].content.contains("'alpha'"));
    assert!(beta_hits[0].content.contains("'beta'"));
    assert!(engine.search(alpha.path(), "beta", 5).unwrap().is_empty());
}

#[test]
fn reconciles_renames_deletions_and_new_ignore_rules() {
    let repo = tempdir().unwrap();
    let database = tempdir().unwrap();
    fs::write(repo.path().join("old.md"), "migration guide\n").unwrap();
    fs::write(repo.path().join("removed.py"), "def removed(): pass\n").unwrap();
    let mut engine = Engine::open(database.path().join("context.db")).unwrap();
    engine.refresh(repo.path()).unwrap();
    fs::rename(repo.path().join("old.md"), repo.path().join("new.md")).unwrap();
    fs::remove_file(repo.path().join("removed.py")).unwrap();
    let report = engine.refresh(repo.path()).unwrap();
    assert_eq!(report.indexed, 1);
    assert_eq!(report.removed, 2);
    let hits = engine.search(repo.path(), "migration", 5).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, "new.md");
    fs::write(repo.path().join(".tinymemignore"), "new.md\n").unwrap();
    assert_eq!(engine.refresh(repo.path()).unwrap().removed, 1);
    assert!(
        engine
            .search(repo.path(), "migration", 5)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn ignores_credentials_generated_files_binary_large_files_and_gitignored_source() {
    let repo = tempdir().unwrap();
    let database = tempdir().unwrap();
    fs::create_dir(repo.path().join("node_modules")).unwrap();
    fs::write(repo.path().join(".gitignore"), "ignored.py\n").unwrap();
    for path in [
        ".env",
        "secrets.json",
        "credentials.json",
        "node_modules/generated.js",
        "ignored.py",
    ] {
        fs::write(repo.path().join(path), "needle confidential\n").unwrap();
    }
    fs::write(repo.path().join("binary.py"), b"needle\0confidential").unwrap();
    fs::write(repo.path().join("large.py"), "needle".repeat(200_000)).unwrap();
    fs::write(repo.path().join("public.py"), "def needle(): pass\n").unwrap();
    let mut engine = Engine::open(database.path().join("context.db")).unwrap();
    let report = engine.refresh(repo.path()).unwrap();
    assert_eq!(report.indexed, 1);
    let hits = engine.search(repo.path(), "needle", 100).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, "public.py");
}

#[test]
fn enforces_serialized_context_budget_and_literal_query_handling() {
    use tinymem_engine::SearchOptions;
    let repo = tempdir().unwrap();
    let database = tempdir().unwrap();
    for number in 0..10 {
        fs::write(
            repo.path().join(format!("{number}.md")),
            "needle context\n".repeat(10),
        )
        .unwrap();
    }
    let mut engine = Engine::open(database.path().join("context.db")).unwrap();
    engine.refresh(repo.path()).unwrap();
    let hits = engine
        .search(
            repo.path(),
            "needle",
            SearchOptions {
                limit: 100,
                max_bytes: 1000,
            },
        )
        .unwrap();
    assert!(!hits.is_empty());
    assert!(serde_json::to_vec(&hits).unwrap().len() <= 1000);
    assert!(
        engine
            .search(repo.path(), "\"()**\"", 5)
            .unwrap()
            .is_empty()
    );
    assert!(
        engine
            .search(
                repo.path(),
                "needle",
                SearchOptions {
                    limit: 5,
                    max_bytes: 0
                }
            )
            .unwrap()
            .is_empty()
    );
}

#[test]
fn survives_restart_and_labels_invalid_syntax_without_discarding_source() {
    let repo = tempdir().unwrap();
    let database = tempdir().unwrap();
    let db = database.path().join("context.db");
    fs::write(repo.path().join("broken.py"), "def broken(:\n    pass\n").unwrap();
    {
        let mut engine = Engine::open(&db).unwrap();
        engine.refresh(repo.path()).unwrap();
    }
    let engine = Engine::open(&db).unwrap();
    let hits = engine.search(repo.path(), "broken", 5).unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].syntax_errors);
    assert!(hits[0].content.contains("def broken(:"));
}

#[cfg(unix)]
#[test]
fn never_follows_symlinks_or_returns_a_replaced_symlink_as_evidence() {
    use std::os::unix::fs::symlink;
    let repo = tempdir().unwrap();
    let external = tempdir().unwrap();
    let database = tempdir().unwrap();
    fs::write(
        external.path().join("outside.py"),
        "def confidential(): pass\n",
    )
    .unwrap();
    symlink(
        external.path().join("outside.py"),
        repo.path().join("link.py"),
    )
    .unwrap();
    fs::write(repo.path().join("auth.py"), "def confidential(): pass\n").unwrap();
    let mut engine = Engine::open(database.path().join("context.db")).unwrap();
    assert_eq!(engine.refresh(repo.path()).unwrap().indexed, 1);
    fs::remove_file(repo.path().join("auth.py")).unwrap();
    symlink(
        external.path().join("outside.py"),
        repo.path().join("auth.py"),
    )
    .unwrap();
    assert!(
        engine
            .search(repo.path(), "confidential", 5)
            .unwrap()
            .is_empty()
    );
    assert_eq!(engine.refresh(repo.path()).unwrap().removed, 1);
}

#[test]
fn refuses_legacy_or_unknown_databases_without_changing_their_contents() {
    let database = tempdir().unwrap();
    let db = database.path().join("legacy.db");
    // Construct a foreign database as external input; don't inspect engine internals.
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE memories(content TEXT); INSERT INTO memories VALUES ('retain me');",
        )
        .unwrap();
    }
    let before = fs::read(&db).unwrap();
    assert!(Engine::open(&db).is_err());
    assert_eq!(fs::read(&db).unwrap(), before);
}

#[test]
fn failed_refresh_preserves_previous_snapshot_and_does_not_ignore_invalid_exclusions() {
    let repo = tempdir().unwrap();
    let database = tempdir().unwrap();
    fs::write(repo.path().join("auth.py"), "def authenticate(): pass\n").unwrap();
    let mut engine = Engine::open(database.path().join("context.db")).unwrap();
    engine.refresh(repo.path()).unwrap();
    fs::write(repo.path().join("new.py"), "def new_symbol(): pass\n").unwrap();
    fs::write(repo.path().join(".tinymemignore"), "[z-a]\n").unwrap();
    assert!(engine.refresh(repo.path()).is_err());
    assert_eq!(
        engine.search(repo.path(), "authenticate", 5).unwrap().len(),
        1
    );
    assert!(
        engine
            .search(repo.path(), "new_symbol", 5)
            .unwrap()
            .is_empty()
    );
    fs::remove_file(repo.path().join(".tinymemignore")).unwrap();
    let report = engine.refresh(repo.path()).unwrap();
    assert_eq!(report.indexed, 1);
    assert_eq!(report.unchanged, 1);
}

#[test]
fn hidden_files_and_directories_cannot_be_whitelisted_into_the_index() {
    let repo = tempdir().unwrap();
    let database = tempdir().unwrap();
    fs::create_dir(repo.path().join(".private")).unwrap();
    fs::write(repo.path().join(".private.json"), "needle confidential\n").unwrap();
    fs::write(
        repo.path().join(".private/source.py"),
        "def needle(): pass\n",
    )
    .unwrap();
    fs::write(
        repo.path().join(".gitignore"),
        "!.private.json\n!.private/\n!.private/source.py\n",
    )
    .unwrap();
    let mut engine = Engine::open(database.path().join("context.db")).unwrap();
    assert_eq!(engine.refresh(repo.path()).unwrap().indexed, 0);
    assert!(engine.search(repo.path(), "needle", 5).unwrap().is_empty());
}

#[test]
fn refuses_view_only_foreign_database_without_modifying_it() {
    let database = tempdir().unwrap();
    let db = database.path().join("views.db");
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch("CREATE VIEW foreign_view AS SELECT 42 AS value;")
            .unwrap();
    }
    let before = fs::read(&db).unwrap();
    assert!(Engine::open(&db).is_err());
    assert_eq!(fs::read(&db).unwrap(), before);
}

#[cfg(unix)]
#[test]
fn preserves_literal_backslashes_in_unix_source_names() {
    let repo = tempdir().unwrap();
    let database = tempdir().unwrap();
    fs::create_dir(repo.path().join("a")).unwrap();
    fs::write(
        repo.path().join("a\\b.py"),
        "def literal_backslash(): pass\n",
    )
    .unwrap();
    fs::write(repo.path().join("a/b.py"), "def nested_source(): pass\n").unwrap();
    let mut engine = Engine::open(database.path().join("context.db")).unwrap();
    assert_eq!(engine.refresh(repo.path()).unwrap().indexed, 2);
    let hits = engine.search(repo.path(), "literal_backslash", 5).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, "a\\b.py");
    assert_eq!(
        engine.search(repo.path(), "nested_source", 5).unwrap()[0].path,
        "a/b.py"
    );
    assert_eq!(engine.refresh(repo.path()).unwrap().indexed, 0);
}

#[cfg(unix)]
#[test]
fn unreadable_ignore_file_fails_closed_and_preserves_the_index() {
    use std::os::unix::fs::PermissionsExt;
    let repo = tempdir().unwrap();
    let database = tempdir().unwrap();
    let ignore = repo.path().join(".tinymemignore");
    fs::write(&ignore, "private.json\n").unwrap();
    fs::write(repo.path().join("public.py"), "def public_symbol(): pass\n").unwrap();
    fs::write(repo.path().join("private.json"), "confidential\n").unwrap();
    let mut engine = Engine::open(database.path().join("context.db")).unwrap();
    assert_eq!(engine.refresh(repo.path()).unwrap().indexed, 1);
    let original_permissions = fs::metadata(&ignore).unwrap().permissions();
    fs::set_permissions(&ignore, fs::Permissions::from_mode(0o0)).unwrap();
    if fs::read(&ignore).is_ok() {
        // Privileged test users can bypass mode bits; this environment cannot exercise the failure.
        fs::set_permissions(&ignore, original_permissions).unwrap();
        return;
    }
    let result = engine.refresh(repo.path());
    fs::set_permissions(&ignore, original_permissions).unwrap();
    assert!(result.is_err());
    assert_eq!(
        engine
            .search(repo.path(), "public_symbol", 5)
            .unwrap()
            .len(),
        1
    );
    assert!(
        engine
            .search(repo.path(), "confidential", 5)
            .unwrap()
            .is_empty()
    );
}
