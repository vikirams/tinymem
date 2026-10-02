# Test TinyMem with Pi and Cloudflare AI Gateway

## Status and scope

The Cloudflare-only access slice is implemented in [`integrations/cloudflare`](../integrations/cloudflare): a private-gateway provisioning command, explicit model/data-transfer approval, secret-safe preflight, a restricted Pi SDK adapter using **Pi 0.99.2's native OpenAI Responses transport**, the native `CommandEmbedder` executable, and a private usage ledger. Node **22.19.0+** is required; dependencies are pinned in `package-lock.json`.

**No live gateway has been provisioned or verified here: Cloudflare credentials were absent.** The deterministic suite uses fake HTTP and the real Pi/native Rust transports. Scripted save → fresh Pi session → recall is a protocol test, not a memory-effectiveness result. The tested-code-change milestone and a paid/live baseline remain pending. TinyMem summary trees are not implemented.

This slice deliberately enables **no shell/file tools, codemode, external extensions, classifiers, judges, resource discovery, idle cache warming or automatic compaction**. It is a model-access/memory smoke, not an unrestricted coding agent. Explicit Pi compaction is supported and tested through the same route. General coding tools must later run inside a verified OS isolation/egress boundary; a model `baseUrl` or a fetch check is not a sandbox.

## 1. Deterministic checks — no model credentials or paid inference

From the repository root:

```sh
cargo build --locked -p tinymem-engine
npm ci --prefix integrations/cloudflare --ignore-scripts
npm test --prefix integrations/cloudflare
python3 scripts/smoke-pi-mcp.py  # optional global Pi discovery check
```

The gateway suite checks:

- Missing credentials, unresolved placeholders, unapproved policy, invalid IDs, unusable/private ledger and transport environment overrides fail closed.
- Exact HTTPS Cloudflare account/gateway/provider/operation paths; one approved model per operation; direct providers, local inference, other models and redirects rejected.
- Real Pi Responses SSE text/Unicode, tool call/result, reported usage/cache-token cost, cancellation, incomplete/malformed streams, 401/429/5xx and explicit compaction.
- Real Rust `CommandEmbedder` → gateway adapter: write, query, explicit backfill, update and restart; failed inference cannot commit a new memory or replace existing evidence.
- Scripted Pi save and a **separate in-memory session**, with no prior transcript, recalling the same native database through MCP.
- No provider/gateway secrets or memory payloads in errors/ledger; ordinary child environments are allowlisted. Only the trusted embedding child receives its required gateway/upstream keys, never the management token.

Tests inject fake HTTP at an internal dependency seam. There is **no production alternate-origin environment variable or test-mode route**. Native storage/retrieval remains model-free unless embeddings are explicitly configured; the legacy Python/local-inference path is not loaded.

## 2. Approve the test models and privacy policy

The fixed policy in [`policy.mjs`](../integrations/cloudflare/policy.mjs) is:

| Operation | Approved candidate | Route |
| --- | --- | --- |
| Pi chat, including its explicit compaction | `gpt-4.1-mini-2025-04-14` | `/openai/responses` |
| Native memory write/query/backfill embeddings | `text-embedding-3-small`, 1536 dimensions | `/openai/embeddings` |

The embedding cache identity is `openai/text-embedding-3-small/d1536/raw-utf8-v1`. Documents and queries use the same unchanged UTF-8 text recipe. Cloudflare/upstream availability of these models is **not live-verified**. OpenAI does not supply an immutable snapshot ID for this embedding model; change the cache/policy identity and rebuild if weights, dimensions or preprocessing change. Do not reuse cached vectors across identities.

Approval means selected memory/code may leave the machine for Cloudflare and OpenAI. Use only a dedicated, non-sensitive fixture initially. Gateway response caching is bypassed, upstream Responses storage is disabled (`store: false`), and request/response payload collection is disabled. This is **not** a guarantee about all Cloudflare/OpenAI retention or zero-data-retention enrollment. Agree retention separately before sensitive data use.

After reviewing those choices, explicitly approve them:

```sh
export TINYMEM_GATEWAY_APPROVAL=tinymem-cloudflare-openai-v1
```

Changing models/prices/recipes requires a reviewed new policy identity, adapter tests, and explicit reapproval, not a fallback switch. This is a **managed-client allowlist**, not an account-wide Cloudflare model ACL. A gateway token used outside this module may permit other models; keep it private and enforce OS egress when introducing more tools or processes.

## 3. Create a private test gateway

Supply `CF_ACCOUNT_ID` (32 lowercase hex characters), a dedicated `CF_GATEWAY_ID`, and a separate `CF_AIG_ADMIN_TOKEN` with **AI Gateway Write** permission from a secret manager/private environment. Never put token values in commands, committed files, model prompts or shared transcripts.

Explicitly create the gateway (a management-plane operation, not inference):

```sh
node integrations/cloudflare/provision.mjs --create-private-test-gateway
unset CF_AIG_ADMIN_TOKEN
```

The command sets and checks the returned configuration: authentication required, gateway-default logs disabled, response-cache TTL zero and one upstream attempt. It also sets a 60-request/minute test rate limit. It does not replace an existing gateway or silently accept a public one. Creation failure prints only a bounded typed diagnostic, not the API response or token. **This command has not been run against Cloudflare here.**

Alternatively create a dedicated gateway in the dashboard with those settings, inspect that authentication is enabled, and disable gateway retries/fallback, caching, classifiers and guardrails. Create a narrowly scoped gateway token with **Run** permission (`CF_AIG_TOKEN`). Do not use the Write token in the agent. Native key-in-request routing is the initial supported choice: also supply `OPENAI_API_KEY` privately. BYOK is deferred until native header omission is verified; dummy upstream keys are not supported.

Requests enable only **metadata logs** via `cf-aig-collect-log: true` and `cf-aig-collect-log-payload: false`; gateway-default log collection remains off. Metadata contains an opaque request UUID and operation, never raw owner IDs, paths or memory text. The gateway log ID, when available, is captured for reconciliation.

## 4. Preflight and live transport smoke

Keep test state and ledger outside the fixture worktree, with private directory permissions. Set an **absolute** `TINYMEM_USAGE_LEDGER` path in an existing private directory. The adapter creates the ledger with mode 0600; an existing group/world-accessible file or symlink is rejected. Use a separate ledger per controlled run/scenario; sharing one file across the corresponding chat and embedding processes is supported.

```sh
node integrations/cloudflare/preflight.mjs
```

Required variables are `CF_ACCOUNT_ID`, `CF_GATEWAY_ID`, `CF_AIG_TOKEN`, `OPENAI_API_KEY`, `TINYMEM_GATEWAY_APPROVAL`, and `TINYMEM_USAGE_LEDGER`. Proxy/Node transport override variables are rejected rather than silently changing trust/routing. Preflight does **no inference** and reports `live_verified: false`; local readiness is not proof that remote credentials work or the gateway is private.

A deliberately paid/live text check:

```sh
printf '%s' 'Reply with: gateway transport ready.' | node integrations/cloudflare/pi-chat.mjs
```

The runner uses a temporary private agent directory and an in-memory Pi session, strips ambient secrets/personal configuration before creating children, and rejects any default in-process fetch outside the explicitly checked model transport. It waits for `agent_settled`, rejects assistant/tool failures and truncated output, imposes a 90-second run limit and eight-assistant-message cap, and explicitly shuts down MCP before disposal. It prints only a redacted status, not the model response or transcript. Every required live command fails nonzero when prerequisites are absent; no credential-missing success/implicit skip.

To use native memory, pass all five fixed-scope parameters together (absolute paths):

```sh
printf '%s' 'Save this owner-approved preference: prefer explicit UTC timestamps.' | \
  node integrations/cloudflare/pi-chat.mjs \
    --binary /absolute/tinymem/target/debug/tinymem \
    --db /absolute/private-test/native.db \
    --root /absolute/fixture-repository --user eval-owner --project fixture \
    --embeddings
```

Run a second invocation with the **same database and scope**, a fresh in-memory transcript, and a recall task that does not repeat the expected answer. Omit `--embeddings` for the explicit lexical baseline. Omitting agent ID deliberately shares only the exact owner/project namespace; scope is not authentication. No tool/lifecycle extension automatically mines or saves the transcript.

For embedding-only/native integration, the executable is:

```sh
/absolute/tinymem/target/debug/tinymem --db /absolute/private-test/native.db \
  mcp /absolute/fixture-repository --user eval-owner --project fixture \
  --embedding-command /absolute/tinymem/integrations/cloudflare/embed.mjs \
  --embedding-model openai/text-embedding-3-small/d1536/raw-utf8-v1 \
  --embedding-dimensions 1536 --embedding-timeout-ms 30000
```

`embed.mjs` must be executable on Unix. It uses Node built-ins; Pi packages are not needed by the embedding executable. Its protocol is exactly `{"model":"cache-identity","text":"..."}` → `{"embedding":[...]}`; stdout carries only the vector. It rejects wrong identities, extra fields, blank/oversized input, wrong response model/index/dimension, non-finite/float32-overflow/zero vectors and missing usage. One request per invocation, no retries, shell, downloads, local inference or direct fallback. The HTTP deadline is 25 seconds; the executable's full stdin/network deadline is 27 seconds, below Rust's 30-second default. Request/response bodies are capped; Rust independently caps executable stdout/stderr and normalizes vectors.

This standalone command is trusted configuration, **not a sandbox**. Do not launch it from an uncontrolled personal Pi CLI environment. The managed smoke curates child credentials; arbitrary callers of native `CommandEmbedder` still own executable/environment trust.

## 5. Usage/cost and next milestone

The JSONL ledger includes policy/model identity, opaque request UUID, operation, HTTP status, gateway log ID, attempt/outcome, latency, reported token counts and configured-price USD estimates. It never records headers, prompts, vectors, owner identities, tool arguments or raw provider errors. Missing/failed usage is `null` with an unknown/reconciliation cost basis, **not zero**. A successful chat with no reported usage fails the measurable smoke. Embedding usage is recorded even when a subsequently validated vector is rejected. Pi compaction is currently counted as a chat request; finer purpose attribution remains pending.

Prices are explicit estimates per million tokens: chat input $0.40, output $1.60, cached input $0.10; embedding input $0.02. Verify current pricing before running; estimates are not a bill or a reconciled Cloudflare total. Reconcile request IDs and ledger totals against gateway metadata, upstream usage and any gateway charges. Aborted/failed upstream calls may be billed despite no reported usage.

**Remaining live acceptance:** verify private authentication remotely, real text/tool streaming and abort, embedding model availability/usage, effective payload-log/cache policy, and usage reconciliation. Add isolated coding tools and deterministic fixture assertions, then demonstrate:

> Pi saves an approved memory → a fresh Pi session recalls it without the previous transcript → uses it in a tested code change.

Do not count the scripted fake-model protocol test as this milestone. Do not build summary trees until this baseline is working and measured. Today's assembly interface still accepts at most 32 retrieval words; use a short query and a bounded budget (about 2,000 conservative byte-tokens) and inject only `AssembledContext.prompt`, not the duplicated MCP envelope. Long-task handling and model-aware context packing remain separate pending slices.

## Sources

- [Cloudflare gateway creation](https://developers.cloudflare.com/api/resources/ai_gateway/methods/create/), [authenticated gateways](https://developers.cloudflare.com/ai-gateway/configuration/authentication/), [OpenAI-native routes](https://developers.cloudflare.com/ai-gateway/usage/providers/openai/), [logging/privacy headers](https://developers.cloudflare.com/ai-gateway/observability/logging/).
- [OpenAI embeddings](https://platform.openai.com/docs/api-reference/embeddings/create), [pricing](https://openai.com/api/pricing/).
- [Pi SDK](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/sdk.md), [model configuration](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/models.md), [MCP](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/mcp.md), [security/isolation](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/security.md). Behavior tests and the lockfile are pinned to **0.99.2**, not those moving documentation branches.
