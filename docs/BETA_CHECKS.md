# TinyMem beta checks

Dates: 2026-10-02 (development checks), 2026-10-03 (release qualification).
Machine: Darwin 25.5.0 arm64 (MacBook Pro). Python: 3.14.7.
Release: `0.2.0-beta.1`, non-editable wheel in a fresh temporary venv.
The older checks below used `0.2.0-dev`; they are retained as history.

## Suite

`python3 -m unittest discover -s tests -v`: 36 tests, passing on this
machine (Python 3.14.7). Includes Context and generated-hook regression tests.

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

## Hook isolation and non-editable package recheck (PASS)

Rechecked 2026-10-02 against the working-tree hook fix on top of `897e5b6`:

- Fresh temporary venv, `python -m pip install <repo>` (not editable).
  Wheel: `tinymem-0.2.0.dev0-py3-none-any.whl`; imported module confirmed
  under the venv's `site-packages`, not the source tree.
- `tinymem connect --agent claude --agent cursor --hooks --discipline`
  against a temporary project and database. Both generated hook commands
  executed with no host `TINYMEM_DB` or `TINYMEM_NAMESPACE`: both returned
  that project's Profile and Index. Database path contained spaces,
  apostrophes, `$`, and `;` to check shell quoting.
- Packaged MCP: initialize → `0.2.0-dev`; add →
  `a39fe89ad4534851bb3e685b4eafdf2d` in `release-project`. A second process
  searched `blue green`, updated the same id to `red black`, found it,
  hid it, and returned no results for `red black`.
- Regression tests also cover conflicting host database/namespace values,
  upgrading legacy commands, repeated connect, changed database bindings,
  and preservation of unrelated hooks.

These are CLI/subprocess checks, not live Claude Code or Cursor sessions.
They do not complete the live-agent qualification gate.

## Pre-bump live Pi qualification (PASS)

Rechecked 2026-10-03 using Pi 1.0.0, provider `openai-codex`, model
`gpt-6.1-sol`, Python 3.14.7, and a fresh non-editable `0.2.0-dev`
install in a temporary venv. This completes Gate 7 before the version bump.
Earlier provider-blocked attempts on 2026-10-02 did not qualify the agent.

`tinymem connect --agent pi` generated `.pi/mcp.json` in a temporary
`release-project`. Both independent Pi processes used `tinymem mcp` on the
venv PATH, one temporary database, and `TINYMEM_NAMESPACE=release-project`.
No production database was used. The packaged stdio check initialized the
server and listed all seven tools before the live agent run.

Each Pi invocation used this command, with the phase prompt appended:

```sh
pi --approve --mode json --no-session --offline \
  --no-extensions --extension builtin:mcp \
  --no-skills --no-prompt-templates --no-themes --no-context-files \
  --tools mcp__tinymem__add_memory,mcp__tinymem__search_memories,mcp__tinymem__update_memory,mcp__tinymem__get_profile,mcp__tinymem__set_profile,mcp__tinymem__memory_stats,mcp__tinymem__delete_memory \
  --provider openai-codex --model gpt-6.1-sol --thinking minimal \
  --system-prompt "You perform authorized release qualification using ONLY the available TinyMem MCP tools against a disposable database. Synthetic test notes are permitted. Use actual tool calls, never simulate results. Follow the requested order and stop on any error."
```

JSON `tool_execution_start`/`tool_execution_end` records were checked, not
just the model's final PASS text. All calls returned `isError: false`.

Process 1, session `01a1001d-95fe-70f5-a5df-7d6ad41c377d`:

- `get_profile {}` → namespace `release-project`.
- `add_memory {content: "Live qualification deploys blue green releases",
  kind: "decision"}` → id `91b3f43ad90b484e9df21120656f67a7`,
  namespace `release-project`, `created: true`.
- `search_memories {query: "blue green"}` → that same id.

Process 2, session `01a1001d-d3b9-77bb-a3f3-f937350a9244`:

- `search_memories {query: "blue green"}` → the first process's id.
- `update_memory {id: "91b3f43ad90b484e9df21120656f67a7",
  content: "Live qualification deploys red black releases"}` → `updated: true`.
- `search_memories {query: "red black"}` → that same id and new content.
- `delete_memory {id: "91b3f43ad90b484e9df21120656f67a7"}` → `deleted: true`.
- `search_memories {query: "red black"}` → `memories: []`.
- `memory_stats {}` → `active: 0`, `deleted: 2` (one packaged-smoke note
  plus one live-agent note, both hidden).

## Release-built package and live Pi recheck (PASS)

On 2026-10-03, after the dev-version Gate 7 check above:

```sh
python3 -m unittest discover -s tests -v
python3 -m compileall -q tinymem
python -m build --outdir /tmp/tinymem-release-XXXXXX/dist .
python3 -m venv /tmp/tinymem-release-XXXXXX/beta-venv
/tmp/tinymem-release-XXXXXX/beta-venv/bin/python -m pip install \
  /tmp/tinymem-release-XXXXXX/dist/tinymem-0.2.0b1-py3-none-any.whl
```

The builder was a separate temporary venv with `build` installed. Default
`python -m build` built an sdist, then built the wheel from that sdist.
Artifacts: `tinymem-0.2.0b1.tar.gz`, `tinymem-0.2.0b1-py3-none-any.whl`.
They were checked locally, not uploaded. The wheel contains all seven
Python modules plus `tinymem/data/discipline.md`, entry point, and license.
No database or credentials were included.

With cwd outside the repository and the beta venv first on PATH:

- `import tinymem` resolved inside the venv's `site-packages`.
- Runtime `__version__` and MCP `serverInfo.version`: `0.2.0-beta.1`.
- The 36-test suite passed again using the installed package. The source
  launch and generated-hook subprocess fixtures still exercise source
  paths; separate packaged checks below cover the actual wheel entry point.
- Packaged MCP initialized and listed seven tools. Added id
  `c03a62ae60954f188887aa6f8f828fb2`; a second process searched it,
  updated it, searched the new content, hid it, and returned no results.
- Packaged `connect --agent pi --agent claude --agent cursor --hooks
  --discipline` succeeded. Generated Claude/Cursor hook commands returned
  the correct Profile with no inherited database/namespace values.
- `pi mcp list --json` against the generated **project** config:
  `state: connected`, all seven tools, `errors: []`. Only this disposable
  project was trusted in an isolated Pi agent directory.

Two new Pi invocations used the same command and phase prompts as the
pre-bump qualification above, with the beta venv on PATH. The system prompt
and tool allowlist excluded filesystem/shell tools. Real JSON tool events
were checked; every call returned `isError: false`.

Process 1, session `01a10021-2236-73d2-ac88-26d8240d0ec3`:

- `get_profile {}` → the expected `release-project` Profile.
- `add_memory {content: "Live qualification deploys blue green releases",
  kind: "decision"}` → `e3c939cd928347f5bba14ff9776f20cd`,
  namespace `release-project`, `created: true`.
- `search_memories {query: "blue green"}` → that same id.

Process 2, session `01a10021-62c7-759a-a2c1-4a5ea14fea64`:

- `search_memories {query: "blue green"}` → the first process's id.
- `update_memory {id: "e3c939cd928347f5bba14ff9776f20cd",
  content: "Live qualification deploys red black releases"}` → `updated: true`.
- `search_memories {query: "red black"}` → that same id and new content.
- `delete_memory {id: "e3c939cd928347f5bba14ff9776f20cd"}` → `deleted: true`.
- `search_memories {query: "red black"}` → `memories: []`.
- `memory_stats {}` → `active: 0`, `deleted: 2` (packaged-smoke and live
  notes), `expired: 0`.

Neither run accessed the user's database. Temporary copies of Pi credentials
were removed after each live run. Raw JSONL transcripts stay outside the repo;
the synthetic tool results above are the qualification record.

## Other agents

Cursor, Claude Code, VS Code, Codex: config written by
`tinymem connect` (`command: tinymem, args: [mcp]`,
`TINYMEM_NAMESPACE: tinymem`), verified on disk. No live tool
round-trip recorded. Status: config written, not qualified. Not listed
as supported.
