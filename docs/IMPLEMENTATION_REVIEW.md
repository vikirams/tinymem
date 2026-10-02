# Native agent-memory implementation review

## Scope

Historical baseline v2 MVP report. The later [semantic retrieval slice](SEMANTIC_RETRIEVAL.md)
adds opt-in embeddings, a tenth MCP tool and schema v3; counts/deferred scope below describe
the earlier snapshot, not the current feature set.

Single-session review of the current implementation against [NATIVE_MVP.md](NATIVE_MVP.md),
README/architecture guidance and the documented local-first/simplicity principles. This checkout has
no Git metadata, originating issue tracker or available peer reviewer; this is not a commit-diff or
independent parallel review. Legacy Python modules/database were deliberately left unchanged and
were not model-loaded or revalidated as part of the native contract.

Reviewed native engine/schema, memory lifecycle, source/parser, CLI/MCP and their integration tests;
Python and TypeScript SDK transports/types, real-binary tests, package checks and CI. Python inspection
covered `sdks/python/tinymem_native/__init__.py`, `types.py` and
`sdks/python/tests/{test_client,test_integration,fake_server,typecheck_usage,verify_package}.py`.

## Standards

**0 non-tooling findings.** No additional documented-standard breach or actionable baseline code
smell was established. Tool-enforced formatting/type/lint failures are recorded below rather than
counted as separate heuristic findings. Legacy Rust compatibility APIs were retained instead of
breaking existing consumers; their different semantics are now documented explicitly.

## Spec

**3 gaps addressed:**

1. **High — incompatible schema could be overwritten after a lock wait.**
   Contract: “Migrate native schema v1 to v2 transactionally while preserving repository data.
   Reject foreign/newer databases.” `Engine::open` checked identity/version before acquiring its
   immediate transaction. A concurrent writer could commit a foreign identity or newer version
   while the opener waited; the opener would then migrate it and reset the version to v2.
   The existing deterministic `migration_lock` test failed with
   `Engine accepted a concurrently committed incompatible schema: PRAGMA user_version=3`.
   **Fixed:** `crates/tinymem-engine/src/lib.rs:23–39,99–105` shares the compatibility check and repeats
   it under the writer lock before DDL. The real SQLite/VFS lock test now passes, including preserving
   the database/WAL on rejection and retaining v1 repository evidence through migration.

2. **Medium — Python SDK static checks were not clean.**
   Contract: “Build/typecheck/lint all touched surfaces.” Root-level mypy inherited the legacy Python
   3.8 settings; its supported-runtime override still reported ten SDK errors, including a loop
   variable inferred as a number but reused for optional string scope IDs and unchecked `Any` returns.
   **Fixed:** `sdks/python/tinymem_native/__init__.py:90,289–291,321,343–364` separates the identifier
   variable and uses explicit casts after existing runtime shape checks. Public exports are explicit.
   `sdks/python/pyproject.toml` now isolates strict Python 3.10 typing and Ruff checks from the legacy
   package. Strict mypy and Ruff pass; casts do not claim full nested-record runtime validation.

3. **Medium — modern Engine verification was partial.**
   Contract: “Tests cover … CRUD/history/deletion/expiration, profiles, conflicts …” Existing Rust
   memory tests primarily exercised older replacement/profile/history APIs; MCP/SDK tests covered
   modern behavior but not simultaneous optimistic writers at the modern Engine seam.
   **Completed:** `crates/tinymem-engine/tests/native_memory.rs` tests stable-ID CRUD, immutable audit,
   no-op and stale-version behavior, deletion/restart/non-resurrection, all seven native profile kinds,
   expiration and exact profile byte boundaries, and two actual SQLite writers competing at version 1.
   Exactly one commits version 2 and one reports a conflict. CI now requires both real SDK integration
   suites plus typing/lint/package checks on Linux/macOS; unavailable native support cannot silently skip.

## Documentation and integration completion

- Added [native memory/MCP setup](NATIVE_MEMORY.md), including fixed exact scopes, nine tools,
  generic stdio configuration, provenance/expiration, profiles, audit events and resource/safety limits.
- Corrected README, architecture, repository guide, quick start and MCP setup: native memory/MCP/SDKs
  are implemented; legacy Python remains separate. Removed an unmeasured quick-start latency claim.
- Corrected both SDK guides: history returns `MemoryEvent` objects, including deletion, rather than
  bare memory snapshots; native compatibility fields are present but not used by SDK consumer types.
- Added SDK CI gates at Python 3.10 and Node 22.18, while retaining Rust Linux/macOS/Windows checks.
  Workflow execution on remote runners has not been verified locally.

## Verification results

All checks below passed locally on macOS with Rust/Cargo 1.93.1:

| Surface | Result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS |
| `cargo test --workspace --locked` | 64 tests passed |
| `cargo build --release --locked -p tinymem-engine` | PASS |
| Repository smoke and foreground watch SIGINT shutdown | PASS |
| Python SDK suite against release binary, integration required | 14 passed, no skips |
| TypeScript SDK suite against release binary, integration required | 14 passed, no skips |
| Minimum-runtime Python 3.10.18 and Node 22.18 SDK runs | Both suites passed, no skips |
| Strict SDK mypy, Ruff and Python compileall | PASS |
| Python wheel build and isolated import/dependency/type-marker smoke | PASS |
| TypeScript typecheck/build and compiled-package smoke | PASS |
| Migration and optimistic-writer regression loop | 5 repeated runs passed |
| Local Markdown links in updated documentation | PASS |

Current-runtime SDK tests also passed on Python 3.14.7 and Node 26.1.0. Package smoke verifies the
Python wheel/compiled Node SDK locally; neither package was published. Windows SDK subprocess
fixtures are not claimed verified. These tests are correctness checks, not latency/resource benchmarks.

## Remaining scope

No further required MVP behavior gap was identified in this review. Explicitly deferred capabilities
remain: autonomous extraction, semantic embeddings, complete code/dependency graphs, enterprise auth,
HTTP service, OS-event watching, exact tokenizer budgets, published packages, measured benchmarks and
verified assistant-specific installations. Scopes are not authentication; deletion is not secure erasure.

**Summary:** Standards 0 non-tooling findings (no worst issue); Spec 3 gaps addressed, worst issue the
schema-lock compatibility race, now fixed. Remote CI and legacy Python validation remain unverified.
