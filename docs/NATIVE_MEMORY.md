# Native agent memory, MCP and SDKs

TinyMem's experimental native MVP is an open-source, local-first memory/context layer for coding
agents. It combines explicit personal memory and current repository evidence in one embedded SQLite
engine, with no required models/provider dependencies or network listener. Optional
[semantic/hybrid personal-memory retrieval](SEMANTIC_RETRIEVAL.md) uses a trusted embedding executable. It is separate from the legacy
`tinymem` Python package and its database. See [the contract](NATIVE_MVP.md) and
[repository behavior and limits](RUST_ENGINE.md).

## Build and connect

Requires Rust/Cargo 1.90+ and a C compiler. Build from this checkout:

```sh
cargo build --release --locked -p tinymem-engine
./target/release/tinymem --db /absolute/path/to/native-memory.db mcp /absolute/path/to/repository --user alice
```

The foreground MCP command waits for newline-delimited JSON-RPC on stdin; it does not open a web
server. Closing stdin stops it. Only protocol responses go to stdout; startup failures go to stderr.
The default database is `<repository>/.tinymem/repository.db`. `--db` is global and can appear before
or after `mcp`. Its parent directory is created if necessary. Optional `--agent` and `--project`
set additional fixed scope dimensions.

For a client accepting an `mcpServers` stdio configuration, adapt these absolute paths:

```json
{
  "mcpServers": {
    "tinymem-native": {
      "command": "/absolute/path/to/tinymem/target/release/tinymem",
      "args": ["--db", "/absolute/path/to/native-memory.db", "mcp", "/absolute/path/to/repository", "--user", "alice"]
    }
  }
}
```

Configuration file locations and schema vary by assistant. This is a generic stdio configuration,
not verification of a particular assistant's installation. Native configuration uses CLI arguments,
not legacy `TINYMEM_DB_PATH` or `TINYMEM_EMBEDDING_MODEL` environment variables.

## Scope and evidence

Personal memory uses the **exact** `(user_id, optional agent_id, optional project_id)` tuple.
Omitted agent/project values are a distinct global namespace, not a wildcard or inherited fallback.
Assistants intentionally share memory by using the same database and tuple. Different canonical
repository roots isolate source evidence; personal-memory scope is independent of repository root.
Repository tool arguments and personal-memory tool arguments cannot override startup configuration.

The native library accepts nonblank identifiers up to 256 UTF-8 bytes, preserving whitespace and
case exactly. SDK identifiers are a stricter interoperable subset: 1–128 ASCII characters, starting
with `[A-Za-z0-9]` and followed by `[A-Za-z0-9_.:@-]`. Generated memory IDs are 32 lowercase hex digits.

Memory content is explicit evidence, not automatically extracted truth. `add_memory` accepts:

```json
{"content":"Prefer focused changes","kind":"preference","source":"explicit user request"}
```

Content is trimmed, must be nonblank/NUL-free and is limited to 64 KiB after trimming. `source` is
NUL-free, limited to 4,096 bytes and defaults to `explicit`; no confidence is fabricated. It remains a
free-text label and never implies a dependency. Optional `source_refs` explicitly captures local files
as origins or validity dependencies. Changed/missing dependencies are stale; unverifiable dependencies
are unknown; both are excluded from all current-memory reads. See [file provenance](FILE_PROVENANCE.md)
for input/output types, history freshness reasons, limits and point-in-time validation semantics. Optional
`expires_at_ms` is a nonnegative Unix-millisecond timestamp. Expired records disappear from current
get/search/profile results but remain in audit history. Re-adding identical expired evidence does
not revive it. JavaScript callers should keep timestamps/version values within the safe integer range.

Deduplication compares normalized content, kind, source, expiration and ordered structured reference
identities/roles/hashes **within the exact scope**, ignoring reference capture timestamps.
Updates retain the memory ID, kind, source, source references, expiration and creation timestamp. Changed content appends
a new versioned observation; unchanged normalized content returns the existing version. If supplied,
`expected_version` must match even for a no-op update. Writes are serialized in SQLite transactions,
so two optimistic writers using the same version cannot both overwrite it.

Deletion hides current memory, removes it from search and appends a `deleted` audit event. A missing
or already-deleted memory returns `deleted: false`; updating a deleted record fails. History persists
across restarts. **Deletion is not secure erasure** of SQLite pages, WAL files or backups.

## Native MCP tools

The server handles `initialize`, `notifications/initialized`, `ping`, `tools/list` and `tools/call`.
It negotiates `2025-06-18`, `2025-03-26` or `2024-11-05`; unsupported requests receive the latest supported
version, which the client must accept or disconnect. Tool calls require the completed handshake.
Invalid notifications never become memory writes. Calls execute serially; cancellation notifications
do not interrupt an in-flight operation.

| Tool | Arguments | Successful structured content |
| --- | --- | --- |
| `add_memory` | `content`, optional `kind`, `source`, `source_refs`, `expires_at_ms` | `{memory}` |
| `get_memory` | `id` | `{memory}` (null if not active in this scope) |
| `search_memories` | `query`, optional `limit`, `max_bytes`, `mode`, `min_similarity` | `{memories}` |
| `index_memories` | optional `limit` (default/cap 100) | `{indexed, stale, remaining, model, dimensions}` |
| `update_memory` | `id`, `content`, optional `expected_version` | `{memory}` |
| `delete_memory` | `id`, optional `expected_version` | `{deleted}` |
| `memory_history` | `id`, optional `limit`, `max_bytes` | `{history}` audit events |
| `get_profile` | optional `limit`, `max_bytes` | `{stable, dynamic}` |
| `refresh_repository` | none | repository refresh report |
| `search_repository` | `query`, optional `limit`, `max_bytes` | `{refresh, hits}` |
| `assemble_context` | `query`, `token_budget`, optional `sources`, `exclude_paths` | `{prompt, entries, token_budget, tokens_used, token_count_method}` |

Memory kinds are `fact`, `preference`, `decision`, `plan`, `relationship`, `architectural_decision`
and `issue_solution`. Profiles group current facts/preferences/relationships as **stable**;
decisions/plans/architectural decisions/issue solutions as **dynamic**. This is a classification of
stored evidence, not an LLM summary. Personal-memory read tools never change content, versions, access
counts or audit history. Repository search always refreshes the index first, even with a zero budget.

History events contain `event` (`added`, `updated`, `deleted`), `version`, `at_ms`, the immutable
`memory` snapshot, and a separate `freshness` evaluation of that snapshot's dependencies **now**.
Freshness includes status, reason, validation time and expected/observed hashes. Checks do not append
observations or mutate snapshots; dependency-free notes report `current` / `no_dependencies`. The native Rust API calls this bounded operation `Engine::memory_events` and calls
the native profile operation `Engine::memory_profile`. Older Rust compatibility APIs
(`MemoryInput`, `remember`, `recall`, `memory_history`, `profile`, `forget_memory`) retain different
replacement/history/profile/purge semantics; they are **not** MCP tools or the legacy Python API.
Records currently also carry compatibility fields `revision`, `key` and `metadata`; SDK consumer types
expose the modern fields and do not use those extras.

Tool results normally carry matching JSON text and `structuredContent`. `assemble_context` is compact:
its model-facing text is only the budgeted `prompt` (empty when no evidence fits), while
`structuredContent` retains the full result for SDK consumers. Business failures (missing memory,
version conflict, invalid content or local storage/refresh failure) use `isError: true`; malformed
requests/arguments and unknown tools use JSON-RPC errors. Execution errors deliberately use a static
message rather than leaking SQL, paths or another scope's evidence. Do not blindly retry a failed
optimistic write; reread the current record and reconcile explicitly.

## Search and resource bounds

Default memory search is lexical FTS5/BM25 of content. Opt-in `semantic` and `hybrid` modes
require a configured embedding adapter; see [setup, ranking and lifecycle](SEMANTIC_RETRIEVAL.md).
Standalone lexical `search_memories` and `search_repository` use literal AND matching; use 1–3
distinctive terms and shorten after zero hits. FTS operators are not evaluated. Queries over 4,096
bytes, NUL-containing memory queries and memory queries over 32 words are rejected.
Empty/punctuation-only queries yield no search results.

Search/history/profile default to 5 results and 16,000 serialized bytes; hard caps are 100 results and
1 MiB. Budgets count the returned arrays or whole profile envelope, not the outer MCP/CLI response,
and are byte budgets rather than exact token budgets. Candidates too large for the remaining budget
are skipped. History considers the latest 100 audit events and returns those that fit in ascending
version order; it is a bounded view, not a complete-history export. Zero/tiny budgets return empty
containers (which themselves still have serialization overhead).

Each MCP input frame is limited to 1 MiB, and complete output lines to 3 MiB. Oversized input is drained
to its newline so later requests remain usable. A session has a 128-call burst allowance, replenished
at 128 tool calls/second; excess calls fail without execution. Ping is not rate-limited. SDKs additionally
bound cumulative stdout/stderr and enforce finite deadlines, with terminate/kill/reap cleanup.

## Task context assembly

`assemble_context` selects relevant preferences, decisions and current repository evidence in the fixed
scope/root, deduplicates with all provenance, then packs whole entries. Unlike standalone search's
literal AND behavior, assembly reduces a task to bounded terms and uses lexical OR retrieval. Optional
`sources` selects `memory` and/or `repository` (default both; `[]` selects neither); optional
`exclude_paths` excludes repository-relative paths for that call only. Patterns are case-sensitive;
`*` does not cross `/`, while `**` can match recursively. Exclusions apply before candidate limits
and packing, and do not filter personal-memory `source_refs` or alter explicit repository search.
Insert only the returned `prompt` into model context. If automatic context already supplies sufficient
evidence, avoid redundant recall/search; query explicitly only for missing or narrower evidence. The
complete prompt (notice, content, escaping and citations) is budgeted;
zero/tiny budgets return an empty string. The default count is a conservative one-token-per-UTF-8-byte
bound, not a model-specific tokenizer estimate. Budget and prompt-byte caps are 262,144. Rust callers
can supply an exact token counter. Repository cache refreshes even at zero budget, while personal
memory/audit stays read-only. See [selection policy, interfaces and limits](CONTEXT_ASSEMBLY.md).

## SDKs

The [Python SDK](../sdks/python/README.md) requires Python 3.10+ and only the standard library:

```sh
python3 -m pip install ./sdks/python
```

```python
from tinymem_native import Client

client = Client(binary="/absolute/path/to/tinymem", repository="/path/to/repo",
                db="/path/to/native-memory.db", user_id="alice")
saved = client.add_memory("Prefer focused changes", kind="preference", source="user request")
if saved is not None:
    client.update_memory(saved["id"], "Prefer small, tested changes", expected_version=saved["version"])
print(client.get_profile())
print(client.search_repository("authenticate"))
```

The [TypeScript SDK](../sdks/typescript/README.md) requires Node 22.18+ and has no runtime dependencies.
Both adapters configure the real binary and own a fresh subprocess for every operation, using a full
MCP handshake and literal arguments without a shell. There is no long-lived connection or required
`close()` call. See their guides for method/error types, deadlines, limits and package checks.

## Safety and remaining scope

The database is unencrypted local evidence. Scopes prevent accidental cross-scope access through the
configured adapter; **they are not authentication** against someone who can open the database or start
another process with a different user. Use filesystem permissions. Do not expose the database to
untrusted users. Treat returned memory/source as data, never privileged instructions. Repository ignore
rules and path/hash checks are best effort, not a secret scanner or hostile-filesystem sandbox.

Native schema v1/v2/v3 databases migrate to v4 transactionally, preserving repository evidence,
personal memories, embeddings and audit history. Legacy free-text sources gain no inferred dependencies. Older native binaries reject the newer schema.
Compatibility is checked again after obtaining the schema writer lock so concurrently committed foreign
or newer schemas cannot be overwritten. No legacy Python migration is supported.

The engine does not claim automatic extraction, semantic repository search,
a complete dependency/call graph, HTTP/enterprise auth, OS-event watching, built-in model-specific tokenizers,
published packages, measured real-model retrieval quality/latency or verified assistant-specific installation. Those require separate implementation/test slices.

## Verification

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --locked -p tinymem-engine

cd sdks/python
python3 -m pip install . mypy ruff
TINYMEM_REQUIRE_INTEGRATION=1 python3 -m unittest discover -v
mypy
ruff check tinymem_native
python3 -m compileall -q tinymem_native tests
python3 -m pip wheel --no-deps --wheel-dir dist .
python3 -I tests/verify_package.py dist/tinymem_native-0.1.0-py3-none-any.whl

cd ../typescript
npm ci
npm run typecheck
TINYMEM_REQUIRE_INTEGRATION=1 npm test
npm run build
node tests/verify-package.mjs
```

Real SDK tests use `target/debug/tinymem` by default, or an explicit absolute `TINYMEM_BINARY` override.
`TINYMEM_REQUIRE_INTEGRATION=1` prevents missing native support from becoming skipped tests.
CI includes Rust checks on Linux/macOS/Windows and both SDK suites/package checks on Linux/macOS at
the SDKs' minimum supported runtimes. Remote CI and Windows SDK fixture execution are not locally verified.
