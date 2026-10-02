# Pi/Grok recall-efficiency check

Verified locally on **2026-10-02**, macOS arm64, Pi **1.0.0**, native TinyMem **0.1.0**.
This is a recall/context experiment, **not a coding-outcome benchmark**.
[Sanitized measurements](benchmarks/pi-grok-recall-efficiency-20261002.json) retain counts, usage,
configuration and private-artifact hashes without publishing memories, scopes or transcripts.

## Implemented

- Pi `--tinymem-context-sources` and `--tinymem-context-exclude` select automatic evidence only.
  Default remains both sources, with no exclusions; there is no new repository-specific default.
- Native assembly applies repository path exclusions before candidate/oversampling/byte limits and
  packing. It still refreshes source state. Memory dependencies and provenance are not path-filtered.
- `/memory recall`, explicit `search_repository` and model-issued assembly remain unrestricted unless
  that particular assembly call explicitly supplies selection options. The index is not pruned.
- Assembly's MCP text is exactly the budgeted prompt, not an envelope containing both prompt and
  duplicate entries. Full entries, provenance and counters remain in `structuredContent` and SDK results.
- Tool descriptions and trusted adapter guidance explain literal AND matching: use 1–3 distinctive
  terms, shorten on zero hits, and avoid recalling the same evidence solely to reconfirm injection.
  These are guidance, not a block on useful searches or verification.

See [configuration](PI_LOCAL_SETUP.md) and [assembly semantics](CONTEXT_ASSEMBLY.md).

## Method

The exact question in all three turns was:

> What requirements should I preserve when modifying TinyMem retrieval?

The historical first turn selected **`xai/grok-4.7`, `xhigh`**, with `openai-responses` reported by Pi.
Only that first user question through the first final assistant response was counted, excluding later
inspection turns. Its automatic injection was request-local and not persisted: the 4,093-byte
baseline comes from a historical reviewer replay of that exact question, not an original provider
request capture. It contained the intended decision and frozen Python MCP schema code.

Two fresh, non-persisted CLI processes repeated the question with the existing project-local native
MCP server, exact existing owner/project scope, no agent ID, and the same 4,096-byte budget. Existing
Pi credential resolution was used without reading/copying credentials or changing global settings.
`--approve` granted trust for each process only. The initial **20 tool names were identical** to the
historical session; updated native descriptions/schemas and adapter guidance were part of the treatment.

Common invocation, from the trusted checkout:

```sh
PI_OFFLINE=1 PI_SKIP_VERSION_CHECK=1 PI_TELEMETRY=0 \
pi --approve --no-session --mode json \
  --extension "$PWD/integrations/pi/index.ts" \
  --tinymem-server tinymem-test --tinymem-budget 4096 \
  --provider xai --model grok-4.7 --thinking xhigh \
  <variant options> \
  'What requirements should I preserve when modifying TinyMem retrieval?'
```

The two variant options were:

```sh
# Both sources; exclude this checkout's historical Python surfaces only in auto context.
--tinymem-context-exclude '["tinymem/**","mcp_server.py","example.py","examples/mcp_workflow.py","pyproject.toml","tests/test_core.py","ARCHITECTURE.md"]'

# Explicit opt-in to decision/memory-only auto context; repository tools still enabled.
--tinymem-context-sources memory
```

A read-only observer extension, loaded after TinyMem, recorded its request-local custom evidence
without changing prompts, messages or tools. JSON events supplied completed response usage and tool
results. Hashes of all personal-memory and audit rows were identical before/after each run; only
normal derived repository refresh was permitted. New live contexts were observed before **every**
model request, not reconstructed afterward. Each request had exactly one current injected context message.

Private driver and raw artifacts stay outside the repository under
`~/.local/share/tinymem/pi-recall-efficiency/`. The driver is a local experiment, not a shipped
benchmark runner; original shell argv and a fixed baseline source snapshot are unavailable.

## Results

| Metric | Historical baseline | Legacy-path exclusions | Memory-only automatic context |
| --- | ---: | ---: | ---: |
| Automatic prompt bytes, every request | 4,093 | 3,751 | 493 |
| Frozen legacy bytes added to automatic prompt | 3,600 (~88%) | 0 | 0 |
| Automatic evidence | Decision + frozen MCP code | Decision + current `MCP_SETUP.md` | Decision only |
| Model responses | 4 | 4 | 3 |
| All tool calls | 7 | **9** | 6 |
| Native MCP calls | 7 | 4 | 6 |
| Tool/model errors | 0 | 0 | 0 |
| Pi-estimated model cost (USD) | $0.110536 | $0.099350 | $0.061884 |
| Required decision preserved in final answer | Yes | Yes | Yes |

Memory-only injection used **88.0% fewer bytes** than the historical replay and its observed cost was
**44.0% lower**, with one fewer response/call. Excluding legacy paths removed the known irrelevant
code, but automatic context stayed large and **total calls increased**, despite fewer MCP calls and
10.1% lower estimated cost. Neither configuration establishes that guidance reliably reduces calls.
The current setup excerpt is more applicable than frozen tool schemas, not a general relevance score.
Memory-only context also omits potentially useful current repository evidence: it is an opt-in, not a
claim that disabling repository injection is the right default.

Both new lexical memory searches used short queries and returned the intended memory; the historical
long AND query had returned zero because its extra `requirements` term was absent. Redundant recall
was **not eliminated**: both runs manually assembled context, and the memory-only run searched the
same memory twice. Missing-detail repository searches remained available.

### Compact response and explicit-search escape hatch

The historical manual assembly used a different, expanded query and returned a 3,808-byte prompt in
**7,789 bytes of model text**, duplicating prompt and entries. In the two new live calls, text was
exactly the structured prompt (**3,751** and **3,476 bytes**), with two full structured entries each.
These are different selected evidence, not a same-payload before/after size comparison. Native and SDK
regression tests establish the exact text/structured contract independently of model behavior.

In the memory-only run, the explicit manual assembly still returned `tinymem/scoring.py`, and three
explicit repository searches executed successfully. Thus the automatic setting did not hide legacy
source from intentional lookup or alter the underlying repository index.

### Usage and interpretation

Usage counts include repeated context across requests. Historical input/output/cache-read totals were
40,871 / 2,143 / 31,872; exclusions used 28,077 / 2,922 / 51,328; memory-only used
20,942 / 1,104 / 26,752. Reported reasoning is already included in output, not added again. Costs use
Pi's selected catalog rates ($2/M uncached input, $6/M output, $0.50/M cache read), **not verified
provider invoices or subscription charges**. The selected upstream identity was not independently
verified.

The baseline persisted user→final span was 41.475 seconds; new whole-process times, including startup,
were 41.354 and 18.371 seconds. These measure **different boundaries** and are not a latency comparison,
nor an isolated TinyMem latency measurement.

## Regression coverage and limits

Regression tests cover assembly-only source/path selection, pre-cap starvation, invalid filters,
exact scopes, file dependency freshness despite path exclusions, whole-entry budgets/provenance,
compact MCP text with complete structured results, both SDKs, and automatic-vs-explicit Pi behavior.
Existing correction/deletion/expiry, deduplication, request-local context, custom counters and real-Pi
fixture checks remain applicable.

Passed after the changes: **109 native tests, 9 Pi extension tests, 1 real-Pi fixture test,
17 Python SDK tests, 17 TypeScript SDK tests, and 16 setup tests**, with no required coverage skipped.
Native formatting/Clippy, both SDK type checks, Python source lint and TypeScript build passed;
model-free Pi MCP discovery still found all eleven tools. These checks are separate from paid/live
model measurements.

There was one historical baseline and one live run per new configuration, sequentially, not randomized
paired trials. Source/docs changed since the baseline and were still under development during the new
runs. Provider caching, final-answer length and stochastic tool choices confound cost/call attribution.
No code was modified by Grok, and no outcome test, held-out relevance score, default-policy improvement,
source-scale performance guarantee or other-model/platform claim is established.
