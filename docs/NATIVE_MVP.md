# Native memory MVP implementation contract

This extends the approved Rust repository vertical slice into a usable local-first assistant memory layer. Public seams under test: native Engine memory/repository operations, newline-delimited stdio MCP requests, and Python/TypeScript SDK adapters against the real binary. Existing Python modules and their database stay unchanged; no Git metadata is available, so review uses changed files directly and no commit is requested.

This records the baseline v2 MVP contract. The subsequent [semantic retrieval slice](SEMANTIC_RETRIEVAL.md)
adds optional embeddings, a tenth MCP tool and migration to v3; default lexical behavior remains intact.
The subsequent [context assembly slice](CONTEXT_ASSEMBLY.md) adds an eleventh tool and prompt budgeting
without changing the schema. The [file provenance slice](FILE_PROVENANCE.md) now migrates to v4,
adding explicit file origins/dependencies and read-time freshness across these interfaces.

## Required behavior

- One SQLite engine stores repository evidence separately from personal memory.
- Personal memory is isolated by an exact `(user_id, optional agent_id, optional project_id)` scope. A required nonempty user identifies the owner; omitting agent/project is a distinct global scope, not permission to read all scopes. Global memory can be deliberately shared by multiple assistants configured with the same user and database.
- Explicit add/get/search/update/delete/history/profile operations. Deduplication is scope-local and includes kind/content/source/expiration. Updates keep a stable memory ID and immutable versioned observations; optional expected-version prevents lost updates. Deletion hides current evidence while retaining an audit event (not secure erasure). A deleted record cannot be resurrected by update.
- Kinds: fact, preference, decision, plan, relationship, architectural_decision, issue_solution. Provenance is an explicit source string; there is no automatic truth extraction or fabricated confidence. Optional expiration excludes memories from current retrieval/profile without deleting history.
- Profiles separate stable facts/preferences/relationships from dynamic decisions/plans/solutions. Profiles return current memory evidence; no LLM-generated summary is implied. Search/history/profile results are bounded by result count and serialized-byte budgets.
- Migrate native schema v1 to v2 transactionally while preserving repository data. Reject foreign/newer databases. No legacy Python migration.
- Native stdio MCP server: initialize/initialized/ping, tools/list/tools/call; MCP version negotiation; valid single-line JSON-RPC stdout only. Startup fixes repository and user/scope; tool arguments cannot override them. Every code search refreshes first. Memory writes are explicit tool calls. No network listener, auth service, or external models.
- MCP tools: add_memory, get_memory, search_memories, update_memory, delete_memory, memory_history, get_profile, refresh_repository, search_repository. Tool errors use isError content; protocol errors use JSON-RPC error responses. Inputs/output/resource sizes are bounded; invalid notifications do not become memory writes. Read requests never mutate personal memory.
- Preserve existing index/search/watch CLI behavior. Add `mcp ROOT --user USER [--agent AGENT] [--project PROJECT]`, retaining global --db.
- Python and TypeScript adapters configure binary, database, repository and fixed user/scope. Use real MCP stdio requests without shell interpolation; bounded output and finite request timeouts; support stable-ID optimistic updates. No sentence-transformers, NumPy or provider dependencies.
- Tests cover restart persistence, scoped duplicate content, CRUD/history/deletion/expiration, profiles, conflicts/invalid inputs, schema migration, actual MCP handshake/tool calls/errors/scope isolation, actual SDK end-to-end behavior and existing repository regressions. Build/typecheck/lint all touched surfaces.

## Explicitly not claimed complete

No perfect code understanding, autonomous extraction, semantic embeddings, exact tokenizer budgets, complete dependency/call graph, enterprise tenancy/auth, HTTP server, OS-event watcher, verified OpenClaw/Hermes-specific installation, published packages, or multi-platform benchmarks. These need their own tested slices; local MCP interoperability alone is not proof of any particular assistant integration.
