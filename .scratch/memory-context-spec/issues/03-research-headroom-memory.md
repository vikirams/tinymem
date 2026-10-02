# Research Headroom memory

Type: research
Status: resolved

## Question

What is Headroom's persistent-memory model — scopes, versioning, dedup, provenance, tools vs auto-inject — as distinct from its compression proxy, as stated by its own docs and source?

## Answer

Memory is opt-in beside the compression proxy: computed scopes (user/session/agent/turn), supersession versioning, provenance as metadata, tools and/or auto-inject. Detail: [headroom-memory.md](../research/headroom-memory.md).
