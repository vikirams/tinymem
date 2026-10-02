# turbopuffer and TinyMem agent memory

Checked against official documentation on 2026-10-01. This is research and a proposed direction,
not an implemented integration or measured performance comparison.

## Recommendation

Build **context assembly next**, with a real-model retrieval evaluation alongside it. Keep SQLite
as the local-first default. Consider turbopuffer as an optional remote retrieval adapter when
measured corpus size/concurrency, managed inference needs or multi-machine deployment justify it.
It is not a replacement for TinyMem's memory lifecycle and prompt-selection policy.

TinyMem currently has opt-in semantic/hybrid personal-memory retrieval, but semantic search scans
scoped vectors exhaustively; repository search remains lexical. Current tests validate retrieval
mechanics using deterministic vectors, not real-model relevance. See [the implemented slice](../SEMANTIC_RETRIEVAL.md).

## What turbopuffer can provide

| Capability | Benefit for agent memory | Primary source |
| --- | --- | --- |
| ANN vector search; exact kNN over filtered subsets | An indexed alternative to scanning every scoped vector as the corpus grows; actual latency/recall must be evaluated | [Query docs](https://turbopuffer.com/docs/query) |
| BM25 + vector multi-query + server-side RRF | Hybrid retrieval for concepts and exact identifiers such as error strings and symbol names | [Hybrid guide](https://turbopuffer.com/docs/hybrid) |
| Native embeddings on writes and queries, or caller-supplied vectors | Can remove the need to operate a separate embedding executable; existing vectors must use a compatible model | [Embedding docs](https://turbopuffer.com/docs/embedding) |
| Single-namespace queries and metadata filtering | Building blocks for fixed owner/agent/project scopes, kinds and expiration filtering; not application authentication | [Query docs](https://turbopuffer.com/docs/query) |
| Object-storage-backed indexes, caching and autoscaling | Managed search infrastructure for larger/shared deployments | [Architecture](https://turbopuffer.com/docs/architecture), [Guarantees](https://turbopuffer.com/docs/guarantees) |

Native embeddings are generally available according to the September 2026 changelog, not merely
an unannounced/beta capability. [Embedding changelog](https://turbopuffer.com/docs/embedding/changelog).

These features concern embedding/indexing/retrieval. They do not implement our proposed conversation
fact extraction, controlled memory writes, supersession/conflict policy, memory audit model, or
selection of prompt context under a token budget. That division is an architectural assessment of
TinyMem's needs, not a claim that turbopuffer markets a complete agent-memory framework.

## Proposed placement

```text
explicit writes / future approved extraction
                  |
        authoritative TinyMem evidence + audit
                  |
        durable, versioned indexing outbox
                  |
     local retrieval OR turbopuffer remote index
                  |
      candidate IDs + evidence versions
                  |
  TinyMem revalidates scope/version/deletion/expiry
                  |
  context assembly + source provenance + token budget
                  |
                assistant
```

This is a proposal, not current code. A remote adapter belongs at the **retrieval seam**, not only
the existing `Embedder` seam: turbopuffer can own ANN/BM25 and query embedding together. Preserve
the existing memory CRUD contract and default local behavior. Do not add a generic vector-database
abstraction until actually implementing a second adapter.

For a future integration:

1. Preserve exact tuple scopes, including omitted agent/project dimensions. Use appropriate tenant
   namespaces plus mandatory scope filters on every retrieval branch. Keep API credentials server-side.
2. Mirror stable IDs, current memory versions, content, kind, provenance and expiration. Namespace or
   separate embedding spaces by model identity/dimensions; never mix incompatible vectors.
3. Commit authoritative memory/audit changes before durable queued indexing; make upsert/delete retries
   idempotent and version-aware. SQLite and a remote HTTP write do not form one atomic transaction.
4. Rehydrate and validate remote candidate IDs/versions against authoritative evidence before returning
   anything to the assistant. Handle missing/stale candidates, network failure and index lag explicitly.
5. Keep personal memories and repository chunks as distinct evidence types. Continue repository path,
   line-range and source-hash validation even if chunk retrieval becomes semantic.
6. A genuinely shared multi-machine memory system also needs a shared authoritative store/write
   coordination. A remote search index alone does not synchronize independent SQLite histories.

## Tradeoffs to check before adoption

- **Paid, network-dependent operation.** The current pricing page lists Launch at **$16/month minimum
  usage** and Scale at **$256/month minimum usage**; actual usage beyond the minimum is billed.
  Embedding has separate model/token pricing. Recheck pricing before deciding.
  [Pricing](https://turbopuffer.com/pricing), [Embedding pricing](https://turbopuffer.com/docs/embedding#pricing).
- **Data leaves the local machine.** Native embeddings store source text and vectors in turbopuffer,
  and send input to external inference partners. The docs promise partner zero data retention and no
  training, but this is not equivalent to offline processing or keeping source text out of turbopuffer.
  Review sensitive code/memory handling and subprocessors before exporting data.
  [Embedding limitations/security](https://turbopuffer.com/docs/embedding).
- **Model migration is still our job.** Changing `embed` affects future writes; existing records must
  be re-ingested. Inputs beyond a model's context length are truncated by default. Supplied vectors
  are not checked for semantic compatibility with the configured model.
  [Embedding docs](https://turbopuffer.com/docs/embedding).
- **Remote consistency is not local transactionality.** The guarantees document describes durable
  successful writes, atomic batches/conditional writes, read-consistency caveats, and no general-purpose
  read/write transactions. Choose consistency deliberately and retain lifecycle validation locally.
  [Guarantees](https://turbopuffer.com/docs/guarantees).
- **Measure, don't inherit vendor latency claims.** Compare recall/ranking, scope leakage, stale-memory
  exclusion, cold/warm latency, write/query inference latency and cost on our own workloads. ANN and a
  different embedding model do not automatically improve relevance over the local implementation.
  Vendor architecture/performance figures are not TinyMem benchmarks.
  [Architecture](https://turbopuffer.com/docs/architecture), [Model-selection guidance](https://turbopuffer.com/docs/embedding#models).

## Concrete next task

Design and implement `assemble_context(query, scope, token_budget)` through Engine/MCP/SDKs. It should
select stable preferences and relevant dynamic/project evidence, deduplicate, exclude stale/expired/
deleted records, retain IDs/versions/provenance, and pack the actual model-token budget with an explicit
tokenizer strategy. Any broader-scope inclusion must be authorized/explicit, not accidental wildcard
inheritance. Test the prompt-facing result with realistic agent tasks and a real embedding model.

No turbopuffer account, API calls involving private memory, integration code, or cloud benchmark was
created/run as part of this research.
