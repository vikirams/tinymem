# Security Policy

## Supported versions

Current source beta: `0.2.0-beta.1`. `0.1.0-beta.1` was a different
product (Rust, Pi-only), never published. Do not use it.

No stable release yet. Security fixes land on `main`; beta updates receive
a new tag rather than changing an existing release tag.

## Threat model

TinyMem is a local single-user store:

- Database is plaintext SQLite, default `~/.tinymem/tinymem.db`
  (`TINYMEM_DB` overrides). Anyone with file access can read every
  namespace.
- `namespace` is a label, not auth and not encryption.
- `delete_memory` hides a row from search (`deleted_at` + FTS removal
  in one transaction). The row stays in the file.
- `purge` hard-deletes hidden/expired rows and runs `VACUUM` when
  allowed. Neither is secure erasure: free pages, WAL, and file copies
  may retain content.
- Search is offline lexical (FTS5 + BM25). No network, no embeddings,
  no auth, no sandbox.

Do not store secrets, credentials, tokens, or customer data in TinyMem.

## Reporting a vulnerability

Open a GitHub issue with `[security]` in the title, or contact the
maintainer via the repository. Do not include exploit payloads containing
real secrets.

Include: version (`tinymem --help` or `__version__`), OS/Python,
`TINYMEM_DB` path pattern (not the file), and reproduction steps with a
temporary database.

## Handling of configs

`tinymem connect` writes machine-local MCP configs (`.pi/mcp.json`,
`.cursor/mcp.json`, `.mcp.json`, `.vscode/mcp.json`, `.codex/config.toml`).
These are git-ignored and must not be committed. They contain local
absolute paths.
