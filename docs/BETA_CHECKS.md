# TinyMem beta checks

Date: 2026-10-02. Machine: Darwin 25.5.0 arm64 (MacBook Pro).
Python: 3.14.7. Package: `tinymem 0.2.0.dev0` installed editable
(`/opt/homebrew/bin/tinymem`).

## Suite

`python3 -m unittest tests.test_server tests.test_connect`: 26 tests,
passing, on this machine only.

## Installed-command stdio qualification (PASS)

Against the installed console script, temp DB, namespace `qual-pi`.
Two OS processes share one file; the second process sees the first
process's write.

Process 1 (`tinymem mcp`):

- `initialize` (`2025-06-18`) → `serverInfo: {name: tinymem,
  version: 0.2.0-dev}`.
- `tools/list` → seven tools: `add_memory`, `search_memories`,
  `update_memory`, `get_profile`, `set_profile`, `memory_stats`,
  `delete_memory`.
- `add_memory {content: "Qualification note uses blue green deploy"}`
  → id `7966e94f…`, namespace `qual-pi`.

Process 2 (`tinymem mcp`, same `TINYMEM_DB`, same namespace):

- `search_memories {query: "blue green"}` → the added id.
- `update_memory {id, content: "Qualification note uses red black
  deploy"}` → `updated: true`.
- `search_memories {query: "red black"}` → the same id.
- `delete_memory {id}` → `{deleted: true}`.
- `search_memories {query: "red black"}` → `[]`.
- `tinymem stats --json` → `active: 0, deleted: 1`.

## Pi (PARTIAL)

`pi mcp list --json` from this repo (project scope,
`.pi/mcp.json` → `tinymem mcp`, `TINYMEM_NAMESPACE=tinymem`):

- `state: connected`, `tools` lists the same seven tools, `errors: []`.

Pi agent tool-call round-trip not completed: default provider returned
`403` (Grok credits), and the `openai-codex` models were unusable
(`gpt-5.3-codex-spark` unsupported on this account; `gpt-5.5` usage
limit reached). No Pi-driven add/search/update/delete is recorded here.

## Other agents

Cursor, Claude Code, VS Code, Codex: config written by
`tinymem connect` (`command: tinymem, args: [mcp]`,
`TINYMEM_NAMESPACE: tinymem`), verified on disk. No live tool
round-trip recorded. Status: config written, not qualified. Not listed
as supported.
