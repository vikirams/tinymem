# Local file provenance and freshness (native schema v4)

This slice ships in Rust, native MCP, and both SDKs. It is explicit, local and model-free.
It does not add extraction, Git/PR resolvers, plugins, summaries or a benchmark runner.

## Save an explicit reference

MCP `add_memory` accepts an optional `source_refs` array, alongside the unchanged free-text `source`:

```json
{
  "content": "Use UTC for persisted clock values; the earlier local-time fix was incorrect.",
  "kind": "issue_solution",
  "source": "Explicit developer correction",
  "source_refs": [{"kind": "file", "role": "dependency", "path": "src/clock.rs"}]
}
```

The file must already exist under the server's fixed startup repository. Each input requires
`kind: "file"`, `path`, and an explicit `role`:

- `origin`: records where evidence came from. Subsequent file edits do **not** invalidate the note.
- `dependency`: the captured file bytes must still match for the note to be used as current.

A file may appear in both roles. No role, path or dependency is inferred from `source`, content,
metadata or legacy records. An optional `expected_hash: "blake3:<64 lowercase hex digits>"` adds a
save-time precondition; it does not let callers supply a fabricated capture. MCP rejects repository,
captured-hash and observation-time overrides in reference inputs.

The engine captures this immutable reference in the returned memory and every subsequent observation:

```text
{kind, role, repository, path, content_hash, observed_at_ms}
```

`repository` is the canonical absolute local root; `path` is a normalized relative locator;
`content_hash` is BLAKE3 over the entire file's raw bytes, prefixed with `blake3:`. This is not a
Git revision or line-level digest. `observed_at_ms` is the capture's Unix-millisecond timestamp.
Context memory citations also retain the complete `source_refs` array.

Rust callers capture before adding:

```rust
use tinymem_engine::{FileSourceReference, MemoryKind, NewMemory, SourceRole};

let reference = FileSourceReference::capture(root, "src/clock.rs", SourceRole::Dependency)?;
let memory = engine.add_memory(&scope, NewMemory {
    content: "Use UTC for persisted clock values".into(),
    kind: MemoryKind::IssueSolution,
    source: "Explicit developer correction".into(),
    expires_at_ms: None,
    source_refs: vec![reference],
})?;
```

The new Rust `NewMemory.source_refs` field must be supplied in struct literals (`vec![]` for plain
notes); omitted JSON fields default to an empty array. `MemoryInput` compatibility input is unchanged.

Python / TypeScript:

```python
saved = client.add_memory("Use UTC for persisted clock values", source_refs=[
    {"kind": "file", "role": "dependency", "path": "src/clock.rs"}
])
```

```ts
const saved = await client.addMemory('Use UTC for persisted clock values', {
  source_refs: [{ kind: 'file', role: 'dependency', path: 'src/clock.rs' }],
});
```

## Current reads and historical inspection

Every current-memory path checks dependencies: `get_memory`, lexical recall, semantic/hybrid search,
both Rust profile APIs, and context assembly (with or without a repository). Checks happen before
result budgets/candidate caps; stale candidates cannot fill the top-100 pool and hide current notes.
SQLite snapshots protect database ranking/hydration; filesystem checks are point-in-time only.

Changed hashes produce **stale / `content_changed`**; a missing path under an accessible root produces
**stale / `file_missing`**. Renames are missing paths, not automatically followed. Permission/read
errors, inaccessible roots, symlinks, nonregular or oversized files, and detected read races produce
**unknown / `source_unavailable`**. Both stale and unknown dependent memories are excluded by default.
Origin-only and legacy notes remain eligible under their existing expiry/deletion policy; their
freshness is **current / `no_dependencies`**, not a claim that a free-text source was verified.

Inspect an excluded memory using `memory_history` (Rust: `Engine::memory_events`). Each event retains
its immutable `memory` snapshot and adds a separate, newly evaluated `freshness` object:

```text
{status, reason, checked_at_ms, checks: [
  {source_index, expected_hash, observed_hash, status, reason}
]}
```

`source_index` addresses that snapshot's `source_refs`. Each dependency reports `hash_matches`,
`content_changed`, `file_missing`, or `source_unavailable`. Aggregate reasons are `dependencies_current`,
`no_dependencies`, or the first stale/unknown check's reason; known stale takes precedence over unknown.
`checked_at_ms` is the inspection time, or null when there are no dependencies. `observed_hash` is null
when no digest could be read. Rust can also call `MemoryRecord::freshness()` on historical records.
Freshness checks do not override expiry/deletion and do not establish factual correctness.

Validation does not modify memory content, timestamps, versions, embeddings or audit events. History
freshness is evaluated **now**, not the status at the original event time; repeated inspections can
have different validation times. The immutable snapshot and captured hash do not change. No persistent
validation log is shipped. Restoring the exact captured bytes restores dependency eligibility; there
is no latched invalidation, automatic rewrite, reapproval or silent rebinding to a new hash.

## Writes, migration and limits

- Exact user/agent/project scope rules are unchanged. The capture's repository identity does not
  implicitly change a memory's scope. Rust may explicitly reference more than one local repository;
  MCP/SDK capture is limited to the configured root.
- All captures, including origins, must still match when first saved. Capture/optional provider work
  precedes the writer transaction; bounded hash revalidation after obtaining the writer lock rejects
  files changed while waiting. SQLite and the filesystem do not share an atomic commit.
- Dedup includes ordered reference identities, roles and hashes, but excludes capture timestamps.
  Retries retain the original ID, observation time and history. Distinct hashes/roles/roots do not
  merge. Plain-note dedup is unchanged, including expired-note behavior.
- Updates retain source references, stable IDs and optimistic version checks. Editing stale content
  does **not** recapture its dependencies. This milestone has no reference-rebinding/correction API;
  explicitly delete the obsolete memory and add a new capture if needed. Deletion remains soft audit
  retention, not secure erasure.
- Native schemas v1/v2/v3 migrate to v4 in the existing immediate schema transaction. A new dedup key
  defaults to `[]`; old input JSON and audit snapshots are not rewritten. Old records deserialize
  with no structured references. Foreign/newer schemas are rejected, including after lock waits.
- At most 16 references per memory, 4,096 UTF-8 bytes per locator/root, and 1 MiB per file. Relative
  paths use `/`; absolute paths, `..`, `.`, empty components, backslashes, NUL and symlinks are refused.
  Explicit references can name ignored/hidden files or any file extension; they are independent of
  repository-index inclusion rules. File bytes are hashed, not copied into memory or sent remotely.
- No watcher or persisted freshness cache. Repository refresh reconciles repository chunks; memory
  reads validate referenced paths directly, even without a preceding refresh. Each validation hashes
  matching dependencies again; work scales with candidates and referenced bytes, not just output size.
  Local IO has no engine deadline or filesystem snapshot/sandbox guarantee. SDK process deadlines still
  apply. Large-store latency, network filesystems and hostile filesystem races are unmeasured.

## Verification

Tests include unchanged/changed/deleted/restored and renamed files, inaccessible/oversized/nonregular
sources, POSIX permissions and symlinks, ignored explicit references, exact scopes, reference dedup,
all retrieval modes, immutable history, restart, genuine v3 migration and migration rollback. Writer-lock
barriers exercise edits/deletions/unavailability during a save; optimistic concurrent updates still
produce one winner. MCP and real-binary SDK tests cover capture, current reads, change/delete/unknown,
provenance-bearing context and history. These are deterministic engine/protocol tests, not coding-agent
outcome or latency benchmarks.

Commands and local results are recorded in [the milestone verification report](FILE_PROVENANCE_CHECKS.md).
