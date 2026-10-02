---
name: tinymem
description: Read and write TinyMem — project Namespace memories and Profile. Use when starting work in a repo, when a durable fact or decision appears, or when the user says remember/forget.
---

# TinyMem

Local memory. One MCP server. Tools are the only write path.

## Read

1. `get_profile` first (stack, conventions, commands, short code map).
2. `search_memories` with 1-3 distinctive terms, limit 5. Shorten the query on zero hits.
3. An optional SessionStart Hook may inject Profile plus a recency Index. That Index is not the Memory body; use `search_memories` for full text.

## Write

`add_memory` only for a durable sentence that will still matter next week (kinds: fact, preference, decision, plan). One or two sentences, aim under 500 characters. Never re-add a correction; `update_memory`. Set `valid_until` (YYYY-MM-DD) for branches, WIP, exams.

`set_profile` for stack, conventions, commands, and the short code map. Not a repository dump.

Refuse transcripts, tool logs, file bodies, same-day recaps, and duplicates.

Hooks must not persist. Do not capture tool output.
