# Task context assembly

`assemble_context(query, scope, token_budget)` answers which stored evidence should go into a task's
prompt. It selects and packs evidence, not a generated summary. It adds no model/runtime dependency,
changes no personal memories or audit events. Assembly originally shipped on native v3;
[file provenance](FILE_PROVENANCE.md) now uses native v4 and validates saved-memory dependencies.

## Interface

Rust:

```rust
use tinymem_engine::{ContextOptions, ContextScope, ContextSource, Engine, MemoryScope};

let mut engine = Engine::open("/tmp/native-memory.db")?;
let scope = ContextScope {
    memory: MemoryScope {
        user_id: "alice".into(), agent_id: None, project_id: Some("app".into()),
    },
    repository: Some("/path/to/repo".into()), // None for personal-memory-only context
};
let context = engine.assemble_context("Update auth cookie handling", &scope, 4000)?;
// Defaults include both memory and repository evidence.
let options = ContextOptions {
    sources: vec![ContextSource::Memory, ContextSource::Repository],
    exclude_paths: vec!["generated/**".into()],
};
let filtered = engine.assemble_context_with_options(
    "Update auth cookie handling", &scope, 4000, &options,
)?;
// Insert context.prompt or filtered.prompt, not the whole result envelope.
```

The MCP tool takes `{"query":"Update auth cookie handling","token_budget":4000}`; optional
`sources` selects `memory` and/or `repository`, and `exclude_paths` filters repository paths for this
assembly only. User/agent/project and repository remain fixed at startup. Model-facing
`content[0].text` is exactly the budgeted prompt (including an empty string when no evidence fits);
`structuredContent` retains the complete SDK result. It refreshes the repository index, so its
`readOnlyHint` is false, though personal evidence is strictly read-only.

SDKs use their configured scope/root and can pass optional assembly-only filters:

```python
context = client.assemble_context(
    "Update auth cookie handling", 4000,
    sources=["memory", "repository"], exclude_paths=["generated/**"],
)
prompt_context = context["prompt"]
```

```ts
const context = await client.assembleContext('Update auth cookie handling', 4000, {
  sources: ['memory', 'repository'], exclude_paths: ['generated/**'],
});
const promptContext = context.prompt;
```

The SDK result has `prompt`, `entries`, `token_budget` (effective capped budget), `tokens_used` and
`token_count_method`; MCP `structuredContent` preserves that complete object while model-facing text
contains only `prompt`. Each entry has `category`, unabridged `content`, and a `provenance` array.
The prompt is an evidence-only notice followed by compact JSON of exactly these entries; escaping
keeps quotes/newlines/control characters inside data. It does not include the query or response metadata.
An empty selection returns `prompt: ""`, `entries: []`, `tokens_used: 0`, even for a zero/tiny budget.

## Selection policy

1. Validate the exact memory tuple and query before any repository side effects. Scope has no wildcard
   or automatic global/project inheritance. Refresh the optional canonical repository, even at zero
   budget or with an empty query. A refresh/provider/storage failure fails the operation; it does not
   silently return stale evidence or fall back from a configured provider.
2. Accept task text up to **65,536 UTF-8 bytes** (64 KiB), with no NUL. Scan the entire task for
   alphanumeric words, lowercase them, remove non-alphanumeric marks introduced by lowercasing,
   and ignore common English function words. Words exceeding **127 bytes** before or after
   normalization are skipped, never truncated into different identifiers. Count repeated normalized
   words; select at most **32 unique terms** by frequency, breaking ties by first occurrence, then
   restore their original order. A repeated topic near the task's end can therefore displace earlier
   singleton words. This is a deterministic lexical heuristic, not a relevance model. The joined
   retrieval query is at most 4095 bytes; even an optional embedder receives only these selected terms,
   not the full task. Empty/punctuation/stopword/oversized-word-only tasks return no entries.
   Lexical retrieval uses OR, rather than requiring an entire task sentence in each record. Operators
   in query text are not executed. Standalone search limits and lexical AND behavior are unchanged.
3. Source selection defaults to both `memory` and `repository`; an empty list selects neither.
   `exclude_paths` accepts up to 64 case-sensitive, repository-relative globs (each at most 1024
   UTF-8 bytes and 16 KiB total). `*` does not cross `/`; `**` can match recursively. Absolute paths,
   empty/dot/dot-dot components, backslashes, control characters, colons and malformed globs are
   rejected. Exclusions apply to repository hits before candidate caps and byte packing; they do not
   filter personal-memory `source_refs`. They affect only this assembly call: explicit
   `search_repository` and `search_memories` remain unrestricted.
   Without an embedder, fetch the top 100 active scoped memories by BM25, then latest update and ID.
   With an embedder, use existing hybrid retrieval (lexical OR plus cosine/RRF, default similarity
   floor 0.25), with a 100-record/1-MiB hydration bound. Assembly never backfills embeddings. Retrieve
   up to 100 current hash-validated source chunks from the fixed repository (1-MiB hydration bound).
   Expired/deleted memories and foreign scopes/roots are excluded. Changed/ignored/missing repository
   chunks are excluded. Saved memories with explicit file dependencies are checked before candidate
   caps; stale or unknown dependencies exclude them from lexical and hybrid assembly. A free-text
   `source` alone remains untracked. Origin-only file references do not create validity dependencies.
4. Merge identical evidence. Personal-memory comparison collapses whitespace, case-sensitively;
   source comparison retains exact code whitespace. A source matching normalized personal evidence
   shares the entry and supplies the exact source text. All contributing provenance survives the merge:
   memory ID, exact scope, kind, explicit source, structured file references, version, timestamps and expiration; or canonical
   repository, path, full line range, file hash, language, syntax status and indexing timestamp.
   Deduplication is exact/whitespace-based, not semantic consolidation or contradiction resolution.
5. Pack ranked categories in round-robin order: preferences, decisions/architectural decisions,
   repository, other memories. Within a category, retrieval order remains intact. Cross-source BM25
   and cosine scores are not comparable; no fabricated confidence or truth ranking is returned.
   For duplicate kinds, the earliest category in this order wins, but all original kinds remain in
   provenance. Whole entries (including every citation) that do not fit are skipped; later smaller
   candidates are still considered. Content and line ranges are never truncated or summarized.

## Search guidance

Standalone lexical `search_memories` and `search_repository` use literal **AND** matching: query
with 1–3 distinctive terms and shorten the query after zero hits. `assemble_context` instead reduces
the task to a bounded set of useful terms and uses lexical OR retrieval so task sentences do not
need to appear verbatim in each record. When automatic context has already injected sufficient
evidence, use it rather than issuing another redundant recall/search; search explicitly only to find
missing evidence or inspect a narrower question.

## Budget contract

The budget includes the **complete rendered prompt**, including the notice, JSON escaping, categories
and every provenance field. The outer MCP/SDK response is transport metadata and is not included;
insert only `prompt`. Reserve tokens for your other messages, task text, chat framing and output yourself.
The effective token budget is capped at **262,144**, with an independent **262,144-byte** prompt cap.
There are at most 200 candidates before deduplication. Packing is deterministic for unchanged evidence
and a deterministic provider/counter, but is a bounded greedy policy, not a globally optimal knapsack.

No universal tokenizer exists. The default MCP/SDK/Rust counter is **one token per UTF-8 byte**, labelled
`utf8_bytes_upper_bound`. This is conservative for byte-fallback tokenizers, not exact model usage and
not a characters/4 heuristic. Unicode, code and escaped citations cannot silently exceed that bound.
It intentionally leaves capacity unused; do not interpret `tokens_used` as your provider's billing count.
For other tokenizers or tighter utilization, Rust callers can supply the target tokenizer:

```rust
let context = engine.assemble_context_with_counter(query, &scope, 4000, |text| {
    tokenizer.encode_ordinary(text).len() // caller's own tokenizer; no TinyMem dependency
})?;
```

This returns `token_count_method: "custom"`. Each candidate is serialized once and the accepted JSON
prefix reused, avoiding repeated serialization of already-packed evidence. The deterministic callback
is still invoked on each entire trial prompt within the byte cap and the final prompt, not just on
content or by adding independently tokenized lengths.
Treat special-token-like evidence as ordinary text. The engine enforces the supplied counter's budget;
correctness for a particular model depends on choosing the right tokenizer. The MCP/SDK surfaces do
not currently accept a remote/custom tokenizer.

## Verification and limits

Rust public-interface tests cover mixed selection, exact scopes, latest versions, deleted/expired
memories, provenance-preserving memory/source deduplication, code whitespace, exact/zero/tiny/Unicode
budgets, oversized-entry skipping, nonadditive custom counting, deterministic ordering, cache refresh,
source changes/deletions/ignore rules and read-vs-write separation. Long-task tests exercise exact
64-KiB/UTF-8 limits, validation before refresh, useful repeated terms beyond a long preamble, bounded
Unicode normalization and unchanged standalone search limits. Real stdio MCP and both SDK suites
exercise assembly, strict arguments, fixed scope/root, hybrid retrieval and complete prompt budgeting.

For a reproducible in-process packing microbenchmark, run
`cargo run --release -p tinymem-engine --example context_benchmark`. It assembles 100 synthetic,
roughly 2-KiB personal memories 200 times at small and large budgets, checking unchanged prompt output.
It excludes process startup, repository refresh and file-dependency validation; it is not an end-to-end
Pi latency or evidence-relevance benchmark. Assembly retains its freshness checks on every call; there
is no cross-call cache of dependency validity.

This does not claim real-model relevance benchmarks, automatic extraction, semantic source search,
semantic deduplication, inference of which generic preference applies to every task, contradiction
resolution or filesystem snapshot isolation against concurrent edits. Evidence remains untrusted data,
not privileged instructions. See [native safety/scoping](NATIVE_MEMORY.md) and
[retrieval limitations](SEMANTIC_RETRIEVAL.md).
