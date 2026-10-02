# Open-source memory for coding agents

## 1. Product decision and current status

**TinyMem is an open-source memory layer for coding agents, not a chatbot memory platform or a new agent.**
Help agents preserve decisions, corrections and repo conventions, avoid repeated mistakes, and reuse
current evidence under a small context budget. Keep the MIT-licensed native core local, independently
usable and hostable by its operator, with no mandatory cloud account, agent vendor or inference provider.
Self-hostable here does not imply that an authenticated network service already exists.

This roadmap supersedes the previous gateway-first product sequence. Pi remains our initial coding-agent
evaluation runtime. The existing Cloudflare integration retains its strict gateway-only policy for runs
that select it; do not weaken it or silently fall back to other providers. It is not a core adoption or
release dependency. The [previous detailed Pi/gateway plan](PI_GATEWAY_PLAN.md) is retained as a scoped
integration reference; its task priorities are no longer the product backlog. See [ADR 0003](adr/0003-open-source-coding-agent-memory.md).

### Implemented foundations, not proof of product outcomes

- Rust/SQLite core; exact user/agent/project scopes; stable IDs and optimistic updates.
- Explicit writes, normalized exact-input deduplication, expiry, deletion and immutable observations.
- Incremental repository indexing with current path/line/hash evidence.
- [Structured local file references and read-time freshness](FILE_PROVENANCE.md), including explicit
  origin/dependency roles, stale/unknown exclusion across current reads, and historical inspection.
- Lexical and optional semantic/hybrid memory retrieval; provenance-preserving context assembly.
- Eleven native stdio MCP tools and standalone Python/TypeScript SDKs.
- Pi 1.0.0 explicit live MCP lifecycle; optional local Pi context/command extension with model-free
  real-runtime tests, setup helper and [initial synthetic latency traces](PI_LOCAL_CHECKS.md).
- Model-free Pi MCP discovery and deterministic Cloudflare adapter tests.

### Important gaps

Memory `source` remains free text; only explicit `source_refs` create tracked file dependencies.
Commit/conversation references, PR resolvers, persistent validation logs and reference-reapproval are
not implemented. Context budgets use a conservative byte counter over MCP/SDKs, not a target tokenizer.
There is no write-quality gate, semantic merge, structured supersession, complete portable export,
team/org authorization, verified Claude Code/Cursor plugin, or published coding-quality/latency result.
The legacy Python company-brain prototype does not fill these gaps.

## 2. Target architecture and module ownership

```text
Claude Code / Cursor / Pi / other coding agents
                 |
          MCP / native SDKs
                 |
       TinyMem local Rust core
       +-- Evidence lifecycle: sources, versions, freshness, corrections
       +-- Write policy: proposals, deduplication, explicit acceptance
       +-- Context selection: scoped eligibility, ranking, token packing
       +-- Repository evidence: refresh, paths, hashes, citations
       +-- Portability: export/import, deletion, audit, purge
                 |
              SQLite

Optional: trusted source resolvers and model adapters
Later: authenticated sharing boundary for team/org access
Independent: reproducible coding-memory evaluation using existing agents
```

Keep source resolution, optional inference and maintenance out of the critical writer transaction.
Revalidate dependency versions at commit. Reads never create durable memories or accept proposals.
Cache invalidation must cover every current retrieval surface, not only `assemble_context`.

Preserve exact scope semantics, stable IDs, transactional migrations, schema identity checks,
optimistic concurrency, expiry, bounded outputs and model-free defaults. No automatic hierarchy or
team inheritance is added to today's scope tuple. New public fields must be supported and tested across
Rust, MCP and both SDKs before claiming them.

## 3. Product contracts to implement

The following are target contracts, not a complete list of supported API parameters.
The shipped file-only subset is documented in [file provenance](FILE_PROVENANCE.md); other source
kinds and the broader policies below remain targets.

### A. Provenance and staleness

- A source reference identifies its kind (`file`, `commit`, `pull_request`, `conversation`), stable
  repository/provider/session identity, locator, captured revision or digest, and observation time.
  Line numbers and a URL alone are not a source version. Explicit owner notes remain valid origins;
  do not invent external provenance for legacy free-text sources.
- Distinguish an **origin** (where a decision was recorded) from a **validity dependency** (what must
  remain unchanged for it to apply). A decision recorded in a commit does not expire on every new HEAD.
  Commit content can remain immutable while a referenced file dependency changes.
- Freshness reports `current`, `stale` or `unknown`, with reason, last validation time and the versions
  checked. Freshness is not factual correctness. Missing credentials, timeouts and inaccessible PRs
  mean unknown—not proof that the underlying content changed or was deleted.
- Validate local dependencies automatically during refresh/recall. Check remote dependencies only
  through explicitly configured resolvers with bounded deadlines and a declared validation age.
  Never fetch arbitrary memory URLs or send private repository data merely to validate a note.
- Changed/deleted dependencies flag the memory stale and exclude it from default current context;
  an explicit per-memory policy may expire it instead. Unknown dependent evidence is excluded by
  default and inspectable with a warning. Standalone owner notes use their own correction/expiry policy.
- Source changes never silently rewrite a memory or destroy audit history. Revalidation/approval
  records which new evidence supports the next version. Branch switches, renames, ignored files,
  rebases, inaccessible sources and concurrent edits have explicit tests. No filesystem snapshot
  isolation is claimed where only a point-in-time hash check is possible.

### B. Compact context and quality per token

- Keep the existing `assemble_context` entry point. Return a ranked, deduplicated selection of eligible
  evidence with traceable citations, not an indiscriminate top-k dump or an unsupported summary.
- Enforce the cap on the entire injected prompt, including framing, escaping and provenance. Expose
  count method and selection/omission diagnostics separately; callers reserve the rest of their request.
- Add trusted target-tokenizer support to MCP/SDKs while retaining the labelled byte-bound default.
  Do not call a heuristic count billed tokens. Count transport duplication separately from injected text.
- Rank relevance and durable constraints without treating recency as truth. Skip oversized entries;
  never truncate away negation or the evidence's supporting source. Long task text must not be rejected
  merely because the short-query API accepts at most 32 words.
- Publish task success and evidence utility at fixed budgets; token savings count only alongside
  non-inferior task outcomes and zero budget/scope violations. See the [benchmark](CODING_MEMORY_BENCHMARK.md).

### C. Write-time quality

- Prefer atomic, durable decisions with rationale, corrections with the mistake and fix, conventions,
  tested solutions, and explicit developer preferences. Reject or propose rather than auto-store raw
  transcripts, secrets, transient tool output, guesses and facts trivially recoverable from current code.
- Preserve the explicit-write path. Automatic extraction is opt-in and produces inspectable proposals;
  recording a proposal is not accepting it as durable evidence. An explicit acceptance policy is required.
- Add exact-scope idempotency keys and normalized duplicate detection with visible outcomes such as
  accepted, duplicate, rejected or needs review. Retries must not create new observations accidentally.
- Safe merges retain every source and prior version. Near-duplicates and conflicting decisions require
  explicit resolution, not last-write-wins or silent model consolidation. Corrections supersede a
  specific memory/version; obsolete text disappears from all current reads, not from historical audit.
- Evaluate acceptance precision, missed durable items, duplicate/conflict rate and retained tokens.
  Include a store-everything baseline; reducing storage alone is not evidence of better memory.

### D. Latency

- Keep lexical recall and context assembly model-free. Measure before adding remote rerankers, vector
  infrastructure, persistent embedding workers or derived summary trees.
- Publish p50/p95/p99 for engine selection, source validation, persistent MCP calls, SDK process startup,
  and complete context assembly. Label cold/warm cache, corpus size, concurrent writers and hardware.
- Include repository refresh/hash work in end-to-end numbers. Never achieve a headline by silently
  omitting freshness checks or comparing warm in-process recall to someone else's network API.
- First collect baselines; then set versioned regression budgets. Sub-300ms competitor advertising is
  not our measurement or release guarantee. Claims must link to runnable workloads and raw results.

### E. Scoping, permissions and deletion

- Local v0.1 keeps exact user/repo-project/optional-agent namespaces and OS file permissions. Bind the
  root and scope at startup; use an explicit project ID rather than assuming a repository root scopes
  personal memory automatically. No unauthenticated remote listener.
- Team/org sharing is a separate post-local-release phase: authenticated actors, explicit membership,
  read/write/admin grants, revocation, audited decisions and server-enforced filtering before ranking.
  No caller-supplied owner ID or namespace is proof of identity. Do not reuse legacy company-brain ACLs.
- Every shared surface—search, history, export, proposals, caches and maintenance—must enforce the same
  authorization boundary. Cross-scope merges and derived caches must not leak denied evidence.
- Document current soft deletion versus planned authorized purge. Purge covers evidence, audit and
  derived caches according to a declared retention policy; backups/WAL and exported copies need their
  own erasure procedures. Audit exports can contain deleted sensitive text and require explicit consent.

### F. Portability and open-source trust

- Publish a versioned, documented JSONL interchange format and schema, independent of SQLite internals
  and any inference vendor. The core, MCP adapters, format, benchmark harness and redistributable fixtures
  belong in the open repository; no cloud service is needed to read exported evidence.
- Export explicit scopes and a consistent snapshot with IDs, versions, kinds, content, source references,
  lifecycle/freshness metadata and checksums. Default to current evidence; historical/deleted observations
  require an explicit audit export mode. Derived embeddings are optional rebuildable caches, not truth.
- Import validates versions, limits and checksums; offers a dry-run; maps scopes explicitly; and never
  widens permissions, revives deleted evidence or overwrites a conflicting version silently. Treat source
  freshness as unknown until revalidated locally. Export/import must not trigger inference or URL fetches.
- Test clean-store round trips, complete audit mode, >100 versions, interrupted operations, duplicate
  imports, incompatible schemas and malicious archives. Keep database backup/restore distinct from the
  open format; a SQLite file alone is not the promised portability interface.

## 4. Coding-agent integrations are release gates

Ship one memory engine and thin adapters, not separate stores per assistant. Native MCP is the common
contract; agent-specific packages supply configuration, context injection and explicit remember/correct
workflows where supported. A generic MCP config example is not a verified plugin.

For the first public coding-agent release:

- Native MCP with a clean-machine install path and versioned release artifacts/checksums.
- Installable Claude Code plugin and Cursor-supported integration package, documented against pinned
  client versions. Verify the actual client's extension/configuration mechanism before choosing package
  layout; do not invent a plugin API or treat Claude Desktop configuration as Claude Code installation.
- Each passes install → remember → new session → budgeted recall → correction → source change →
  deletion → uninstall on supported platforms. Uninstall must not erase the database without consent.
- Pi discovery stays a tested reference path; its lifecycle extension and additional MCP clients follow
  the same contract. Publish tested versions/platforms, unsupported features and troubleshooting steps.
- Writes remain intentional; no silent capture of every session, secrets or abandoned branches. Inject
  memory as untrusted evidence. Expose source/freshness and a way to inspect or disable injected context.

Current instructions and honest support status live in [MCP_SETUP.md](../MCP_SETUP.md). Verified native
binaries, package signing/checksums and assistant-specific testing are pending, not accomplished by this plan.

## 5. Reproducible benchmark, not vendor theatre

The [coding-memory benchmark protocol](CODING_MEMORY_BENCHMARK.md) is the release contract for claims.
Start with repeated-mistake avoidance, repo conventions, decision recall after 50 intervening sessions,
source invalidation and cross-scope/deletion safety. Use existing agent runtimes, deterministic code tests,
paired equal-budget baselines and held-out temporal tasks. Publish failures, variance, artifacts and cost.

Separate storage/protocol correctness, retrieval microbenchmarks, and end-to-end coding outcomes. An MCP
smoke test is not evidence that an agent naturally uses memory. LongMemEval/LoCoMo may provide additional
conversational checks, but cannot substitute for coding-task results. Do not copy unlicensed datasets.

## 6. Ordered implementation backlog

All unchecked items are pending; documentation does not complete a runtime milestone.

| Order | Slice | Completion evidence |
| --- | --- | --- |
| 0 | [x] Coding-only positioning and release contracts | Native-first landing docs, explicit gaps, benchmark protocol and contribution guide |
| 1 | [ ] Baseline harness and launch scaffolding (local Pi scaffolding/latency slice shipped) | Pi helper, source package and offline lifecycle checks ship; paired natural coding fixtures, no-memory/native outcomes and clean-client packaging remain pending |
| 2 | [ ] Structured sources and freshness (file slice shipped) | v4 migration, file roles/hashes, current-read exclusion and change/delete/concurrency tests ship; commit/conversation references, PR resolver contract and broader validation remain pending |
| 3 | [ ] Write quality and corrections | Idempotency, proposals, explicit supersession and merge provenance; store-all ablation and duplicate/conflict tests |
| 4 | [ ] Context efficiency | Long-task handling, target-tokenizer transport, compact citations/diagnostics; exact whole-prompt cap tests and budget-sweep results |
| 5 | [ ] Portable lifecycle | Published JSONL schema; export/import dry-run and round trips; scoped purge/retention documentation and tests |
| 6 | [ ] Agent packages and native distribution | Claude Code/Cursor clean-machine lifecycle checks, checksummed native artifacts and uninstall docs; Pi smoke remains green |
| 7 | [ ] Public v0.1 evidence | Fixed held-out benchmark run, all failures/raw artifacts, latency/quality/token report and supported-version matrix |
| Later | [ ] Authenticated team/org sharing | Threat model, enforced grants/revocation, deny-case tests for every data surface and audited deletion/export |
| Only if measured | [ ] Optional extraction, summary caches and deeper retrieval | Ablation showing utility or cost improvement without freshness, scope or retention regressions |

Installation work and measurement start alongside the core work; neither is an afterthought. No cloud
credential blocks lexical development, client packaging, export or deterministic tests. Live runs using
the Cloudflare adapter still require explicit credentials/data policy and existing no-fallback controls.

### Immediate implementation slice

The local file-provenance runtime slice now ships across Rust/MCP/SDKs, with migration and deterministic
correctness tests. This does not complete the temporal agent fixture, latency measurements or plugin
verification below; see [actual shipped behavior and limits](FILE_PROVENANCE.md).

1. Add a minimal temporal fixture: an agent repeats a previously corrected bug without memory; the
   remembered correction names a source file/version and is recalled in a fresh session.
2. Introduce structured file provenance and read-time freshness without changing existing scope semantics;
   mutate/delete the source and prove the saved note is no longer returned as current.
3. Exercise it through native MCP and both SDKs, including migration, legacy free-text compatibility, scope and
   audit behavior. Measure complete recall latency before and after the change.
4. Use this vertical slice as the first Claude Code/Cursor install smoke and benchmark case, not as a
   statistically meaningful product result.

## 7. Non-goals and non-negotiable gates

Do not build a new agent loop, chatbot platform, social/personal-profile product, external vector store,
graph service, distributed storage or mandatory summary tree. Freeze the Python product path. Delay
enterprise marketing until authorization—not namespacing—has been tested.

Release gates: no observed cross-scope exposure, stale/deleted/expired evidence presented as current,
unapproved durable writes, budget overflow or secret leakage in the adversarial suite; passing native/MCP/
SDK tests; validated export round trips; verified day-one integrations; public reproducibility artifacts.
A finite suite is not proof of universal safety. Label known limitations and failed runs. Do not invent
performance guarantees, assert superiority from vendor numbers, or make a benchmark green by weakening
source validation, token accounting or scope enforcement.
