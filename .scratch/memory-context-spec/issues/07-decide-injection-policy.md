# Decide injection policy

Type: grilling
Status: resolved
Blocked by: 02, 03, 05

## Question

When, if ever, does TinyMem put retrieved content into an agent's window without a Tool call? Never, optional Hooks on hosts that have them, or another rule — and what is injected (Memories, Profile, something else)?

## Answer

**Optional Hooks, session start only.** On hosts that can attach Context, a Hook injects **Profile + Index** (Memory id, kind, one line). Full Memory text is retrieved with Tools. Hosts without hooks still work via `get_profile` / `search_memories`. Inject failure must not block the session. No every-turn inject, no required proxy, no Read-gate.
