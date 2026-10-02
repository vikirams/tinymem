# Pi: local TinyMem setup (source-checkout path)

> **For the `0.1.0-beta.1` Pi-only release artifacts, use the [beta release guide](PI_BETA_RELEASE.md)** for pinned platform/Node qualification, offline package installation, project ownership, backup/restore, upgrade and uninstall. This page describes the source-checkout/developer workflow and still requires a Rust build; it is not the packaged beta installation path.

This installs TinyMem's **native stdio MCP server** for one Pi project from a source checkout. It uses no Python packages, model calls, hosted service or credentials for setup. Pi's own model provider remains whatever you configure for Pi.

The MCP server exposes eleven native memory/context tools. An optional source-checkout Pi extension adds budgeted task-context recall and `/memory` commands; setup does **not** install or enable that extension. These are separate integration surfaces: MCP tools remain available to the model, while the extension uses the native SDK in short-lived local subprocesses for request-local context and explicit commands.

## 1. Build and add the project server

From a TinyMem checkout, build the native release executable (Rust/Cargo 1.90+ and a C compiler are required):

```sh
cargo build --release --locked -p tinymem-engine
```

Choose the project directory where you will run Pi. Preview first, then write its project-local `.pi/mcp.json`:

```sh
python3 scripts/setup-pi.py --root /absolute/path/to/your/project --dry-run
python3 scripts/setup-pi.py --root /absolute/path/to/your/project
```

The script is standard-library-only. By default it points to this checkout's `target/release/tinymem`; `--binary` overrides it but must resolve to an executable named `tinymem` (or `tinymem.exe`) so the optional extension can validate it. It resolves the executable, repository root and database to absolute paths at setup time, so the generated config uses this machine's actual install paths rather than a checked-in machine-specific template. Re-run setup after moving the checkout/project or changing the binary/database location.

Useful options:

```sh
python3 scripts/setup-pi.py --root /path/to/project \
  --user alice --project my-repo --agent pi \
  --db "$HOME/.local/share/tinymem/my-repo.db" \
  --exposure direct
```

- `--user` defaults to the current OS username. `--project` defaults to the directory name when it is already a valid scope ID; otherwise setup derives a deterministic ASCII slug plus a short hash of the canonical project path. Both are exact scope values, not filters or wildcards.
- `--user`, `--project` and `--agent` must match `[A-Za-z0-9][A-Za-z0-9_.:@-]{0,127}` (1–128 ASCII characters), matching the native TypeScript SDK contract. The native Rust server itself accepts broader strings, but the optional Pi extension does not. If your OS username is not valid, setup stops and tells you to provide a valid `--user` explicitly.
- `--agent` is optional. Omitting it intentionally shares the exact user/project scope across agent IDs. Set it when you want a separate exact agent scope.
- If an invalid directory name is sanitized, its default project ID hashes the canonical absolute path; moving/renaming that directory produces a different default ID and a distinct scope. Pass an explicit valid `--project` when you need a stable ID across moves. The default database directory also hashes the canonical root, so setup after a move selects a new, initially empty database; pass `--db` with the existing database path if you intend to keep using it. Setup never copies or migrates old database files.
- `--db` is optional. The default is a per-project database under the OS user data directory (`~/Library/Application Support/tinymem` on macOS, `%LOCALAPPDATA%/TinyMem` on Windows, or `$XDG_DATA_HOME/tinymem` / `~/.local/share/tinymem` on Linux). The helper creates only its missing parent directory (mode `0700` on POSIX; on Windows it relies on the per-user app-data directory's inherited ACL). It does not create, open, migrate, truncate, or delete the database. The native server creates it on first connection. Existing directories are not chmodded and existing database contents are untouched.
- `--server-name` (alias `--tinymem-server`) defaults to `tinymem`.
- `--exposure` defaults to `deferred`, which keeps the eleven MCP tool schemas out of the initial model tool list; Pi discovers them through `tool_search` when needed. Choose `direct` if you want the tools declared to the model immediately (at the cost of more prompt/schema tokens), or `codemode` for indirect calls through Pi's codemode. Pi's built-in MCP support enables the corresponding tool mechanism when needed.
- `--force` replaces only the selected server entry when it already exists with different settings. If that entry has credential-bearing `env`, `headers` or `oauth` settings, the helper refuses to remove or transfer them; reconcile those manually. Credentials in unrelated entries and Pi's stored auth are not changed. No database is deleted or modified. Review the entry and keep its old `--db` path if you intend to continue using that database.

The helper safely merges the JSON object, preserving unrelated servers and top-level settings. A conflicting `tinymem` entry is not overwritten unless `--force` is supplied; an identical generated entry is a no-op. It also refuses to add a second native TinyMem server under a different name, preventing duplicate MCP servers/tools by accident. Pi treats names differing only by `-` and `_` as the same server name. Malformed, non-regular, symlinked or over-64-KiB configs are refused. A new config is mode `0600` on POSIX; an existing config's permission bits are preserved.

## 2. Trust and connect in Pi

Project MCP configuration is trust-gated. Start Pi in the configured project, review and approve the project resources when prompted, then inspect the connection:

```sh
cd /path/to/project
pi
```

In the interactive session use `/mcp`; from a shell, `pi mcp list` connects enabled servers and reports tools/errors. If Pi already has a saved trust decision, verify that the project is trusted. Pi project trust allows `.pi/mcp.json` to load; it is not a sandbox and does not restrict what an enabled server can access.

The first native server connection creates the database (if it does not exist) and may index/update local repository evidence. MCP calls and stored records are scoped to the configured exact user/project/optional-agent tuple. Keep the database private: it is unencrypted, and anyone who can read it can inspect its contents. No setup step requires a cloud account or embedding provider; optional external embedding commands are separate and should be configured only deliberately.

For diagnostics, check that the configured executable exists and is executable, the project path matches where Pi runs, and the scope values are the ones used when notes were saved. Native diagnostics go to stderr; protocol data uses stdout. See [native tools and scope details](NATIVE_MEMORY.md) and [context-budget semantics](CONTEXT_ASSEMBLY.md).

## Existing `tinymem-test` in this checkout

This checkout already has a project `.pi/mcp.json` entry named `tinymem-test`, configured with local absolute paths and its own database. The setup helper deliberately refuses to add a second native TinyMem server while that entry is present. Do not run the default setup command here and leave both servers enabled: that would start duplicate native MCP servers and expose duplicate tools.

The optional Pi extension defaults to reading the server named `tinymem`. To use the existing entry without changing MCP configuration, pass `--tinymem-server tinymem-test` when loading the extension. If you intentionally want to replace the existing MCP server configuration instead, use `--server-name tinymem-test --force` and explicitly preserve its database path and scope. Replacing an MCP entry does not delete its old database, but omitting `--db` points the new entry at a different, initially empty database.

## Optional Pi extension: task context and `/memory`

The extension is a separate opt-in load from the project config; it does not auto-install or edit Pi settings. From a TinyMem source checkout, run Pi in the project root and load:

```sh
pi --extension /absolute/path/to/tinymem/integrations/pi/index.ts
```

Alternatively install the local Pi package project-locally with `pi install -l /absolute/path/to/tinymem`; the setup helper itself never installs it. The extension requires Pi 1.0.0 or newer, a trusted project and a project-local `.pi/mcp.json` entry containing an absolute native `tinymem` command and explicit `--db`, `mcp`, repository, `--user` and `--project` args. The repository in that entry must match Pi's current working directory. It intentionally rejects HTTP, environment overrides, wrappers, unsupported args and embedding-command options; automatic recall is lexical/local and model-free.

The extension's behavior is deliberately bounded and explicit:

- Automatic task context is enabled by default, with a **4096 conservative UTF-8-byte** prompt budget and a **5000 ms** subprocess deadline. These units are a byte upper bound, not the model's tokenizer. Set `--tinymem-budget` or `--tinymem-timeout` to adjust them; `--tinymem-off` starts with context disabled. `/memory off` and `/memory on` toggle it for the session (`on` checks project trust/config first).
- Automatic source selection and repository exclusions are request-local only. Use `--tinymem-context-sources memory` to inject only personal memory, `repository` for source only, or the empty string for neither. `--tinymem-context-exclude '["generated/**", "vendor/**"]'` accepts a JSON array of repository-relative globs. `*` does not cross `/`; `**` matches recursively. Exclusions do not filter saved-memory file dependencies or affect explicit search.
- It assembles current evidence for the task before each model request, removes its previous injected entry, and does not persist that context in the session transcript. A missing/bad configuration or failed recall omits memory evidence and reports status/warning; it does not stop the coding session. A zero budget makes no TinyMem recall call.
- `/memory status` reports the state and automatic source/exclusion settings; `/memory recall <task>` inspects current context without saving or injecting it and ignores automatic-context filters; `/memory remember <note>` is an explicit durable decision write; `/memory correct <id> <version> <note>` and `/memory delete <id> <version>` are optimistic-version writes. Deletion hides the current memory but retains audit history; it is not secure erasure.
- Model-issued TinyMem write tools are blocked by default. `/memory` is the intentional write path; `--tinymem-allow-agent-writes` opts into model-issued writes. There is no automatic conversation capture.

The Pi built-in MCP connection remains the persistent tool server; extension recalls/commands launch short-lived native SDK processes. This is local process overhead, not an extra Pi MCP server registration. Automatic context already appears in each model request: use that evidence rather than repeating a recall/search when it answers the task. Standalone lexical searches use literal AND matching; use 1–3 distinctive terms and shorten a query after zero hits. The extension does not claim natural-task quality or eliminate the need to review memory as untrusted evidence. A one-question live Pi/Grok recall-efficiency check is documented separately in the [experiment report](PI_GROK_RECALL_EFFICIENCY.md); it is not a coding-outcome benchmark.

## Remove or disable

To disable the optional extension for one invocation, use `--tinymem-off`; to disable task-context injection during a session, use `/memory off`. To stop exposing the MCP tools, remove or set the selected server entry to `"enabled": false` in `.pi/mcp.json` (or use Pi's `/mcp` controls). Removing an MCP entry or extension does not delete the database. Database removal, if ever desired, is a separate manual operation after making any needed backup.
