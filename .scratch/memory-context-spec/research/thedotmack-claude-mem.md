# Research: thedotmack/claude-mem

Ticket: Research thedotmack/claude-mem  
Question: What is thedotmack/claude-mem's capture pipeline, compression, retrieval layers, and injection mechanism, as stated by its own repository and docs?  
Primary source: [thedotmack/claude-mem](https://github.com/thedotmack/claude-mem) — README, `docs/`, source. Official Mintlify site [docs.claude-mem.ai](https://docs.claude-mem.ai/) is the repo's `docs/public/` (see [docs.claude-mem.ai/llms.txt](https://docs.claude-mem.ai/llms.txt) and repo `docs/public/`). No secondary blogs.  
Read date: 2026-10-02. Repo HEAD sampled as `039c6160f0ff26e9fab37cae7f50b994ba68f7ff`.

First-party docs disagree on a few counts (hook count, MCP tool count, telemetry, observation type enum). Those clashes are called out next to the claim.

---

## 1. Product one-liner

Claude-Mem is a **persistent memory compression system**: it **automatically captures tool-usage observations**, **compresses them with a background LLM observer**, and **injects a compact index** of past work into later sessions so the agent can fetch details on demand. ([README.md](https://github.com/thedotmack/claude-mem/blob/main/README.md); [docs.claude-mem.ai/introduction](https://docs.claude-mem.ai/introduction.md); [docs.claude-mem.ai/usage/getting-started](https://docs.claude-mem.ai/usage/getting-started.md))

GitHub blurb: "Captures everything your agent does during sessions, compresses it with AI, and injects relevant context back into future sessions." ([github.com/thedotmack/claude-mem](https://github.com/thedotmack/claude-mem))

License: **Apache-2.0** for the public core; open-core with reserved commercial areas (Magic Recall hosted cloud, team/org sync, SSO, enterprise RBAC, DLP). ([LICENSE](https://github.com/thedotmack/claude-mem/blob/main/LICENSE); [docs/ip-boundary.md](https://github.com/thedotmack/claude-mem/blob/main/docs/ip-boundary.md); [docs/license.md](https://github.com/thedotmack/claude-mem/blob/main/docs/license.md))

---

## 2. Nouns (what each IS)

These are claude-mem nouns, not TinyMem nouns. TinyMem already uses Memory / Profile / Namespace / Tool / Hook ([CONTEXT.md](../../../CONTEXT.md)).

| Noun | What it IS | Source |
| --- | --- | --- |
| **Project** | Scope key for stored rows. Derived from cwd / worktree; SessionStart injects memory for the current project. | [docs.claude-mem.ai/configuration](https://docs.claude-mem.ai/configuration.md); [docs/architecture-overview.md](https://github.com/thedotmack/claude-mem/blob/main/docs/architecture-overview.md) |
| **Session (`sdk_sessions`)** | One tracked work span. Has `content_session_id` (host-invariant for the chat) and `memory_session_id` (SDK observer id; changes on worker restart). Status `active` → completed. Multi-prompt: `prompt_counter` / `prompt_number`. `/clear` does **not** end the session. | [docs/architecture-overview.md](https://github.com/thedotmack/claude-mem/blob/main/docs/architecture-overview.md); [docs.claude-mem.ai/architecture/database](https://docs.claude-mem.ai/architecture/database.md); [docs.claude-mem.ai/usage/getting-started](https://docs.claude-mem.ai/usage/getting-started.md) |
| **User prompt (`user_prompts`)** | Verbatim user text, numbered, FTS/substring searchable. Caps exist (plan text: 4,000-char cap). `<private>` stripped before insert. Cloud sync: prompts >200 KB upload truncated. | [src/services/sqlite/types.ts](https://github.com/thedotmack/claude-mem/blob/main/src/services/sqlite/types.ts); [docs.claude-mem.ai/usage/private-tags](https://docs.claude-mem.ai/usage/private-tags.md); [docs.claude-mem.ai/cloud-sync](https://docs.claude-mem.ai/cloud-sync.md) |
| **Pending message / queue** | Raw tool I/O waiting for the observer. Hooks enqueue; worker claims. Docs name this `observation_queue` in the hooks page and `pending_messages` in architecture-overview. Rows are deleted after a parseable observer response. | [docs.claude-mem.ai/hooks-architecture](https://docs.claude-mem.ai/hooks-architecture.md); [docs/architecture-overview.md](https://github.com/thedotmack/claude-mem/blob/main/docs/architecture-overview.md) |
| **Tool use (`tool_uses`)** | Durable **raw** `tool_input` / `tool_response` index (schema v51). Same ingest choke point as the observer. Truncated at **64 KB** with `…[truncated: N bytes]`. `UNIQUE(content_session_id, tool_use_id)`. Not a transcript store. | [docs.claude-mem.ai/architecture/database](https://docs.claude-mem.ai/architecture/database.md); [docs.claude-mem.ai/progressive-disclosure](https://docs.claude-mem.ai/progressive-disclosure.md) |
| **Observation** | LLM-compressed unit of what was **learned / built / fixed**, not a raw tool log. Fields: `type`, `title`, `subtitle`, `narrative`, `facts[]`, `concepts[]`, `files_read[]`, `files_modified[]`, `discovery_tokens`. Deduped by `SHA256(memory_session_id + title + narrative)[:16]` within a 30s window. | [src/sdk/parser.ts](https://github.com/thedotmack/claude-mem/blob/main/src/sdk/parser.ts) `ParsedObservation`; [src/services/sqlite/types.ts](https://github.com/thedotmack/claude-mem/blob/main/src/services/sqlite/types.ts); [docs/architecture-overview.md](https://github.com/thedotmack/claude-mem/blob/main/docs/architecture-overview.md); [plugin/modes/code.json](https://github.com/thedotmack/claude-mem/blob/main/plugin/modes/code.json) |
| **Observation type** | Classification of an observation. Default `code` mode declares **9** ids: `bugfix`, `feature`, `refactor`, `change`, `discovery`, `decision`, `security_alert`, `security_note`, `sensitive`. TypeScript `ObservationRow.type` still lists only the first six. Search docs list the six. Grok awareness treats `decision`, `bugfix`, `security_alert`, `sensitive` as needle types. | [plugin/modes/code.json](https://github.com/thedotmack/claude-mem/blob/main/plugin/modes/code.json); [src/services/sqlite/types.ts](https://github.com/thedotmack/claude-mem/blob/main/src/services/sqlite/types.ts); [docs.claude-mem.ai/usage/search-tools](https://docs.claude-mem.ai/usage/search-tools.md); [README.md](https://github.com/thedotmack/claude-mem/blob/main/README.md) |
| **Concept** | Separate axis from type. Default keywords: `how-it-works`, `why-it-exists`, `what-changed`, `problem-solution`, `gotcha`, `pattern`, `trade-off`. Injection SQL matches concepts exactly. | [plugin/modes/code.json](https://github.com/thedotmack/claude-mem/blob/main/plugin/modes/code.json); [src/sdk/parser.ts](https://github.com/thedotmack/claude-mem/blob/main/src/sdk/parser.ts) |
| **Session summary** | LLM checkpoint, not a session ending. Fields: `request`, `investigated`, `learned`, `completed`, `next_steps`, `notes` (+ file lists). Multiple summaries per session. Stop hook. | [docs.claude-mem.ai/usage/getting-started](https://docs.claude-mem.ai/usage/getting-started.md); [docs.claude-mem.ai/hooks-architecture](https://docs.claude-mem.ai/hooks-architecture.md); [src/sdk/parser.ts](https://github.com/thedotmack/claude-mem/blob/main/src/sdk/parser.ts) `ParsedSummary` |
| **Observer / worker** | Local Express daemon (Bun). Enqueues raw events, runs the LLM observer, writes observations/summaries, serves search + viewer. Default bind `127.0.0.1`, port `37700 + (uid % 100)` (macOS often 37777). | [docs.claude-mem.ai/architecture/overview](https://docs.claude-mem.ai/architecture/overview.md); [docs.claude-mem.ai/architecture/worker-service](https://docs.claude-mem.ai/architecture/worker-service.md); [docs.claude-mem.ai/grok-bot](https://docs.claude-mem.ai/grok-bot/index.md) |
| **Index (injected block)** | Compact markdown table of observation IDs, times, type icons, titles, **token costs**. Not full narratives. | [docs.claude-mem.ai/progressive-disclosure](https://docs.claude-mem.ai/progressive-disclosure.md); [docs.claude-mem.ai/hooks-architecture](https://docs.claude-mem.ai/hooks-architecture.md) |
| **Platform source** | Harness stamp on rows (`claude`, `codex`, `cursor`, `grok-bot`, `omp`, …). SessionStart can include one harness or all. | [docs/adapters.md](https://github.com/thedotmack/claude-mem/blob/main/docs/adapters.md); [README.md](https://github.com/thedotmack/claude-mem/blob/main/README.md) `CLAUDE_MEM_SESSION_START_INCLUDE_ALL_SOURCES`; [docs.claude-mem.ai/platform-integration](https://docs.claude-mem.ai/platform-integration.md) |
| **Folder CLAUDE.md** | Optional per-folder generated timeline inside `<claude-mem-context>` tags. Off by default. Root `.git` dir excluded. | [docs.claude-mem.ai/usage/folder-context](https://docs.claude-mem.ai/usage/folder-context.md) |
| **Knowledge agent / corpus** | Queryable “brain” built from observation history (filter by type/concept/file/query). Product feature, not the core store noun. | [docs.claude-mem.ai/usage/knowledge-agents](https://docs.claude-mem.ai/usage/knowledge-agents.md) |

**Not a Memory in TinyMem’s sense.** An observation is a session-level compressed event with narrative + facts. A TinyMem Memory is a durable sentence-level note the agent writes. A claude-mem Profile does not exist; Grok Bot explicitly does **not** write `profile.md`. ([docs.claude-mem.ai/grok-bot](https://docs.claude-mem.ai/grok-bot/index.md); TinyMem [CONTEXT.md](../../../CONTEXT.md))

---

## 3. Capture pipeline: what, when, auto vs tools

### Automatic (default). Agents do not call a write tool to persist work.

Core loop ([docs.claude-mem.ai/architecture/overview](https://docs.claude-mem.ai/architecture/overview.md); [docs/architecture-overview.md](https://github.com/thedotmack/claude-mem/blob/main/docs/architecture-overview.md); shipped [plugin/hooks/hooks.json](https://github.com/thedotmack/claude-mem/blob/main/plugin/hooks/hooks.json)):

```
Setup (version-check)
  → SessionStart: start worker (async) + inject context
  → UserPromptSubmit: session-init + save prompt
  → PreToolUse matcher Read: file-context gate
  → PostToolUse matcher *: enqueue observation
  → Stop: summarize
  → SessionEnd: mark complete (skip on /clear)
```

README says “5 Lifecycle Hooks … (6 hook scripts)”. Architecture overview says Setup + 5 lifecycle. `hooks.json` registers **Setup, SessionStart, UserPromptSubmit, PostToolUse, PreToolUse(Read), Stop, SessionEnd**. Treat `hooks.json` as the shipped contract.

| When | What is captured | Auto vs tool |
| --- | --- | --- |
| UserPromptSubmit | Raw prompt → `user_prompts`; session row created | Auto hook (`session-init`) |
| PostToolUse `*` | `tool_name`, `tool_input`, `tool_response`, `cwd` → queue + `tool_uses` | Auto hook (`observation`). Not an MCP write. |
| Every listed Claude Code tool | Read, Write, Edit, Bash, Glob, Grep, “and all other Claude Code tools” | Auto ([getting-started](https://docs.claude-mem.ai/usage/getting-started.md)) |
| Stop | Last assistant message → LLM summary XML | Auto hook (`summarize`) |
| SessionEnd | Status = complete; drain pending | Auto; skips `/clear` |
| Hosts without hooks | Transcript watcher on JSONL (Grok Bot); OpenClaw `tool_result_persist`; REST V1 ingest | Auto, different adapter |

Skip / filter (still automatic, just dropped):

- `CLAUDE_MEM_SKIP_TOOLS` default: `ListMcpResourcesTool,SlashCommand,Skill,TodoWrite,AskUserQuestion` ([configuration](https://docs.claude-mem.ai/configuration.md))
- `CLAUDE_MEM_SKIP_BASH_PATTERNS` — regex skip for shell
- `CLAUDE_MEM_SKIP_SUBAGENT_OBSERVATIONS` / `CLAUDE_MEM_SKIP_AGENT_TYPES`
- Entire prompt wrapped in `<private>` → `{ status: "skipped", reason: "private" }` ([platform-integration](https://docs.claude-mem.ai/platform-integration.md); [private-tags](https://docs.claude-mem.ai/usage/private-tags.md))
- `CLAUDE_MEM_DISABLE_OBSERVATION` / `CLAUDE_MEM_DISABLE_TOOL_HOOKS` — hook no-ops ([configuration](https://docs.claude-mem.ai/configuration.md))

Hook policy: enqueue fast, never block the host on worker failure. Transport errors → exit 0; client bugs → exit 2. ([docs/architecture-overview.md](https://github.com/thedotmack/claude-mem/blob/main/docs/architecture-overview.md))

REST ingest for other hosts ([docs/adapters.md](https://github.com/thedotmack/claude-mem/blob/main/docs/adapters.md); [platform-integration](https://docs.claude-mem.ai/platform-integration.md)):

```
POST /api/sessions/observations
{ claudeSessionId / contentSessionId, tool_name, tool_input, tool_response, cwd, platformSource }
```

OpenClaw truncates tool responses to **1000 chars** before POST. (`memory_*` tools skipped to avoid recursion.) ([openclaw-integration](https://docs.claude-mem.ai/openclaw-integration.md))

There is **no agent-facing “add_memory” write tool** in the local MCP set. Writes are hooks / REST / transcript watch. Hosted server adds `POST /v1/events` and `POST /v1/memories` (different product surface). ([search-architecture](https://docs.claude-mem.ai/architecture/search-architecture.md); [hosted-server](https://docs.claude-mem.ai/hosted-server.md))

---

## 4. Compression / summarization: local vs LLM, what is stored

### Two stores

1. **Raw (local, no LLM):** `tool_uses` (capped 64 KB), `user_prompts`, pending queue. ([database](https://docs.claude-mem.ai/architecture/database.md))
2. **Compressed (LLM observer):** `observations` and `session_summaries`. Worker sends tool I/O to a provider and parses XML. ([architecture-overview](https://github.com/thedotmack/claude-mem/blob/main/docs/architecture-overview.md); [src/sdk/parser.ts](https://github.com/thedotmack/claude-mem/blob/main/src/sdk/parser.ts))

### Observer is always an LLM (or a hosted LLM). Not model-free.

Providers: `claude` (Claude Agent SDK, default model `claude-haiku-4-5-20251001`), `codex`, `gemini`, `openrouter`, `openai-compatible` (NIM, Ollama, LM Studio, …). Installer can default to **CMEM Pro** (`cmem-observer` at `https://cmem.ai/api/inference/v1`) so compression runs **off-plan**. ([configuration](https://docs.claude-mem.ai/configuration.md); [README.md](https://github.com/thedotmack/claude-mem/blob/main/README.md); [cursor](https://docs.claude-mem.ai/cursor/index.md); [grok-bot](https://docs.claude-mem.ai/grok-bot/index.md))

Observer contract ([plugin/modes/code.json](https://github.com/thedotmack/claude-mem/blob/main/plugin/modes/code.json); [parser.ts](https://github.com/thedotmack/claude-mem/blob/main/src/sdk/parser.ts)):

- Idle / noise: `<skip_summary reason="noise" />`
- Work unit: `<observation><type>…</type><title>…</title><subtitle>…</subtitle><narrative>…</narrative><facts><fact>…</fact></facts><concepts><concept>…</concept></concepts><files_read/><files_modified/></observation>`
- Checkpoint: `<summary><request/>…<next_steps/></summary>`
- Parser is binary: `{ valid, observations, summary }` or `{ valid: false }`. Unparseable output leaves the queue.

What is stored after compression:

- Observation: type, title (max 120 graphemes), subtitle, narrative, facts (self-contained, no pronouns), concepts, files read/modified, token accounting (`discovery_tokens`)
- Summary: request / investigated / learned / completed / next_steps / notes
- Optional Chroma docs per observation: `obs_{id}_narrative`, `obs_{id}_fact_0`, … ([docs/architecture-overview.md](https://github.com/thedotmack/claude-mem/blob/main/docs/architecture-overview.md))

Oversized tool fields can be **LLM-condensed** first (`CLAUDE_MEM_FIELD_OPTIMIZE_TIMEOUT_MS`, default 180s) or truncated. ([configuration](https://docs.claude-mem.ai/configuration.md))

Local-only pieces: SQLite WAL at `~/.claude-mem/claude-mem.db`, optional Chroma at `~/.claude-mem/chroma/`. Compression **leaves the machine** via the chosen provider unless that provider is a local OpenAI-compatible endpoint. ([SECURITY.md](https://github.com/thedotmack/claude-mem/blob/main/SECURITY.md))

Memory ingest of Claude Code auto-memory markdown is the exception: “no model spend.” ([docs.claude-mem.ai/usage/memory-ingest](https://docs.claude-mem.ai/usage/memory-ingest.md))

---

## 5. Retrieval layers and token strategy

Progressive disclosure: **show what exists and its retrieval cost first; let the agent fetch.** ([progressive-disclosure](https://docs.claude-mem.ai/progressive-disclosure.md); [search-tools](https://docs.claude-mem.ai/usage/search-tools.md); [search-architecture](https://docs.claude-mem.ai/architecture/search-architecture.md))

### Local MCP tools (stdio → worker HTTP)

README + search-architecture name **4 tools** / **3-layer workflow**:

| Layer | Tool | Returns | Claimed cost |
| --- | --- | --- | --- |
| 1 Index | `search` | Compact table: IDs, titles, dates, types. Filters: query, `type`/`obs_type`, project, dates, limit/offset, `orderBy` | ~50–100 tokens/result |
| 2 Context | `timeline` | Chronology around `anchor` id or query (`depth_before`/`depth_after`, default 3) | ~100–200 tokens/obs |
| 3 Details | `get_observations` | Full narrative/facts/files/concepts for batched IDs | ~500–1,000 tokens/obs (getting-started also says ~100–500; file-read-gate says ~300) |
| Always-visible | `important_workflow` | Text reminder of the 3-layer rule | definition tokens |

Progressive-disclosure page adds **Layer 4 `get_tool_uses`**: raw `tool_input`/`tool_response` (up to 64 KB/row). “Never start here.” ([progressive-disclosure](https://docs.claude-mem.ai/progressive-disclosure.md)) README’s MCP section does **not** list this tool.

Claimed savings: **~10×** vs fetching full observations up front. ([README.md](https://github.com/thedotmack/claude-mem/blob/main/README.md); [search-tools](https://docs.claude-mem.ai/usage/search-tools.md))

### How `search` matches

- If Chroma is up: **meaning first**, then keyword fill. ([search-tools](https://docs.claude-mem.ai/usage/search-tools.md))
- Keyword on observations/summaries: every word must appear (no AND/OR/NOT operators — those are literal words). Case-insensitive. Hyphenated tokens stay together. Fallback: substring inside longer words.
- User prompts: whole query as one substring.
- Hosted server: PostgreSQL FTS (stems, `OR`, quotes, leading `-`). Different rules.

FTS5 virtual tables: `observations_fts` (title, subtitle, narrative, text, facts, concepts), `session_summaries_fts`. Prompts have **no** FTS index (LIKE). Queries quote-escaped. ([database](https://docs.claude-mem.ai/architecture/database.md))

MCP server is a thin wrapper: `plugin/scripts/mcp-server.cjs` → `GET /api/search`, `GET /api/timeline`, `POST /api/observations/batch`. ([search-architecture](https://docs.claude-mem.ai/architecture/search-architecture.md); [plugin/.mcp.json](https://github.com/thedotmack/claude-mem/blob/main/plugin/.mcp.json))

`mem-search` skill: README still lists it; search-architecture says skill-based search was **removed** in favor of MCP. Do not treat the skill as the current retrieval surface without re-checking `plugin/skills/`.

Hosted recall MCP (`/v1/mcp`) is a **different** trio: `search`, `context`, `recent` — read-only, team/project scoped. ([hosted-server](https://docs.claude-mem.ai/hosted-server.md))

---

## 6. Injection: how future sessions receive context

### Claude Code (hooks, automatic)

SessionStart (`startup|resume|clear|compact`) runs `hook claude-code context`. Since Claude Code 2.1.0, output is **silent** via `hookSpecificOutput.additionalContext` (no user-visible banner). ([hooks-architecture](https://docs.claude-mem.ai/hooks-architecture.md); [plugin/hooks/hooks.json](https://github.com/thedotmack/claude-mem/blob/main/plugin/hooks/hooks.json))

What is injected ([getting-started](https://docs.claude-mem.ai/usage/getting-started.md); [hooks-architecture](https://docs.claude-mem.ai/hooks-architecture.md); [configuration](https://docs.claude-mem.ai/configuration.md)):

1. Recent observations for the project — default **50** (`CLAUDE_MEM_CONTEXT_OBSERVATIONS`, range 1–200)
2. From last **10** sessions (`CLAUDE_MEM_CONTEXT_SESSION_COUNT`)
3. Formatted as a **progressive-disclosure index** (IDs, type icons, titles, token costs), grouped by date / file
4. Full summary details only if that summary is **newer than the last observation** (avoids stale summaries)
5. Optional: first N observations expanded (`CLAUDE_MEM_CONTEXT_FULL_COUNT`, default 5)

Defaults that shrink the window:

- `CLAUDE_MEM_SESSION_START_INCLUDE_ALL_SOURCES=false` — current harness only ([README.md](https://github.com/thedotmack/claude-mem/blob/main/README.md))
- `CLAUDE_MEM_CONTEXT_MAIN_AGENT_ONLY=true` — drop subagent rows from the index
- Optional type/concept filters; optional ACT-R ranking (`CLAUDE_MEM_REINFORCE_ALPHA`, default 0 = recency)

`/clear` re-fires SessionStart (`source: "clear"`) and **re-injects** the index while the same session keeps capturing. ([getting-started](https://docs.claude-mem.ai/usage/getting-started.md))

### File-read gate (hook, automatic, invasive)

PreToolUse on `Read`: if the file is ≥1500 bytes, project not excluded, and observations exist → **deny the Read** and inject a per-file timeline. Agent then chooses: use titles / `get_observations` / smart_outline / full read. Threshold hardcoded in `src/cli/handlers/file-context.ts`. ([file-read-gate](https://docs.claude-mem.ai/file-read-gate.md))

### MCP (on-demand, not injection)

Agent calls `search` / `timeline` / `get_observations` after seeing the index. Claude Desktop can attach the same MCP server. ([search-architecture](https://docs.claude-mem.ai/architecture/search-architecture.md); [claude-desktop](https://docs.claude-mem.ai/usage/claude-desktop.md))

### Other hosts

| Host | Injection mechanism |
| --- | --- |
| Cursor | Hooks (`beforeSubmitPrompt` → `session-init` + `context`). Next chat gets past notes. ([cursor](https://docs.claude-mem.ai/cursor/index.md); [claude-mem-cursor/hooks/hooks.json](https://github.com/thedotmack/claude-mem/blob/main/claude-mem-cursor/hooks/hooks.json)) |
| OpenClaw | `before_prompt_build` → `GET /api/context/inject` as `appendSystemContext`. 60s cache. Leaves `MEMORY.md` to the agent. ([openclaw-integration](https://docs.claude-mem.ai/openclaw-integration.md)) |
| Grok Bot | **No host hooks.** Worker overwrites `agents/<uuid>/memory/log/zz-claude-mem-inject.md` (sliding INDEX, default window 80). Awareness pilot appends needle lines to `YYYY-MM.md`. Does not write `profile.md`. ([grok-bot](https://docs.claude-mem.ai/grok-bot/index.md); [README.md](https://github.com/thedotmack/claude-mem/blob/main/README.md)) |
| OMP | Hook module emits REST V1 including context injection. ([docs/adapters.md](https://github.com/thedotmack/claude-mem/blob/main/docs/adapters.md)) |
| Folder CLAUDE.md | Optional file write, not a hook. Off by default. ([folder-context](https://docs.claude-mem.ai/usage/folder-context.md)) |

Cloud sync: worker **pulls remote rows before injecting** (bounded 1.5s) so other-device memory can appear at session start. ([cloud-sync](https://docs.claude-mem.ai/cloud-sync.md))

---

## 7. Storage and privacy

### Local default

- DB: `~/.claude-mem/claude-mem.db` (SQLite WAL, bun:sqlite)
- Vectors: `~/.claude-mem/chroma/` (optional; needs `uv`)
- Settings: `~/.claude-mem/settings.json` (mode `0600` when installer writes cloud tokens)
- Logs: `~/.claude-mem/logs/`
- Worker: `127.0.0.1` only unless Observation TV is enabled ([SECURITY.md](https://github.com/thedotmack/claude-mem/blob/main/SECURITY.md); [configuration](https://docs.claude-mem.ai/configuration.md))

### What leaves the machine (first-party, even when “local”)

SECURITY.md: state files are not uploaded **by claude-mem itself**, and it says “Claude-mem does not collect telemetry.” Same file then lists provider egress: Claude Agent SDK → Anthropic; Gemini / OpenRouter / openai-compatible → those endpoints; Chroma embeddings may be remote. ([SECURITY.md](https://github.com/thedotmack/claude-mem/blob/main/SECURITY.md))

**Clash:** [telemetry.md](https://docs.claude-mem.ai/telemetry.md) says **PostHog analytics are on by default** (opt-out via `npx claude-mem telemetry disable` or `DO_NOT_TRACK`). Whitelisted install UUID + rollups; no prompts/paths. Treat telemetry.md as the current product statement; SECURITY.md is stale on this point.

Optional / commercial cloud:

- **CMEM Pro observer:** tool I/O sent to `cmem.ai` for compression. Installer sign-in provisions a memory key; 14-day free trial then falls back to Anthropic plan. ([README.md](https://github.com/thedotmack/claude-mem/blob/main/README.md))
- **Cloud sync:** uploads observation narratives **and full prompt text** to a per-user hub. “Don’t enable it if that content must stay on your machine.” ([cloud-sync](https://docs.claude-mem.ai/cloud-sync.md))
- **Hosted server (beta):** Postgres per team/project; authenticated MCP recall; deletion APIs. ([hosted-server](https://docs.claude-mem.ai/hosted-server.md))

### Privacy controls

| Control | Behavior | Default |
| --- | --- | --- |
| `<private>…</private>` | Strip at hook layer from prompts and serialized tool I/O before worker/DB | Always on ([private-tags](https://docs.claude-mem.ai/usage/private-tags.md); [SECURITY.md](https://github.com/thedotmack/claude-mem/blob/main/SECURITY.md)) |
| Auto-redaction | Regex replace secrets with `<redacted type='…'/>` | **Off** until `CLAUDE_MEM_REDACT_ENABLED=true` ([auto-redaction](https://docs.claude-mem.ai/usage/auto-redaction.md)) |
| Observation TV | LAN stream of titles; token-gated; `GET /api/observations` still returns **full bodies for every project** | Off unless host + token set ([configuration](https://docs.claude-mem.ai/configuration.md)) |

Worker HTTP is local-only and unauthenticated. ([search-architecture](https://docs.claude-mem.ai/architecture/search-architecture.md))

---

## 8. Agent hosts supported

First-party installer / adapter table ([platform-integration](https://docs.claude-mem.ai/platform-integration.md)) plus README extras:

| Harness | `platformSource` | How capture/inject is wired | Install |
| --- | --- | --- | --- |
| Claude Code | `claude` | Native plugin hooks + MCP | `/plugin marketplace add thedotmack/claude-mem` or `npx claude-mem install` |
| Codex CLI | `codex` | `~/.codex/config.toml` hooks | `npx claude-mem install --ide codex` |
| Cursor | `cursor` | `~/.cursor/hooks.json` + MCP | `npx claude-mem install --ide cursor` |
| Windsurf | `windsurf` | `~/.codeium/windsurf/hooks.json` | `--ide windsurf` |
| OpenCode | `opencode` | `~/.config/opencode/opencode.json` | `--ide opencode` |
| OpenClaw | `openclaw` | Gateway plugin events | `curl -fsSL https://install.cmem.ai/openclaw.sh \| bash` |
| Antigravity CLI | `antigravity-cli` | Gemini settings + MCP | `--ide antigravity` |
| Kimi Code | `kimi` | hooks + MCP | `npx claude-mem kimi install` |
| Grok Bot | `grok-bot` | Transcript watcher + file INDEX | `--ide grok-bot` (npm ≥ 13.24) |
| OMP (Oh My Pi) | `omp` | `~/.omp/agent/hooks/pre/` | `--ide omp` ([adapters.md](https://github.com/thedotmack/claude-mem/blob/main/docs/adapters.md)) |
| Claude Desktop | (search only) | MCP attach, no capture hooks | [claude-desktop](https://docs.claude-mem.ai/usage/claude-desktop.md) |
| Cowork | — | Thin HTTP shims to cmem.ai Pro | [cowork/hooks/hooks.json](https://github.com/thedotmack/claude-mem/blob/main/cowork/hooks/hooks.json) |

README marketing line also names **Gemini, Hermes, Copilot**. Gemini is covered as Antigravity CLI + Gemini *provider*. Hermes / Copilot are not in the platform-integration table; treat as claimed, not documented as first-class harnesses in that guide.

Generic REST V1 is the intended new-adapter contract (`contentSessionId`, `tool_name`, `tool_input`, `tool_response`, `cwd`, `platformSource`). ([docs/adapters.md](https://github.com/thedotmack/claude-mem/blob/main/docs/adapters.md))

Runtime: Node 20+, Bun ≥ 1.0 (auto-installed), uv for Chroma, SQLite via bun:sqlite. ([README.md](https://github.com/thedotmack/claude-mem/blob/main/README.md); [introduction](https://docs.claude-mem.ai/introduction.md))

---

## 9. What TinyMem should consider stealing vs skipping

TinyMem already decided: Tools are the source of truth; Hooks are optional; a required proxy is out; retrieval today is model-free FTS5. ([map.md](../map.md); [CONTEXT.md](../../../CONTEXT.md)) Steal means copy the *idea*, not merge the product.

### Steal (sourced)

1. **Layered retrieval, not dump-all.** Index (`search`) → narrative (`timeline`) → details (`get_observations`) → raw evidence last. Put token cost on the index. Feeds [08-decide-retrieval-shape](../issues/08-decide-retrieval-shape.md). ([search-tools](https://docs.claude-mem.ai/usage/search-tools.md); [progressive-disclosure](https://docs.claude-mem.ai/progressive-disclosure.md))
2. **Keep Observation ≠ Memory ≠ Summary ≠ raw tool I/O as separate nouns** if TinyMem adds session-level records. Do not stuff compressed session events into TinyMem’s sentence-level Memory. Feeds [05-name-the-domain-nouns](../issues/05-name-the-domain-nouns.md). (This file §2; [database](https://docs.claude-mem.ai/architecture/database.md))
3. **If a Hook injects, inject an index (IDs + titles + cost), not full narratives.** Re-inject on clear/resume. Feeds [07-decide-injection-policy](../issues/07-decide-injection-policy.md). ([hooks-architecture](https://docs.claude-mem.ai/hooks-architecture.md))
4. **Strip at the write edge** (`<private>` / redaction) before anything hits the store. ([private-tags](https://docs.claude-mem.ai/usage/private-tags.md))
5. **Never block the host** when memory is down (exit 0 on transport failure). ([docs/architecture-overview.md](https://github.com/thedotmack/claude-mem/blob/main/docs/architecture-overview.md))
6. **Thin adapter event shape** (`contentSessionId`, tool name/input/response, `cwd`, `platformSource`) if capture is ever opt-in. Feeds [11-decide-the-adapter-surface](../issues/11-decide-the-adapter-surface.md). ([adapters.md](https://github.com/thedotmack/claude-mem/blob/main/docs/adapters.md))

### Skip (sourced, and it fights TinyMem)

1. **Auto-capture of every tool use.** Claude-Mem’s value *is* this; TinyMem’s map says writes go through Tools. ([getting-started](https://docs.claude-mem.ai/usage/getting-started.md); [map.md](../map.md))
2. **Required LLM observer / worker / Bun / uv / Chroma.** Compression and hybrid search need a provider and a daemon. TinyMem is model-free and offline. ([SECURITY.md](https://github.com/thedotmack/claude-mem/blob/main/SECURITY.md); [configuration](https://docs.claude-mem.ai/configuration.md))
3. **File-read gate that denies `Read`.** Hook-invasive token trick; TinyMem Hooks are optional and must not become a proxy. ([file-read-gate](https://docs.claude-mem.ai/file-read-gate.md); [map.md](../map.md))
4. **Cloud observer, cloud sync, hosted Postgres, CMEM Pro default.** Uploads narratives and prompts. TinyMem store is local. ([cloud-sync](https://docs.claude-mem.ai/cloud-sync.md); [README.md](https://github.com/thedotmack/claude-mem/blob/main/README.md))
5. **Auto-writing `CLAUDE.md` into project folders.** Off even in claude-mem; pollutes repos. ([folder-context](https://docs.claude-mem.ai/usage/folder-context.md))
6. **Telemetry on by default / Observation TV full-body LAN leak.** Wrong default for a local memory product. ([telemetry](https://docs.claude-mem.ai/telemetry.md); [configuration](https://docs.claude-mem.ai/configuration.md))
7. **Knowledge agents / brains / Magic Recall.** Commercial or extra surface, not v1 infrastructure. ([ip-boundary.md](https://github.com/thedotmack/claude-mem/blob/main/docs/ip-boundary.md); [knowledge-agents](https://docs.claude-mem.ai/usage/knowledge-agents.md))

---

## First-party inconsistencies (do not flatten)

- Hook count: README “5 hooks / 6 scripts” vs `hooks.json` (Setup + SessionStart×2 + UserPromptSubmit + PostToolUse + PreToolUse + Stop + SessionEnd).
- MCP surface: README 3 tools + workflow; progressive-disclosure adds `get_tool_uses`; hosted `/v1/mcp` is another 3 tools.
- Observation types: `code.json` has 9; `ObservationRow` and search docs list 6.
- Telemetry: SECURITY.md “does not collect”; telemetry.md PostHog opt-out default.
- Queue table name: `observation_queue` (hooks page) vs `pending_messages` (architecture-overview).
- `mem-search` skill: README yes; search-architecture “removed.”
- Token costs for a full observation: 100–500 / 300 / 500–1,000 depending on page.

---

## Source index

- Repo: https://github.com/thedotmack/claude-mem
- README: https://github.com/thedotmack/claude-mem/blob/main/README.md
- Docs index: https://docs.claude-mem.ai/llms.txt
- Architecture: https://docs.claude-mem.ai/architecture/overview.md — https://github.com/thedotmack/claude-mem/blob/main/docs/architecture-overview.md
- Hooks: https://docs.claude-mem.ai/hooks-architecture.md — https://github.com/thedotmack/claude-mem/blob/main/plugin/hooks/hooks.json
- Database: https://docs.claude-mem.ai/architecture/database.md
- Search: https://docs.claude-mem.ai/usage/search-tools.md — https://docs.claude-mem.ai/architecture/search-architecture.md — https://docs.claude-mem.ai/progressive-disclosure.md
- Privacy: https://docs.claude-mem.ai/usage/private-tags.md — https://docs.claude-mem.ai/usage/auto-redaction.md — https://github.com/thedotmack/claude-mem/blob/main/SECURITY.md
- Hosts: https://docs.claude-mem.ai/platform-integration.md — https://docs.claude-mem.ai/openclaw-integration.md — https://docs.claude-mem.ai/cursor/index.md — https://docs.claude-mem.ai/grok-bot/index.md
- Cloud: https://docs.claude-mem.ai/cloud-sync.md — https://docs.claude-mem.ai/hosted-server.md
- Observer schema: https://github.com/thedotmack/claude-mem/blob/main/plugin/modes/code.json — https://github.com/thedotmack/claude-mem/blob/main/src/sdk/parser.ts
