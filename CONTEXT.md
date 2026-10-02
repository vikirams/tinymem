# TinyMem

TinyMem is the shared memory-and-context product in this repository. Coding agents read and write the same store through it.

## Language

**TinyMem**:
The product in this repository: memory-and-context infrastructure for coding agents.
_Avoid_: Claudemem, Claude-Mem, Headroom, Ponytail, SuperMemory, vault

**Store**:
The single local durable record of Namespaces, Profiles, and Memories. It is the source of truth, not a cache of files.
_Avoid_: markdown corpus, dual-write, vault

**Namespace**:
The project bucket inside the shared store, and the only isolation unit. It is not a user, not an agent, not a session, not a turn, and not a permission.
_Avoid_: workspace, vault, account, tenant, User, Global

**Profile**:
One static text blob per namespace: stack, conventions, commands, and a short code map. Not a memory and not a repository index.
_Avoid_: system prompt, CLAUDE.md, AGENTS.md, Map, repo dump

**Memory**:
One durable sentence-level note (one or two sentences) with a kind: fact, preference, decision, or plan. A correction replaces it; it is not a second note.
_Avoid_: document, embedding, file, session, observation, episode, transcript

**Context**:
The assembled window contents for one turn, built from Profile and an Index. Not stored. Full Memory text is not Context.
_Avoid_: prompt dump, transcript, injected row

**Index**:
The recency-capped compact listing of Memories (id, kind, one line) that may appear in Context. Not stored and not the Memory body.
_Avoid_: timeline dump, observation table, vector hit

**Hide**:
A memory marked not searchable whose row remains in the file.
_Avoid_: erase, delete (as if gone)

**Purge**:
Hard-delete of hidden or long-expired rows. Not secure erasure.
_Avoid_: wipe, secure delete

**Tool**:
An MCP operation the agent calls. Tools are the only write path and the source of truth for reads and writes.
_Avoid_: API, endpoint, proxy, capture

**Hook**:
An optional host callback that may read the store, remind the agent, or at session start assemble Context (Profile + Index). It must not persist. Not required to get value. If it fails, the session still starts.
_Avoid_: proxy, wrapper, middleware, observer, auto-capture

**Adapter**:
A thin host-specific loader that registers the MCP server, and may opt in a SessionStart Hook and a discipline file. It does not reimplement Tools.
_Avoid_: fork, proxy, plugin-as-core
