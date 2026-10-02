# tinymem-native (Python)

Standalone Python **3.10+** SDK, using only the standard library. Import `tinymem_native`, never the legacy `tinymem`/model stack. The native binary must implement:

```text
tinymem [--db PATH] mcp ROOT --user USER [--agent AGENT] [--project PROJECT]
```

```sh
python3 -m pip install ./sdks/python
```

```python
from tinymem_native import Client

memory = Client(binary="tinymem", db="/tmp/native-memory.db", repository=".",
                user_id="alice", agent_id="assistant", project_id="project")
saved = memory.add_memory("Prefer small changes", kind="preference", source="explicit user request")
if saved is not None:
    print(memory.get_memory(saved["id"]))
    updated = memory.update_memory(saved["id"], "Prefer focused changes", expected_version=saved["version"])
print(memory.search_memories("changes"))
print(memory.search_repository("authenticate"))
```

Construction validates configuration but starts no process. Every method starts and owns a fresh child, sends initialize (`2025-06-18`, ID 1), validates the response, sends `notifications/initialized` and `tools/call` (ID 2), closes stdin, and waits for EOF/exit. A method never returns with a live child. There is no persistent connection, `close()` requirement, or async/model dependency. Shared clients can be used across threads; each call has independent process/stream state.

`binary` defaults to `tinymem` on PATH, `repository` to `.`, and optional `db` to the native default. Scope is exact and fixed in subprocess arguments; absent agent/project is a distinct scope, not a wildcard. Scope/memory IDs use 1–128 ASCII token characters: first `[A-Za-z0-9]`, then `[A-Za-z0-9_.:@-]`. Paths/content are literal values, passed without a shell, interpolation, or evaluation. `call_tool` rejects attempts to override scope/repository/database.

| Method | Result |
| --- | --- |
| `add_memory(content, *, kind=None, source=None, expires_at_ms=None, source_refs=None)` | `MemoryRecord` or `None` |
| `get_memory(id)` | `MemoryRecord` or `None` |
| `search_memories(query, *, limit=None, max_bytes=None, mode=None, min_similarity=None)` | `list[MemoryRecord]` |
| `index_memories(*, limit=None)` | `EmbeddingIndexReport` |
| `update_memory(id, content, *, expected_version=None)` | `MemoryRecord` or `None` |
| `delete_memory(id, *, expected_version=None)` | `bool` (`deleted`) |
| `memory_history(id, *, limit=None, max_bytes=None)` | `list[MemoryEvent]` (including deletion) |
| `get_profile(*, limit=None, max_bytes=None)` | `{stable, dynamic}` |
| `refresh_repository()` | `RefreshReport` |
| `search_repository(query, *, limit=None, max_bytes=None)` | `{refresh, hits}` |
| `assemble_context(query, token_budget, *, sources=None, exclude_paths=None)` | `AssembledContext` (`prompt`, provenance-bearing `entries`, budget/count) |
| `call_tool(name, arguments=None)` | raw successful `structuredContent` object |

Optional `None` arguments are omitted, leaving defaults to the native server. Kind defaults there to `fact`; other kinds are `preference`, `decision`, `plan`, `relationship`, `architectural_decision`, `issue_solution`. Optimistic `expected_version` is forwarded unchanged for updates/deletes; no retry hides conflicts. SDK record types expose `version`; the native engine currently also returns compatibility fields `revision`, `key` and `metadata`, which the SDK does not use. Audit `MemoryEvent` types expose `event`, `version`, `at_ms`, an immutable `memory` snapshot and a separate current `freshness` evaluation. Types and `py.typed` are included. The SDK validates the handshake/envelopes/content and top-level wrapper shapes, not every nested record field.

Read methods never send memory-write tools. Repository refresh/search may update the repository index; that is separate from personal memory. History is audit evidence, and deletion is not secure erasure. No assistant-specific integration is claimed.

## Explicit file provenance

```python
saved = memory.add_memory("Use UTC for persisted clock values", source_refs=[
    {"kind": "file", "role": "dependency", "path": "src/clock.rs"}
])
```

Paths resolve under the configured repository. The engine captures `source_refs` with canonical root,
BLAKE3 digest and observation time. `origin` records provenance only; `dependency` makes changed,
missing or unverifiable files exclude the note from current reads. Free-text `source` stays untracked.
`memory_history` retains snapshots and reports `freshness` status/reason, check time and hashes.
Exported types include `FileSourceInput`, `FileSourceReference`, `FileDependencyCheck` and `Freshness`.
Optional `expected_hash` on an input is a save precondition. See [the full contract and limits](../../docs/FILE_PROVENANCE.md).

## Optional semantic retrieval

Configure `embedding_command`, `embedding_model` and `embedding_dimensions` together on `Client`;
`embedding_timeout_ms` defaults to 30000 (maximum 120000). Then use
`index_memories(limit=1)` for explicit backfill and `search_memories(query, mode="hybrid")`.
Modes are `lexical` (default), `semantic` and `hybrid`; `min_similarity` defaults to 0.25 and
only affects the semantic branch. Result records are unchanged. The SDK still needs no model
packages; the trusted executable may require its own dependencies. Increase `timeout` for slow
inference/backfill and use small pages. See [the complete setup and limitations](../../docs/SEMANTIC_RETRIEVAL.md).
Repository search remains lexical.

## Task context assembly

`context = memory.assemble_context("Update auth cookies", 4000, sources=["memory"],
exclude_paths=["generated/**"])` selects relevant scoped evidence, refreshes repository source,
and merges duplicates without losing citations. `sources` defaults to both memory and repository;
`[]` selects none. `exclude_paths` filters repository-relative paths in this assembly only, before
candidate caps/packing; patterns are case-sensitive (`*` does not cross `/`, `**` is recursive).
They do not filter personal-memory `source_refs` or restrict explicit searches. Insert only
`context["prompt"]`, not the response envelope. The complete prompt is counted using a conservative
UTF-8-byte token upper bound (`token_count_method="utf8_bytes_upper_bound"`), not a model-specific
tokenizer; zero/tiny budgets return an empty prompt. Negative/noninteger budgets fail before spawn.
The native engine caps budgets at 262,144. Standalone lexical search uses literal AND: use 1–3
terms and shorten after zero hits. Avoid redundant recall when automatic context already answers the
task. See [policy and exact Rust counter support](../../docs/CONTEXT_ASSEMBLY.md).

## Bounds, deadlines and errors

`timeout=10.0` seconds covers the child operation: initialize, blocked writes, tool response, stdin EOF, exit, and draining readers. Input encoding/validation happens before spawn. Reader/writer threads prevent full pipes from blocking the calling thread indefinitely. On failure the adapter closes stdin, waits `close_timeout=0.5` seconds, sends terminate, waits again, then kills and reaps. Cleanup may add those grace waits to the request deadline; final reaping after kill waits for the OS. No child detaches.

Defaults: `max_frame_bytes=3*1024*1024` per input/output JSON frame excluding newline, `max_output_bytes=4*1024*1024` cumulative stdout including notifications/newlines, `max_stderr_bytes=65536` cumulative stderr. Exceeding any output cap aborts; stderr diagnostics retain at most that byte cap (UTF-8 replacement decoding). Duplicated text/structured tool results fit within the default frame cap. `max_bytes` is the separate native result budget. Invalid output, duplicate/wrong IDs, unsupported versions, truncated frames, nonzero exit, and missing results cannot silently succeed.

Errors inherit `NativeError`: `TransportError`, `ProtocolError`, `OutputLimitError`, `RPCTimeoutError` (also `TimeoutError`), `RPCError` (`code`, `data`), and `ToolError` (`result`). Operation errors expose bounded `stderr` and `returncode` when available. `isError: true` is a business failure even without structured content; valid JSON-RPC `.error` is distinct. Local invalid configuration/oversized input raises `ValueError`/`TypeError`. All failures leave the configuration reusable for a new operation.

## Checks

```sh
cd sdks/python
python3 -m unittest discover -v
TINYMEM_BINARY=/absolute/path/to/target/release/tinymem python3 -m unittest discover -v
python3 -m pip install mypy ruff
mypy
ruff check tinymem_native
python3 -m compileall -q tinymem_native tests
python3 -m pip wheel --no-deps --no-cache-dir --wheel-dir dist .
python3 -I tests/verify_package.py dist/tinymem_native-0.1.0-py3-none-any.whl
```

Real tests use `TINYMEM_BINARY`, defaulting to repository `target/debug/tinymem` (resolved from the test file). They cover CRUD, dedup, restart, exact scopes, conflicts/history/deletion, expiration/profiles, read-vs-write separation and repository updates using temporary directories. A missing default binary or one rejecting `mcp` reports **PENDING/skipped**. Other failures, an explicit unusable `TINYMEM_BINARY`, or `TINYMEM_REQUIRE_INTEGRATION=1` fail rather than silently skip. Controlled subprocess tests cover deadline/output/protocol failures and reaping, not native engine emulation. Tests do not import legacy dependencies, build Rust, create Git metadata, publish, or write hosted memory.
