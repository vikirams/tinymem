# Coding-memory benchmark protocol

**Status: proposed protocol. No runner, fixture corpus, measurements or leaderboard is shipped by this
document.** The [file-provenance milestone](FILE_PROVENANCE.md) supplies deterministic engine/MCP/SDK
correctness tests, not the temporal agent fixture corpus or benchmark measurements described here.
Version the protocol before collecting claim-bearing results. The benchmark must be usable
against other memory layers, not encode TinyMem's implementation as the correct answer.

## 1. Question and units of evaluation

Does persistent memory help an otherwise identical coding agent complete tested changes, avoid known
mistakes and obey current repo constraints across fresh sessions—at what context, latency and total cost?

Separate three tracks:

1. **Engine correctness:** deterministic lifecycle, provenance, budget, scope and portability assertions.
2. **Recall performance:** fixed evidence corpora and queries; no agent/model needed for lexical runs.
3. **Agent effectiveness:** real coding changes checked by executable tests in isolated repositories.

A passing tool handshake, scripted lookup, matching string or model judge alone is not coding success.
Report forced-tool protocol runs separately from natural agent-use runs.

## 2. Temporal scenario contract

Each case must declare:

- Stable case/version ID, fixture license, repository snapshot/tree digest, runtime/toolchain lockfiles.
- Owner/repo/agent scopes, allowed tools, source identities and the clock schedule.
- An ordered training history of sessions: prompts, corrections, decisions and source changes. Record
  the exact information each variant may observe and which writes were explicit versus proposed.
- A cutoff followed by a fresh agent session with no inherited chat transcript or hidden prompt state.
  Preserve only the declared variant's persistent artifacts. All variants start from the same task tree.
- Task prompt, allowed context budget, expected evidence IDs for retrieval diagnostics, expected current
  and forbidden obsolete facts, and executable success/regression checks.
- Deterministic intervening sessions, distractors and mutations, with seed and expected lifecycle states.
- Explicit timeout/error rules and a trace schema. No evidence created after the cutoff enters retrieval.

For long-horizon cases, place the tested decision **before 50 intervening sessions**, then ask for a
change that requires it. Those sessions must create realistic competing evidence, not just 50 resets.
The whole-session reset must be verified: hidden session state or a shared baseline memory directory
would invalidate the comparison. Include both documented conventions and decisions absent from current
repo files; ordinary repository reading must remain available to every variant.

### Initial case matrix

Start with at least 30 independent task cases, stratified across these families; this is an initial
coverage target, not a statistical sufficiency claim. Do not count reruns or cosmetic prompt changes as
new independent tasks.

| Family | Example | Primary oracle |
| --- | --- | --- |
| Repeated mistake | Earlier correction says database timestamps must remain UTC; later agent edits date handling | Regression tests reject the old timezone bug and verify the new feature |
| Repo conventions | Use the existing transaction wrapper and error type, not a new abstraction | Behavioral tests plus fixed syntax/structure checks where the convention requires them |
| Decision retention | A recorded compatibility decision still applies after 50 intervening sessions | Compatibility tests and a cited current decision; code behavior is primary |
| Changed source | A file/PR dependency changes after a remembered workaround | Stale note absent from default context; new code follows current behavior |
| Correction/conflict | A deprecated retry rule is explicitly superseded | Old rule absent, correction attributed to the right version, tested retry behavior |
| Scope and retention | Similar tasks across two repos/users; one note deleted or expired | Foreign/deleted evidence never surfaced; coding result remains valid |
| Write discipline | Durable correction mixed with logs, duplicates and malicious instructions | Accepted-evidence labels, no secret retention or injected-command execution |

Include negative controls: tasks requiring no history, irrelevant memories, contradicted memories,
source outages, tiny/zero budgets and decisions whose immutable origin is old but still applicable.
For planned authenticated sharing, add revoked team/org grants only when an authorization boundary
exists; exact-scope tests must not be advertised as enterprise access-control tests.

## 3. Paired baselines and ablations

Use the same agent version, model/configuration, tools, task snapshot, prompts, seeds and total request
budget. Isolate each case/variant/run in a new database, worktree and agent configuration. Randomize
variant execution order to reduce time/provider effects; record uncontrolled provider nondeterminism.

Required initial baselines:

- **No persistent memory:** same repository access and coding agent, no prior transcript.
- **Flat notes:** append-only notes from the allowed history, selected by a published deterministic policy
  under the same injected-token cap; not a deliberately oversized transcript dump.
- **Store everything + lexical top-k:** same allowed history, frozen chunking/ranking and budget cap.
- **TinyMem explicit writes + lexical context:** no extraction, remote reranker or summary required.

Add hybrid retrieval, write-quality policy, source invalidation and target-tokenizer packing as isolated
ablations after the baseline works. To isolate retrieval quality, also run identical pre-populated
approved evidence across memory variants; to evaluate write policy, give them the same unfiltered input
stream and include all write/maintenance costs. Do not conflate these experiments.

An oracle-evidence condition may bound achievable utility but must be labelled non-deployable. Other
products can be added through thin adapters with pinned versions, public configuration and disclosed
feature mismatches. Do not tune only TinyMem or compare unequal context/model budgets.

## 4. Metrics and accounting

### Primary outcomes

- Tested task success rate, repeat-mistake rate and convention-violation rate, per family and overall.
- Stale-as-current, unsupported-evidence, cross-scope, deletion and expiry violations, with denominators.
- Success-versus-budget curves at 0, 512, 1,024, 2,048 and 4,096 **target-model tokens** of injected memory.
  Use the same full-request limit/reserved output budget across variants; cap framing and citations too.

The headline is the quality/budget frontier, not recall@k or a single composite score. Report paired
success lift over no-memory and the smallest budget reaching a predeclared target success rate. A
secondary quality-per-token metric may be paired success lift per 1,000 mean injected memory tokens;
report negative lift, uncertainty and absolute success alongside it. At zero memory tokens that ratio
is undefined, not infinite. Never optimize token count by dropping correctness requirements.

### Diagnostic and resource metrics

- Labelled evidence precision/recall/MRR; source/version citation correctness; eligible evidence omitted.
- Write acceptance precision/recall, duplicates per attempted write, unresolved conflicts, retained bytes
  and tokens. Refusing every write is not high-quality memory.
- Actual injected tokens, full agent input/output and cached tokens, tool-response/transport bytes,
  embedding/extraction/maintenance/judge usage, retries, failed and aborted requests.
- Memory-attributable cost and **total** cost per attempted/successful task. Amortize write/index costs
  over a disclosed session count and also publish raw totals. Do not hide setup/maintenance overhead.
- Wall-clock task time, peak memory, database growth, writer-lock wait, and latency distributions below.

Pin the target tokenizer/library and serialize/count the actual injected string. The current
`utf8_bytes_upper_bound` counter is an engine safety bound, not evidence of model-token consumption.
Record model/gateway usage separately; billed request tokens may differ due to framing and caching.

### Latency boundaries

Measure with monotonic clocks and publish sample counts, distributions and error counts:

| Boundary | Include |
| --- | --- |
| Engine recall | Query validation, eligibility, ranking and hydration |
| Context assembly | Source refresh/validation, retrieval, deduplication, rendering and token counting |
| Persistent MCP | Request serialization through complete response receipt on an established session |
| SDK call | Process spawn, MCP handshake, operation and shutdown; today's SDKs start one process per call |
| Cold startup/index | Process/database open, first query or full index, reported separately |
| Remote variant | Network and provider work, retries and timeouts, with region/model identifiers |

Use fixed 1k/10k/100k-memory datasets with recorded content-length and scope distributions; separately
vary repository files/bytes, changed-file proportion and concurrent writers. Publish fixture generator
seeds/digests and queries. Disclose machine/OS/filesystem, build profile, warm-up, repetitions and cache
reset method. A fresh process is not necessarily a cold filesystem cache. Target at least 1,000 measured
calls per microbenchmark configuration for tail reporting, and still show uncertainty/sample size.

Never exclude source checks from a figure labelled end-to-end. Report lookup-only and source-validation
costs separately when useful, rather than trading away freshness to hit a target. Do not compare local
warm latency with a vendor's hosted end-to-end headline as though the workloads were equivalent.

## 5. Experimental hygiene and scoring

- Freeze train/development/test task splits by repository/scenario lineage. Tune policies on development
  cases; do not tune against held-out outcomes. Keep test checks outside the agent-visible workspace.
- Publish the held-out suite with the release report; use a new versioned holdout for future tuning cycles
  and accept independently contributed cases. Public fixtures can eventually enter model training, so
  disclose contamination risk and supplement with newly authored tasks.
- Start with at least five repeated runs per stochastic variant/case. Pair seeds where supported, but
  do not claim deterministic model behavior. Report per-case results and paired intervals using cases
  as the independent unit (or a hierarchical bootstrap), not every rerun as an independent sample.
- Predeclare timeouts and retry policy. Timeouts, provider failures and failed tests remain in the primary
  attempted-task denominator; any successful-run-only diagnostic must be secondary and labelled.
- Pin code/model/agent versions, decoding settings and tools. For unpinnable models, record the exact
  returned model identity, date and limitation. Separate provider response caching from prompt caching.
- Use hidden executable tests as the primary oracle; disclose gaps and audit a sample for reward hacking.
  If a semantic judge is unavoidable, blind it to variants, publish rubric/model/cost and human-audit a
  sample. A judge cannot establish permission isolation, provenance correctness or secret erasure.
- Run coding tools in an isolated workspace with controlled network and credentials. Benchmark fixtures
  must be synthetic or redistributable and non-sensitive. Agents must not read held-out answers, another
  variant's database or raw evaluator credentials.
- Selected Cloudflare runs obey the existing gateway-only policy and require approved data transfer.
  Model-free tracks need no credentials; missing live credentials yield “not run,” not a passing result.

## 6. Reproducibility artifact and release gates

Each report must include:

1. Protocol and implementation revisions, fixture manifest/licenses/digests, environment lockfiles and
   exact commands for setup, correctness tests, microbenchmarks and agent runs.
2. All variant configurations, budgets, seeds, train/test separation and tuning history.
3. Machine-readable per-case/run outcomes, raw latency samples, token/cost ledger and redacted traces
   sufficient to trace each injected claim to the evidence/version available at that time.
4. Aggregation code producing tables/plots and paired uncertainty from those raw artifacts, plus all
   failures, excluded runs with predeclared reasons, limitations and negative results.
5. An independent clean-environment reproduction before publishing a performance or superiority claim.

The first public release requires passing correctness/safety fixtures and reproducible baseline results;
it does not require inventing a positive effect. If TinyMem fails to improve outcomes, publish that and
iterate. No observed scope/deletion/staleness/budget violations in the gate suite are acceptable, but
zero observations are not proof that the system is universally safe. Contributions to the benchmark
must remain possible without adopting TinyMem or paying a particular provider.
