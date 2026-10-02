---
status: accepted
---

# Make coding-agent memory an independently usable open-source core

TinyMem will focus on MIT-licensed, local-first memory for coding agents, prioritizing source freshness,
write quality, budgeted context, reproducible coding outcomes and portable evidence over a general chatbot
or managed-cloud product. MCP and thin Claude Code/Cursor integrations are release requirements; Pi remains
the initial evaluation runtime, not a dependency imposed on users of other agents.

This narrows [ADR 0001](0001-reuse-pi-and-route-inference-through-cloudflare.md): Cloudflare-only routing
remains mandatory within the existing controlled gateway integration, but that integration and its
credentials are not prerequisites for the open core, deterministic benchmarks or public release.
This gives up one universal model-policy boundary in favor of independently deployable memory; optional
adapters must state their own trust, data-transfer and reproducibility contracts without silent fallback.

The [roadmap](../MEMORY_SYSTEM_PLAN.md) and [benchmark protocol](../CODING_MEMORY_BENCHMARK.md) separate
shipped behavior from these commitments. Team/org authorization is a later explicit security boundary,
not a new name for existing local scopes.
