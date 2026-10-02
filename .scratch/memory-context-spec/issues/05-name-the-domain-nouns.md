# Name the domain nouns

Type: grilling
Status: resolved
Blocked by: 01, 02, 03

## Question

Which nouns exist in TinyMem's spec — Memory, Profile, Namespace, Context, Session, Observation, or others — and what is each one? A correction, a session summary, and an injected block must each have a home or be refused.

## Answer

Stored nouns are **Namespace**, **Profile**, and **Memory**. **Context** is assembled for a turn from Profile + retrieved Memories; it is not stored. **Session** and **Observation** are refused as stored types. SuperMemory inspires atomic direct-write Memories only (no Documents, graph, or required embeddings).

Homes:

- **Correction** — replaces a Memory.
- **Session summary** — a Memory, only if the takeaway will still matter next week; transcript recaps are refused.
- **Injected block** — assembled Context, not a row.
- **Code map** — lives in Profile, bounded; TinyMem does not index the repository.

Headroom memory ideas and Ponytail portability stay on their existing tickets. Headroom compression and Ponytail's coding ruleset stay out of destination.

Glossary: [`CONTEXT.md`](../../../CONTEXT.md).
