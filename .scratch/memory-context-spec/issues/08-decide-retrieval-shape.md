# Decide retrieval shape

Type: grilling
Status: resolved
Blocked by: 01, 02, 03

## Question

What retrieval does the spec require: today's lexical search only, a layered search/timeline/get pattern, hybrid lexical+vector, or another shape — and what stays model-free and offline?

## Answer

**Lexical only, model-free, offline.** One Tool: `search_memories` returns full matching Memories, capped. `get_profile` first. No embeddings, no hybrid, no timeline Tool, no get-by-id layer (a Memory is already one sentence). The SessionStart Index is the same store ranked by recency with a hard cap, not a second engine. Exact N stays unspecified.
