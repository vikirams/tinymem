# Contributing to TinyMem

TinyMem focuses on open-source memory for coding agents. Start with the [roadmap](docs/MEMORY_SYSTEM_PLAN.md),
[domain language](CONTEXT.md), [native contract](docs/NATIVE_MEMORY.md) and
[benchmark protocol](docs/CODING_MEMORY_BENCHMARK.md). The shipped v4
[file-provenance contract](docs/FILE_PROVENANCE.md) distinguishes capture, current eligibility and
historical freshness inspection. The core is [MIT licensed](LICENSE).

## Where to work

- `crates/tinymem-engine/`: native storage, repository evidence, context selection and stdio MCP.
- `sdks/python/` and `sdks/typescript/`: dependency-free adapters for the native binary.
- `integrations/pi/`: optional native-first Pi context/command extension; `npm test` and `TINYMEM_REQUIRE_PI=1 npm run test:pi` cover its local behavior.
- `integrations/cloudflare/`: optional controlled provider integration; not a prerequisite for core contributions.
- `docs/`: current contracts, planned work and evidence-backed reports.

The root Python package, HTTP/company-brain server and Python examples are historical prototypes,
not the foundation for new features. Propose a small issue/design before changing public APIs, schema,
scope semantics or inference policy. Do not build another agent runtime.

## Local checks

Use Rust/Cargo 1.90+ and a C compiler:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --locked -p tinymem-engine
```

For Pi/setup changes, run `python3 -m unittest tests.test_setup_pi -v`, the root npm tests above,
and `python3 scripts/smoke-pi-mcp.py`. Pi runtime tests use an offline fixture provider, not credentials;
[local checks](docs/PI_LOCAL_CHECKS.md) distinguish that from natural coding-task evaluation.

For SDK changes, run the type, unit, real-binary integration and package checks in the
[Python guide](sdks/python/README.md) or [TypeScript guide](sdks/typescript/README.md).
Set `TINYMEM_REQUIRE_INTEGRATION=1` so a missing binary does not silently skip required coverage.
Live inference tests are optional and require explicit credentials/data policy; deterministic correctness
must remain runnable without a model account. Never commit private memories, databases or credentials.

## Contribution expectations

- Keep changes small and explain the coding-agent use case, scope and observable behavior.
- Test public behavior, especially migration, conflicts, source changes, budgets, expiry/deletion and
  scope boundaries. New transport fields need Rust/MCP/Python/TypeScript compatibility tests.
- Keep memory/source text as untrusted evidence. Reads do not accept proposals or create durable memories.
- Preserve provenance and audit; distinguish current reads, historical access and secure-erasure limits.
- No mandatory cloud dependency, hidden telemetry or unreviewed model/network call in core operations.
- Do not claim permissions from namespace filtering or freshness from a free-text source label.
- Include fixture licenses and reproducible commands. Performance claims need workload definitions,
  baseline comparisons and raw samples; report regressions and failures as well as wins.
- Update docs to distinguish implemented, tested, planned and unmeasured behavior. List checks actually
  run and any platform/live verification still missing in the contribution description.

Benchmark cases and third-party memory adapters are welcome even if they show TinyMem performing poorly.
Do not tune against held-out tasks or make a result better by changing another variant's resources.
