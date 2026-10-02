# Research: zelinewang/claudemem

**Question:** What is [zelinewang/claudemem](https://github.com/zelinewang/claudemem)'s memory model, storage, retrieval, sync, and agent integration, as stated by its own repository and docs?

**Primary source:** https://github.com/zelinewang/claudemem — README, `docs/`, source. No secondary blogs.

**Snapshot (this note):** clone of default branch at commit [`ffded1e9f747cd6bfa22615143e4f4e2d49b5f40`](https://github.com/zelinewang/claudemem/commit/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40) (2026-09-08, “Merge pull request #25 … vectors-schema-version-gate”). Latest GitHub **release tag** is [v3.0.12](https://github.com/zelinewang/claudemem/releases/tag/v3.0.12) (2026-05-13); HEAD is ahead of that tag. License: MIT ([`LICENSE`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/LICENSE)).

**Stale-doc flags (use source over these when they conflict):**

- [`docs/COMPARISON_AND_ROADMAP.md`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/docs/COMPARISON_AND_ROADMAP.md) itself says it predates v3.x (semantic search, context inject, sync, health). Treat it as historical philosophy, not current feature list.
- [`skills/claudemem/SKILL.md`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/skills/claudemem/SKILL.md) still says hybrid fusion is Reciprocal Rank Fusion. Source replaced RRF with min-max score fusion in [v3.0.2](https://github.com/zelinewang/claudemem/releases/tag/v3.0.2) (`pkg/storage/filestore_vectors.go`).

---

## 1. Product one-liner

Persistent memory for AI coding agents: **knowledge notes** and **work-session reports** that survive conversations, stored as human-readable Markdown, indexed by SQLite FTS5 plus optional semantic vectors, in a **single zero-network-by-default Go CLI** (no MCP server, no required daemon).

Sources: [README.md](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/README.md) opening paragraph; [`CLAUDE.md`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/CLAUDE.md) “Persistent memory CLI”; [`docs/UNIVERSAL_AGENT_INTEGRATION.md`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/docs/UNIVERSAL_AGENT_INTEGRATION.md) “Positioning”.

Install path advertised: `npx skills add zelinewang/claudemem --skill claudemem --global` — “Next time you start Claude Code (or Cursor, Gemini CLI, etc.), it just works.” Data lives in `~/.claudemem/` and “is never touched by upgrades.” ([README.md](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/README.md) Install.)

---

## 2. Nouns (what each IS)

| Noun | What it is | Where defined |
|---|---|---|
| **Note** | A **knowledge fragment**: UUID `id`, `type: "note"`, `category` (folder), `title`, markdown `content`, `tags[]`, `created`/`updated`, optional `metadata` map. Purpose: durable facts that accumulate by **topic**. | [`pkg/models/note.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/pkg/models/note.go); README Architecture table |
| **Session** | A **work report** for one conversation: UUID `id`, `type: "session"`, `title`, `date` (YYYY-MM-DD), `branch`, `project`, `session_id` (conversation key), `tags`, plus structured body sections (Summary, What Happened, Decisions, Changes, Problems, Insights, Questions, Next Steps, Related Notes, ExtraSections). Distinct conversations stay separate; re-saves of the same `session_id` merge. | [`pkg/models/session.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/pkg/models/session.go); README Architecture table |
| **Entry** | Interface over note or session: `GetID/Type/Title/Content/Tags/Created`. Used by unified search. | [`pkg/models/entry.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/pkg/models/entry.go) |
| **Category** | Note taxonomy and filesystem folder under `notes/<category>/`. Not a project namespace. | README; [`pkg/storage/filestore.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/pkg/storage/filestore.go) |
| **Tag** | Free-form labels on notes (and sessions). Facet filter on search. | README CLI; `SearchOpts.Tags` in [`pkg/storage/store.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/pkg/storage/store.go) |
| **`session_id` (conversation key)** | String that identifies one conversation. Notes store it in `metadata.session_id`. Sessions store it as frontmatter `session_id`. Same key → merge; different keys → separate sessions. | README Architecture table; [`pkg/storage/filestore_sessions.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/pkg/storage/filestore_sessions.go) comments |
| **RelatedNotes** | Session → notes list (`id`, `title`, `category`). Reverse of note `metadata.session_id`. Bidirectional cross-ref. | [`pkg/models/session.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/pkg/models/session.go) `RelatedNote`; SKILL Protocol 3 |
| **Store / memory dir** | Default `~/.claudemem/` (override `--store`). User-global, not per-repo. Contains `notes/`, `sessions/`, `.index/`, `config.json`. | [`cmd/root.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/cmd/root.go); README “What Gets Saved” |
| **Index (cache)** | SQLite at `~/.claudemem/.index/search.db`: `entries` table + `memory_fts` FTS5 + optional vector tables. Regenerable. | [`pkg/storage/filestore.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/pkg/storage/filestore.go) `initSchema`; SKILL “Data Model” |
| **Candidate (hook)** | Classifier output, not a stored noun yet: `{action, should_save, signal, reason, proposed_note}`. `claudemem hook event` is **dry-run only** — does not write notes, sessions, or a candidate queue. Queue commands (`candidates accept`) are **roadmap**, not shipped as CLI. | [`pkg/hooks/event.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/pkg/hooks/event.go); [`cmd/hook.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/cmd/hook.go); [`docs/UNIVERSAL_AGENT_INTEGRATION.md`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/docs/UNIVERSAL_AGENT_INTEGRATION.md) roadmap step 5 |
| **Capture observation (legacy)** | Bounded Bash-output snippet saved as a note in category `auto-capture` when `features.auto_capture=true`. Predates the universal hook-event design. | [`cmd/capture.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/cmd/capture.go) |
| **Config** | Per-machine JSON at `~/.claudemem/config.json`. Embedding backend, feature flags. **Not** synced. Secrets never stored — only env-var **names**. | [`pkg/config/config.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/pkg/config/config.go); [`SECURITY.md`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/SECURITY.md) |
| **Hook event** | Normalized JSON contract `schema_version: "claudemem.hook_event.v1"`: agent, event type, session (id/cwd/project/branch), tool summaries, text, limits. Agent-neutral; adapters translate native payloads. | [`pkg/hooks/event.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/pkg/hooks/event.go); [`docs/UNIVERSAL_AGENT_INTEGRATION.md`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/docs/UNIVERSAL_AGENT_INTEGRATION.md) |
| **MEMORY.md** | Optional extra file `Push` will `git add` if present. Not a first-class model type. | [`pkg/sync/gitsync.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/pkg/sync/gitsync.go) `Push` path list |

**Not present as first-class nouns:** TinyMem-style Namespace, Profile, Memory-kind (`fact`/`preference`/`decision`/`plan`), `valid_until`. Project is a **session field**, not a store partition. Default identity is **one user-global bucket**.

**Dedup keys (README table, implemented in storage):**

- Notes: same **title + category** (exact), plus fuzzy title word-overlap **≥ 50%** in the same category ([`findDedupCandidate`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/pkg/storage/filestore_notes.go)). Merge **appends** content with an `--- Updated … ---` separator.
- Sessions: same **`session_id`**; empty `session_id` falls back to date+project+branch. Merge **appends all sections**.

---

## 3. What gets written, by whom

Hooks are **control-plane**, not the default writer. Writes are **CLI mutations** of Markdown + index. Who invokes the CLI:

### Agent via skill / slash commands (primary write path)

[`skills/claudemem/SKILL.md`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/skills/claudemem/SKILL.md) states two behaviors:

1. **During work:** the *model* is instructed to **silently** save knowledge notes (`note add` / `note append`) — “no user action needed.” This is **skill-driven agent judgment calling the CLI**, not a hook dumping transcripts.
2. **On `/wrapup` or “wrap up”:** extract notes + save a structured session report + bidirectional cross-links. Session reports are **only** created via explicit wrapup — “never automatically.”

Write protocol (SKILL Protocol 1): search for duplicates → `note append` if topic exists → else `note add`. Wrapup ([`skills/claudemem/commands/wrapup.md`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/skills/claudemem/commands/wrapup.md)): review conversation → `session resolve` → extract notes with `--session-id` → `session save` with `--related-notes`.

What the skill says is **worth saving:** project-specific decisions, bug root causes, config quirks, user preferences. **Skip:** transient debug state, general programming knowledge, user-declared ephemeral info.

Natural-language triggers: “remember this”, “what do you remember about…”, “wrap up”, “what did we do last time” ([README.md](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/README.md) “Talk to It Naturally”).

### Human via CLI (or editing Markdown)

Any `claudemem note *` / `session save` command. Markdown files are inspectable/editable; index can be rebuilt (`reindex`, `repair`, `health`). ([README.md](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/README.md); [`CLAUDE.md`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/CLAUDE.md) constraint 4.)

### Hooks (do not, by design, write the long-term store)

[`docs/HOOK_INTEGRATION.md`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/docs/HOOK_INTEGRATION.md) + [`docs/UNIVERSAL_AGENT_INTEGRATION.md`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/docs/UNIVERSAL_AGENT_INTEGRATION.md):

- Hooks: context inject, health, optional sync pull/push, candidate classification, wrapup **reminders**.
- Must **not** save every tool output or dump transcripts.
- Automatic capture must be candidate-based: `detect → classify → redact → dedupe/merge → save/skip`.
- High-quality session reports still require `/wrapup` (model-active).
- Shipped `claudemem hook event` is **non-mutating** ([`cmd/hook.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/cmd/hook.go): “does not write notes, sessions, or candidate queue rows”; `--dry-run` cannot be turned off).
- Classifier ([`pkg/hooks/classifier.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/pkg/hooks/classifier.go)): high-signal **should_save=true** only for explicit user memory requests (“remember this”, etc.) and verified release/merge/deploy phrases; root-cause/config-gotcha → review, not auto-save; else skip.

Suggested Claude Code hook snippets (manual, not auto-installed): SessionStart `claudemem health --quick` + optional `sync pull` if `~/.claudemem/.sync_auto_pull` exists; SessionEnd optional `sync push` if `.sync_auto_push` exists. Flag files are **outside** `config.json` so they stay per-machine. ([`docs/HOOK_INTEGRATION.md`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/docs/HOOK_INTEGRATION.md).)

SKILL also documents a SessionStart hook example that runs `claudemem context inject --limit 5` ([SKILL.md](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/skills/claudemem/SKILL.md) “Context Injection”). That **reads** and prints compact context; it does not write notes.

### Legacy opt-in auto-capture (hook-adjacent, not default)

[`cmd/capture.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/cmd/capture.go): stdin JSON `{tool_name, tool_input, output}`. Only Bash; output 20–2000 chars; keeps error/fail/warning/passed/permission-denied lines; writes `auto-capture` notes. **Off unless** `features.auto_capture = true`. Docs call this legacy; new agents should go through `hook event` candidates first.

### Codex instruction path (same semantics, AGENTS.md)

Codex: SessionStart inject/health/optional pull; PostToolUse candidate dry-run; Stop = wrapup reminder, do not auto-save session. Memory discipline also stated in `AGENTS.md`: search at task start, save durable discoveries silently, wrap up before ending substantial sessions. ([`docs/HOOK_INTEGRATION.md`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/docs/HOOK_INTEGRATION.md) Codex section.)

---

## 4. Storage (files vs SQLite vs both) and source of truth

**Both, with Markdown authoritative.**

| Layer | Path | Role |
|---|---|---|
| Markdown + YAML frontmatter | `~/.claudemem/notes/<category>/<slug>.md`, `~/.claudemem/sessions/<slug>.md` | **Source of truth.** Human-readable, git-friendly, portable. |
| SQLite index | `~/.claudemem/.index/search.db` | **Regenerable cache:** `entries` metadata + `memory_fts` FTS5 (`id UNINDEXED, title, content, tags`). Tokenizer is **not** specified in schema (not porter). WAL, `busy_timeout=5000`, `MaxOpenConns(1)`. |
| Vectors | same DB (vector tables, schema gated in `pkg/vectors/store.go`) | Regenerable cache of embeddings; per-row `(backend, model, dim)`. |
| Config | `~/.claudemem/config.json` | Per-machine backend + flags. `0600`. |
| Permissions | dirs `0700`, files `0600` | [`SECURITY.md`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/SECURITY.md); `NewFileStore` mkdir 0700 |

Every write dual-writes Markdown **and** SQLite ([README.md](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/README.md) Architecture; [`CLAUDE.md`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/CLAUDE.md) Storage Pattern). `health` / `repair` / `reindex` exist because the two can drift.

**Filename stability (PR #21, 2026-09-03):** a note keeps the file it was created with across title changes (title lives in frontmatter). Only a **category** change moves it. `freeFilename` → `slug-2.md` on collision. Reason given in [`CLAUDE.md`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/CLAUDE.md): two-machine sync is **add-only**; rename-and-delete resurrects the old file from the peer and two files claim one UUID.

SQLite via `modernc.org/sqlite` (pure Go, `CGO_ENABLED=0`). ([`go.mod`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/go.mod); [`pkg/storage/filestore.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/pkg/storage/filestore.go).)

Backup: `claudemem export` / `import` tar.gz ([README.md](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/README.md) CLI). Archives are gitignored in the memory repo.

---

## 5. Retrieval (FTS, embeddings, hybrid) and whether network/model is required

### Default: local FTS5, no network, no embedding model

`claudemem search` uses FTS5 `MATCH` on `memory_fts`, joined to `entries`, with query sanitization (quote tokens that contain hyphens/specials). ([`pkg/storage/filestore_search.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/pkg/storage/filestore_search.go).)

Ranking:

- Base score = negated FTS5 `rank` (BM25-style).
- **Recency boost:** entries **&lt;7 days** get **+20%**; decays linearly to 0% by day 30. Then sort by fused score. Fetch `max(limit*2, 50)` before rerank.

Facets: `--type note|session`, `--category`, `--tag`, `--after`/`--before`, `--sort date` vs default relevance. Progressive disclosure: `--compact` → IDs + titles (+ score). [`cmd/search.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/cmd/search.go); SKILL “Search Modes”.

`claudemem context inject [--limit N] [--project]` returns recent notes + recent sessions + counts (~1–2KB), designed for SessionStart. ([`cmd/context_inject.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/cmd/context_inject.go).)

`/recall [topic]` skill: compact search then `note get` / `session get`; no topic → `context inject`. ([`skills/claudemem/commands/recall.md`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/skills/claudemem/commands/recall.md).)

### Optional hybrid: FTS ⊕ vectors (feature-flagged)

Hybrid runs when `features.semantic_search` is true **or** `--semantic` is passed (the latter errors if the flag is off). [`cmd/search.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/cmd/search.go).

Algorithm in source ([`FileStore.HybridSearch`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/pkg/storage/filestore_vectors.go)): **not RRF**. Min-max normalize each list to [0,1], then convex combination **`final = 0.7 * norm_fts + 0.3 * norm_semantic`**. Keyword-weighted because “queries are predominantly exact keyword lookups.” `--fts-only` skips vectors for one query.

### Embedding backends — explicit pick, no silent fallback

[`claudemem setup`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/README.md) wizard: Local Ollama / Gemini / Voyage / OpenAI / **TF-IDF**. First-run default backend string is **`tfidf`** if unset ([`loadBackendConfig`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/pkg/storage/filestore_vectors.go)). TF-IDF is a **first-class choice**, not a hidden fallback ([`pkg/vectors/tfidf_embedder.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/pkg/vectors/tfidf_embedder.go); [`docs/HYBRID_EMBEDDING_PLAN.md`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/docs/HYBRID_EMBEDDING_PLAN.md) design rule 5).

| Backend | Network? | When |
|---|---|---|
| TF-IDF | Never | Default / airgap / CI |
| Ollama | localhost (or `embedding.endpoint`) | Opt-in local model daemon **user** runs |
| Gemini / Voyage / OpenAI / Vertex | External API | Opt-in per machine via setup |

API keys: env vars only (`GEMINI_API_KEY`, `VOYAGE_API_KEY`, `OPENAI_API_KEY`). Config stores **`embedding.api_key_env`** (the name). `IsSecretKey` rejects writing secrets into config. ([README.md](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/README.md) Search Backends; [`pkg/config/wizard.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/pkg/config/wizard.go).)

**When the configured backend is down:** non-TTY → error + exit 1; TTY → prompt retry / FTS-only-this-query / run setup. No silent degrade. ([README.md](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/README.md); [`cmd/search.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/cmd/search.go) `handleBackendFailure`.)

**Network required for retrieval?** No, unless the user enabled a cloud (or remote Ollama) backend. Default FTS and TF-IDF are offline. Ollama is optional localhost.

---

## 6. Multi-machine sync

**Opt-in git of Markdown only.** Index and config stay per-machine; each machine rebuilds FTS and embeds missing vectors under **its** backend on pull. A Gemini laptop and an Ollama workstation share one corpus with “zero backend coupling.” ([README.md](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/README.md) “Cross-Machine Memory”; [`pkg/sync/gitsync.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/pkg/sync/gitsync.go) package comment.)

Commands ([`cmd/sync.go`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/cmd/sync.go)):

- `sync init <remote-url>` — `git init` at store dir; write `.gitignore` + `.gitattributes`; if remote has history, backup notes/sessions then `reset --hard origin/main`.
- `sync push` — `git add` `notes/`, `sessions/`, optional `MEMORY.md`, `.gitignore`, `.gitattributes`; commit; push. On conflict: `pull --rebase` then push again.
- `sync pull` — pull, then `Reindex()` FTS, then `IndexMissingVectors()` if semantic search is on.
- `sync status` — git status + index health.

Gitignored (never travel): `.index/`, `index.db`, `config.json`, `.sync_auto_pull`, `.sync_auto_push`, archives, editor junk, AppleDouble `._*`. ([`writeGitignore`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/pkg/sync/gitsync.go).)

**Union merge:** `.gitattributes` sets `*.md merge=union` so two machines appending the same note keep both sides without conflict markers. Documented as correct for **accumulative** notes. ([`writeGitattributes`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/pkg/sync/gitsync.go); [v3.0.10](https://github.com/zelinewang/claudemem/releases/tag/v3.0.10).)

**Auto-sync** is opt-in per machine via flag files + Claude Code SessionStart/SessionEnd snippets — “Off by default to avoid surprising network calls.” ([`docs/HOOK_INTEGRATION.md`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/docs/HOOK_INTEGRATION.md).)

**Not sync:** SQLite, vectors, embedding config, auto-pull/push toggles. Shells out to system `git` (not libgit2) to keep the binary small.

---

## 7. Agent integration (MCP, skills, hooks, which hosts)

| Surface | Status in this repo |
|---|---|
| **MCP server** | **Not shipped.** Comparison doc lists “MCP server integration” under **What NOT to Borrow**: “Adds complexity. CLI invocation via skills is simpler and sufficient.” ([`docs/COMPARISON_AND_ROADMAP.md`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/docs/COMPARISON_AND_ROADMAP.md) §5.) No MCP package/server in `cmd/` or `pkg/`. The only “MCP” hits in skills are wrapup *quality* questions about *other* MCPs the agent might have used. |
| **CLI** | Core product. Cobra commands; `--format json` for agents. |
| **Skill** | [`skills/claudemem/SKILL.md`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/skills/claudemem/SKILL.md) + `commands/wrapup.md` + `commands/recall.md`. Installed via `npx skills add … --global`. |
| **Slash commands** | `/wrapup`, `/recall [topic]`. |
| **Hooks** | Universal event protocol + dry-run classifier shipped; Claude Code / Codex **adapter snippets** documented, not a plugin with `hooks.json` in-repo. Control-plane only. |
| **Daemon / HTTP worker** | Explicitly rejected. Single binary, no required process. User may run Ollama separately. ([`CLAUDE.md`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/CLAUDE.md) constraint 3; comparison “Do not borrow required background workers”.) |

**First-class hosts** ([`docs/UNIVERSAL_AGENT_INTEGRATION.md`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/docs/UNIVERSAL_AGENT_INTEGRATION.md)):

| Agent | Role |
|---|---|
| **Claude Code** | P0 — mature hooks + slash commands |
| **Codex** | P0 — config-driven `AGENTS.md` + hooks |
| **Other coding agents** | P2 — “if they can invoke a CLI with JSON” |

README install copy also names **Cursor** and **Gemini CLI** as skill hosts (“it just works”). Integration is still **skill + CLI**, not MCP.

Three-layer design: (1) agent-neutral CLI/store, (2) normalized hook JSON, (3) thin adapters that must **not** implement memory policy. P0 rule: every new hook/capture/wrapup capability must be reviewed for **both** Claude Code and Codex.

**Extra CLI surfaces (not the memory model):** `code outline` (regex-ish symbols, Go/Python/TS/Rust); `graph` DOT/JSON of note↔session links. ([SKILL.md](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/skills/claudemem/SKILL.md).)

Installer: [`skills/claudemem/scripts/install.sh`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/skills/claudemem/scripts/install.sh) curls a GitHub-release binary (`claudemem-{os}-{arch}`) into `~/.local/bin`, else `go install`.

---

## 8. What TinyMem should consider stealing vs skipping

TinyMem today (this repo’s `README.md`): one Python **stdio MCP** server, **SQLite as the record**, FTS5 porter + BM25 + recency, **model-free**, namespaces + kinds + profile, tools as source of truth, hooks optional. Destination map: steal from claudemem, do not merge it ([`.scratch/memory-context-spec/map.md`](../map.md)). Each item below is still a **claudemem-sourced** idea, not a TinyMem decision.

### Steal (ideas, not implementation)

1. **Two nouns with opposite merge keys** — Notes accumulate by topic (title+category); sessions stay conversation-separate (`session_id`) and merge on re-wrap. Directly feeds TinyMem ticket *Name the domain nouns*. Sources: README Architecture table; `filestore_notes.go` / `filestore_sessions.go`.
2. **Hooks as control-plane only** — inject, health, optional sync, classify; **no transcript dumps, no “save every tool output.”** Session reports stay **model-active** (`/wrapup`). Matches TinyMem map “Tools are the source of truth. Hooks are optional.” Sources: `docs/HOOK_INTEGRATION.md`, `docs/UNIVERSAL_AGENT_INTEGRATION.md`, `cmd/hook.go` dry-run-only.
3. **Candidate-first capture pipeline** — `detect → classify → redact → save/skip`; autosave only after evals; unbounded queues called another log sink. Sources: `UNIVERSAL_AGENT_INTEGRATION.md` Invariants + Quality Gates; `pkg/hooks/classifier.go`.
4. **Markdown (or other files) as syncable SoT, index as cache** — if v1 sync exists, ship the human record and rebuild FTS locally. Per-machine index/config so backends (or “no backend”) do not couple machines. Sources: README; `pkg/sync/gitsync.go`; `HYBRID_EMBEDDING_PLAN.md` rule 4. Feeds ticket *Decide multi-machine sync*.
5. **Stable identity under add-only sync** — do not rename files that peers still have; dual files claiming one UUID is the failure mode they already hit. Source: `CLAUDE.md` filename-stability note (PR #21).
6. **Compact retrieval + optional SessionStart inject** — `--compact` then `get`; `context inject` as a **hook**, not a required proxy. Sources: `cmd/search.go`, `cmd/context_inject.go`, SKILL Search Modes. Feeds tickets *injection policy* and *retrieval shape*.
7. **Skill/instruction write discipline** — search before add; append vs new; wrapup template; “what is worth saving.” TinyMem already has write discipline in MCP tool descriptions; claudemem shows the same rules as **agent-facing skill text**. Source: SKILL Protocols 1–3; wrapup.md.
8. **Zero-network default + no silent fallback** — extra retrieval power is opt-in and fails loud. TinyMem already ships model-free FTS; this is a reason **not** to require embeddings later. Sources: README Search Backends; `SECURITY.md`; `HYBRID_EMBEDDING_PLAN.md` rules 1–2.
9. **Env-var-only secrets; per-machine feature flags outside the synced corpus.** Sources: `pkg/config/wizard.go` `IsSecretKey`; `.sync_auto_*` files outside `config.json`.

### Skip (or treat as anti-patterns for TinyMem v1)

1. **Rejecting MCP in favor of skills-only** — claudemem’s own comparison skip. TinyMem’s product **is** one MCP server for every agent (this repo’s `README.md`; map “Tools are the source of truth”). Source for their skip: `docs/COMPARISON_AND_ROADMAP.md` §5 “MCP server integration”.
2. **Embeddings / hybrid / Ollama-or-cloud as part of the core path** — TinyMem beta is explicitly lexical and model-free (this repo’s `README.md` “What search is”). Claudemem’s hybrid is optional and still pulls in backend failure UX. Sources: README Search Backends; TinyMem README.
3. **User-global `~/.claudemem` as the only identity** — no Namespace; `project` is a session field. TinyMem already partitions by namespace. Source: `cmd/root.go` default store.
4. **Legacy Bash `capture` auto-notes** — feature-flagged, keyword-scraped tool output; docs tell new agents not to use it. Source: `cmd/capture.go`.
5. **Large mandatory session journals as the default write** — wrapup.md demands multi-paragraph work journals. TinyMem write discipline is 1–2 sentences. Source: `skills/claudemem/commands/wrapup.md` vs TinyMem README Write discipline.
6. **Code outline / knowledge-graph CLI** — adjacent token-saving tools, not memory. Source: SKILL “Code Intelligence” / “Knowledge Graph.”
7. **Required daemon, Chroma, web viewer, agent-specific storage paths in the core** — claudemem already skips these; TinyMem should too. Source: `UNIVERSAL_AGENT_INTEGRATION.md` “Do not borrow”; comparison §5.
8. **Union-merge of contradictory edits** — correct for append-only notes; dangerous if TinyMem’s `update_memory` is a **correction** (replace, not concatenate). Source: `gitsync.go` gitattributes vs TinyMem `update_memory`.

### Stated “do not borrow” list (their words, useful as-is)

From [`docs/UNIVERSAL_AGENT_INTEGRATION.md`](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/docs/UNIVERSAL_AGENT_INTEGRATION.md): do not borrow saving every tool output, transcript dumps, opaque storage, required background workers, agent-specific storage paths in the core. Borrow: lifecycle trigger points, health/context/sync checks, compact injection, progressive disclosure, candidate capture and review queues.

---

## Appendix: command map (for later spec work)

From [README.md](https://github.com/zelinewang/claudemem/blob/ffded1e9f747cd6bfa22615143e4f4e2d49b5f40/README.md) CLI Reference, verified present under `cmd/`: `note add|search|list|get|append|update|delete|categories|tags`; `session save|list|search|get` plus `resolve|validate|repair`; unified `search`; `context inject`; `setup|health|repair|reindex`; `sync init|push|pull|status`; `stats|verify|config|export|import`; `hook event|eval`; `capture` (legacy); `code outline`; `graph`.
