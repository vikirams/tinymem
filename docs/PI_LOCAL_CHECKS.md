# Local Pi readiness and checks

Verified on **2026-10-02**, macOS arm64, Pi **1.0.0**, Node **26.1.0**, native TinyMem
**0.1.0**, schema v4. Experimental source-checkout integration, not a public binary release.

## What is ready

- Local Rust/SQLite/FTS5 memory: no memory account, hosted server, model or vector service required.
- Native stdio MCP and dependency-free SDKs; exact configured owner/project/optional-agent scope.
- `scripts/setup-pi.py`: reviewable dry run, private database location, safe config merge,
  conflict/duplicate-server safeguards; does not open a database or change credentials.
- Optional `integrations/pi/index.ts`: budgeted evidence before each model request, including tool
  follow-ups; latest queued user text drives recall. Evidence is untrusted, request-local and not saved
  into the conversation. No cross-call cache of source validity, old injected notes or full transcripts.
- `/memory remember`, `correct`, `delete`, `recall`, `status`, `on`, `off`. Corrections/deletion require
  a version. Model-issued writes to the selected server are blocked unless explicitly enabled.
- Full task text up to 64 KiB for assembly; deterministic selection of up to 32 useful literal terms.
  Whole-prompt byte budgets and source checks remain enforced. Standalone search limits are unchanged.

**Pi is ready for local developer testing now.** Memory stays local; Pi's own coding model may still
use your chosen external provider. Loading this extension does not make the coding model local.

For this existing checkout, leave the experimental database/configuration intact and try:

```sh
cargo build --locked -p tinymem-engine
pi --extension "$PWD/integrations/pi/index.ts" --tinymem-server tinymem-test
```

Then `/memory status`, `/memory remember <durable decision>`, start a new Pi process with the same
options, and ask a relevant coding question. `/memory recall <task>` shows the selected evidence.
Use [the setup guide](PI_LOCAL_SETUP.md) for a different repository or an intentional permanent setup.
The existing `.pi/mcp.json`, test database and global Pi settings were not changed during these checks.

## Deterministic verification

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --locked -p tinymem-engine
python3 -m unittest tests.test_setup_pi -v
npm test
TINYMEM_REQUIRE_PI=1 npm run test:pi
python3 scripts/smoke-pi-mcp.py --binary target/release/tinymem
(cd sdks/python && TINYMEM_REQUIRE_INTEGRATION=1 python3 -m unittest discover -v)
(cd sdks/typescript && TINYMEM_REQUIRE_INTEGRATION=1 npm test && npm run typecheck && npm run build)
```

Results: **102 native tests**, **16 setup tests**, **8 extension tests**, **1 real-Pi runtime test**,
**17 Python SDK tests**, **17 TypeScript SDK tests**; all passed without skipped required coverage.
Formatting, Clippy and TypeScript SDK type/build checks passed. A separate temporary strict TypeScript
configuration checked the extension against installed Pi 1.0.0 declarations; no machine-specific
configuration was committed. Pi MCP discovery found all eleven tools.

The real-Pi test uses Pi's actual resource loader, slash-command pipeline, context conversion and
agent loop with an in-process deterministic provider. It creates a disposable repository, database,
auth directory and two independent in-memory Pi sessions. It verifies intentional remember → new
session recall → correction → deletion, file-dependent invalidation, off control, no transcript
accumulation, and a nested call to a deferred write blocked before execution. It does not use personal
auth or make model/network requests. `TINYMEM_REQUIRE_PI=1` prevents a missing Pi installation from
silently skipping this test. `TINYMEM_PI_MODULE` can select a module entry explicitly.

An `npm pack` archive was also extracted outside the checkout and the real-Pi test rerun with
`TINYMEM_PI_EXTENSION` pointing at its entry. It passed, checking that the package includes the native
SDK source required by the extension. This is package-closure verification on an existing developer
machine, not installation of published artifacts on a clean machine.

The [earlier explicit live-agent check](PI_NATIVE_MCP_CHECKS.md) remains separate: it used a real model
through Pi's built-in MCP tools, but did not test automatic extension context or natural coding quality.
The new CI job pins Pi 1.0.0 and performs model-free checks on Linux/macOS; those CI/platform runs are
configured, not claimed as executed by this local report. A separate one-question live Pi/Grok
recall-efficiency repeat is documented in the [recall-efficiency report](PI_GROK_RECALL_EFFICIENCY.md);
it is not part of these deterministic test counts or a coding-outcome benchmark.

## Local latency baseline

```sh
cargo build --release --locked -p tinymem-engine
python3 scripts/benchmark-local.py --samples 50 --output /tmp/tinymem-latency.json
cargo run --release --locked -p tinymem-engine --example context_benchmark
```

The local MCP workload uses **100 synthetic memories, 25 tiny Python files, one explicit file
dependency, 4096 byte-bound budget**, 50 measured calls per transport, warm filesystem cache,
no concurrent writers and no provider. It checks that the intended current note fits, that every
returned prompt respects its budget, and removes its temporary database/repository afterward.

| Measured surface | p50 | p95 | p99 |
| --- | ---: | ---: | ---: |
| Persistent native MCP assembly, including refresh/dependency checks | 3.81 ms | 6.77 ms | 9.26 ms |
| One-operation process, including start/initialize/EOF/exit/reader cleanup | 10.34 ms | 15.16 ms | 21.26 ms |

The second surface is a Python stdlib **SDK-shaped** transport, not a measurement of the TypeScript
SDK or full Pi/model latency. Pacing of persistent calls occurs outside timing to avoid the MCP rate
limiter; throughput is not measured. With 50 samples, p99 is effectively the maximum. No performance
threshold or comparison against another product is established.

[Raw samples and binary hash](benchmarks/local-macos-arm64-20261002.json) are retained. That run's
single start/initialize observation was **18.59 ms**, and first repository refresh **12.48 ms**.
An [earlier first-run artifact](benchmarks/local-macos-arm64-first-run-20261002.json) retains a much
slower **840.74 ms** start/initialize observation rather than hiding it. This is not enough data to
characterize cold-start tails or establish the cause of that outlier.

Astra's in-process packing comparison (200 assemblies, 100 roughly 2-KiB memories, no repository,
source dependencies, provider or transport) measured large-budget packing **2374 ms → 284 ms** after
reusing serialized accepted entries. At a 4000-unit budget it measured **345 ms → 300 ms**. Exact
prompt/budget tests stayed green. These are narrow optimization measurements, not end-to-end Pi gains.
The shipped Rust example reproduces the current workload, not the old implementation.

## Still pending

1. **Natural coding-task evaluation:** no-memory vs TinyMem on repeated-mistake/convention fixtures,
   held-out tasks, equal budgets, source changes, intervening sessions and outcome tests. Neither an
   offline provider fixture nor a prompted MCP lifecycle proves usefulness.
2. **Clean-machine distribution:** published checksummed native artifacts, supported OS/version
   matrix, complete install/restart/uninstall acceptance. Source-package closure is only one step.
3. **Efficiency at realistic scale:** cold/warm tails, real SDK/full-Pi traces, larger repositories,
   concurrent writers, target-tokenizer transport and relevance/budget sweeps.
4. **Memory lifecycle depth:** write-quality proposals/conflict review, explicit supersession with
   richer provenance, dependency reapproval, versioned export/import and scoped purge.
5. **Other agents:** verify Claude Code/Cursor/Codex mechanisms and actual client lifecycle before
   declaring support. MCP compatibility is a foundation, not a tested plugin.

Known limits: SDK cancellation waits for its bounded deadline/cleanup rather than immediate child
cancellation; `/memory` inspection/notifications are most useful in TUI/RPC (print modes suppress UI).
Automatic recall supports only plain trusted project-local native lexical configuration, not HTTP,
wrappers, custom env/cwd or embedding commands. Reads are point-in-time checks, not a filesystem
snapshot. Old evidence already paraphrased by the model in ordinary conversation cannot be removed by
filtering the extension's own request-local message. Deletion is not secure erasure, scopes are not
permissions, and repository indexing is not a secret scanner. See the [roadmap](MEMORY_SYSTEM_PLAN.md).
