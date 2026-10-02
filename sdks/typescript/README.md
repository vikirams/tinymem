# tinymem-native (Node / TypeScript)

Node **22.18+** (tested on Node 26); no runtime dependencies. Local development uses TypeScript **5.9+** and `@types/node`. Requires the native MCP CLI:

```text
tinymem [--db PATH] mcp ROOT --user USER [--agent AGENT] [--project PROJECT]
```

```sh
cd sdks/typescript
npm ci
npm run build
```

```ts
import { Client } from './dist/index.js';

const memory = new Client({ binary: 'tinymem', db: '/tmp/native-memory.db', repository: '.',
                            userId: 'alice', agentId: 'assistant', projectId: 'project' });
const saved = await memory.addMemory('Prefer small changes', { kind: 'preference', source: 'explicit user request' });
if (saved !== null) {
  console.log(await memory.getMemory(saved.id));
  await memory.updateMemory(saved.id, 'Prefer focused changes', { expected_version: saved.version });
}
console.log(await memory.searchMemories('changes'));
console.log(await memory.searchRepository('authenticate'));
```

Construction only validates/copies configuration. Each awaited operation owns a fresh non-detached child through its entire initialize → initialized notification → tool call → stdin EOF → exit sequence. Initialize has ID 1 and negotiates protocol `2025-06-18`; the tool call has ID 2 and is not sent before validating initialization. No persistent connection, `connect()` factory, or `close()` requirement. Concurrent calls have independent subprocess state; caller-controlled concurrency determines the number of children.

`binary` defaults to `tinymem` on PATH, `repository` to `.`, optional `db` to the native default. Required `userId` and optional `agentId`/`projectId` form an exact fixed scope, including absent dimensions. Scope and memory IDs must be 1–128 ASCII token characters: first `[A-Za-z0-9]`, then `[A-Za-z0-9_.:@-]`. Paths/content are literal `spawn` arguments/JSON values with `shell: false`, never evaluated. Tool arguments cannot override scope/repository/database.

| Async method | Result |
| --- | --- |
| `addMemory(content, {kind?, source?, expires_at_ms?, source_refs?}?)` | `MemoryRecord \| null` |
| `getMemory(id)` | `MemoryRecord \| null` |
| `searchMemories(query, {limit?, max_bytes?, mode?, min_similarity?}?)` | `MemoryRecord[]` |
| `indexMemories({limit?}?)` | `EmbeddingIndexReport` |
| `updateMemory(id, content, {expected_version?}?)` | `MemoryRecord \| null` |
| `deleteMemory(id, {expected_version?}?)` | `boolean` (`deleted`) |
| `memoryHistory(id, {limit?, max_bytes?}?)` | `MemoryEvent[]` (including deletion) |
| `getProfile({limit?, max_bytes?}?)` | `{stable, dynamic}` |
| `refreshRepository()` | `RefreshReport` |
| `searchRepository(query, {limit?, max_bytes?}?)` | `{refresh, hits}` |
| `assembleContext(query, tokenBudget, options?)` | `AssembledContext` (`prompt`, provenance-bearing `entries`, budget/count) |
| `callTool(name, args?)` | raw successful `structuredContent` |

Wire/options/result keys stay snake_case. Undefined options are omitted; defaults belong to the native server (`fact` for kind). Other kinds: `preference`, `decision`, `plan`, `relationship`, `architectural_decision`, `issue_solution`. Optimistic `expected_version` is forwarded without retrying conflicts. SDK record types expose `version`; the native engine also returns compatibility fields `revision`, `key` and `metadata`, which the SDK does not use. Audit `MemoryEvent` types expose `event`, `version`, `at_ms`, an immutable `memory` snapshot and a separate current `freshness` evaluation. Exported types cover options, records, audit events, scope, profile and native repository hits. Runtime checks cover handshake, envelopes, content and top-level wrapper shapes, not every nested field.

Read methods never send personal-memory writes. Code search refreshes the repository index, separate from personal memory. History/deletion is audit evidence, not secure erasure. No specific assistant installation is claimed.

## Explicit file provenance

```ts
const saved = await memory.addMemory('Use UTC for persisted clock values', {
  source_refs: [{ kind: 'file', role: 'dependency', path: 'src/clock.rs' }],
});
```

Paths resolve under the configured repository. The engine captures `source_refs` with canonical root,
BLAKE3 digest and observation time. `origin` records provenance only; `dependency` makes changed,
missing or unverifiable files exclude the note from current reads. Free-text `source` stays untracked.
`memoryHistory` retains snapshots and reports `freshness` status/reason, check time and hashes.
Exported types include `FileSourceInput`, `FileSourceReference`, `FileDependencyCheck` and `Freshness`.
Optional `expected_hash` on an input is a save precondition. See [the full contract and limits](../../docs/FILE_PROVENANCE.md).

## Optional semantic retrieval

Configure `embeddingCommand`, `embeddingModel` and `embeddingDimensions` together;
`embeddingTimeoutMs` defaults to 30000 (maximum 120000). Use `indexMemories({limit: 1})`
for explicit backfill and `searchMemories(query, {mode: 'hybrid'})` for hybrid retrieval.
Modes are `lexical` (default), `semantic` and `hybrid`; `min_similarity` defaults to 0.25 and
only affects the semantic branch. Result records are unchanged. The SDK still needs no model
packages; the trusted executable may require its own dependencies. Increase `timeoutMs` for slow
inference/backfill and use small pages. See [the complete setup and limitations](../../docs/SEMANTIC_RETRIEVAL.md).
Repository search remains lexical.

## Task context assembly

`const context = await memory.assembleContext('Update auth cookies', 4000, {
  sources: ['memory'], exclude_paths: ['generated/**'],
})` selects relevant scoped memory and refreshed source, merging duplicates without losing citations.
`sources` defaults to both memory and repository; `[]` selects none. `exclude_paths` filters
repository-relative paths for this assembly only, before candidate caps/packing; patterns are
case-sensitive (`*` does not cross `/`, `**` is recursive). They do not filter personal-memory
`source_refs` or restrict explicit searches. Insert only `context.prompt`, not the response envelope.
The complete prompt is counted using a conservative UTF-8-byte token upper bound
(`token_count_method: 'utf8_bytes_upper_bound'`), not a model-specific tokenizer. Zero/tiny budgets
return an empty prompt; negative/noninteger/unsafe budgets fail before spawn. The engine caps budgets
at 262,144. Standalone lexical search uses literal AND: use 1–3 terms and shorten after zero hits.
Avoid redundant recall when automatic context already answers the task. See [policy and exact Rust
counter support](../../docs/CONTEXT_ASSEMBLY.md).

## Bounds, deadlines and errors

`timeoutMs=10000` covers initialize, blocked pipe writes, the tool response, EOF/exit and stream draining. Input encoding/validation is before spawn. Every promise settles only after its child is reaped/streams closed. On timeout or failure, stdin ends, then `closeTimeoutMs=500` grace precedes SIGTERM, another grace precedes SIGKILL, followed by waiting for exit. Cleanup can add these grace waits to the deadline; OS reaping after kill has no separate hard timer.

Defaults: `maxFrameBytes=3*1024*1024` per input/output JSON frame excluding newline; `maxOutputBytes=4*1024*1024` cumulative stdout including notifications/newlines; `maxStderrBytes=65536` cumulative stderr. Any output overflow aborts, retaining no more than the stderr cap for diagnostics. The default allows duplicated text/structured results. `max_bytes` is the native tool's separate result budget. Missing results, malformed/truncated UTF-8 JSON, invalid IDs, unsupported protocol versions, nonzero exit and malformed results reject rather than silently succeed.

`NativeError` subclasses: `TransportError`, `ProtocolError`, `OutputLimitError`, `RPCTimeoutError`, `RPCError` (`code`, `data`), `ToolError` (`result`). Transport/protocol operation failures carry bounded `stderr` and `returncode` when available. Business `isError: true` rejects even without structured content; JSON-RPC `.error` remains distinct. Invalid local options/input raise `TypeError`/`RangeError`. Millisecond timers are positive integers within Node's timer range. Configurations are reusable after failed operations.

## Checks

```sh
npm run typecheck
npm test
npm run build
node tests/verify-package.mjs
TINYMEM_BINARY=/absolute/path/to/target/release/tinymem npm test
```

Tests use Node's built-in test runner/type stripping; consumers use compiled `dist/index.js` and declarations. Real end-to-end tests default to repository `target/debug/tinymem` or `TINYMEM_BINARY`. They cover CRUD/dedup/restart, exact scopes, history/conflicts/deletion, expiration/profiles, read-vs-write separation and repository edits/deletions in temporary directories. A missing default binary or one rejecting `mcp` explicitly reports **PENDING/skipped**. Other errors, an unusable explicit binary, or `TINYMEM_REQUIRE_INTEGRATION=1` fail. Controlled executable subprocess tests cover hostile transport behavior and child reaping, not a native engine emulator. No global package install, Rust build, Git metadata, publishing or hosted memory writes are needed.
