# TinyMem memory-and-context spec

Label: wayfinder:map

## Destination

A decision-complete spec for TinyMem as open-source memory-and-context infrastructure for coding agents — including license, public GitHub, and what v1 includes. This map does not implement the spec.

## Notes

- Domain: this repo. Read `CONTEXT.md` before writing; update it when a noun is resolved.
- Skills every session should consult: grilling, domain-modeling. Research tickets also consult research.
- Tracker: local markdown under `.scratch/memory-context-spec/` (no git remote yet).
- Plan; do not implement TinyMem in this map.
- Tools are the source of truth. Hooks are optional. A required proxy is out of destination.
- Inspirations to steal from, not merge: [zelinewang/claudemem](https://github.com/zelinewang/claudemem), [thedotmack/claude-mem](https://github.com/thedotmack/claude-mem), Headroom's memory model, Ponytail's portability pattern only, [SuperMemory](https://github.com/supermemoryai/supermemory) for atomic Memories + Profile + container (direct writes only).
- Refer to tickets by **name** (the name wraps its link).

## Decisions so far

- [Research zelinewang/claudemem](issues/01-research-zelinewang-claudemem.md) — Notes + session reports as markdown SoT; FTS5 cache; CLI writes; opt-in git sync; no MCP.
- [Research thedotmack/claude-mem](issues/02-research-thedotmack-claude-mem.md) — Auto-capture + LLM compression into observations; layered retrieval; hook-injected index; not model-free.
- [Research Headroom memory](issues/03-research-headroom-memory.md) — Opt-in memory beside a proxy: computed scopes, supersession, provenance metadata, tools and/or auto-inject.
- [Research Ponytail portability](issues/04-research-ponytail-portability.md) — One core skill, three loader tiers (hooks, plugins, `AGENTS.md` copies); MCP cannot inject every turn.
- [Name the domain nouns](issues/05-name-the-domain-nouns.md) — Stored: Namespace, Profile, Memory. Context is assembled. No Session/Observation. Code map lives in Profile. SuperMemory = direct atomic Memories only.
- [Decide capture policy](issues/06-decide-capture-policy.md) — Capture refused. Only Tools write. Hooks may remind or inject; they must not persist.
- [Decide injection policy](issues/07-decide-injection-policy.md) — Optional SessionStart Hook injects Profile + Index; full Memories stay behind Tools; fail-open.
- [Decide retrieval shape](issues/08-decide-retrieval-shape.md) — Lexical FTS only; one `search_memories` Tool; Index is recency-capped; no vectors.
- [Decide identity and scope](issues/09-decide-identity-and-scope.md) — Namespace only; no User/Session/Agent/Turn scopes; no writer field.
- [Decide multi-machine sync](issues/10-decide-multi-machine-sync.md) — Store is SoT; no v1 sync protocol; no markdown dual-write.
- [Decide the adapter surface](issues/11-decide-the-adapter-surface.md) — Three tiers (MCP, optional SessionStart Hook, discipline file); `connect` MCP-always; five named hosts.
- [Decide license and public GitHub](issues/12-decide-license-and-public-github.md) — MIT; public GitHub after this spec; PyPI later; this map does not create the remote.
- [Decide write discipline](issues/14-decide-write-discipline.md) — 1–2 sentences; kinds fact/preference/decision/plan; Memory vs Profile vs refuse as above.
- [Decide what v1 of the spec includes](issues/13-decide-what-v1-includes.md) — Today’s MCP + optional Hook/Index/discipline file + MIT GitHub; caps unchanged; spec at [spec.md](spec.md).

## Not yet specified

None. The way is clear. Implement from [spec.md](spec.md) and [`CONTEXT.md`](../../CONTEXT.md).

## Out of scope

- Implementing the spec; this map ends when the spec is decision-complete.
- A required Headroom-style proxy or Headroom's compression product.
- Ponytail's lazy-senior coding ruleset (portability pattern only).
- SuperMemory Documents, graph dreaming, required embeddings, connectors, and hosted API.
- davidlenz/claudemem and any semantic codebase indexer.
- The withdrawn Rust / Pi-only TinyMem `0.1.0-beta.1`.
- Team auth, encryption, and secure deletion.
