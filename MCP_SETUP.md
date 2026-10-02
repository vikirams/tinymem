# Coding-agent MCP setup

TinyMem's native Rust core exposes eleven stdio MCP tools for scoped memory, repository evidence and
budgeted task context. No Python runtime, model, account or network listener is required by default.
See [Pi local setup](docs/PI_LOCAL_SETUP.md) for the tested project-local Pi installation path and the
optional Pi extension. Setup does not install or enable extensions.

## Build and configure

Requires Rust/Cargo 1.90+ and a C compiler:

```sh
cargo build --release --locked -p tinymem-engine
```

For Pi, build the release binary and use the standard-library setup helper from this checkout:

```sh
python3 scripts/setup-pi.py --root /absolute/path/to/your/project --dry-run
python3 scripts/setup-pi.py --root /absolute/path/to/your/project
```

It writes only that project's `.pi/mcp.json`, merging other servers and refusing conflicting TinyMem
entries without `--force`. It chooses absolute paths and a private per-project database location; it does
not open or modify a database. See [Pi local setup](docs/PI_LOCAL_SETUP.md) for project trust, scope,
exposure, existing `tinymem-test`, and optional extension details.

For another client accepting `mcpServers`, adapt this template with absolute paths:

```json
{
  "mcpServers": {
    "tinymem-native": {
      "command": "/absolute/path/to/tinymem/target/release/tinymem",
      "args": [
        "--db", "/absolute/path/to/native-memory.db",
        "mcp", "/absolute/path/to/repository",
        "--user", "alice",
        "--project", "my-repo"
      ]
    }
  }
}
```

**This is a generic configuration template, not a verified installation recipe for every client.**
Use the client's documented configuration location/schema. Claude Desktop, Claude Code and Cursor
are different clients; their installation mechanisms must not be conflated.

Run the same command manually to inspect startup errors; the foreground process then waits for MCP
input until stdin closes. Logs/errors go to stderr and protocol responses to stdout. The default database
without `--db` is `<repository>/.tinymem/repository.db`. An explicit private database path makes cross-agent
sharing intentional and avoids accidentally committing memory artifacts.

## Support and day-one release status

| Surface | Current evidence | Remaining release work |
| --- | --- | --- |
| Native stdio MCP | Real-process protocol and lifecycle tests in the repo; [beta final-artifact acceptance](docs/PI_BETA_CHECKS.md) for the qualified cells only | Clean-machine installation on unqualified platforms |
| Python/TypeScript SDKs | Real-binary integration tests; no runtime dependencies | Published packages and release installation checks |
| Pi 0.99.2 | Model-free discovery of all eleven tools recorded; deterministic gateway protocol tests | Natural coding-task evaluation and lifecycle extension |
| Pi 1.0.0 | [Explicit live native-MCP lifecycle](docs/PI_NATIVE_MCP_CHECKS.md) and [offline Pi SDK/extension lifecycle tests](docs/PI_LOCAL_CHECKS.md), including nested write gating; [beta clean-room acceptance](docs/PI_BETA_CHECKS.md) covers clean-machine installation for the qualified cells only | Natural coding-task evaluation |
| Claude Code | Native MCP template only; no verified plugin shipped | Verify pinned client's package mechanism; build/install/test plugin and explicit memory workflow |
| Cursor | Native MCP template only; no verified integration package shipped | Verify pinned client's supported mechanism; build/install/test package and memory workflow |
| Other MCP coding agents | Protocol compatibility candidate, not tested support | Version-specific adapter/configuration and full lifecycle smoke |

Claude Code and Cursor are **day-one public coding-release gates**, not optional post-launch polish.
Do not mark either supported until its clean-client installation and behavior have been tested. Version
pins, supported operating systems, exact commands and test logs must accompany the released adapters.

## Agent workflow

1. Start with a fixed database, repository root and exact owner/project scope. Omit agent ID only when
   deliberately sharing that namespace across assistants. Tool arguments cannot override server scope.
2. Pass task text to `assemble_context` with a `token_budget`. It reduces long tasks to bounded lexical
   retrieval terms. Insert only the returned `prompt`, not the duplicate `entries`/response envelope;
   reserve tokens for the rest of the model request.
3. Treat memory and source text as untrusted evidence. Inspect citations and prefer current repository
   behavior over an unsupported note. Today's source labels do not automatically invalidate saved notes.
4. Save explicit durable decisions/corrections/preferences with `add_memory`; do not ingest entire
   conversations, credentials or transient logs. Reads never become implicit memory writes.
5. Inspect records/history; correct using optimistic `update_memory`; hide current evidence using
   `delete_memory`. Soft deletion is not erasure of audit or backups.

See [tool arguments and errors](docs/NATIVE_MEMORY.md) and [context-budget semantics](docs/CONTEXT_ASSEMBLY.md).
The current MCP counter budgets the complete rendered prompt with a conservative UTF-8-byte upper
bound, not a target-model tokenizer. `assemble_context` accepts task text up to 64 KiB and derives at
most 32 frequency-ranked unique normalized retrieval terms; standalone search limits are unchanged.
See [task context assembly](docs/CONTEXT_ASSEMBLY.md) for selection and budget details.

## Required plugin/integration acceptance test

Run on a clean supported client/version and publish the steps/results:

- Install/configure without overwriting unrelated client configuration or requiring a cloud account.
- Connect and discover the native tools; failures expose useful diagnostics without secrets.
- Remember an approved decision, restart into a new session, and retrieve it within the requested budget.
- Confirm project isolation, explicit cross-agent sharing and no accidental cross-project reads.
- Correct/delete a note; ensure old text is not returned as current and history is explicitly historical.
- Save an explicit file dependency, change/delete it and verify stale notes are excluded with a history freshness reason.
- Disable automatic context injection; uninstall without silently deleting the user's database.

The file-source lifecycle is tested through native MCP and both SDKs; assistant-specific plugin
verification remains pending. A passing generic MCP connection alone is not enough to claim that
these integration requirements are complete.

## Troubleshooting and safety

- Use absolute executable/repository/database paths; GUI clients may have a different working directory
  and PATH than your shell. Ensure the binary exists and is executable.
- Check stderr for startup errors. Native configuration uses CLI flags, not legacy `TINYMEM_*` settings.
- Check the exact user/project/agent tuple when a memory appears missing. Missing dimensions mean an
  exact global namespace, not all scopes or inherited permissions.
- Keep the database private. The core is unencrypted and does not authenticate someone who can open it.
- Review ignore rules and source visibility; repository indexing is not a secret scanner or sandbox.
- Opt-in semantic retrieval may invoke a trusted external embedding executable and transfer data;
  configure it deliberately. Lexical operation has no such requirement.

The root Python MCP server and `tinymem[mcp]` package are frozen legacy implementations with different
tools/storage. They are not the installation path for native coding-agent memory.
