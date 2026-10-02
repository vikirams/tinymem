# Experimental Pi native MCP lifecycle check

**Result: PASS — explicit live-agent lifecycle, not natural coding-task effectiveness.**

Verified locally on 2026-10-02 (UTC), macOS arm64, Pi **1.0.0**, native TinyMem
**0.1.0** (debug build, schema v4). Pi selected `openai-codex/gpt-6.1-sol` with
`low` thinking. This records the selected provider/model, not an independently
verified upstream routing identity.

Native binary SHA-256:
`24053f18db7b72e91d80fb84b79f2fe292619925682745680b6a1ba08270a9d7`.

## Configuration and isolation

From the repository:

```sh
cargo build --locked -p tinymem-engine
pi mcp add -l tinymem-test --exposure direct -- \
  "$PWD/target/debug/tinymem" \
  --db "$HOME/.local/share/tinymem/pi-test.db" \
  mcp "$PWD" --user alice --project tinymem-test
pi
```

Approve project configuration when prompted, then inspect `/mcp`. The local
`.pi/mcp.json` contains machine-specific absolute paths; it is not a portable
installation manifest. The test database is separate from the default repository
database. Scope is exactly user `alice`, project `tinymem-test`, with no agent ID.
The private data directory and database were restricted to modes `0700` and `0600`.

Automated sessions used the following options, with one prompt per new process:

```sh
PI_OFFLINE=1 PI_SKIP_VERSION_CHECK=1 PI_TELEMETRY=0 \
pi --approve --no-session \
  --no-extensions --extension builtin:mcp \
  --no-skills --no-prompt-templates --no-context-files --no-builtin-tools \
  --provider openai-codex --model gpt-6.1-sol --thinking low \
  --mode json '<explicit lifecycle prompt>'
```

This loads only Pi's built-in MCP extension, not a third-party adapter or memory
lifecycle extension. Default file/shell tools were disabled. `--approve` grants
trust for each process; the experiment did not change saved global trust settings.
`PI_OFFLINE` disables automatic Pi network activity, not the explicitly requested
model calls. TinyMem itself used local lexical retrieval with no embedding provider.

A separate RPC-mode process sent `/mcp` through the `prompt` command and recorded:

```text
tinymem-test: connected, 11 tools (direct)
```

Its response disposition was `handled`, with no `agent_start` event. Print mode
suppressed this notification, so RPC notification capture was used for evidence.
In Pi 1.0.0, `pi mcp list --approve` is rejected; use process-level `--approve`
for a session, or grant saved project trust before shell-level discovery.

## Live test and assertions

The dependency was a disposable `.pi/pitest319950484941/dependency.txt` file, not
an existing source file. Its original bytes were `clock storage: UTC\n`.
Explicit references may capture hidden files even though repository indexing
excludes them. The fixture directory was removed after testing.

The first live session explicitly called `mcp__tinymem_test__add_memory` with:

```json
{
  "content": "pitest319950484941: Persist clock timestamps in UTC, never local time.",
  "kind": "decision",
  "source": "approved Pi native MCP lifecycle fixture",
  "source_refs": [
    {"kind": "file", "role": "dependency", "path": ".pi/pitest319950484941/dependency.txt"}
  ]
}
```

Saved memory ID: `eae938516d5c71110cae05bb4fdbe5a2`, version **1**.
Captured dependency hash:
`blake3:7faa7b393262e6b43fb6d00b4de857d5c87a928d2174d2b3cb39646dd5df28ab`.
The returned exact scope, canonical repository, relative path and explicit role
were asserted against the configuration.

The next session received **only the lookup marker**, not the saved content or ID.
It called `search_memories`, used the returned ID for `get_memory`, and also called
`get_profile` and `assemble_context` with an 8,000-token budget. Its final answer
correctly recalled the UTC decision from returned evidence.

Each subsequent session explicitly called those four current-read tools plus
`memory_history`. The external test driver changed only the disposable file
between sessions; it made no direct memory writes.

| Step | File state | Four current-read surfaces | History freshness | Result |
| --- | --- | --- | --- | --- |
| Save | Original bytes | Structured dependency captured | Not requested | PASS |
| Fresh recall | Original bytes | All return/include the saved memory | Not requested | PASS |
| Edit | `clock storage: local time\n` | All exclude it | `stale` / `content_changed` | PASS |
| Restore | Exact original bytes | All include it again | `current` / `dependencies_current`; check `hash_matches` | PASS |
| Delete | Missing file | All exclude it | `stale` / `file_missing`; observed hash null | PASS |

The successful run used **five distinct ephemeral session IDs** and **20 direct
native MCP tool calls**. Every requested call executed exactly once, without a
tool error or a failed/aborted model response. Checks inspected JSON tool results,
not just the assistant's claims. At every history inspection there was exactly
one `added` event with an unchanged memory snapshot, version and captured hash.
Freshness evaluation did not create audit events or rewrite the memory.

Unchanged/restored context contained one provenance-bearing memory entry and used
**772** of the 8,000 budget units (`utf8_bytes_upper_bound`). Edited/deleted context
had no entries, an empty prompt and zero units. These units are a conservative
byte bound, not provider billing tokens.

## Evidence and regression checks

Private artifacts, deliberately outside the repository:

```text
~/.local/share/tinymem/pi-test-runs/20261002T024754Z-pitest319950484941/
```

This directory retains `summary.json`, exact prompts, JSONL session/tool events,
extracted tool results, stderr, command options, and `cargo-test.log`. The private
experimental driver is `~/.local/share/tinymem/pi-test-runs/lifecycle.py`; it is not
a shipped integration or benchmark runner and uses this machine's checkout path.

Also passed:

```sh
cargo build --locked -p tinymem-engine
cargo test --workspace --locked        # 97 passed; 0 failed; 0 ignored
python3 scripts/smoke-pi-mcp.py         # 11 tools; model-free discovery
```

An earlier run successfully saved a fixture memory but stopped because the
experimental verifier incorrectly expected top-level scope fields (`KeyError:
user_id`). The verifier was corrected to inspect `memory.scope`; the full rerun
above passed. This was a harness error, not an engine failure. That earlier run's
artifacts remain under `20261002T024716Z-pitest97901558e377`, and its synthetic
memory remains historical/stale because its fixture was removed. Neither run's
database records were silently erased.

## Limits

This verifies explicit Pi tool use across fresh processes and file-dependent
freshness. It does **not** establish automatic capture/startup injection, natural
coding-task usefulness, cross-project/cross-agent acceptance, Pi-side
update/delete workflows, installation on a clean machine, uninstall behavior, or
latency/relevance benchmarks. Those remain separate integration/release gates.
The final saved memory is intentionally still present in history but excluded
from current retrieval because its dependency was deleted.
