# Changelog

## `0.2.0-beta.1` — 2026-10-03

- One Python stdio MCP server, stdlib-only, `tinymem mcp`.
- Seven tools: `add_memory`, `search_memories`, `update_memory`,
  `get_profile`, `set_profile`, `memory_stats`, `delete_memory`.
- Store: `~/.tinymem/tinymem.db`, WAL, FTS5 porter + prefix, BM25,
  exact + normalized dedup, `valid_until`, soft delete, `purge`,
  2000 active rows per namespace.
- `tinymem connect` writes same command to Pi/Cursor/Claude/VSCode/Codex.
- Optional SessionStart hooks (Claude/Cursor): Profile + 5-entry Index,
  pinned DB/namespace, fail-open.
- 36 unittests green on Python 3.14.7 arm64; clean non-editable package
  and generated-hook checks pass.
- Pi 1.0.0 live add/search/update/delete qualification across two processes.
  Other agent configs are not live-qualified.
- Source-only beta tag; no PyPI package or GitHub release assets.

## `0.1.0-beta.1` — withdrawn

Tag on `43c5398`. Different product (Rust, Pi-only). Never published.
Do not use. First beta of this product will be `0.2.0-beta.1`.
