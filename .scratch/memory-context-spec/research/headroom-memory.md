# Headroom persistent memory (primary-source research)

Question: What is Headroom's persistent-memory model — scopes, versioning, dedup, provenance, tools vs auto-inject — as distinct from its compression proxy, as stated by its own docs and source?

Retrieved: 2026-10-02. GitHub default branch `main` (API SHA `65e63c08835410149bec4acba8e3737366eb0586`).

Sources used (Headroom only; no secondary blogs):

- Repo: https://github.com/headroomlabs-ai/headroom
- README: https://github.com/headroomlabs-ai/headroom/blob/main/README.md
- Official docs: https://docs.headroomlabs.ai/docs/memory (source: `docs/content/docs/memory.mdx`)
- Wiki / GitHub Pages: https://headroomlabs-ai.github.io/headroom/memory/ (source: `wiki/memory.md`)
- Proxy docs: https://docs.headroomlabs.ai/docs/proxy
- Architecture (compression, not memory): https://docs.headroomlabs.ai/docs/architecture
- Filesystem contract: https://docs.headroomlabs.ai/docs/filesystem-contract
- Configuration: https://docs.headroomlabs.ai/docs/configuration
- Source: `headroom/memory/models.py`, `core.py`, `config.py`, `wrapper.py`, `inline_extractor.py`, `tools.py`, `wrapper_tools.py`, `ports.py`, `backends/local.py`, `adapters/sqlite.py`, `storage_router.py`, `mcp_server.py`, `headroom/proxy/memory_handler.py`

**Source-of-truth rule used here.** Where wiki/Pages and `memory.mdx` disagree, the Python model and handler win. Discrepancies are called out in place.

---

## 1. Product split: compression proxy vs persistent memory

Keep these as two products that share a process, not one model.

**Compression** is the default product. The README lead is "Compress tool outputs, logs, files, and RAG chunks before they reach the LLM" ([README.md](https://github.com/headroomlabs-ai/headroom/blob/main/README.md)). Architecture docs describe intercept → compress → forward; they do not describe the memory store ([docs/architecture](https://docs.headroomlabs.ai/docs/architecture)). Entry points are `compress()`, `headroom proxy`, `headroom wrap`, MCP compress/retrieve/stats.

**Persistent memory** is an opt-in sidecar on that same proxy/wrap path, plus a separate SDK wrapper:

| Surface | How you turn memory on | What it does |
|---|---|---|
| Proxy | `headroom proxy --memory` (default **off**) | Injects `memory_*` tools and (optionally) retrieved context; stores in SQLite ([docs/proxy](https://docs.headroomlabs.ai/docs/proxy), `wiki/memory.md`) |
| Wrap | `headroom wrap <agent> --memory` | Starts the proxy with memory and routes the agent through it (`wiki/memory.md`; README agent table lists `--memory` for Claude Code) |
| SDK inline | `with_memory(client, user_id=...)` | Intercepts chat completions; inline JSON extraction; no proxy required (`docs/content/docs/memory.mdx`, `headroom/memory/wrapper.py`) |
| SDK tools | `with_memory_tools(client, ...)` | Function-calling tools instead of inline extraction (`headroom/memory/__init__.py`, `wrapper_tools.py`) |
| MCP | `python -m headroom.memory.mcp_server` (registered by `headroom wrap codex --memory`) | stdio tools `memory_search` / `memory_save` only (`headroom/memory/mcp_server.py`) |

The proxy flag table treats compression and memory as independent switches: `--memory` default `false`; `--no-memory-tools` and `--no-memory-context` can disable each half even when memory is on ([docs/proxy](https://docs.headroomlabs.ai/docs/proxy)). `--learn` implies `--memory`. `--stateless` disables filesystem writes including memory.

Headroom itself names the memory feature **"temporal compression"** (carry 100 tokens of extracted facts instead of 10k of history) (`docs/content/docs/memory.mdx`). That is a slogan for *fact extraction*, not the token-crushing pipeline (SmartCrusher / CCR / CacheAligner). Mixing those two "compressions" is the main confusion to avoid.

CCR (`headroom_retrieve`) is reversible **payload** cache for compressed tool output ([docs/architecture](https://docs.headroomlabs.ai/docs/architecture)). It is not the memory store.

`[memory]` is a separate install extra from `[proxy]` ([docs/installation](https://docs.headroomlabs.ai/docs/installation); README extras list).

---

## 2. Nouns and scopes (user / session / agent / turn)

The stored noun is `Memory`: one content string plus hierarchy IDs, temporal fields, importance, lineage, entity refs, optional embedding, and a free-form `metadata` dict (`headroom/memory/models.py`).

There is **no stored `scope` enum**. `scope_level` is computed from which IDs are set, narrowest wins:

```text
if turn_id:    TURN
elif agent_id: AGENT
elif session_id: SESSION
else:          USER
```

(`Memory.scope_level` in `headroom/memory/models.py`.)

| Noun | What it IS (source wording) | Required? |
|---|---|---|
| **user_id** | Top of hierarchy; "Persistent across all sessions" (`ScopeLevel.USER`). Isolation key: Bob cannot see Alice's rows even on the same DB (`wiki/memory.md` "Multi-User Isolation"; `add()` requires `user_id` in `core.py`). Proxy default: OS `$USER`, override `x-headroom-user-id` (`wiki/memory.md`). | Required on every row (`sqlite.py` `user_id TEXT NOT NULL`) |
| **session_id** | "Persistent within a task/conversation" (`ScopeLevel.SESSION`) | Optional |
| **agent_id** | "Persistent within an agent's lifetime" (`ScopeLevel.AGENT`). Docs table: "Current agent in session" (`wiki/memory.md`). **Not** the same as provenance `metadata.source_agent` (see §5). | Optional |
| **turn_id** | "Ephemeral, single LLM call" (`ScopeLevel.TURN`) | Optional |

`user_id` is the only required scope field. Narrower IDs nest under it; they do not replace it.

**What bubbling does to these nouns.** High-importance memories at SESSION/AGENT/TURN are **copied** (not moved) to USER: `session_id`/`agent_id`/`turn_id` cleared, `promoted_from` + `promotion_chain` set (`HierarchicalMemory._maybe_bubble` in `core.py`). Defaults: `auto_bubble=True`, `bubble_threshold=0.7` (`config.py`). `with_memory` stores extracted memories at `importance=0.7` (`wrapper.py`), so with a `session_id` they land at SESSION **and** get a USER copy.

Docs table vs example: the `with_memory(..., session_id="morning-session")` snippet comments "Memory stored at USER level" (`wiki/memory.md`). Source stores with the provided `session_id`, then bubbles. The comment is the bubbled outcome, not the first write.

**Physical project scope (proxy only, orthogonal to the four IDs).** Default `MemoryStorageMode.PROJECT` opens a **different SQLite file per resolved workspace** so cross-project bleed is structurally impossible (`storage_router.py`, GH #462). Resolution order: `x-headroom-project-id` → `x-headroom-cwd` → CLI `--memory-project-root` → cwd line in system/user prompt. Unresolved PROJECT requests fail closed (skip injection) unless `unresolved_project_fallback="global"`. USER mode = one DB per user-id; GLOBAL = one shared DB (pre-fix behavior).

---

## 3. Categories / kinds of memories

**Source: there is no first-class category on `Memory`.** `models.py` has `importance` and `metadata`, not `category`. `HierarchicalMemory.add()` has no `category` param (`core.py`). Official docs now say this explicitly and tell you to put `metadata={"category": "fact"}` (`docs/content/docs/memory.mdx`).

SQLite still has `category TEXT NOT NULL` and an index, but writes `""` with the comment "Deprecated - kept for backwards compatibility" (`adapters/sqlite.py` `_memory_to_row`).

**Convention (docs examples only), not an enum in code:**

| Conventional string | Docs meaning |
|---|---|
| `preference` | Likes, dislikes, preferred approaches |
| `fact` | Identity, role, constraints |
| `context` | Current goals, ongoing tasks |
| `entity` | Information about entities |
| `decision` | Decisions made |
| `insight` | Derived insights |

(`docs/content/docs/memory.mdx`; same table in `wiki/memory.md` as `PREFERENCE` / `FACT` / …)

**Wiki/Pages lag.** `wiki/memory.md` and GitHub Pages still show `category=MemoryCategory.FACT` and `client.memory.add(..., category="fact")`. `MemoryCategory` is **not** in `models.py` / `core.py` / `wrapper.py` as of this snapshot. Treat those examples as stale.

Tool descriptions (`tools.py`) talk about preferences, personal facts, project context, decisions, relationships, technical insights as **what to save**, not as stored kinds. Optimized `memory_save` can pass `facts[]` (each fact stored as its own row — `LocalBackend.save_memory`).

`importance` (0.0–1.0, default 0.5) **is** first-class: retrieval priority, bubbling threshold, `MemoryFilter.min_importance` (`ports.py`, `tools.py` tool text).

---

## 4. Temporal versioning / supersession

A memory is current iff `valid_until is None` (`Memory.is_current` in `models.py`).

`HierarchicalMemory.supersede(old_memory_id, new_content)` (`core.py`):

1. Copies scope IDs, importance, entity_refs, metadata onto a new row.
2. Store links the chain (`supersedes` / `superseded_by`) and sets the old row's `valid_until`.
3. Drops the old id from vector + FTS indexes (history remains in SQLite; default search will not return it).
4. Indexes the new row.

Query default: `MemoryFilter.include_superseded=False`. Point-in-time: `valid_at`. Full chain: `get_history(id)` (oldest first). `detach_supersession` exists to undo one edge.

**Proxy `memory_update` uses supersession when the backend has `update_memory`.** `LocalBackend.update_memory` calls `supersede()` (`backends/local.py`). The `reason` argument is accepted and discarded (`_ = reason`). The proxy still logs `"Updated by {agent} via {provider}: {reason}"` and keeps an in-memory `edit_entry`; that reason is **not** written onto the Memory row unless some other path puts it in metadata.

Fallback if the backend has no `update_memory`: delete + save, with `metadata.supersedes_id` (`memory_handler.py` `_execute_update`). That is not the `HierarchicalMemory` chain.

**Docs vs code.** `tools.py` `memory_delete` says "Deletions are soft by default". `SQLiteMemoryStore.delete` is `DELETE FROM memories WHERE id = ?` (hard delete). Supersession is the versioning path; delete is removal.

MCP `memory_save` will **not** auto-supersede on similarity: "Only explicit update paths with a caller-supplied memory ID may create a supersession chain" (`mcp_server.py`). The MCP server currently exposes search+save only — no update tool — so MCP writes cannot version.

---

## 5. Dedup and provenance (which agent wrote it)

### Provenance

Proxy `memory_save` stamps metadata (`memory_handler.py` `_execute_save`):

```text
source_agent      = MemoryHandler.agent_type   # e.g. "claude"
source_provider   = provider                   # e.g. "anthropic"
created_via       = "tool_call"
created_at_utc    = ISO-8601 UTC
workspace_display / workspace_key / storage_mode  # when a project scope resolved
```

This is **metadata**, not the `agent_id` scope column. Docs example matches this (`wiki/memory.md` "Agent Provenance").

Updates: proxy builds `edit_entry` with agent/provider/timestamp/reason; `LocalBackend.update_memory` does not persist `reason` (see §4). Docs' `"reason": "Updated by codex via openai: …"` is the log/tool-result string, not a guaranteed column.

SDK `with_memory` extraction path does **not** set `source_agent` (`wrapper.py` `_store_memories`).

### Dedup — docs overclaim relative to source

**What source actually does** (`memory_handler.py`):

1. Save immediately.
2. Semantic-search similar rows (exclude the new id).
3. If top score ≥ `DEDUP_HINT_THRESHOLD` (**0.75** cosine), return a `note` telling the model to `memory_update` the existing id **or ignore**.
4. No background delete. No 0.92 threshold in this file. GitHub code search for `0.92` / `auto_dedup` in this repo returned **zero hits** on 2026-10-02.

**What wiki/Pages still say:** "Background auto-dedup: If similarity >92%, the older duplicate is automatically removed (async, non-blocking)" plus README bullets "automatic dedup" / "auto-dedup". **Not implemented in `memory_handler.py` as of this snapshot.** The live behavior is LLM-mediated merge **suggestion**, not auto-remove.

MCP save is even stricter: similarity is "a retrieval signal, not proof that two memories represent versions of the same fact" (`mcp_server.py`).

---

## 6. Tools vs auto-inject vs inline extraction (`with_memory`, wrap)

Three write/read mechanisms. They are not interchangeable.

### A. SDK `with_memory()` — inline extraction + auto-inject (no tools)

`headroom/memory/wrapper.py` + `inline_extractor.py`. Letta/MemGPT style.

Per `chat.completions.create`:

1. Semantic search on last user message; prepend a context block to the **first** user message (system prompt left intact for cache).
2. Append a memory-extraction instruction to the system prompt (`MEMORY_INSTRUCTION_SHORT` by default): after responding, emit `{"memories": [{"content": "..."}]}` (or empty).
3. Call the LLM.
4. Regex-parse a memory block out of the reply; strip it before returning.
5. `HierarchicalMemory.add(..., importance=0.7)` for each extracted item.

Zero extra API call; cost is ~50–100 extra output tokens (`docs/content/docs/memory.mdx`). Tool-call turns (`content is None`) extract nothing.

Direct API: `client.memory.search/add/get_all/clear/stats` — `stats()` returns only `{"total": N}` (`wrapper.py`; `memory.mdx` notes this).

### B. Proxy / wrap — tools + optional auto-inject

`MemoryHandler` (`headroom/proxy/memory_handler.py`).

**Tools injected** (OpenAI/Anthropic/Gemini native shapes): `memory_save`, `memory_search`, `memory_update`, `memory_delete`, `memory_list` (`MEMORY_TOOL_NAMES`; schemas in `tools.py`). Optional experimental Anthropic native `memory_20250818` (`use_native_tool`).

**Injection modes** (`MemoryMode`):

| Mode | Behavior |
|---|---|
| `AUTO_TAIL` (default) | Retrieve at request entry; append to the **latest user-message tail** (live zone). System/frozen prefix is not mutated (PR-B6 / I2). |
| `TOOL` | `search_and_format_context` returns `None`. Model must call `memory_search`. |

Config flags: `inject_tools`, `inject_context` (CLI `--no-memory-tools`, `--no-memory-context`). Default `top_k=10`, `min_similarity=0.3`. Injected lines include `[id]` so the model can `memory_update`/`memory_delete` without a search round-trip.

Env `HEADROOM_MEMORY_INJECTION_MODE`: `live_zone_tail` (default) or `disabled`. Legacy `system_prompt` raises ([docs/configuration](https://docs.headroomlabs.ai/docs/configuration)). Naming does not match the Python enum (`auto_tail` / `tool`).

Anthropic + `--mode cache` (compression cache mode): README says automatic `--memory` context injection is **skipped** so the provider prefix stays stable. OpenAI/Gemini still append to the live-zone tail. Use `--mode token` if you want auto memory on the Anthropic path ([README.md](https://github.com/headroomlabs-ai/headroom/blob/main/README.md)).

`headroom wrap <agent> --memory` = start proxy with memory + launch the agent through it (`wiki/memory.md`). Cross-agent sharing is "whoever routes through this proxy hits this project DB", not a sync protocol.

### C. MCP memory server — tools only, subset

`memory_search` + `memory_save` (atomic `facts[]`). No update/delete/list. Default DB `{cwd}/.headroom/memory.db`. Warm-up re-embeds rows missing vectors so proxy-saved rows are searchable (`mcp_server.py`).

### D. `with_memory_tools()`

Library path that injects the same function-calling tools (optionally `MEMORY_TOOLS_OPTIMIZED` with pre-extracted facts/entities/relationships) (`__init__.py`, `tools.py`). Distinct from the proxy.

---

## 7. Storage (per-project SQLite, embeddings, FTS)

**Default local stack** (`config.py`, `backends/local.py`, `adapters/sqlite.py`):

| Layer | Protocol | Default adapter |
|---|---|---|
| CRUD + supersession | `MemoryStore` | SQLite (`memories` table) |
| Semantic search | `VectorIndex` | `AUTO` → sqlite-vec if present, else HNSW |
| Keyword search | `TextIndex` | FTS5 |
| Embedder | `Embedder` | LOCAL sentence-transformers; docs recommend ONNX (`BAAI/bge-small-en-v1.5`, 384-d). Also OPENAI, OLLAMA |
| Cache | `MemoryCache` | LRU, max 1000 |
| Graph (optional) | `GraphStore` | SQLite graph (`memory_graph.db`) or in-memory |

Proxy default path: `{cwd}/.headroom/memory.db` ([docs/proxy](https://docs.headroomlabs.ai/docs/proxy); MCP same default). **This path does not obey `HEADROOM_WORKSPACE_DIR`** — intentional project isolation ([filesystem contract](https://docs.headroomlabs.ai/docs/filesystem-contract)). Override: `--memory-db-path`. With PROJECT routing, files are under `storage_root/projects/{key}/memory.db` (`storage_router.py`).

SDK `with_memory` default is `headroom_memory.db` in cwd (`wrapper.py`) — a **different filename** from the proxy's `.headroom/memory.db`. Same library, not automatically the same file.

Optional production backend: `qdrant-neo4j` (`[memory-stack]` extra). Qdrant partitioning uses composite `user_id::project_key` instead of separate files (`memory_handler.py`).

Embedder GPU offload: `HEADROOM_EMBEDDER_RUNTIME=pytorch_mps` + `[pytorch-mps]` extra (`memory.mdx`, `memory_handler.py`).

Schema extras that are **not** on the Python dataclass: SQLite `category` (deprecated empty string). Schema extras that **are** on the dataclass: `valid_from`/`valid_until`, `supersedes`/`superseded_by`, `promoted_from`/`promotion_chain`, `access_count`/`last_accessed`, `entity_refs`, embedding blob, metadata JSON.

---

## 8. Cross-agent sharing model

Headroom's pitch: "Claude saves a fact, Codex reads it back" by routing both through **one proxy, one project SQLite** (`wiki/memory.md` "Cross-Agent Memory (Proxy)"; README "one shared store across Claude, Codex, Gemini and Grok").

What that actually means:

1. **Shared file, not shared agent identity.** Isolation is `user_id` (and PROJECT db file). `agent_id` scope is optional and usually unset on the proxy save path (`save_memory` is called without `agent_id` in `_execute_save`). Agents share USER-level rows for the same OS user in the same project DB.
2. **Provenance is metadata** (`source_agent`) so you can see who wrote it; it does not hide the row from other agents.
3. **MCP is a second door into the same file.** Codex wrap registers `headroom_memory` MCP against `.headroom/memory.db` (`mcp_server.py` docstring). Startup reindexes embeddings so proxy writes are visible to vector search.
4. **Unresolved project → no injection** (fail closed) to prevent the TAM-550 cross-thread leak (`storage_router.py`).
5. **No multi-machine sync in the core model.** `headroom/memory/sync.py` / `MemoryBridge` (markdown ↔ Headroom) exist as optional extras (`__init__.py`, handler `bridge_enabled`); they are not required for cross-agent sharing.

This is **not** TinyMem-style "every agent speaks MCP to one stdio server." Headroom's default coding-agent path is: wrap/proxy intercepts HTTP, injects tools, writes SQLite. MCP memory is an adapter for clients that will not take tools from the proxy.

---

## 9. What TinyMem should consider stealing vs skipping

TinyMem has already ruled out a **required** proxy (map: "A required Headroom-style proxy or Headroom's compression product" is out of destination; tools are source of truth). The list below is still sourced from Headroom; "steal/skip" is a recommendation against that constraint plus TinyMem's existing nouns (Namespace, Memory kinds, `valid_until`, FTS5, MCP).

**Steal (ideas, not code):**

- Keep compression/proxy and persistent memory as **distinct products**. Headroom already does; TinyMem should not inherit CCR, SmartCrusher, or wrap. (§1)
- **Explicit correction = new version of the same id**, not a second save. Headroom's MCP text: similar save does not replace; corrections need the existing id (`mcp_server.py`). TinyMem already has `update_memory`; keep that discipline.
- **`valid_until` + current-vs-history query default.** Headroom's supersession is the same shape TinyMem already stores (`models.py` / TinyMem `valid_until`). Steal the *query default* (hide superseded) if session-level records appear; skip full `supersedes`/`superseded_by` chains unless identity/scope grilling needs an audit trail. (§4)
- **Writer provenance as metadata**, not as a scope. `source_agent` / `created_via` answers "who wrote this" without making Agent a bucket (`memory_handler.py`; identity ticket). (§5)
- **Dedup as a hint on save, never silent delete.** Source is 0.75 cosine **note**, not 92% auto-remove. TinyMem is lexical/FTS5 — if anything, return "similar note exists, call update" rather than cosine auto-merge. (§5)
- **Project-file isolation as a structural invariant**, not a query filter. Headroom learned this the hard way (GH #462, TAM-550). TinyMem Namespace ≈ this physical bucket. Do not pool unresolved requests into a global DB. (§2, §8)
- **Optional injection (TOOL / `--no-memory-context`)** as a mode, not the default. Headroom's TOOL mode is "model must call search" — that matches TinyMem's "Tools are the source of truth." AUTO_TAIL is the thing TinyMem already declined to require. (§6)

**Skip:**

- Required HTTP proxy / `headroom wrap` as the way agents share memory. (§1, TinyMem map)
- Embeddings, HNSW/sqlite-vec, importance-ranked semantic inject, GPU embedder. TinyMem is FTS5 lexical and model-free.
- Inline extraction (`with_memory` JSON trailer, +50–100 tokens, regex strip). That is capture-without-a-tool; capture policy is still open and this path fights "tools are the source of truth." (§6A)
- Auto-inject into the user-message tail / system prompt. Same reason; also fights Anthropic prefix cache (Headroom already has to disable it in `--mode cache`). (§6B)
- Four-level User/Session/Agent/Turn **as stored IDs on every Memory**, plus bubbling copies. TinyMem Namespace is the project bucket; Agent as scope ≠ Agent as provenance. Turn-scoped ephemeral rows are a different noun if they exist at all. (§2)
- First-class Headroom categories (`insight`/`entity`/`context`) and `MemoryCategory` (already gone from source). TinyMem kinds are `fact|preference|decision|plan`. (§3)
- Graph/Qdrant/Neo4j, native Anthropic memory tool, `headroom learn`, markdown bridge, CCR. Adjacent products.
- Wiki's 92% auto-dedup. It is not in source; do not copy a claim Headroom does not run. (§5)

---

## Doc vs source cheat sheet (do not copy blindly)

| Claim in wiki/Pages or README | Source |
|---|---|
| `MemoryCategory` / `category=` on `add()` | **Stale.** No field on `Memory`; SQLite writes `""`. Official `memory.mdx` corrected. |
| Background auto-dedup >92% cosine removes older row | **Not in `memory_handler.py`.** Live path is 0.75 hint only. |
| `with_memory` stores at USER when `session_id` is set | Writes SESSION (if `session_id` set), then **bubbles a USER copy** at importance 0.7. |
| `memory_delete` is soft | **Hard** `DELETE FROM memories`. |
| `client.memory.stats()['categories']` | `stats()` returns `{"total": N}` only. |
| `HEADROOM_WORKSPACE_DIR` holds the project memory DB | Project memory paths **ignore** that env; use `--memory-db-path` or cwd `.headroom/memory.db`. |
| Compression architecture = memory | Architecture page is the token pipeline; memory is `--memory` / `with_memory`. |
