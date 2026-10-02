# TinyMem

**Open-source memory for coding agents. Remember the decision, not the entire conversation.**

TinyMem is an MIT-licensed, local-first memory layer for carrying repository decisions, corrections,
and conventions across coding sessions. The Rust core stores evidence in SQLite and exposes it through
stdio MCP and Python/TypeScript SDKs. Default operation needs no account, API key, model, or network service.

The goal: help an agent avoid repeating a mistake, follow repository conventions, and recall why a
technical decision was made—without filling its context window with old or irrelevant notes.

**Status: experimental Pi-only beta (`0.1.0-beta.1`).** The beta bundles the native engine,
SDK and optional Pi extension for the specifically qualified platform/runtime cells in the [release guide](docs/PI_BETA_RELEASE.md).
Release archives are not yet published (no download URL; see the [qualification ledger](docs/PI_BETA_CHECKS.md)).
Its clean-room acceptance verifies integration/lifecycle behavior, not whether memory improves natural coding outcomes.
It does not claim support for other agents, portable export, remote source resolvers, or a coding-task quality advantage.
The standalone native MCP/SDK surfaces remain experimental development interfaces.

## What matters for coding memory

| Priority | Available today | Next release work |
| --- | --- | --- |
| Provenance and staleness | File origins/dependencies with captured hashes; stale/unknown exclusion from current reads; versioned audit and freshness inspection | Commit/PR/conversation references, reapproval and resolver policies |
| Quality per token | Ranked, deduplicated context packed into a caller budget with citations | Target-tokenizer support over MCP/SDKs; publish coding outcomes across budget sizes |
| Write-time quality | Explicit writes and exact-scope normalized deduplication | Select durable decisions/corrections/conventions; idempotent proposals, conflict review and provenance-preserving merges |
| Latency | Model-free lexical recall in a local engine | Reproducible cold/warm p50/p95/p99 results, including MCP overhead and source validation |
| Scoping and control | Exact user/agent/project namespaces, deletion and immutable audit | Scoped export/purge; authenticated team/org access as a separate, tested phase |
| Portability | MIT source and user-owned local storage | Versioned open interchange format and validated export/import round trips |

**Scopes are not permissions.** Anyone who can read the database can access its contents. There is no
native authenticated multi-user service. Deletion hides current evidence but does not erase audit,
SQLite pages, WAL files, or backups.

## Install the Pi beta

The beta package installs into a single trusted Pi project and requires no Rust or Python at runtime. See the
[versioned install, platform matrix, backup/restore and removal guide](docs/PI_BETA_RELEASE.md); do not infer
support for platforms or versions not listed there.

## Try the native core (developer/source-checkout path)

Requires Rust/Cargo 1.90+ and a C compiler. From this checkout:

```sh
cargo build --release --locked -p tinymem-engine
./target/release/tinymem index /absolute/path/to/repository
./target/release/tinymem search /absolute/path/to/repository "authenticate"
./target/release/tinymem --db /absolute/path/to/native-memory.db mcp /absolute/path/to/repository --user alice --project my-repo
```

The final command starts a foreground stdio MCP server, not an HTTP service. Configure your agent to
launch it with absolute paths. Use a project scope deliberately: repository root and memory scope are
independent. Omitting `--project` selects the exact global project namespace, not all projects.

- [Pi-only beta release, install guide and qualification matrix](docs/PI_BETA_RELEASE.md)
- [Source-checkout Pi developer setup](docs/PI_LOCAL_SETUP.md) · [legacy local check report](docs/PI_LOCAL_CHECKS.md)
- [Quick start](QUICKSTART.md)
- [MCP setup and agent integration status](MCP_SETUP.md)
- [Native tools, scope semantics and safety](docs/NATIVE_MEMORY.md)
- [Python SDK](sdks/python/README.md) · [TypeScript SDK](sdks/typescript/README.md)

Use `assemble_context` for compact task evidence rather than dumping top-k search results:

```json
{"query":"Update authentication cookie handling","token_budget":2000,
 "sources":["memory","repository"],"exclude_paths":["generated/**"]}
```

Source selection defaults to memory and repository; path exclusions affect repository evidence in
this assembly only, not personal-memory dependencies or explicit search. MCP model-facing text is
only the budgeted prompt; `structuredContent` and SDK results retain entries/provenance/counts.
Insert only `prompt`. The complete prompt, including citations, is budgeted. The current MCP/SDK count
is a conservative UTF-8-byte bound, **not model-token usage**; Rust callers can supply a custom
tokenizer. Standalone lexical search uses literal AND: query with 1–3 distinctive terms, shorten after
zero hits, and avoid redundant recall when automatic context already supplies the answer. See
[context assembly](docs/CONTEXT_ASSEMBLY.md).

## Coding-memory benchmark

We will evaluate whether an agent:

- Avoids a previously corrected mistake and follows repository conventions.
- Recalls a decision after 50 intervening sessions.
- Stops using evidence after its source changes, is deleted, or becomes inaccessible.
- Improves tested coding outcomes at a fixed context budget without leaking another scope's memory.

The [benchmark protocol](docs/CODING_MEMORY_BENCHMARK.md) specifies paired baselines, temporal fixtures,
held-out tasks, latency boundaries, token/cost accounting, failure reporting and reproducible artifacts.
It is a **specification, not a published coding-quality result or implemented coding-task runner**.
A [model-free local latency workload](docs/PI_LOCAL_CHECKS.md#local-latency-baseline) now retains raw
samples; it does not establish task effectiveness. Conversational benchmarks can be supplementary.

## Roadmap and open-source development

The [coding-agent roadmap](docs/MEMORY_SYSTEM_PLAN.md) is authoritative. Day-one release gates include
native MCP, installable Claude Code/Cursor integrations, portability, and reproducible measurements—not
just a working database. Pi remains the initial evaluation runtime; the existing Cloudflare adapter is
an optional controlled evaluation integration, not an installation requirement for the core.

- [Architecture](ARCHITECTURE.md) · [Domain language](CONTEXT.md)
- [Contributing](CONTRIBUTING.md)
- [Pi/Cloudflare evaluation setup](docs/PI_GATEWAY_TESTING.md)
- [MIT license](LICENSE)

The legacy Python `tinymem` package, HTTP/company-brain prototype, and root Python examples are frozen
historical implementations. They are not the native product, do not share its database, and must not
be presented as authenticated enterprise memory. `pip install tinymem` is **not** installation of this
Rust core; the separate native Python adapter lives in `sdks/python`.
