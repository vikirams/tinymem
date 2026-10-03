# Contributing

## Scope

TinyMem is one Python stdio MCP server shared by every coding agent.
Out of scope for beta: Rust engine, embeddings, repo indexing,
team auth, encryption, export/import, Windows/Intel macOS/Linux
qualification beyond `docs/BETA_CHECKS.md`, PyPI/GitHub Releases.

See `docs/BETA.md` for release gates. Gate 7 (live Pi round-trip) is
recorded in `docs/BETA_CHECKS.md` for `0.2.0-beta.1`. New platform/agent
support requires a separate live qualification; config generation alone
is not qualification.

## Setup

Requires Python 3.14+. Stdlib only.

```sh
git clone https://github.com/vikirams/tinymem
cd tinymem
python3 -m unittest discover -s tests -v
python3 -m pip install -e .  # or non-editable: pip install .
```

## Checks before PR

```sh
python3 -m unittest discover -s tests -v  # must stay 36 green
python3 -m compileall tinymem
```

No ruff/mypy in CI yet; keep imports sorted, line length ~100,
`from __future__ import annotations` in new modules.

## Conventions

- Language in `CONTEXT.md` is normative: Store, Namespace, Profile,
  Memory, Context, Index, Hide, Purge, Tool, Hook, Adapter.
- Tools are the only write path. Hooks must not persist.
- `add_memory` only for durable 1-2 sentence notes. Corrections use
  `update_memory`, never re-add.
- `connect` must preserve unrelated user config; refuse on unreadable
  JSON, non-object shapes, or symlinks (`tinymem/connect.py`).
- Never amend `43c5398`, never push tag `0.1.0-beta.1`.

## PRs

Small, one-topic PRs. Include tests for store/protocol/connect changes.
Update `docs/BETA.md` + `docs/BETA_CHECKS.md` when behavior or
qualification state changes. Machine-local configs (`.mcp.json`,
`.pi/`, `.cursor/`, `.vscode/`, `.codex/`, `*.db*`) must not be committed.
