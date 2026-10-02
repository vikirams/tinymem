# TinyMem v1 spec

Decision-complete. Implement from this file plus [`CONTEXT.md`](../../CONTEXT.md). Detail lives in the tickets under [`issues/`](issues/). This spec does not implement TinyMem.

## What v1 is

Today’s local Python MCP server, plus three additions: an optional SessionStart Hook that injects Profile + Index, a discipline file, and a public GitHub repository under MIT.

Not a rewrite. Not a proxy. Not a sync product.

## Language

Use [`CONTEXT.md`](../../CONTEXT.md). Stored: **Store**, **Namespace**, **Profile**, **Memory**. Assembled: **Context**, **Index**. **Session** and **Observation** are not stored types.

## Store

One local file is the source of truth (SQLite today). FTS is an index inside that file, not a second record. No markdown corpus, no dual-write. Copying the file is allowed and unspecified. No v1 sync protocol.

Before any incompatible on-disk change, bump `user_version`. v1 needs no schema migration: Index is computed, not stored.

## Isolation

Namespace only. Every agent on a project shares that bucket. Cross-project read is an explicit `namespace` on search. No User, Session, Agent, or Turn scopes. No writer field.

## Tools (only write path)

Same seven Tools: `add_memory`, `search_memories`, `update_memory`, `get_profile`, `set_profile`, `memory_stats`, `delete_memory`. Adapters must not fork them.

## Capture

Refused. Hooks must not persist.

## Injection

Optional SessionStart Hook (and resume/clear if the host has them): inject **Profile + Index**. Index is recency-capped, five entries, each id / kind / one line. Full Memory text stays behind `search_memories`. Hosts without hooks still work. Inject failure must not block the session. No every-turn inject. No Read-gate.

## Retrieval

Lexical FTS only, model-free, offline. `get_profile` first, then `search_memories` (1–3 terms, default limit 5, max 20). No embeddings, no timeline Tool, no get-by-id layer.

## Write discipline

Memory: one or two sentences (aim under 500 characters); kinds `fact`, `preference`, `decision`, `plan`; `valid_until` for decay; corrections via `update_memory`.

- Memory — durable sentence that will still matter next week (including a durable code fact).
- Profile — stack, conventions, commands, short code map (max 4000 characters).
- Refuse — transcripts, tool logs, file bodies, same-day recaps, duplicates.

Quota: 2000 active Memories per Namespace.

## Adapters

Three tiers: MCP always; optional SessionStart Hook; opt-in discipline file (the rules above, not Ponytail taste). `connect` always writes MCP config. v1 hosts: Pi, Cursor, Claude Code, VS Code, Codex.

## License and publish

MIT. After this spec, first implementation step is a public GitHub repository. PyPI is later. This map does not create the remote.

## Runtime

Python 3.14+, stdlib only, install from the git repo. OS qualification is not a v1 bar.

## Later

Sync protocol; PyPI; more hosts; hybrid/embeddings; Windows/Linux qualification as a bar.

## Never (this product)

Capture; Session/Observation as stored types; required proxy; Headroom compression; Ponytail coding ruleset; SuperMemory Documents/graph/connectors/hosted API; semantic codebase indexer; team auth; encryption; secure deletion.
