# TinyMem beta plan

Updated 2026-10-03. This file replaces `docs/PI_BETA_RELEASE.md`,
`docs/PI_BETA_CHECKS.md`, and `docs/MEMORY_SYSTEM_PLAN.md`. Those described
a Rust, Pi-only product. That product is withdrawn.

## Release identity

The first beta of the Python product is **`0.2.0-beta.1`**. It is a
source-only release at `https://github.com/vikirams/tinymem`, installed
from that exact tag. Build tools normalize the wheel/sdist version to
`0.2.0b1`; the runtime and source version are `0.2.0-beta.1`.

`0.1.0-beta.1` (local tag on `43c5398`) is a different product. It was
never published. Do not push that tag, attach its archives to a release,
or reuse it. Do not amend `43c5398` or move a published tag.

## Decision and contract

Beta is one local Python stdio MCP server. Pi, Cursor, Claude Code,
VS Code, and Codex configs launch the same process and share one SQLite
file. There is no per-agent memory implementation, Rust engine, or
hosted service. Config generation is not live-agent qualification.

A person can clone the pinned source tag, install it in a Python 3.14+
venv, and run `tinymem connect` in a project. The venv's `bin` directory
must be on the agent's PATH. Memory is explicit sentence-level notes:
facts, preferences, decisions, plans, plus one short static Profile.
Retrieval is offline lexical search. Beta does not claim that memory
makes coding better.

Qualified live-agent cell: **Darwin 25.5.0 arm64 / Python 3.14.7 /
Pi 1.0.0** (`openai-codex`, `gpt-6.1-sol`). Two independent live Pi
processes completed add/search/update/delete against a non-editable
wheel install, with the same temporary database and project namespace.
Evidence is in `docs/BETA_CHECKS.md`.

Cursor, Claude Code, VS Code, and Codex remain **config written, not
live-qualified**. Automated macOS/Linux package tests in CI do not
qualify a live agent on those platforms.

## Words

- **Namespace.** Project bucket inside the shared database. `connect`
  derives it from the git toplevel directory name, sanitized to
  `[A-Za-z0-9_:-]{1,64}`. A label, not a permission or agent identity.
- **Profile.** One static text blob per namespace: stack, conventions,
  commands, short code map. Not a Memory. Read it before search.
- **Memory.** Durable sentence-level note with a kind. Corrections use
  `update_memory`, not another note.
- **Hide.** `delete_memory` sets `deleted_at` and removes the FTS row
  in one transaction. The database row remains.
- **Purge.** Hard-delete hidden or long-expired rows, then `VACUUM` when
  SQLite allows it. Still not secure erasure.
- **Qualified.** Recorded OS/Python/agent cell with initialize, seven
  tools listed, add, search in a new process, update, delete, and an
  empty search afterward. Config output alone is not qualification.

## Gates — all seven passed

1. **Default namespace on every tool.** Search defaults to
   `TINYMEM_NAMESPACE`; explicit namespace enables cross-project reads.
   Update/delete refuse ids outside the session namespace.
2. **One transaction for hide.** Hide plus FTS removal is atomic;
   startup repairs stale FTS, or fails rather than serving stale data.
3. **Version/package identity.** Stdlib-only `pyproject.toml`, Python
   `>=3.14`, `tinymem.cli:main` console script. Version bumped only after
   the recorded dev-version live qualification, then the beta wheel
   was qualified again.
4. **Stable connect command.** `tinymem mcp` plus pinned `TINYMEM_DB`
   and `TINYMEM_NAMESPACE`. Optional hooks pin the same values and
   replace legacy/stale TinyMem hooks without removing unrelated hooks.
5. **README contract and refusals.** Pinned install, seven tools,
   discipline, plaintext file, namespace is not auth, hide vs purge,
   lexical search is not semantic, no coding-quality claim.
6. **Regression suite.** 36 tests pass locally, both from the source
   tree and with an installed wheel imported outside the checkout.
   Packaged MCP and generated hooks also pass separate smoke checks.
7. **Live qualification.** Pi-driven add/search/update/delete recorded
   on 2026-10-03, first before the bump and again on `0.2.0-beta.1`.

## Cut the source beta

1. Review the pending changes; confirm no local configs, databases,
   credentials, or qualification JSONL logs are staged.
2. Build sdist and wheel; install the wheel non-editably in a fresh
   venv. Confirm imports resolve to `site-packages`; run the suite,
   packaged MCP round-trip, and generated-hook checks outside checkout.
3. Commit and push `main`. Wait for both GitHub CI jobs on that exact
   commit to pass. CI builds distributions, installs the wheel, and
   tests the installed package outside checkout.
4. Tag only that passing commit and push only the new tag:
   ```sh
   git tag -a 0.2.0-beta.1 -m "TinyMem Python MCP beta"
   git push origin 0.2.0-beta.1
   ```
   Never use `git push --tags`: the withdrawn tag remains local.
5. Verify the remote tag and a clean pinned clone/install. Give testers
   the README installation instructions and GitHub issue-report link.

## Limits and exclusions

Plaintext SQLite (`~/.tinymem/tinymem.db`, overridden by `TINYMEM_DB`);
anyone with file access can read all namespaces. Neither Hide nor Purge
is secure erasure. Do not store secrets.

Out of beta: Rust engine, native archives, Pi extension, Cloudflare
gateway, Python/TypeScript SDKs, embeddings, repository indexing,
provenance, context budgets, conversation/tool-output capture, team auth,
encryption, export/import, backup command, Windows/Intel macOS and other
unqualified agent/platform cells. Optional SessionStart hooks inject
Profile plus a five-entry recency Index, not full Memory bodies.

Publishing to PyPI or GitHub Releases (including uploaded release assets)
is a separate later plan, not part of this source beta. Any future
published download must have a URL that serves the documented artifact.

## Later

Schema `user_version` before incompatible on-disk changes. Backup/restore.
Quota/rate-limit tests. A second qualified agent. Linux/Windows live-agent
qualification. Semantic search, provenance, or context packing only if a
measured coding task needs them. None of those reopen the Rust tree.
