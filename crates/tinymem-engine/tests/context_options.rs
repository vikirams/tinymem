//! Request-local assembly filtering never changes repository search or memory freshness.
use std::fs;
use tempfile::tempdir;
use tinymem_engine::{
    ContextOptions, ContextProvenance, ContextScope, ContextSource, Engine, FileSourceReference,
    FreshnessReason, MemoryKind, MemoryScope, NewMemory, SearchOptions, SourceRole,
};

fn memory_scope() -> MemoryScope {
    MemoryScope {
        user_id: "alice".into(),
        agent_id: Some("worker".into()),
        project_id: Some("app".into()),
    }
}

fn add_memory(
    engine: &mut Engine,
    scope: &MemoryScope,
    content: &str,
    source_refs: Vec<FileSourceReference>,
) -> String {
    engine
        .add_memory(
            scope,
            NewMemory {
                content: content.into(),
                kind: MemoryKind::Decision,
                source: "explicit test evidence".into(),
                expires_at_ms: None,
                source_refs,
            },
        )
        .unwrap()
        .id
}

fn repo_scope(memory: MemoryScope, repository: &std::path::Path) -> ContextScope {
    ContextScope {
        memory,
        repository: Some(repository.to_path_buf()),
    }
}

#[test]
fn source_selection_and_exclusions_are_assembly_only_and_preserve_dependency_freshness() {
    let directory = tempdir().unwrap();
    let repository = directory.path().join("repo");
    fs::create_dir_all(repository.join("src")).unwrap();
    fs::write(
        repository.join("src/auth.rs"),
        "auth cookie policy uses secure defaults\n",
    )
    .unwrap();
    fs::write(
        repository.join("README.md"),
        "auth cookie notes for this repository\n",
    )
    .unwrap();

    let scope = memory_scope();
    let dependency =
        FileSourceReference::capture(&repository, "src/auth.rs", SourceRole::Dependency).unwrap();
    let mut engine = Engine::open(directory.path().join("native.db")).unwrap();
    let memory_id = add_memory(
        &mut engine,
        &scope,
        "auth cookie decision: secure defaults — café",
        vec![dependency],
    );
    let context_scope = repo_scope(scope.clone(), &repository);
    let selection = ContextOptions {
        sources: vec![ContextSource::Memory, ContextSource::Repository],
        exclude_paths: vec!["src/**".into()],
    };

    let assembled = engine
        .assemble_context_with_options("auth cookie", &context_scope, 8000, &selection)
        .unwrap();
    assert_eq!(
        assembled.entries.len(),
        2,
        "memory evidence is not path-filtered"
    );
    assert!(assembled.entries.iter().all(|entry| {
        entry.provenance.iter().all(|provenance| match provenance {
            ContextProvenance::Repository { path, .. } => path != "src/auth.rs",
            ContextProvenance::Memory {
                id,
                source_refs,
                scope: actual_scope,
                version,
                ..
            } => {
                id == &memory_id
                    && source_refs.len() == 1
                    && actual_scope == &scope
                    && *version == 1
            }
        })
    }));
    assert!(assembled.entries.iter().any(|entry| entry.content.contains("README.md") ||
        entry.provenance.iter().any(|p| matches!(p, ContextProvenance::Repository { path, .. } if path == "README.md"))));
    assert_eq!(assembled.tokens_used, assembled.prompt.len());
    assert!(
        assembled.prompt.chars().count() < assembled.prompt.len(),
        "Unicode is budgeted by UTF-8 bytes"
    );
    assert!(assembled.tokens_used <= assembled.token_budget);
    assert_eq!(
        engine
            .assemble_context_with_options(
                "auth cookie",
                &context_scope,
                assembled.tokens_used,
                &selection,
            )
            .unwrap()
            .prompt,
        assembled.prompt,
        "the whole prompt fits at its exact byte count"
    );

    // A source exclusion affects only automatic assembly; explicit repository search is untouched.
    let explicit = engine
        .search(
            &repository,
            "auth cookie",
            SearchOptions {
                limit: 100,
                max_bytes: 64_000,
            },
        )
        .unwrap();
    assert!(explicit.iter().any(|hit| hit.path == "src/auth.rs"));

    // Even though the dependency path is excluded as repository evidence, editing it makes its
    // saved-memory dependency stale; exclusions cannot bypass freshness validation.
    fs::write(
        repository.join("src/auth.rs"),
        "auth cookie policy changed\n",
    )
    .unwrap();
    let after_edit = engine
        .assemble_context_with_options("auth cookie", &context_scope, 8000, &selection)
        .unwrap();
    assert!(
        after_edit
            .entries
            .iter()
            .all(|entry| entry.content != "auth cookie decision: secure defaults — café")
    );
    let history = engine.memory_events(&scope, &memory_id, 5).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].freshness.reason, FreshnessReason::ContentChanged);
}

#[test]
fn exclusions_are_applied_before_the_repository_candidate_cap() {
    let directory = tempdir().unwrap();
    let repository = directory.path().join("repo");
    fs::create_dir_all(&repository).unwrap();
    // One bounded source file yields 801 indexed chunks, exceeding the existing 8x
    // repository overscan (100 candidates * 8). They sort ahead of the useful file.
    fs::write(repository.join("excluded.txt"), "needle\n".repeat(64_001)).unwrap();
    fs::write(repository.join("z-visible.rs"), "needle\n").unwrap();

    let mut engine = Engine::open(directory.path().join("native.db")).unwrap();
    let result = engine
        .assemble_context_with_options(
            "needle",
            &repo_scope(memory_scope(), &repository),
            16_000,
            &ContextOptions {
                sources: vec![ContextSource::Repository],
                exclude_paths: vec!["excluded.txt".into()],
            },
        )
        .unwrap();
    let repository_paths: Vec<_> = result
        .entries
        .iter()
        .flat_map(|entry| &entry.provenance)
        .filter_map(|provenance| match provenance {
            ContextProvenance::Repository { path, .. } => Some(path.as_str()),
            ContextProvenance::Memory { .. } => None,
        })
        .collect();
    assert_eq!(repository_paths, ["z-visible.rs"]);

    // Standalone search has no implicit assembly filters.
    let explicit = engine
        .search(
            &repository,
            "needle",
            SearchOptions {
                limit: 100,
                max_bytes: 64_000,
            },
        )
        .unwrap();
    assert!(explicit.iter().any(|hit| hit.path == "excluded.txt"));
}

#[test]
fn glob_exclusions_are_case_sensitive_and_star_does_not_cross_a_separator() {
    let directory = tempdir().unwrap();
    let repository = directory.path().join("repo");
    fs::create_dir_all(repository.join("src/nested")).unwrap();
    fs::write(repository.join("src/direct.rs"), "needle\n").unwrap();
    fs::write(repository.join("src/nested/deep.rs"), "needle\n").unwrap();
    fs::write(repository.join("other.rs"), "needle\n").unwrap();
    fs::write(repository.join("src/Case.rs"), "needle\n").unwrap();
    let mut engine = Engine::open(directory.path().join("native.db")).unwrap();
    let scope = repo_scope(memory_scope(), &repository);

    let single_star = engine
        .assemble_context_with_options(
            "needle",
            &scope,
            16_000,
            &ContextOptions {
                sources: vec![ContextSource::Repository],
                exclude_paths: vec!["src/*.rs".into()],
            },
        )
        .unwrap();
    let paths: Vec<_> = single_star
        .entries
        .iter()
        .flat_map(|entry| &entry.provenance)
        .filter_map(|source| match source {
            ContextProvenance::Repository { path, .. } => Some(path.as_str()),
            ContextProvenance::Memory { .. } => None,
        })
        .collect();
    assert_eq!(paths.len(), 2);
    assert!(paths.contains(&"other.rs"));
    assert!(
        paths.contains(&"src/nested/deep.rs"),
        "single star does not cross a separator"
    );

    let case_sensitive = engine
        .assemble_context_with_options(
            "needle",
            &scope,
            16_000,
            &ContextOptions {
                sources: vec![ContextSource::Repository],
                exclude_paths: vec!["src/case.rs".into()],
            },
        )
        .unwrap();
    assert!(case_sensitive
        .entries
        .iter()
        .flat_map(|entry| &entry.provenance)
        .any(|source| matches!(source, ContextProvenance::Repository { path, .. } if path == "src/Case.rs")),
        "glob matching is case-sensitive");

    let recursive = engine
        .assemble_context_with_options(
            "needle",
            &scope,
            16_000,
            &ContextOptions {
                sources: vec![ContextSource::Repository],
                exclude_paths: vec!["src/**".into()],
            },
        )
        .unwrap();
    let remaining: Vec<_> = recursive
        .entries
        .iter()
        .flat_map(|entry| &entry.provenance)
        .filter_map(|source| match source {
            ContextProvenance::Repository { path, .. } => Some(path.as_str()),
            ContextProvenance::Memory { .. } => None,
        })
        .collect();
    assert_eq!(remaining, ["other.rs"]);
}

#[test]
fn invalid_source_lists_and_unsafe_or_malformed_globs_are_rejected() {
    let directory = tempdir().unwrap();
    let mut engine = Engine::open(directory.path().join("native.db")).unwrap();
    let scope = ContextScope {
        memory: memory_scope(),
        repository: None,
    };
    let mut invalid = vec![
        ContextOptions {
            sources: vec![ContextSource::Memory, ContextSource::Memory],
            exclude_paths: Vec::new(),
        },
        ContextOptions {
            sources: Vec::new(),
            exclude_paths: vec!["/absolute".into()],
        },
        ContextOptions {
            sources: Vec::new(),
            exclude_paths: vec!["../escape".into()],
        },
        ContextOptions {
            sources: Vec::new(),
            exclude_paths: vec!["x/../y".into()],
        },
        ContextOptions {
            sources: Vec::new(),
            exclude_paths: vec!["src//file.rs".into()],
        },
        ContextOptions {
            sources: Vec::new(),
            exclude_paths: vec!["src/".into()],
        },
        ContextOptions {
            sources: Vec::new(),
            exclude_paths: vec!["src/./file.rs".into()],
        },
        ContextOptions {
            sources: Vec::new(),
            exclude_paths: vec!["src\\\\file.rs".into()],
        },
        ContextOptions {
            sources: Vec::new(),
            exclude_paths: vec!["C:/absolute".into()],
        },
        ContextOptions {
            sources: Vec::new(),
            exclude_paths: vec!["nul\0path".into()],
        },
        ContextOptions {
            sources: Vec::new(),
            exclude_paths: vec!["line\nbreak".into()],
        },
        ContextOptions {
            sources: Vec::new(),
            exclude_paths: vec!["é".repeat(513)],
        },
        ContextOptions {
            sources: Vec::new(),
            exclude_paths: vec!["[malformed".into()],
        },
        ContextOptions {
            sources: Vec::new(),
            exclude_paths: vec!["x".repeat(1024); 17],
        },
    ];
    invalid.push(ContextOptions {
        sources: Vec::new(),
        exclude_paths: vec!["x".into(); 65],
    });
    for options in invalid {
        assert!(
            engine
                .assemble_context_with_options("auth", &scope, 100, &options)
                .is_err()
        );
    }
}

#[test]
fn empty_source_selection_returns_no_evidence_without_mutating_memory_history() {
    let directory = tempdir().unwrap();
    let scope = memory_scope();
    let mut engine = Engine::open(directory.path().join("native.db")).unwrap();
    let id = add_memory(&mut engine, &scope, "auth choice", Vec::new());
    let history = engine.memory_events(&scope, &id, 5).unwrap();
    let result = engine
        .assemble_context_with_options(
            "auth",
            &ContextScope {
                memory: scope.clone(),
                repository: None,
            },
            200,
            &ContextOptions {
                sources: Vec::new(),
                exclude_paths: Vec::new(),
            },
        )
        .unwrap();
    assert!(result.entries.is_empty());
    assert_eq!(result.prompt, "");
    assert_eq!(result.tokens_used, 0);
    assert_eq!(engine.memory_events(&scope, &id, 5).unwrap(), history);
}
