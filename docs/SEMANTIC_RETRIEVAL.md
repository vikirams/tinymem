# Opt-in semantic and hybrid personal-memory retrieval

This extends the [native MVP](NATIVE_MVP.md). Repository/source search remains lexical;
automatic extraction and consolidation are still deferred. [Task context assembly](CONTEXT_ASSEMBLY.md)
uses hybrid personal retrieval when configured. The legacy Python `tinymem` package is unchanged.

**Open-core policy:** optional inference uses an explicitly trusted adapter; lexical operation requires no model or cloud service. The [coding-agent roadmap](MEMORY_SYSTEM_PLAN.md) does not require a particular provider. Runs selecting the [controlled Cloudflare integration](../integrations/cloudflare/README.md) must obey its gateway-only policy; that adapter has deterministic tests but live gateway verification remains pending. The local sentence-transformers example below is a separate opt-in reference adapter, not a fallback inside the controlled gateway path. Deterministic test fixtures are not model inference.

## Interface and defaults

- Existing callers keep lexical content-only FTS5/BM25 with literal **AND** matching.
- `search_memories` accepts `mode: lexical | semantic | hybrid` (default `lexical`) and
  `min_similarity` (default `0.25`, finite and in `[-1, 1]`). Result shape stays `{memories}`.
- Semantic/hybrid require a configured embedder. Missing configuration, failed inference,
  malformed vectors and corrupt cached vectors fail explicitly, never silently fall back.
- `index_memories` explicitly backfills existing active memories in the exact startup scope.
  Its optional `limit` defaults to 100 and is capped at 100. Results are
  `{indexed, stale, remaining, model, dimensions}`. Repeat until `remaining` is zero;
  use smaller pages when inference is slow. Zero limit only reports remaining work.
- Configured explicit add/update operations generate and persist a vector atomically with
  their memory write. Inference failure commits neither evidence nor audit events. A no-op
  explicit write can fill a missing vector without changing memory versions/history.
- Search only embeds the query; it never backfills, records access, or writes memory/index state.
  With no cached vectors, semantic returns no hits; hybrid can still return lexical matches.

All current-memory operations retain exact `(user_id, agent_id, project_id)` isolation and
exclude expired/deleted evidence. There is no cross-scope inheritance or authentication.
Search limits and serialized-byte budgets are unchanged; these are not tokenizer budgets.

## Configure a trusted executable

The native binary and SDKs still have no mandatory model/provider dependencies. To opt in:

```sh
tinymem --db /path/to/native.db mcp /path/to/repo --user alice \
  --embedding-command /absolute/path/to/embed \
  --embedding-model immutable-model-identity \
  --embedding-dimensions 384 \
  --embedding-timeout-ms 30000
```

The embedding command is immutable startup configuration, never a tool argument. It is one
executable path (no shell, command string or interpolated arguments). Each invocation receives
one JSON object on stdin, then EOF:

```json
{"model":"immutable-model-identity","text":"Prefer small, tested changes"}
```

It must emit exactly one JSON object on stdout and exit successfully:

```json
{"embedding":[0.1,0.2,0.3]}
```

The example above illustrates the shape only; actual vectors must match the configured
number of dimensions. Diagnostics belong on stderr. Dimensions must be 1–4096; elements
must be finite, with nonzero norm. The engine normalizes vectors before persistence/search.
Model identities are nonblank, NUL-free and at most 4096 bytes. Model identity **must change**
when weights, dimension handling, query/document preprocessing or adapter behavior changes.
Weights are not fingerprinted automatically. Model/dimension namespaces never mix vectors.

`CommandEmbedder` bounds stdout to 128 KiB, stderr to 16 KiB, and each invocation to the
configured deadline (default 30 seconds, maximum 120 seconds). It drains pipes concurrently,
kills on failure/timeout, and reaps the owned child. Backfill may invoke the executable up to
100 times sequentially, so set SDK deadlines appropriately or use small pages. The executable
is trusted code, **not sandboxed**: it can read local files, use the network, and must not leave
background descendants holding pipes open. Do not configure untrusted executables or allow
untrusted users to edit one. The engine does not download models or transmit text itself.

[examples/native_embeddings.py](../examples/native_embeddings.py) is a reference adapter
using an **optional, separately installed** `sentence-transformers` and a pre-downloaded local
model directory, with `local_files_only=True`. Use an executable wrapper pointing at the
virtualenv's Python:

```sh
#!/bin/sh
exec /absolute/path/to/venv/bin/python /absolute/path/to/tinymem/examples/native_embeddings.py
```

Make that wrapper executable and pass the absolute local model directory as
`--embedding-model`, with its actual output dimensions. Each invocation loads the model;
this intentionally simple example is inefficient for larger stores and is not a production
inference server. Keep the model directory immutable (or use a new path/adapter identity).

## SDK usage

Python:

```python
from tinymem_native import Client

client = Client(binary="/path/to/tinymem", repository="/path/to/repo",
                db="/path/to/native.db", user_id="alice",
                embedding_command="/path/to/embed",
                embedding_model="/path/to/local/model", embedding_dimensions=384,
                timeout=90)
while client.index_memories(limit=1)["remaining"]:
    pass
print(client.search_memories("preferred development workflow", mode="hybrid"))
```

TypeScript:

```ts
const client = new Client({ binary: '/path/to/tinymem', repository: '/path/to/repo',
  db: '/path/to/native.db', userId: 'alice', embeddingCommand: '/path/to/embed',
  embeddingModel: '/path/to/local/model', embeddingDimensions: 384, timeoutMs: 90000 });
while ((await client.indexMemories({ limit: 1 })).remaining) { /* next page */ }
console.log(await client.searchMemories('preferred development workflow', { mode: 'hybrid' }));
```

Rust callers configure an `Embedder` adapter with `Engine::with_embedder`, backfill with
`Engine::index_memories`, and pass `MemorySearchOptions` to `Engine::search_memories`.
Existing `SearchOptions`/integer-limit callers remain lexical. The adapter supplies model
identity, dimensions and `embed(text)`; the engine freezes metadata at configuration time.
An adapter must produce vectors in the same space for documents and queries.

## Ranking, lifecycle and migration

Semantic search computes exact cosine similarity over active, scoped, matching-model vectors;
there is no ANN index or external vector database. It keeps the best 100 candidates above the
inclusive similarity floor. Similarity ties use memory ID order. Hybrid retrieves the top 100
lexical **OR** matches and top 100 semantic candidates, deduplicates by memory ID, and combines
ranks with equal-weight reciprocal-rank fusion (`k=60`). It uses deterministic ID tie-breaking.
The similarity floor affects only the semantic branch, not lexical-only hybrid matches.
Oversized candidates are skipped while packing the result byte budget; candidates outside
these bounded ranking pools are not reconsidered. No fabricated confidence is returned.

Vectors live in a separate SQLite derived-cache table keyed by memory ID, model identity and
dimensions, with the memory version they represent. Every evidence update invalidates all
cached models in the same transaction, even when written by an unconfigured lexical-only
process. Soft deletion also invalidates vectors; hard purge cascades. Backfill generates outside
the writer lock, then checks version/current visibility under the lock before saving. Concurrent
changes count as `stale`; retrying backfill regenerates only missing/current work. Each backfill
page is atomic on provider failure. Ranked retrieval and record hydration use one read snapshot.

Embeddings originally shipped in v3. The current [file provenance slice](FILE_PROVENANCE.md) migrates
native schemas v1/v2/v3 transactionally to **v4**, preserving repository evidence, memories, vectors
and audit history. Dependency freshness is checked before semantic/hybrid candidate caps and again
at hydration; stale/unknown notes cannot return through cached vectors. Older binaries reject v4:
back up the database before upgrading
if rollback is needed. Foreign/newer databases and the legacy Python database are not migrated.
Derived vectors, like other SQLite data, are unencrypted and deletion is not secure erasure.

## What has and has not been validated

The counts below record the earlier semantic slice. Current file-provenance checks are reported
[separately](FILE_PROVENANCE_CHECKS.md).

Rust tests cover semantic ranking with no keyword overlap, hybrid fusion/union, scope isolation,
expiration, budgets/thresholds, restart persistence, model isolation, vector validation, provider
failure atomicity, update/deletion invalidation and concurrent stale backfill. Real CLI/MCP and
both SDKs exercise opt-in retrieval with a deterministic executable fixture. Process-adapter tests
exercise malformed/oversized responses, failure and deadlines.

Local macOS validation passed: 75 Rust tests, 15 Python SDK tests (Python 3.10 and 3.14),
and 15 TypeScript SDK tests (Node 26), with real SDK integration required and no skips.
Both SDK suites also passed against the release binary. Formatting, Clippy, mypy, Ruff,
TypeScript typecheck/build, Python wheel/compiled Node package smoke, release repository/watch
smoke, and updated local Markdown links passed. No new mandatory dependencies were added.

The fixture is **not an embedding model** and does not establish retrieval quality. No downloaded
model, real-model retrieval benchmark, large-store performance claim, remote CI run, package
publication or assistant-specific integration is implied. The sentence-transformers reference
adapter is optional and has not been validated against a downloaded model in this checkout.
