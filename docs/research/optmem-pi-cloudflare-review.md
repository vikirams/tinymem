# OptMem, Pi, and Cloudflare: architecture review

Reviewed 2026-10-01. This is a source-level architecture review, not a Git diff review: the TinyMem checkout has no `.git` metadata. No independent reviewer or background research agent was available. No inference was launched; gateway credentials were not present. Findings below distinguish code behavior, documentation claims, and proposed product changes.

## Recommendation

Use **Pi as the real coding agent and evaluation runtime**, TinyMem's **native Rust engine as the memory module**, and **Cloudflare AI Gateway for every enabled model operation**. Do not implement a competing agent loop. Borrow OptMem's evidence/cache separation, bounded startup reading, progressive drill-down, and explicit maintenance workflow; retain TinyMem's exact scopes, optimistic updates, expiration, provenance, and transactional storage.

## Sources and reproducibility

OptMem source snapshot: [`1fb164cf39028047781f72ac3bb1e5a691c1dcb0`](https://github.com/VictorTaelin/OptMem/tree/1fb164cf39028047781f72ac3bb1e5a691c1dcb0).

- [README](https://github.com/VictorTaelin/OptMem/blob/1fb164cf39028047781f72ac3bb1e5a691c1dcb0/README.md)
- [Implementation: `memo`](https://github.com/VictorTaelin/OptMem/blob/1fb164cf39028047781f72ac3bb1e5a691c1dcb0/memo)
- [Invariant tests](https://github.com/VictorTaelin/OptMem/blob/1fb164cf39028047781f72ac3bb1e5a691c1dcb0/test.py)
- [Installer](https://github.com/VictorTaelin/OptMem/blob/1fb164cf39028047781f72ac3bb1e5a691c1dcb0/install.sh)

Pi: installed `@earendil-works/pi-coding-agent` **0.99.2**, Node **26.1.0**. Its shipped documentation and checked examples were read; online links may move with Pi's main branch:

- [SDK](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/sdk.md), [MCP](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/mcp.md)
- [Extensions](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md), [Models](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/models.md)
- [Custom providers](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/custom-provider.md), [CLI integration](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/cli-integration.md)
- [Compaction](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/compaction.md), [Security](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/security.md)
- [SDK MCP example](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/examples/sdk/14-codemode-mcp.ts), [explicit-resource SDK example](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/examples/sdk/12-full-control.ts)

Cloudflare first-party documentation:

- [OpenAI native endpoints](https://developers.cloudflare.com/ai-gateway/usage/providers/openai/), [Anthropic native endpoints](https://developers.cloudflare.com/ai-gateway/usage/providers/anthropic/)
- [Unified Chat Completions](https://developers.cloudflare.com/ai-gateway/usage/chat-completion/), [Workers AI](https://developers.cloudflare.com/ai-gateway/usage/providers/workersai/)
- [Gateway authentication](https://developers.cloudflare.com/ai-gateway/configuration/authentication/), [BYOK](https://developers.cloudflare.com/ai-gateway/configuration/bring-your-own-keys/)
- [Caching](https://developers.cloudflare.com/ai-gateway/features/caching/), [Logging](https://developers.cloudflare.com/ai-gateway/observability/logging/), [Metadata](https://developers.cloudflare.com/ai-gateway/configuration/custom-metadata/)

## What OptMem actually implements

| Pattern | Source behavior | TinyMem decision |
| --- | --- | --- |
| Authoritative evidence versus derived cache | `LOG.txt` appends raw notes; `TREE/<size>` holds rebuildable summaries. `tree_drop` truncates a summary level and dependent levels without changing the log. | Keep `memory_events`; add optional summaries as derived records with explicit dependencies, never rewrite evidence to save prompt space. |
| Binary merge tree | Aligned power-of-two note ranges; `pending` visits smaller blocks first. `nap_prompt` uses raw notes for blocks up to 16, then child summaries. | Prototype the idea on scoped, versioned memory leaves, with periodic raw-evidence checks. Do not assume recursive summaries preserve every fact. |
| Detail decreases with age | `cover(T, budget)` tiles the whole history with finer blocks near the present, within a line count. | Startup context should combine pinned durable evidence, a recent tail, and optional old summaries; task search remains a separate path. |
| Progressive disclosure | `zoom` exposes a node's two children; `recall` searches raw log text even if compression lost it. | Add summary expansion and raw-evidence lookup; retain ordinary lexical/hybrid search as the reliable escape hatch. |
| Explicit, agent-driven maintenance | `note` offers one pending merge; `nap` accepts the agent's summary. No scheduler or model client runs inside the script. | Expose bounded maintenance work. Pi or an explicit maintenance runner can perform it through the gateway. It must not block an unrelated coding task. |
| Transport-aware reading | `paginate` limits bytes/lines; `wake part T` holds the note-count snapshot across pages. | Budget the actual model-facing prompt and use stable cursors for history/expansion. Account for Pi's separate MCP truncation. |
| Concurrent writes and torn-record repair | Locking assigns note IDs within the lock; appends flush/fsync; `repair` drops a partial trailing fixed-width record. | Preserve SQLite transactions; add crash/restart/property tests instead of introducing another file-store implementation. |
| Deliberate initialization | Only `init` creates the memory directory; a typo in `MEMORY_DIR` fails instead of inventing another identity. | Add explicit native initialization and optional require-existing startup, with a visible database/scope diagnostic. |
| Subagent policy | The integration prompt tells subagents not to write and treats main parallel sessions as one identity. | Use parent-owned proposal acceptance and idempotency, not prompt compliance as a security mechanism. Parallel sessions still require exact authorized scopes. |

### What not to borrow

- **Fixed-width files and position IDs:** good for OptMem's small interface, but incompatible with TinyMem's variable-size evidence, scope/version semantics, and transactional indexes. No second store.
- **280-byte limits as a universal memory policy:** encourage concise atomic notes, but preserve code, Unicode, citations, and richer evidence. Line counts are not token budgets.
- **Mandatory recursive merges before work:** `wake` can refuse when required summaries are missing. That creates a startup dependency on model availability. TinyMem should return bounded raw evidence and explicitly report missing summaries.
- **A single global identity as authorization:** this is not tenant isolation or access control.
- **`forget` as user deletion:** OptMem forgets summaries, not raw notes. TinyMem deletion must suppress all derived current-context representations of that memory.
- **Unbounded regex work:** OptMem caps retained recall output, but regex evaluation scans the log. Native literal FTS is a safer default.
- **Performance and quality claims:** the README's million-note timing was not reproduced. The tests use join/truncate fake compression, not a real model. Summarization fidelity and coding-task lift are unproven.
- **Source copying:** no license file or explicit source-license grant was found in the reviewed snapshot. Borrow ideas with independent implementation; obtain permission before copying code, prompt text, or assets. GitHub visibility alone is not a reuse license.

The current test program's introductory comment says 5,000 memories, while `N` is 2,000. Running it on this machine produced **109,099 assertions passed, 0 failed**. That is not 109,099 independent integration tests and does not validate model-generated summaries.

## TinyMem: implemented strengths

The native implementation is substantially beyond an empty MVP:

- [Schema/opening](../../crates/tinymem-engine/src/lib.rs): native identity/version checks repeated under the migration writer lock; transactional migrations; SQLite WAL; repository evidence separate from personal memory.
- [Memory](../../crates/tinymem-engine/src/memory.rs): exact tuple scopes, scope-local deduplication, stable IDs, versions, immutable added/updated/deleted observations, expiration, bounded profiles/history; reads do not record access.
- [Retrieval](../../crates/tinymem-engine/src/retrieval.rs): lexical AND default, semantic exact cosine, hybrid lexical OR plus RRF; derived embeddings tied to model/dimension/version; updates invalidate every cached model.
- [Context](../../crates/tinymem-engine/src/context.rs): category round-robin, provenance-preserving exact deduplication, whole-entry packing, conservative byte-token bound; no fabricated summaries/confidence.
- [Source](../../crates/tinymem-engine/src/source.rs): exclusions, symlink refusal, source hashes and grounded citations. Language identification is broad; declaration parsing is Python-only, not a complete code graph.
- [MCP](../../crates/tinymem-engine/src/mcp.rs): eleven strict, bounded tools, startup-fixed root/scope, clean stdio JSON-RPC, separate protocol/tool errors.
- [SDKs](../../sdks/): real-binary integration coverage, finite deadlines, bounded streams, shell-free process startup and cleanup. Current SDKs start a subprocess per operation; Pi's built-in MCP connector instead owns a session connection.

## Actionable gaps and improvements

These are architectural/operational findings, not claims that the existing narrow native contract is failing all its tests.

| Priority | Finding and evidence | Required change |
| --- | --- | --- |
| P0 | **No gateway enforcement or live agent-quality test exists.** `CommandEmbedder` accepts any trusted executable; the reference adapter loads local sentence-transformers. No gateway integration is configured. | Add one gateway policy/adapter and a Pi evaluation wrapper. Reject off-policy inference and direct fallback; leave deterministic retrieval model-free. |
| P0 | **Legacy Python is not a safe company-memory foundation.** `storage.py:147–156,213–217` deduplicates by content hash without scope; `api.py` takes caller-supplied scope and exposes ID operations without authenticated principals. `embeddings.py` creates a local model directly. | Freeze/deprecate this product path and do not expose its HTTP interface as a multi-tenant service. If retained, scope dedup and authorization need dedicated tests/fixes, plus gateway-only embedding migration. Do not auto-migrate its database. |
| P1 | **Ordinary task prompts can fail automatic assembly.** `context.rs:308–316` rejects more than 32 words; a real MCP probe with 33 words returns a generic execution error. | Separate bounded task input from retrieval-term selection. Deterministically select terms without rejecting an otherwise valid task; distinguish this from explicit search validation. |
| P1 | **No dependable evergreen startup context.** `memory.rs:521` profiles by recency; task assembly only retrieves query matches. An older important preference can disappear from a busy profile or not match a new task. | Add explicit pinning and token-budgeted startup selection; test unrelated-task preferences and many newer records. Do not infer that every stored preference must apply to every task. |
| P1 | **Pi can truncate an otherwise valid native prompt.** Native assembly permits 262,144 prompt bytes and returns both `prompt` and `entries`; Pi's documented direct MCP text-result cap is 20 KB. | Use a conservative full-envelope cap for initial direct-tool tests; the extension should inject only `prompt` via structured data and enforce the complete model-context budget. |
| P1 | **Network-capable embedding work is advertised as closed-world.** `mcp.rs:553` assigns `openWorldHint: false` to every tool although configured embeddings may use the network. | Derive annotations from startup capabilities for add/update/index/semantic search/assembly; test both offline and gateway modes. Keep memory-read semantics distinct from network IO. |
| P1 | **Refused updates can still incur inference.** `memory.rs:311–324` embeds before finding the record/checking expected version. | Validate current eligibility/version before remote work, then recheck inside the commit transaction. Keep atomic evidence/cache writes for the existing synchronous policy. |
| P1 | **Cancellation/diagnostics are too coarse for remote workloads.** MCP is serialized and ignores cancellation; execution failures share one static message. Up to 100 sequential backfill calls can exceed client deadlines. | Add redacted typed errors, cancellation/deadline propagation, small explicit maintenance pages and a single retry owner. Never expose secrets or cross-scope details. |
| P2 | **Bounded output is not a bounded work budget.** `lib.rs:163–183` holds a writer transaction throughout repository scanning; lexical recall/profile can scan skipped candidates; cosine scans every eligible vector. | Benchmark at 1k/10k/100k memories and realistic repos. Move expensive source scan/parse outside the writer lock with reconciliation checks if measurements justify it; add work limits/pagination before selecting ANN infrastructure. |
| P2 | **Lexical assembly has a candidate-count limit, not a candidate-byte limit.** The unconfigured branch in `context.rs:162–172` loads 100 full records before packing (up to roughly 6.4 MiB of content alone). The documented 1-MiB return bound applies to configured hybrid retrieval, not this branch. | Add a candidate-byte/work budget if measurements justify it; test large-record assembly. Bounded returned prompts do not bound intermediate work. |
| P2 | **History is not exhaustive through public tooling.** Current history exposes the latest bounded 100 observations of one ID, without continuation. | Add stable version cursors, scoped export/restore verification, and explicit historical labels. Summary drill-down must not promise inaccessible raw history. |
| P2 | **No contradiction/extraction/summary lifecycle exists.** Exact deduplication does not resolve contradictory memories; generated extraction and summary trees are not implemented. | Add explicit proposals and supersession before automation; summary dependencies must honor update/delete/expiry and be measured before enabling by default. |

## Verification performed

On this checkout, 2026-10-01, without downloaded models or live inference:

| Check | Result |
| --- | --- |
| `cargo test --workspace --locked` | **85 passed** |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS |
| Python SDK `unittest`, `TINYMEM_REQUIRE_INTEGRATION=1` | **16 passed, no skips**, Python 3.14.7 |
| TypeScript SDK `npm test`, integration required | **16 passed, no skips**, Node 26.1.0 |
| TypeScript `npm run typecheck` | PASS |
| `cargo build --release --locked -p tinymem-engine` | PASS; rebuilt the previously stale release artifact |
| Pi 0.99.2 `mcp list`, isolated temporary configuration | **Connected; all 11 tools discovered**, debug and rebuilt release |
| Documented gateway model config with fake credentials, offline `--list-models` | **Parser accepts provider/model; no inference or transport verification** |
| OptMem snapshot `python3 test.py` | **109,099 assertions passed, 0 failed** |
| 33-word native `assemble_context` MCP probe | Confirmed `isError: true` |

Pi discovery proves handshake/tool-schema interoperability, **not** that an LLM chooses tools, retains memories across fresh sessions, survives compaction correctly, or routes inference through Cloudflare. Remote CI, performance budgets, privacy settings, actual gateway transport, and real-model quality remain unverified. Legacy Python was inspected selectively, not model-loaded or comprehensively revalidated.

See the [replacement roadmap](../MEMORY_SYSTEM_PLAN.md) and [Pi/gateway evaluation guide](../PI_GATEWAY_TESTING.md).
