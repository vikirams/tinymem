# File-provenance milestone: local verification

Implemented slice: [structured local file origins/dependencies and read-time freshness](FILE_PROVENANCE.md).
No benchmark runner, live inference, PR resolver or assistant plugin was added.

## Tests-first evidence

The initial public Rust test failed on the missing reference/freshness API before implementation.
The MCP test then failed on the missing `source_refs` schema. Python failed on the unsupported
`source_refs` keyword; TypeScript failed because its wrapper dropped the new option. Each was made
green through the real engine/MCP path. A subsequent bounded-candidate regression test failed because
stale records filled the semantic top-100 pool; eligibility now precedes candidate caps in semantic,
hybrid and lexical context selection.

New Rust tests are in `tests/file_freshness.rs`, `tests/freshness_safety.rs` and
`tests/freshness_migration.rs` under `crates/tinymem-engine/`. Existing `tests/migration_lock.rs` also
exercises edit/delete/unavailable source races while a save waits for the real SQLite writer lock.
MCP tests and both SDK integration suites exercise the shipped wire contract. Fixtures use temporary
local files and SQLite databases; optional semantic tests use deterministic fake embedders, not models.

## Actual local results

Environment: macOS / Darwin arm64; Rust/Cargo 1.93.1; Python 3.14.7 and 3.10.18; Node 26.1.0.
Real SDK integration was required (`TINYMEM_REQUIRE_INTEGRATION=1`), using `target/debug/tinymem`.

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `cargo test --workspace --locked` | 97 passed, 0 failed, 0 ignored |
| `cargo build --locked -p tinymem-engine` | Passed (debug binary) |
| Python unittest discovery, Python 3.14 and 3.10 | 17 passed on each, no skips |
| Python mypy (strict, targeting 3.10) | Passed |
| Ruff `check tinymem_native` | Passed |
| Python `compileall` | Passed |
| Python wheel build + isolated package verification | Passed; wheel verified on 3.14 and 3.10 |
| TypeScript `npm ci`, typecheck | Passed |
| TypeScript real/unit test suite | 17 passed, no skips |
| TypeScript build + compiled-package smoke | Passed |

Run Rust checks from the repository root. SDK reproduction commands:

```sh
cd sdks/python
TINYMEM_REQUIRE_INTEGRATION=1 python3 -m unittest discover -v
uv run --no-project --with mypy --with ruff python -m mypy
uv run --no-project --with ruff ruff check tinymem_native
python3 -m compileall -q tinymem_native tests
uv run --no-project --with pip --with setuptools python -m pip wheel --no-deps --wheel-dir dist .
python3 -I tests/verify_package.py dist/tinymem_native-0.1.0-py3-none-any.whl
TINYMEM_REQUIRE_INTEGRATION=1 uv run --no-project --python 3.10 python -m unittest discover -v
uv run --no-project --python 3.10 python -I tests/verify_package.py dist/tinymem_native-0.1.0-py3-none-any.whl

cd ../typescript
npm ci
npm run typecheck
TINYMEM_REQUIRE_INTEGRATION=1 npm test
npm run build
node tests/verify-package.mjs
```

`uv` was only used to supply development check/build tools and the already installed Python 3.10;
it is not an SDK runtime dependency. Rust's dependency lockfile is unchanged.

## Remaining limits / not run

- File-only, local, whole-file BLAKE3 references. No Git revision semantics, rename-following, remote
  resolver, persistent validation history or reference-reapproval API. Identical restored bytes become
  eligible again; editing memory content never silently recaptures its dependencies.
- Point-in-time filesystem checks, not atomic filesystem/SQLite snapshots or a hostile-filesystem
  sandbox. Reads hash candidate dependencies without a global validation-time budget or cache.
- No latency/scale measurements, before/after performance comparison, coding-agent outcome benchmark,
  live model test or plugin/client acceptance run. Correctness tests are not evidence of coding success.
- No Linux/Windows or minimum-Node runtime execution in this local pass; POSIX permission/symlink tests
  ran on macOS. No release-binary build, distribution/signing or remote CI verification in this pass.
- Exact scopes remain namespaces, not authorization. History retains deleted content; no secure purge.
- This checkout has no Git metadata, so no commit or Git diff verification was possible.
