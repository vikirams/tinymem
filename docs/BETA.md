# TinyMem beta plan

Updated 2026-10-02. This file replaces `docs/PI_BETA_RELEASE.md`, `docs/PI_BETA_CHECKS.md`, and `docs/MEMORY_SYSTEM_PLAN.md`. Those described a Rust, Pi-only product. That product is withdrawn.

## Decision

Beta is one local Python stdio MCP server. Pi, Cursor, Claude Code, VS Code, and Codex launch that same process and share one SQLite file. There is no per-agent memory implementation, no Rust engine, and no hosted service.

`0.1.0-beta.1` (tag on `43c5398`) is a different product. It was never published. Do not push that tag, do not attach its archives to a GitHub release, and do not call the current tree `0.1.0-beta.1`.

The first beta of this product is `0.2.0-beta.1`. The working tree currently says `0.2.0-dev`. Do not tag, and do not say "downloadable", until a real install URL exists. A passing local test is not a release.

## What is true now

`HEAD` is `12dba07`, the Python rewrite commit on top of the withdrawn Rust beta (`43c5398`). Nothing is on a remote.

What runs today:

- Python 3.14 stdio MCP (`tinymem/__main__.py` with no args, or `mcp`).
- Tools: `add_memory`, `search_memories`, `update_memory`, `get_profile`, `set_profile`, `memory_stats`, `delete_memory`.
- Store: `~/.tinymem/tinymem.db` unless `TINYMEM_DB` is set. WAL, FTS5 porter + prefix, BM25, exact and normalized dedup, `valid_until`, soft delete, `purge`, 2000 active rows per namespace, namespace profile.
- `connect` writes the same stable command (`tinymem mcp` plus `TINYMEM_DB` and `TINYMEM_NAMESPACE`) into machine-local `.pi/mcp.json`, `.cursor/mcp.json`, `.mcp.json`, `.vscode/mcp.json`, and `.codex/config.toml` (git-ignored, not committed). Re-ran in this repo: all five carry `TINYMEM_NAMESPACE=tinymem`.
- `python3 -m unittest tests.test_server tests.test_connect`: 26 tests, passing, on this machine only (Python 3.14.7).
- Package identity: `pyproject.toml` with `tinymem` console script (`tinymem.cli:main`), stdlib-only, `requires-python >=3.14` (the only version the suite has passed). Installed editable here; `/opt/homebrew/bin/tinymem` serves.
- README states the contract and refusals. Version stays `0.2.0-dev`.
- One recorded stdio qualification against the installed command in `docs/BETA_CHECKS.md` (initialize, seven tools, add, cross-process search, update, delete). Pi `mcp list` shows `connected` with the same seven tools; Pi agent tool-call is still pending (provider blocked).
- Limits that stay: plaintext SQLite file (namespace is a label, not auth); `delete_memory` hides, `purge` hard-deletes, neither is secure erasure; offline lexical search only (no embeddings, repo index, provenance, context budget, backup command); no coding-quality claim.

What is not true:

- No Pi agent tool-call round-trip is recorded. `pi mcp list` is `connected`, but no Pi-driven add/search/update/delete exists (provider blocked at check time). Other agents are config-written only.
- Committed locally as `12dba07` (not amended onto `43c5398`, tag `0.1.0-beta.1` not pushed). No remote configured.
- The database `~/.tinymem/tinymem.db` holds project namespaces including prior notes.

## What beta means

A person can clone this repo, run one command, and have the qualified agents in that project read and write the same namespace through the same tools. Memory is explicit notes: facts, preferences, decisions, plans, plus one short profile. Retrieval is offline lexical search. Beta does not claim that memory makes coding better.

Out of beta, on purpose:

- Rust engine, native archives, Pi extension, Cloudflare gateway, Python/TypeScript SDKs.
- Embeddings, repository indexing, file provenance, automatic context injection, conversation capture.
- Team auth, encryption, secure deletion, export/import.
- Windows, Intel macOS, Linux, and any agent whose config writer has not had a live tool round-trip.
- Publishing to PyPI or GitHub Releases.

## Words

- **Namespace.** The project bucket inside the shared database. `connect` sets it from the git toplevel directory name, sanitized to `[A-Za-z0-9_:-]{1,64}`. It is not a user, not an agent, and not a permission.
- **Profile.** One static text blob per namespace (stack, conventions, commands). Not a memory. Read it before search.
- **Memory.** One durable sentence-level note with a kind. Corrections replace it (`update_memory`). They are not a second note.
- **Hide.** `delete_memory` sets `deleted_at` and drops the FTS row. The file remains.
- **Purge.** Hard-delete of hidden or long-expired rows, then `VACUUM` when SQLite allows it. Still not secure erasure.
- **Qualified.** An OS / Python / agent cell with a recorded live run: initialize, list the seven tools, add, search in a new process, update, delete. Config output alone is not qualification.

## Gates

All of these are required before the tree may say `0.2.0-beta.1`. Gates 1-6 are done (26 green). Gate 7 is partial: installed-command stdio is qualified, Pi list is connected, Pi agent tool-call is pending. Version stays `0.2.0-dev`.

1. **Default namespace on every tool.** DONE in `tinymem/protocol.py` + `tinymem/store.py`: `search_memories` uses `TINYMEM_NAMESPACE` when `namespace` is omitted. Passing `namespace` is the only cross-project read. `update_memory` and `delete_memory` refuse an id whose namespace is not the session namespace. Same-file access outside the tools is unchanged and stays documented as unprotected.
2. **One transaction for hide.** DONE in `tinymem/store.py:delete` (`with self._conn:` covering both writes) and `tinymem/store.py:_sync_fts_if_needed` (raises instead of serving stale; `tinymem/cli.py` exits 1 on `sqlite3.Error`).
3. **Version and package identity.** DONE except the final bump: `pyproject.toml` with `tinymem` console script, stdlib-only, `requires-python >=3.14` (only version the suite has passed). Keep `0.2.0-dev` until gate 7 passes; then set `0.2.0-beta.1`.
4. **Connect records a stable command.** DONE: `connect.launch` returns `tinymem mcp` plus `TINYMEM_DB` and `TINYMEM_NAMESPACE`. Re-ran here; all five machine-local configs carry `TINYMEM_NAMESPACE=tinymem`.
5. **README states the contract and the refusals.** DONE: install, connect, seven tools, write/read discipline, plaintext database, hide vs purge, namespace is not auth, lexical search is not semantic, no coding-quality claim, `0.1.0-beta.1` withdrawn and unpublished, no download sentence.
6. **Tests for the gates.** DONE: default-namespace search isolation; cross-namespace update/delete refused; console-script launch; hide plus FTS consistency; startup rebuild repairs; two processes add without loss. 26 green.
7. **One recorded qualification.** PARTIAL in `docs/BETA_CHECKS.md`: installed-command stdio round-trip passes on this Mac + Python 3.14.7; `pi mcp list` is `connected` with seven tools; Pi agent tool-call is pending (default provider 403, codex models limited at check time). Other agents stay "config written, not qualified". Do not set `0.2.0-beta.1` until a Pi-driven add/search/update/delete is recorded.

## Work order

1. DONE: namespace defaults and hide/FTS transaction, with tests.
2. DONE: `pyproject.toml` + console script; `connect` points at `tinymem mcp`.
3. DONE: README with refusals. Version stays `0.2.0-dev`.
4. DONE: re-ran connect here; `TINYMEM_NAMESPACE=tinymem` in all five configs.
5. PARTIAL: suite is 26 green; installed-command stdio qualification recorded in `docs/BETA_CHECKS.md`. Pi agent tool-call still pending (provider blocked). Do NOT set `0.2.0-beta.1` yet.
6. DONE: committed as `12dba07`. Did not amend `43c5398`. Did not push `0.1.0-beta.1`.

Published beta (PyPI or a GitHub release asset) is a later plan. It starts only after this local beta exists, and only with a URL that serves the file the docs name.

## Later, not this beta

Schema `user_version` before any on-disk change that old processes cannot read. Backup/restore. Quota and rate-limit tests. A second qualified agent. Linux and Windows. Semantic search, provenance, and context packing, if a measured coding task needs them. None of those reopen the Rust tree.
