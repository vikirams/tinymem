# TinyMem native quick start

TinyMem is an experimental open-source memory layer for coding agents. These instructions use the
Rust core, **not** the legacy Python `tinymem` package. No model, API key or cloud account is required.

## 1. Build

Requires Rust/Cargo 1.90+ and a C compiler. From this checkout:

```sh
cargo build --release --locked -p tinymem-engine
./target/release/tinymem --help
```

Prebuilt release binaries and verified one-command agent installers are pending. Do not use
`pip install tinymem` to install the native engine.

## 2. Connect your coding agent

**For Pi**, start with the [local setup and `/memory` guide](docs/PI_LOCAL_SETUP.md).
It provides a safe setup helper and an optional budgeted context extension. If using that extension,
`/memory remember` is the intentional write path; model-issued writes are blocked by default.
See [what is tested and still pending](docs/PI_LOCAL_CHECKS.md).

For other MCP clients, use absolute paths in your stdio configuration:

```json
{
  "mcpServers": {
    "tinymem-native": {
      "command": "/absolute/path/to/tinymem/target/release/tinymem",
      "args": [
        "--db", "/absolute/path/to/native-memory.db",
        "mcp", "/absolute/path/to/repository",
        "--user", "alice", "--project", "my-repo"
      ]
    }
  }
}
```

Client-specific file locations and support status are in [MCP_SETUP.md](MCP_SETUP.md). The server
communicates through stdin/stdout; it does not open a port. Closing stdin stops it.

Use the same database, user and project to share memory intentionally across your coding agents.
Omit `--agent` for that shared namespace; set it for exact per-agent separation. A repository root does
not automatically set the personal-memory project scope. These scopes are not authentication: protect
the database with filesystem permissions and do not expose it to untrusted users.

## 3. Remember, then recall in a fresh session

Ask the agent to call `add_memory` with an explicit durable decision:

```json
{
  "content": "Use the existing transaction wrapper for account updates so audit writes commit atomically.",
  "kind": "architectural_decision",
  "source": "Explicit developer decision in the account-update discussion"
}
```

Open a new session using the same database/scope and call `assemble_context`:

```json
{"query":"Implement account updates with atomic audit writes","token_budget":2000}
```

Inject only its returned `prompt` as untrusted evidence. It contains bounded selected content and
provenance. The default count is a conservative UTF-8-byte bound, not actual model tokens.

Use `update_memory` with `expected_version` for an explicit correction and `delete_memory` to hide a
record from current reads. Audit history remains. The `source` above is **free text** and does not
track a file. To make a note depend on unchanged file bytes, add
`"source_refs": [{"kind":"file","role":"dependency","path":"src/account.rs"}]` when saving it.
The path is relative to the configured repository and must exist. Changed/missing/unverifiable
files then exclude the note from current reads; history retains it with an explicit freshness reason.
Use `role: "origin"` for a citation that should not control validity. See [file provenance](docs/FILE_PROVENANCE.md).

## 4. Optional: inspect repository evidence without an agent

```sh
./target/release/tinymem --db /absolute/path/to/native-memory.db index /absolute/path/to/repository
./target/release/tinymem --db /absolute/path/to/native-memory.db search /absolute/path/to/repository "transaction"
```

Repository search refreshes source evidence before returning path/line/hash citations. It is distinct
from saved decisions and does not currently infer their validity.

## Next steps

- [Native tools, limits, deletion and security](docs/NATIVE_MEMORY.md)
- [Context selection and token budgets](docs/CONTEXT_ASSEMBLY.md)
- [Python SDK](sdks/python/README.md) · [TypeScript SDK](sdks/typescript/README.md)
- [Contribute](CONTRIBUTING.md) · [Roadmap](docs/MEMORY_SYSTEM_PLAN.md)
- [Coding-memory benchmark protocol](docs/CODING_MEMORY_BENCHMARK.md)
