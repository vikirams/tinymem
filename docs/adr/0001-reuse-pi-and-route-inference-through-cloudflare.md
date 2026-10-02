---
status: accepted
---

# Reuse Pi and route inference through Cloudflare

**Scope revised by [ADR 0003](0003-open-source-coding-agent-memory.md).** The gateway-only rule below
continues to govern the controlled Pi/Cloudflare integration, not every deployment of the open-source
core. Pi is the initial evaluation runtime, not a required client. The original decision follows.

TinyMem is a memory product, not another coding-agent runtime. Reuse Pi's agent loop, tools, sessions, compaction and supported provider transports for development and end-to-end evaluation; implement only the memory adapter and scenario runner. Building a new agent would duplicate a large unrelated maintenance surface and make memory quality harder to compare.

All enabled model operations—including embeddings, compaction, summary maintenance and evaluation judges—must use Cloudflare AI Gateway. Preserve model-free local operation; do not provide direct-provider or local-inference fallback in the managed product/evaluation path. This trades on-device inference and gateway-independent availability for centralized model policy, observability and usage control; selected text leaves the machine when inference is enabled.

This records the requested product policy, **not completed enforcement**. See [the scoped Pi/gateway plan](../PI_GATEWAY_PLAN.md) for routing, isolation, privacy and validation tasks; the [coding-agent roadmap](../MEMORY_SYSTEM_PLAN.md) determines current product priorities.
