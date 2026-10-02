# Memory-layer options: mem0 and Supermemory

Researched 2026-09-30 using public official documentation via `web_fetch` and public, read-only GitHub API requests via `gh`. Scope: an open-source, lightweight, local-first layer combining personal memory with continuously refreshed repository context. No installations, runtime measurements, or language/stack benchmark comparisons.

## Conclusions

- **Mem0 is a useful library-level reference for personal memory:** scoped fact extraction, configurable providers, explicit CRUD/history, and optional raw ingestion. Local operation requires deliberate provider configuration; its defaults call OpenAI. It is not a complete repository-refresh system. [M1–M4]
- **Supermemory provides a richer context model:** separate source documents and extracted facts, stable/dynamic profiles, temporal relationships, and combined document/memory retrieval. Current docs also advertise a local server, not merely a hosted API. However, its local engine's complete source/build provenance and binary licensing remain unverified here; the public monorepo's MIT license alone does not establish these. [S1–S5, S9–S10]
- **Design inference for TinyMem:** borrow these interfaces and lifecycle ideas without importing either entire stack. Neither the reviewed mem0 OSS documentation nor Supermemory's hosted GitHub connector establishes continuous indexing of local, uncommitted source code. Build local refresh explicitly; preserve the embedded-storage/minimal-dependency goals documented in this workspace's `README.md` and `ARCHITECTURE.md`.

## Evidence and tradeoffs

| Concern | mem0 | Supermemory | Implication for a small local layer |
| --- | --- | --- | --- |
| Personal memory | OSS `Memory` supports `user_id`, `agent_id`, `run_id`, extraction instructions, metadata, expiration, CRUD and per-memory history. `infer=False` skips extraction but can introduce duplicates. [M2–M4] | Profiles distinguish stable facts from recent activity; graph relations include `updates`, `extends`, `derives`, with current-state retrieval and forgetting. These are documented product behaviors, not locally tested results. [S1–S2] | Keep explicit preferences separate from temporary state; retain provenance and confidence, especially for inferred facts. |
| Local-first | Library defaults: local Qdrant vectors and SQLite history, but OpenAI extraction and embeddings. Ollama/local embedding alternatives are configurable. SQLite history is not the whole memory store. [M1–M2] | Local docs describe one binary, embedded graph storage, local embeddings, and offline operation with an OpenAI-compatible local LLM. Profiles/hybrid search are advertised locally too. [S3–S4] | Self-hosted does not automatically mean offline; account for model downloads, model RAM and configured outbound providers. |
| Scope and retrieval | User/agent/run identifiers plus custom metadata; project/org management is not an OSS feature. Entity-aware ranking is documented, but current OSS lacks a queryable graph. [M3] | Documents/chunks ground answers while extracted memories personalize them; `containerTag` scopes retrieval. Local is single-tenant with one API key, not Enterprise role/scoped-key management. [S2, S5–S6] | Separate user and repository namespaces; explicitly compose allowed results rather than mix every source into a personal profile. |
| Repository freshness | General add/update/delete operations are integration primitives; no local worktree watcher or repository reconciler was established in reviewed OSS sources. [M3–M4] | Hosted GitHub connector incrementally syncs documentation, handles changed/deleted files, and batches webhooks with an approximately 10-minute delay. It requires Scale/Enterprise and excludes code files by default. Connectors are absent from the local binary. [S3–S4, S7] | Borrow incremental reconciliation, not the assumption that a hosted documentation connector solves live code context. |
| Lightweight operation | Library mode avoids the full server stack, but extraction, embeddings and vector storage still have costs; graph/decay/temporal reasoning/Dream are Platform-only in current docs. [M1–M3] | Local ingestion queues and concurrency/RAM controls are documented. The default ingestion allowance is 1 GB above boot baseline, not a total-RAM guarantee. Release `server-v0.0.8` lists a Darwin ARM64 binary of 270,741,952 bytes. [S4, S10] | One binary is operationally simple, not proof of a tiny footprint. Keep heavy extraction optional and reads independent of refresh jobs. |

## Ideas to borrow, not functionality to assume

These are proposed TinyMem design choices, not claims that either vendor implements this exact local workflow:

1. **Two complementary context channels:** a small, always-available personal profile plus query-specific repository chunks, each with separate budgets and scopes. Profiles should not depend on a semantic query happening to match a preference. [S1–S2]
2. **Stable source identity and explicit freshness:** track repository/worktree, branch, relative path, content hash, indexed revision/time and chunk ranges. Re-index only changed files; reconcile deletes, renames and branch switches. Debounce local filesystem events and periodically rescan to recover missed events. Stable document IDs and update/delete operations are useful reference primitives. [S5, S7–S8]
3. **Truth versus history:** immutable personal-memory observations can coexist with a current-state view, but stale repository chunks must stop appearing as current evidence. Invalidate derived summaries when their source changes; do not silently promote `derives` to verified code facts. [S2]
4. **Observable asynchronous refresh:** expose queued/failed/ready states and an indexing watermark. Supermemory distinguishes document indexing completion from later memory extraction under dynamic dreaming: `done` need not mean profiles are fully refreshed. [S5]
5. **Local safety boundaries:** honor ignore rules, exclude credentials and generated/binary files, keep repository text as untrusted data, and require explicit opt-in before remote extraction. These are requirements for our design, not verified vendor guarantees.

## Runtime recommendation for TinyMem

This is an engineering recommendation, not a benchmark result. No language is established as fastest for this workload by this research.

- **Rust core:** preferred for a long-running local memory/indexing engine where predictable resource use, native parser integration and no garbage collector are priorities. Tradeoff: a steeper contributor learning curve and more involved native packaging.
- **Go alternative:** reasonable if simpler implementation, concurrency and contributor onboarding outweigh maximum control over allocations; a native parser/SQLite choice may introduce CGO and complicate cross-compilation.
- **Python:** retain as an SDK and possible model worker. SQLite, NumPy and model kernels already execute native code, so Python orchestration alone is not evidence of poor end-to-end performance. This checkout makes sentence-transformers mandatory and loads its model during generator initialization; measure that cost separately from storage/retrieval.
- **TypeScript:** useful for an SDK and integrations, rather than the initial CPU-heavy indexing implementation.

Suggested arrangement: one Rust process owns SQLite/FTS5, source-versioned repository indexing, scoped memory lifecycle and token-budgeted context retrieval; expose MCP and optional loopback HTTP, with Python/TypeScript SDKs. Use Python-based inference only as an optional worker, not a requirement for basic indexing or retrieval. Syntax parsers recover structure but do not establish complete semantic understanding; resolve references conservatively and retain uncertainty.

Before a rewrite, benchmark cold start, idle/peak RAM, lexical retrieval p50/p95, full repository indexing, incremental edit-to-search freshness, disk growth and concurrent access. Separate model/provider latency from engine latency; use fixed datasets, hardware and offline tests. Migrate one vertical slice at a time and preserve the existing Python interface where practical.

## Licenses and copying code

Root license texts were decoded and read, not inferred from marketing or GitHub labels:

| Public source | Verified root license | Scope/caveat |
| --- | --- | --- |
| `mem0ai/mem0`, inspected main commit `94c3fe9f238f3dbf29c9ce98643bd71eb13077cd` | Apache-2.0; LICENSE blob `d20d5102c3cf97ecbee54afd65893de4a11d26fe` | Applies to covered source; not permission to reuse proprietary hosted implementations. [M5] |
| `supermemoryai/supermemory`, inspected main commit `ce4facf662b278713578450f704ab2d7d11b1865` | MIT; LICENSE blob `7b20e33c1b3d277e200ff1872a109c6d9304bacc` | Verified for covered monorepo source, not every plugin, dependency, model or downloadable engine artifact. [S9] |

The official Supermemory source shortlink resolves to the same repository, but its contributor guide starts local apps against `https://api.supermemory.ai`. The inspected public tree includes UI/integration/docs packages; `packages/memory-graph` is a visualization component, not evidence of the storage engine. Complete engine source and a reproducible local-server build were not established. Treat docs' “open source” local-server description as a vendor assertion until that provenance is resolved. [S3, S9–S11]

**Borrowing ideas:** independently implement scoped APIs, profile/document separation, provenance, queues and freshness rules; citation records inspiration. **Copying code:** pin the exact file/revision and inspect its own license and dependencies. Apache-2.0 redistribution requires license/notice preservation, applicable NOTICE attribution and modification notices; MIT requires retaining its copyright/permission notice. TinyMem's existing MIT license does not erase upstream obligations. Proprietary extraction models are not granted by either repository's root license. This is engineering guidance, not legal advice. [M5, S3, S9]

## Performance claims and unresolved checks

- Mem0's README reports April 2026 latency p50 values around 0.88–1.09 seconds for specified memory evaluations and explicitly attributes its scores to the **managed platform**, including proprietary optimizations unavailable in OSS. These are vendor-reported results, not a local-library latency target. [M6]
- Supermemory's profile docs advertise **50–100 ms** versus **200–500 ms** for search-only retrieval. These are vendor claims without measurements on this project/hardware; do not infer equivalent local extraction quality or end-to-end latency. [S1]
- Before adopting code/runtime, verify Supermemory local engine licensing/build provenance; pin provider/model versions; test offline behavior, RAM/startup cost, ingestion-to-search/profile freshness, stale-source invalidation and cross-scope isolation. No runtime capability or performance was verified in this research.
- Workflow limitations: background-agent capability unavailable (research performed directly); workspace has no `.git` metadata (no Git diff verification). Neither blocked writing this note. No credentials were requested, accounts created, hosted memory writes performed or private URLs accessed; `gh` was used only for public read-only endpoints.

## Primary sources

- [M1: Mem0 OSS overview/defaults](https://docs.mem0.ai/open-source/overview)
- [M2: Mem0 OSS configuration/providers](https://docs.mem0.ai/open-source/configuration)
- [M3: Mem0 Platform versus OSS](https://docs.mem0.ai/platform/platform-vs-oss)
- [M4: Mem0 add/extraction/raw mode](https://docs.mem0.ai/core-concepts/memory-operations/add)
- [M5: Mem0 pinned LICENSE](https://github.com/mem0ai/mem0/blob/94c3fe9f238f3dbf29c9ce98643bd71eb13077cd/LICENSE)
- [M6: Mem0 pinned README/performance scope](https://github.com/mem0ai/mem0/blob/94c3fe9f238f3dbf29c9ce98643bd71eb13077cd/README.md)
- [S1: Supermemory profiles](https://supermemory.ai/docs/concepts/user-profiles.md)
- [S2: Supermemory graph memory/documents versus facts](https://supermemory.ai/docs/concepts/graph-memory.md)
- [S3: Supermemory local overview and hosted-only features](https://supermemory.ai/docs/self-hosting/overview.md)
- [S4: Local configuration/queues/resource controls](https://supermemory.ai/docs/self-hosting/configuration.md)
- [S5: Pipeline, stable identity and dreaming readiness](https://supermemory.ai/docs/concepts/how-it-works.md)
- [S6: Local versus Enterprise auth/team boundaries](https://supermemory.ai/docs/self-hosting/local-vs-enterprise.md)
- [S7: Hosted GitHub connector scope and refresh delay](https://supermemory.ai/docs/connectors/github.md)
- [S8: Document update/delete operations](https://supermemory.ai/docs/ingestion/document-operations.md)
- [S9: Supermemory pinned LICENSE](https://github.com/supermemoryai/supermemory/blob/ce4facf662b278713578450f704ab2d7d11b1865/LICENSE)
- [S10: Local binary release metadata](https://github.com/supermemoryai/supermemory/releases/tag/server-v0.0.8)
- [S11: Public contributor setup](https://github.com/supermemoryai/supermemory/blob/ce4facf662b278713578450f704ab2d7d11b1865/CONTRIBUTING.md) and [graph visualization package](https://github.com/supermemoryai/supermemory/blob/ce4facf662b278713578450f704ab2d7d11b1865/packages/memory-graph/package.json)
