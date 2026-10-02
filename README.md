# TinyMem

Local memory with one MCP server for every coding agent.

One Python stdio MCP server. Pi, Cursor, Claude Code, VS Code, and Codex
launch that same process and share one SQLite file.

`0.1.0-beta.1` was a different product (Rust, Pi-only). It was never
published. Do not use that tag. The first beta of this product is
`0.2.0-beta.1`.

## Install

Requires Python 3.14 or newer. No third-party dependencies.

```sh
git clone <this-repo>
cd tinymem
python3 -m pip install .
```

This provides the `tinymem` console script.

## Connect

In the project directory, register the same server with every agent:

```sh
tinymem connect
```

This writes `tinymem mcp` plus `TINYMEM_DB` and `TINYMEM_NAMESPACE`
into `.pi/mcp.json`, `.cursor/mcp.json`, `.mcp.json`,
`.vscode/mcp.json`, and `.codex/config.toml`.

The namespace defaults to the git toplevel directory name, sanitized to
`[A-Za-z0-9_:-]{1,64}`. Select agents with `--agent pi --agent cursor`.
Config output alone is not qualification; see `docs/BETA_CHECKS.md`.

## Tools

Seven tools, same for every agent:

- `add_memory` — store a durable fact, preference, decision, or plan.
- `search_memories` — search active memories.
- `update_memory` — correct a memory instead of re-adding.
- `get_profile` — get the static profile for a namespace.
- `set_profile` — set the static profile for a namespace.
- `memory_stats` — counts by namespace/kind plus database size.
- `delete_memory` — hide a memory from search.

Other commands:

```sh
tinymem mcp                        # serve the stdio MCP server
tinymem stats [--json]             # counts and database size
tinymem purge [--grace-days N]     # hard-delete hidden/expired rows, then vacuum
tinymem profile get [--namespace N]
tinymem profile set [--namespace N] "text" | --clear
```

## Write discipline

`add_memory` only for durable facts, preferences, decisions, plans in
1-2 sentences (aim under 500 chars). Never re-add a correction; use
`update_memory`, which bumps `updated_at` so fresher facts rank higher.
Set `valid_until` (`YYYY-MM-DD`) for transient facts like exams,
branches, or work in progress. One static profile per namespace holds
stack, conventions, and commands; read it before search.

## Read discipline

Prefer `get_profile` first, then `search_memories` with 1-3 distinctive
terms and limit 3-5; shorten the query on zero hits. Omitting
`namespace` searches the session namespace (`TINYMEM_NAMESPACE`).
Passing `namespace` explicitly is the only cross-project read.
`update_memory` and `delete_memory` refuse ids outside the session
namespace.

## Plaintext database

The database is a plain SQLite file, default `~/.tinymem/tinymem.db`
(`TINYMEM_DB` overrides it). Anyone with file access can read every
namespace. Namespace is a label, not auth and not encryption.

`delete_memory` hides a note from search (`deleted_at` plus FTS removal
in one transaction). The row stays in the file. `purge` hard-deletes
hidden or long-expired rows and runs `VACUUM` when SQLite allows it.
Neither is secure erasure: free pages, WAL, and file copies may retain
content.

## What search is

Offline lexical search: FTS5 with porter stemming, prefix match, and
BM25 plus recency. Model-free and offline. Lexical search is not
semantic search. There is no embedding, repository index, file
provenance, automatic context injection, or conversation capture.

Beta makes no claim that memory improves coding results.

## Out of beta scope

Rust engine, native archives, Pi extension, Cloudflare gateway,
Python/TypeScript SDKs, team auth, encryption, secure deletion,
export/import, Windows, Intel macOS, Linux qualification beyond the
cells recorded in `docs/BETA_CHECKS.md`, and publishing to PyPI or
GitHub Releases.
