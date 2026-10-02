# Decide what v1 of the spec includes

Type: grilling
Status: resolved
Blocked by: 05, 06, 07, 08, 09, 10, 11, 12, 14

## Question

What is in v1 of the TinyMem spec versus later versus never — the cut that makes the spec implementable without reopening capture, injection, retrieval, identity, sync, adapters, or license?

## Answer

**v1 is today’s MCP server plus optional SessionStart Hook (Profile + Index, cap 5), discipline file, and public GitHub under MIT.** Caps stay as in code (search default 5 / max 20, Profile 4000, 2000 active per Namespace). Python 3.14+, stdlib, clone+pip. No v1 schema migration. Sync, PyPI, more hosts, hybrid, OS qualification bars are later. Capture, proxy, embeddings-as-core, code indexer, auth/encryption are never.

Assembled spec: [spec.md](../spec.md).
