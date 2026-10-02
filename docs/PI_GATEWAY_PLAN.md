# Pi/Cloudflare integration plan (retained reference)

**This is the previous gateway-first plan, retained for its integration design and task history.**
The [open-source coding-agent roadmap](MEMORY_SYSTEM_PLAN.md) supersedes its product scope, priority
labels and execution order. References below to the “product” or “all inference” now apply only to the
controlled Pi/Cloudflare path, not the independently usable core. No core feature, install or model-free
benchmark is blocked on gateway credentials. Original status notes are historical, not fresh verification.

## Decision and status

**Use Pi, not a new coding agent. Use Cloudflare AI Gateway for all enabled model usage. Keep TinyMem's native local memory engine.**

This is the replacement roadmap for the next product slices, grounded in the [code/source review](research/optmem-pi-cloudflare-review.md). Existing native behavior, model-free Pi MCP discovery, and the restricted Cloudflare model-access/embedding adapter with deterministic Pi/native/fake-HTTP tests are implemented. The Pi lifecycle extension, general coding-tool isolation/egress, startup selector, proposals, and summary tree remain **planned**, not shipped. Cloudflare credentials were absent: no live gateway has been provisioned/verified and no live model benchmark has run.

Success means a fresh Pi session can reliably reuse appropriate, current, scoped evidence and perform a coding task better than the same agent without memory, at an explainable token, latency, privacy, and dollar cost. A successful database test or MCP connection alone is not success.

## 1. Product scope

### Build now

- Local-first **personal and project memory for coding agents**, with explicit ownership and provenance.
- Real evaluation using Pi's existing agent loop, tools, sessions, compaction, and model transports.
- Model-free lexical retrieval as the offline default; every optional inference operation through an authenticated Cloudflare gateway.
- Dependable startup/task context, explicit writes, safe derived-cache maintenance, and observable retrieval outcomes.

### Preserve

Exact scope semantics, stable IDs/versions, optimistic concurrency, immutable audit, expiration, update/delete invalidation, current-source hashes, bounded outputs, schema identity/migrations, read-versus-write separation, and dependency-free native operation by default.

### Stop or defer

- **Do not build another coding-agent runtime, orchestrator, shell-tool suite, session store, provider streaming client, or compaction implementation.** Pi owns these.
- Freeze the legacy Python product path; do not market its unauthenticated HTTP interface as enterprise tenancy. Do not silently mix or migrate its database into native storage.
- No external vector database, Neo4j, distributed replication, company-brain RBAC, multimodal storage, learning-to-rank, or WebSocket server until measurements and a real product requirement justify them.
- No automatic transcript ingestion or summary generation enabled by default. Do not treat a model-generated sentence as independently verified evidence.

## 2. Target architecture and module ownership

```text
Scenario runner / developer
          |
          v
Pi (existing agent loop, tools, sessions, compaction, provider streaming)
    |                                    |
    | native stdio MCP                   | model requests
    v                                    v
TinyMem Engine                      Gateway model policy
    |                                    |
    +-- Memory evidence                  v
    +-- Repository evidence         Cloudflare AI Gateway
    +-- Derived caches                   |
    +-- Context selection                v
    +-- Explicit maintenance        Allowed upstream models
               |
               +-- embedding / optional summary work
                            through the same gateway policy
```

The diagram names logical modules, not new network services. SQLite remains the initial store. The Rust engine has no mandatory model/network client. Reuse its `Embedder` seam with a trusted gateway executable first; introduce a persistent/batched adapter only if measurements justify it. Pi uses its built-in MCP connection for session-long access; the existing SDK subprocess-per-operation behavior remains supported.

| Module | Small interface | Owns complexity |
| --- | --- | --- |
| Memory evidence | Add/get/search/update/delete/history/profile, existing native operations | Exact scope, validation, versions, dedup, observations, expiration; explicit future supersession/pinning |
| Repository evidence | Refresh/search | Exclusion, hashing, incremental reconciliation, grounded path/line citations; not personal facts |
| Context selection | Existing `assemble_context`; proposed token-budgeted startup context | Candidate retrieval, category budgets, provenance, current-state checks, packing; no inference required |
| Derived caches | Explicit index/rebuild/expand/invalidate operations | Embeddings and optional summaries with immutable model/recipe identity and evidence dependencies |
| Pi adapter | Existing MCP plus a small proposed lifecycle extension | Connect, inject bounded evidence, register commands, collect proposal/usage events; no replacement agent loop |
| Gateway model policy | Approved model configuration; one embedding/maintenance adapter interface | Endpoint/auth validation, operation allowlist, deadlines, redaction, metadata, usage, no direct fallback |
| Evaluation | Run a scenario with a chosen memory variant | Isolated fixtures/state, Pi execution, deterministic checks, trace metrics and reports; no independent agent implementation |

Keep the public memory interface small. Add maintenance/administrative operations separately rather than making ordinary retrieval callers learn tree algorithms, embedding cache state, and job leasing.

## 3. Evidence and derived data

### Current authoritative state

Native schema v3 stores current `memories` and immutable `memory_events`. Repository files/chunks are separate evidence tied to a canonical worktree and content hash. `memory_embeddings` is a versioned derived cache. Retain these; no fixed-width log or position-based replacement IDs.

### Next evidence slice

Add only fields justified by tested behavior: structured origin (session/turn/tool/event IDs), explicit pinned status, optional supersession link, and idempotency key. Preserve existing compatibility semantics and validate any new fields at every transport. Make new schema migrations transactional and reject incompatible newer stores.

A proposed extraction is not yet a memory. Owner-approved acceptance produces the evidence write; rejection is not a hidden delete. Record actor/origin and acceptance policy without inventing truth-confidence scores. A memory scope is a namespace, **not authentication**.

### Optional OptMem-inspired summary tree

Implement independently only after baseline evaluation identifies startup/history pressure:

1. Leaves reference **scoped memory IDs and current versions**. Assign a stable per-scope creation ordinal transactionally when creating a leaf; do not derive global ranges from random IDs or mix users. Updates retain their leaf identity and invalidate dependent summaries. Recent updates can still appear raw in the recent tail.
2. Cache nodes cover deterministic ranges of leaves; store child/member references, source-version digest, recipe version, model identity, creation time, usage, and explicit validity. Omit deleted/expired leaves; empty ranges contribute no facts. Membership policy changes also invalidate nodes.
3. Build bottom-up with bounded work. Start with a small raw-evidence block, then bounded child summaries. Preserve important names, negation, decisions, unresolved questions, and citations; periodically verify/rebuild against raw members to limit repeated-compression drift.
4. Current-context reads must revalidate scope, versions, deletion, and expiry of **all dependencies**, including descendants. A summary containing an invalid member is unusable until rebuilt. Expiry has no write trigger, so it needs a read-time eligibility check, not only an update trigger.
5. Claim a bounded job with a lease and dependency digest, generate **outside** the writer lock, then compare-and-save under the lock. Concurrent updates, double completion, crashes, stale jobs, and cancellation must not publish stale summaries.
6. Expanding a node returns its child nodes or scoped raw current evidence, with provenance and a stable cursor. Historical observations are available only through an explicitly historical read, visibly labelled as superseded/deleted/expired.
7. Invalidating/rebuilding a summary never mutates or deletes raw evidence. Deleting a memory suppresses it from every **current** retrieval path, including summary ancestors. A future purge removes audit and derived references too, with separate backup/WAL erasure caveats.
8. Missing/failed summaries never prevent startup: return a bounded raw fallback and a `maintenance_pending` diagnostic. Summary fallback does not imply silently falling back from a failed embedding operation; each configured policy remains explicit.

This is a derived-cache design, not a claim that the whole native store is event-sourced. The existing audit is useful, but an arbitrary old event range is unsafe for current context because it may contain superseded or deleted facts.

## 4. Retrieval and context policy

Use three paths rather than one mega-search:

- **Startup:** pinned active preferences/decisions, a bounded recent tail, and optional validated summaries of older evidence. Works with no query and no model. Age alone must not erase a durable constraint. Total budget includes framing and provenance.
- **Task:** existing lexical/hybrid memory plus current repository evidence. Separate bounded task text from deterministic selection of at most 32 retrieval terms; do not pass an arbitrary long user task to today's strict short-query interface.
- **Deep recall:** scoped lexical/hybrid lookup and progressive expansion to supporting evidence. History/expansion needs continuation, not just the latest 100 versions.

Reserve separate budgets for startup, task, conversation, tools, output, and optional summaries. Revalidate retrieved citations before use when source may have changed. Prefer current raw evidence over a summary when both are present; retain dependencies in the compact representation without repeating every long record in the prompt.

The existing UTF-8-byte counter remains a safe labelled default, not billed tokens. Add a trusted target-tokenizer policy to MCP/SDKs for better utilization, keeping exact whole-rendered-prompt counting. The Pi extension must account for the **whole model request**, not just TinyMem's budget. Direct MCP responses have their own transport cap and duplicate `prompt`/`entries`; bypass neither cap accidentally.

Contradictory facts need explicit supersession or a visible unresolved conflict. Ranking/recency is not proof of truth. Repository text, imported notes, summaries, and tool output are untrusted data, never privileged instructions.

## 5. Pi integration, without building a new agent

### Slice A: plain MCP baseline

Use a pinned Pi version and its existing native MCP support. Start TinyMem with fixed database, root, owner, and project; omit agent ID when deliberate sharing across assistants is wanted. A fresh Pi session reads a bounded profile and searches task evidence, then explicitly writes an approved note. Record both voluntary tool use and forced tool-protocol tests; the latter must not be mistaken for natural memory effectiveness.

**Already verified:** Pi 0.99.2 discovers all eleven native tools in an isolated configuration. [Setup and smoke commands](PI_GATEWAY_TESTING.md).

### Slice B: small lifecycle extension

- `session_start`: connect/check configured memory identity; prepare bounded startup evidence. Do not run models or remote extraction merely because a session was opened.
- `before_agent_start`: retrieve task context and inject it as untrusted evidence, not as authoritative system instructions. Use the task text from the event, bounded appropriately.
- Explicit commands/tools: inspect context, remember an atomic approved note, correct/supersede an existing memory, and run a bounded maintenance step.
- Before compaction: surface pending approved facts for explicit saving; do not convert Pi's entire compaction summary to permanent truth. After compaction, reestablish the bounded memory context without endlessly appending duplicates.
- Branch/session replacement: rebuild state from the active branch; do not mine abandoned branches for durable facts without approval. External accepted memory persists across branches intentionally and needs provenance to explain that.
- Subagents: submit proposals to the parent; do not receive direct write tools or owner-wide data. Enforce permissions in configuration/transport and OS isolation, not only in a prompt.
- Shutdown/reload: idempotent cleanup. Do not rely on shutdown-only writes for durability or start resources in the extension factory.

For deterministic evaluation use Pi's TypeScript SDK with explicit resources, models, settings, and in-memory sessions; add/bind its MCP extension explicitly because SDK sessions do not automatically load CLI built-ins. CLI JSON/RPC remain valid smoke/debug interfaces; consume `agent_settled`, not only `agent_end`, and inspect message failures even if the process exits zero.

## 6. Cloudflare-only inference policy

**Every actual model operation** must route through the gateway: agent turns, compaction, branch summaries, embeddings, extraction/proposals, summary maintenance, rerankers/classifiers, evaluation judges, and any model-backed guardrails. Keep optional classifier/guardrail features disabled unless their routing and usage accounting are verified. Deterministic fixtures, FTS, hashes, packing, and local tokenizers are not inference and need no gateway.

- Start with **one verified provider-native Pi route**, not a new streaming implementation. Native OpenAI Responses/Chat Completions and Anthropic Messages have different URL conventions; validate actual paths, tools, streaming, usage, reasoning, and aborts. The unified compatibility endpoint is an alternative, not proof of feature parity.
- Require account ID, gateway ID, gateway token, approved models, and appropriate provider authentication. Missing credentials fail live tests before inference; there is no direct-provider or local-model escape hatch.
- Prefer BYOK to remove upstream keys from the agent environment once the chosen client is verified. Provider-native BYOK requires omitting provider auth headers; do not send arbitrary placeholders. A native key-in-request route is acceptable for the first controlled transport check.
- Keep credentials out of ordinary tool environments and traces. The managed SDK/extension must curate shell/MCP child environments; only the embedding adapter receives its necessary gateway credential. Isolation and trusted resources are still required because extensions execute inside Pi's process.
- Give chat/embedding/maintenance/judge models explicit immutable configuration identities. Keep query/document preprocessing compatible; schema model/dimension isolation remains intact. Test embedding batch/response support, do not assume chat compatibility covers embeddings.
- One retry owner, finite request and total-operation deadlines, cancellation, bounded body/streams, and redacted typed errors. No cross-model fallback during controlled benchmarks; any future fallback remains gateway-only and records the actual model used.
- Validate HTTPS origin, account/gateway path, operation, and redirects in the adapter. Restrict available Pi models/resources and enforce network egress in an isolated environment. A `baseUrl`, `enabledModels`, or a default model alone cannot stop a shell tool/extension/SDK from calling another host.
- Disable response caching for baseline quality experiments (`cf-aig-skip-cache: true`) and Pi idle cache warming. Keep provider prompt-cache accounting separate from gateway response caching. Any future cache must include scope/input/model/recipe identity; never use a shared custom key for different owners.
- Gateway logging is enabled by default. Prefer metadata-only logs (`cf-aig-collect-log-payload: false`) or no logs for sensitive operations. Do not send raw owner IDs, paths, credentials, or memory text in metadata. Correlate opaque run/case IDs and operation type.
- Keep a local redacted usage ledger, record gateway request IDs when available, and reconcile totals—including embeddings, maintenance, retries, compaction, and judges. Pi's chat estimates are not the complete product bill.

**Privacy trade-off:** storage and ordinary retrieval remain local; gateway-backed inference sends selected text/code to Cloudflare and the upstream model. Obtain the user's permission for that transfer and define log/cache/retention policy. "Local-first" does not mean "all inference stays on-device."

The managed access slice now checks approved gateway routes/models, credentials, privacy/retry headers, deadlines and redacted usage accounting. Its Pi smoke enables only explicit native MCP tools, not general shell/file tools or discovered resources. The embedding executable implements `CommandEmbedder` without changing model-free engine behavior. This is **not general OS egress enforcement**: arbitrary standalone embedders, shell tools and external extensions remain trusted configuration. Live gateway privacy/authentication/model availability and billing still require operator verification. The legacy/reference local embedding path stays historical and is excluded from the new managed evaluation/product path until retired or migrated.

## 7. Evaluation and release gates

### Experiments

Run paired scenarios against the same Pi version/model, repo snapshot, prompts, tool access, budgets, and scenario seeds:

1. No TinyMem (Pi-only baseline).
2. Explicit native lexical memory plus profile/task context.
3. Gateway embeddings plus hybrid memory.
4. Startup pinning/recent-tail policy.
5. Optional summary tree plus raw-evidence expansion.

Do not build all variants before measuring the baseline. Test at least 30 fixed scenarios across 5 or more independent runs per stochastic variant initially; report distributions and uncertainty, not one lucky response. Treat this as an initial suite, not statistically sufficient proof for every claim.

### Required scenarios

- Save an approved preference; recall it from a **fresh session with no prior transcript**, on an unrelated task.
- Preserve a project decision and issue solution; actually use them in a tested code change.
- Correct/supersede a fact; deleted/expired/old-version evidence must not reappear in profile, hybrid, context, or summary expansion.
- Same text in two users/projects and deliberate global sharing; unauthorized foreign scopes never appear.
- New Pi session, restart, forced compaction, branch/fork, changed model, concurrent writers, aborted run.
- Changed/deleted/ignored repository source; citations must match current hashes and line content.
- Missing gateway credentials, 401/429/5xx, timeout, aborted stream, malformed vector/summary, and unavailable gateway: no direct fallback, partial evidence commits, or stuck startup.
- Many newer memories versus an older pinned constraint; 33+ word tasks, Unicode, oversized evidence, tiny/zero budgets, MCP truncation.
- Malicious memory/source instructions, secrets, and subagent duplicate proposals; no privileged instruction promotion, unintended writes, or unsafe egress.
- Broken summary, missing child, update/delete/expiry during generation, rebuild from raw evidence, and stable paged drill-down.

### Metrics

Recall@k/MRR on labelled evidence; precision/unsupported-memory rate; outdated-fact rate; scope/citation violations; actual coding-task tests; memory-write precision/duplicates; startup/task/maintenance latency p50/p95; tokens by operation; total cost per successful task; bytes, DB growth, and writer-lock wait time.

Prefer deterministic scenario assertions and code tests as the primary oracle. If using a model judge, route it through the gateway, blind it to the variant, record its cost, and manually inspect a sample. Judges cannot establish security isolation.

### Non-negotiable gates

Zero observed cross-scope leaks, deleted/expired evidence in current context, stale accepted dependency writes, unintended durable writes, direct/off-policy inference, unhandled budget overflow, or leaked credentials in the adversarial suite. Keep existing native/SDK suites green.

Before performance/quality release claims, publish the fixed workload, model identities, runs, uncertainty, full cost accounting, and a measured baseline. Choose latency/cost thresholds after the initial measurement; no invented performance guarantee. Enable summaries by default only if they improve context utility/task outcomes or reduce total cost at non-inferior utility without violating the safety gates.

## 8. Pending tasks, dependencies, and completion criteria

All unchecked tasks are pending. IDs are implementation slices, not a promise that the whole phase lands in one change.

### Foundation — first

- [x] **F0:** Source review and revised architecture/roadmap; record OptMem pin and Pi version.
- [x] **F1:** Re-run native/SDK correctness checks and add a repeatable model-free Pi MCP discovery smoke.
- [ ] **F2 (P0):** Make native the primary packaged/documented product; prominently quarantine legacy HTTP/company claims and decide deprecation versus scoped fixes. Acceptance: no new example/evaluation loads legacy model inference.
- [ ] **F3 (P0):** Supply Cloudflare account/gateway credentials, choose an approved chat and embedding model, agree data-transfer/log policy. Acceptance: preflight is secret-safe; live jobs fail clearly when prerequisites are absent.

### Gateway transport — depends on F3

- [ ] **G1 (P0):** Pi native provider config and request verification. Touch evaluation/config, not Rust storage. Acceptance: text, tool call/result, streaming, Unicode, usage, abort, overflow, compaction/branch summary all use gateway routes.
- [x] **G2 (P0), deterministic slice:** Trusted gateway embedding executable using the existing `CommandEmbedder` contract; fake-HTTP and real Rust-process tests cover exact path/auth/model, vector validation, finite IO, failure atomicity, query/write/backfill and no local/direct fallback. Actual Cloudflare/model availability remains a required F3/live check.
- [ ] **G3 (P0):** Model policy preflight plus isolated egress restrictions, curated tool credential environments and resource/model allowlist. Acceptance: blocked direct-provider, alternate model, redirect, and tool/extension bypass probes; no secret inheritance into ordinary shell tools or dependence on prompt compliance.
- [ ] **G4 (P1):** Request correlation, redacted errors, privacy headers, usage/cost ledger, retry ownership. Acceptance: chat + embed + compaction + maintenance + retries reconcile; no payload/secret leakage in normal telemetry.

### Core integration reliability — can run alongside gateway work

- [ ] **C1 (P1):** Task text versus retrieval-term handling in `context.rs`/MCP/SDK contracts. Acceptance: long/Unicode tasks produce bounded retrieval without silent arbitrary truncation of the stored evidence.
- [ ] **C2 (P1):** Model-aware MCP annotations and explicit typed error codes in `mcp.rs`. Acceptance: offline/gateway tools advertise IO correctly; invalid input, conflict, provider outage, and storage failures are distinguishable and redacted.
- [ ] **C3 (P1):** Eligibility precheck plus commit-time recheck for remote updates. Acceptance: missing/stale target rejected before inference, races still cannot commit stale evidence/cache.
- [ ] **C4 (P1):** Cancellation, end-to-end deadlines and maintenance page limits. Acceptance: abort/backfill cannot wedge a Pi session, owned processes are reaped, maximum total duration is tested.
- [ ] **C5 (P1):** Pi-aware compact context rendering/transport cap and exact-tokenizer policy. Acceptance: only intended `prompt` reaches injection, complete request/transport budgets are respected, byte default remains compatible.
- [ ] **C6 (P2):** Explicit init/require-existing and identity diagnostics. Acceptance: typoed database path cannot silently create a second identity in managed sessions; existing CLI compatibility is deliberate.

### Pi baseline and startup — depends on G1/G3 and C1/C5

- [ ] **P1 (P0):** Thin scenario runner using Pi SDK, controlled resources and fixtures. Acceptance: no-memory and explicit-memory paired fresh-session runs; deterministic assertions, settled/error/deadline handling; no custom agent loop.
- [ ] **P2 (P1):** Small lifecycle extension for startup/task injection and explicit write/inspect commands. Acceptance: restart/compaction/branch tests, no duplicate injected blocks or read-triggered evidence writes; SDK MCP extensions explicitly bound.
- [ ] **P3 (P1):** Pinning + token-budgeted startup selector in engine/MCP/SDKs; migrate/test new fields. Acceptance: old pinned constraints survive new-record pressure and unrelated tasks; deleted/expired pins disappear.
- [ ] **P4 (P1):** Execute baseline lexical/hybrid suite and publish quality/cost/latency report. Acceptance: labelled evidence and real coding-test outcomes, operation-complete gateway telemetry, no unmeasured lift claims.

### Evidence lifecycle — after explicit-memory baseline

- [ ] **L1 (P1):** Structured provenance, idempotency and explicit supersession/proposal acceptance. Acceptance: user correction resolves current fact, parent/subagent retries do not duplicate writes; evidence remains auditable.
- [ ] **L2 (P2):** Optional extraction proposals through gateway. Acceptance: no inferred durable write without configured acceptance policy; false positives, sensitive facts, malformed outputs and deletion are tested.
- [ ] **L3 (P2):** Stable history/export cursors, backup/restore verification and explicit scoped purge. Acceptance: >100 versions accessible with authorized pagination; restore preserves IDs/history; deletion versus erasure documented.

### OptMem-derived caches — depends on P4/L1; initially opt-in

- [ ] **S1 (P2):** Prototype startup coverage/summary utility on fixed scenarios, independently implemented. Acceptance: raw-recall escape hatch and fidelity/cost comparison; no copied unlicensed material.
- [ ] **S2 (P2):** Summary dependency schema, invalidation and transactional bounded job claim/commit. Acceptance: update/delete/expiry/purge, races, leases, failure and crash/restart tests; no stale current summary.
- [ ] **S3 (P2):** Bottom-up gateway maintenance and progressive expansion. Acceptance: child/raw provenance, snapshot cursors, bounded cost, rebuild/repair, missing-cache raw fallback; no mandatory nap loop.
- [ ] **S4 (P2):** Ablation/large-history report and default-on decision. Acceptance: measured utility versus full operation cost and compression drift; keep disabled if it does not earn its complexity.

### Performance and release — ongoing, changes gated by measurements

- [ ] **R1 (P2):** Candidate hydration/work caps, 1k/10k/100k memory and repository/lock benchmarks; measure lexical assembly's count-only candidate bound. Acceptance: reproducible p50/p95/resource/lock report and tests for oversized candidates.
- [ ] **R2 (P2):** Batch/persistent embedding or scan-outside-lock optimization only if R1 identifies the bottleneck. Acceptance: preserves source/cache freshness and concurrent-write invariants; no premature external store.
- [ ] **R3 (P1):** Pin Pi/Node versions in evaluation lockfiles/CI; add deterministic Pi smoke on Linux/macOS and opt-in secret-backed gateway evaluation. Acceptance: missing credentials skip only explicitly optional live jobs, not required native/Pi protocol tests; remote runs actually verified.
- [ ] **R4 (P2):** Publish native binary/SDK/Pi package installation, migration/rollback/backups, privacy and operations guide. Acceptance: clean-machine install and supported-platform smoke; historical test counts/docs not confused with current reports.

**Execution order:** F2/F3 → G1/G2/G3 + C1/C2/C3/C5 → P1/P2/P3 → P4 → L1 → decide S1–S4. C4/G4/R1 run alongside the integration slices. Do not hold baseline evaluation hostage to extraction, trees, graphs, or distributed infrastructure.

## 9. Immediate next slice

**Access-slice progress:** `integrations/cloudflare` pins Pi 0.99.2, provides explicit private-gateway provisioning and policy approval/preflight, reuses native Responses streaming, supplies the embedding executable and a private redacted token/cost-estimate ledger. Deterministic tests include Pi text/tool/abort/compaction routing, Rust embedding write/query/backfill/failure atomicity, and a scripted save → fresh Pi session → native recall protocol check. It loads no general-purpose shell/file tools or discovered resources. This does not complete F3/G1/G3/G4 or the real coding milestone: credentials, remote gateway checks, general tool isolation, branch-summary/overflow verification and billing reconciliation remain pending.

1. Supply credentials; provision/verify the private test gateway and explicitly approve the fixed chat/embedding/data policy.
2. Run real gateway text/tool/embedding/abort checks and reconcile the ledger. Add OS-isolated coding tools before enabling arbitrary execution.
3. Fix long-task assembly and Pi context transport handling.
4. Run one save → fresh session → recall → tested code-change scenario through Pi, with all inference recorded at the gateway.
5. Grow that into the paired baseline suite before implementing the summary tree.

See [Pi/gateway testing](PI_GATEWAY_TESTING.md) for setup examples and exact limits. Architectural trade-offs are recorded in [ADR 0001](adr/0001-reuse-pi-and-route-inference-through-cloudflare.md) and [ADR 0002](adr/0002-derived-summary-cache-not-memory-replacement.md).
