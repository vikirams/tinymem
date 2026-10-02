---
status: proposed
---

# Derived summaries, not memory replacement

If measured startup/history pressure justifies OptMem-inspired hierarchical compression, keep native memory observations authoritative and implement summaries as optional, rebuildable caches tied to exact scoped memory IDs and versions. Do not replace SQLite with fixed-width logs, silently overwrite evidence with generated text, or block startup on unfinished model work.

This costs dependency tracking, current-state validation and explicit maintenance, but preserves corrections, deletion/expiration semantics and access to raw evidence when compression loses detail. Default-on adoption requires measured utility/cost and drift results; the reviewed OptMem snapshot has no identified reuse license, so implementation must be independent. See [the retained summary design](../PI_GATEWAY_PLAN.md#optional-optmem-inspired-summary-tree) and [current product gates](../MEMORY_SYSTEM_PLAN.md). Summaries remain deferred until coding-memory measurements justify them.
