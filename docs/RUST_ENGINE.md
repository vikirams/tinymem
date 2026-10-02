# Rust repository engine

The experimental native engine provides repository context and scoped personal memory alongside the
separate legacy Python library. This guide covers repository behavior; see
[native memory and MCP setup](NATIVE_MEMORY.md) for CRUD, audit history, profiles and SDKs.
This is not a replacement for every Python feature or a completed mem0/Supermemory equivalent.

## Scope and public interface

The implemented workflow is **index → edit → refresh → retrieve current evidence**. The repository public library seam is `Engine::open`, `Engine::refresh`, and `Engine::search`; the command-line adapter exposes the same workflow as JSON. Tests exercise these interfaces against real temporary repositories and SQLite databases.

- SQLite with bundled FTS5; no Python runtime, embeddings, LLM, API keys, or outbound requests at runtime.
- Canonical repository/worktree paths isolate files, chunks and symbols, including identical relative paths in different repositories. Moving a repository creates a new namespace; relocation and garbage collection of old namespaces are not implemented.
- BLAKE3 source hashes avoid rewriting or reparsing unchanged files. Reconciliation removes deleted, renamed-away, newly ignored, binary and oversized source from the current index. Renames are represented as delete/add, not inferred historical relationships.
- Tree-sitter extracts Python function/class declarations (including async and nested definitions) and import statements. Imports remain **unresolved**; no inferred call graph or cross-file dependency resolution is claimed. Other supported file types receive lexical source indexing only.
- Python syntax errors are reported on each hit; recovery can produce partial symbols. Empty symbols in other languages do not establish that a file has no declarations.
- Results include relative path, 1-based inclusive source line range, file content hash, language, source snippet, overlapping declarations, syntax-error flag and per-file indexing timestamp.
- Search checks candidate source hashes and withholds changed, deleted, unreadable or symlink-replaced evidence, even before another refresh. Low-level library callers should refresh first to apply changed ignore rules and discover new content. CLI search always refreshes first.
- Refresh publishes changes in a single database transaction; traversal, read or ignore-rule errors roll back the refresh. The filesystem is not frozen: results describe observed source snapshots, not a guaranteed simultaneous snapshot of every file. Files may change after the final hash check.

## Build and run

Requires Rust/Cargo 1.90 or newer (the pinned dependencies' minimum) and a C compiler for bundled SQLite and the parser. The current implementation is verified with Rust 1.93.1 on macOS; CI is configured for stable Rust on Linux, macOS and Windows, but those remote runs have not been verified locally.

From the project root:

```bash
cargo build --release --locked -p tinymem-engine
./target/release/tinymem index /absolute/path/to/repository
./target/release/tinymem search /absolute/path/to/repository "authenticate"
./target/release/tinymem watch /absolute/path/to/repository --interval-ms 2000
```

The default database is `<repository>/.tinymem/repository.db`. To share one database across worktrees:

```bash
./target/release/tinymem --db /absolute/path/to/context.db index /path/to/repo-a
./target/release/tinymem --db /absolute/path/to/context.db search /path/to/repo-a "authenticate"
./target/release/tinymem --db /absolute/path/to/context.db index /path/to/repo-b
```

Every successful index emits one compact JSON report with `indexed`, `unchanged`, `removed`, `skipped` and `indexed_at_ms`. `indexed` counts new/changed files, not every scanned file. `skipped` counts visited regular files rejected for type/size/content; entries excluded by ignore rules or directory filters are not counted. CLI search emits `{repository, refresh, hits}`. Errors go to stderr with a nonzero exit code.

Watch is **foreground polling reconciliation**, not an OS-event watcher or detached daemon. It performs an initial refresh, then scans and hashes eligible files at the requested interval (minimum 100 ms); unchanged files are not parsed or rewritten. Successful scans emit JSON lines. Later failures are logged to stderr and retried; Ctrl-C stops the process. `watch --once` performs one reconciliation and exits. Branch switches are observed as working-tree file changes; branch/commit identity and historical branch context are not recorded yet.

## Retrieval and resource limits

```bash
./target/release/tinymem search /path/to/repo "authentication token" --limit 5 --max-bytes 16000
```

Search is lexical FTS5/BM25, not semantic search. Punctuation is stripped, query terms are combined with AND, and FTS operators are not interpreted. At most the first 32 terms are used; queries over 4,096 bytes are rejected. Identifiers such as `authenticate_user` are split into lexical terms.

`SearchOptions { limit, max_bytes }` bounds the compact serialized **hits array**, including source metadata and symbols, not the outer CLI report. Defaults are 5 hits and 16,000 bytes; hard caps are 100 hits and 1 MiB. Oversized hits are skipped rather than returned as misleadingly truncated source. A zero or sub-two-byte budget yields no hits (the empty JSON array itself is two bytes). Up to eight times the requested hit count is considered so stale or oversized candidates can be skipped. Consequently, empty/fewer results do not prove a symbol is absent from the repository.

This is a byte budget, **not an exact model-token budget**. Model-aware tokenization and semantic repository retrieval are future work. Optional
[semantic/hybrid personal-memory search](SEMANTIC_RETRIEVAL.md) is separate from source retrieval.

Eligible source files are UTF-8, contain no NUL bytes and are at most 1 MiB. Indexing processes one file at a time, with a path/hash map for reconciliation; polling still reads unchanged source to detect same-size/same-timestamp edits. Large-repository event-driven refresh and benchmarks are future work; no latency or RAM performance target has been measured.

## Local safety and compatibility

- Honors repository `.gitignore`, `.ignore` and `.tinymemignore`, including folders without `.git` metadata. Parent-directory, global Git ignore files and `.git/info/exclude` are intentionally not used. Ignore files must be readable regular, non-symlink UTF-8 files; otherwise refresh fails closed.
- Skips hidden entries, symlinks and common generated/dependency directories (`node_modules`, `target`, `dist`, `build`, `vendor`, `venv`, `__pycache__`).
- Excludes common `.env`, `secret.*`, `secrets.*` and `credential(s).*` names, and unsupported key/binary extensions. This is **not a secret scanner**; use `.tinymemignore` for sensitive files with other names or embedded credentials.
- Indexed repository text is untrusted data, never instructions. Source is stored on disk and returned to the requesting local caller; there is no authentication or encryption layer. Use normal filesystem access controls and do not expose the database to untrusted users.
- Canonical-path and hash checks are best effort, not a sandbox against a hostile process racing filesystem changes. Do not index an attacker-controlled, concurrently mutated filesystem as a security boundary.
- The native database is deliberately separate from the legacy Python memory schema. It stores repository and personal-memory evidence in separate tables. Native schemas v1/v2/v3 migrate to v4 transactionally (including [explicit file provenance](FILE_PROVENANCE.md)); foreign/newer databases are rejected, with compatibility rechecked under the writer lock before migration. No legacy Python migration is supported. Native memory history/profiles, stdio MCP and lightweight SDKs are available; automatic extraction and HTTP serving are not.
- Original implementation under the repository's MIT license. Runtime/build dependencies retain their own licenses; Cargo.lock pins dependencies. No mem0/Supermemory implementation code was copied.

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Focused tests: `cargo test -p tinymem-engine --test repository` and `cargo test -p tinymem-engine --test cli`. These tests do not download models or call live providers. After the release build, `python3 scripts/smoke-rust-engine.py` optionally checks indexing/search against this checkout using a temporary external database and tests graceful SIGINT watch shutdown on POSIX. It uses only Python's standard library for development verification; Python is not an engine runtime dependency. Cargo dependency installation/builds may require network access; subsequent locked, cached builds can run offline.

The [native memory MVP](NATIVE_MVP.md) adds scoped lifecycle/history, stdio MCP and lightweight SDKs.
Next slices: additional syntax parsers and conservative dependency resolution; optional embeddings/inference;
filesystem-event ingestion with reconciliation; tokenizer-aware context budgets; reproducible resource/freshness benchmarks.
