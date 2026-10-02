# TinyMem Pi-only beta (`0.1.0-beta.1`)

This is a **Pi-only, local-first beta** for one trusted Pi project. It bundles the TinyMem native engine, TypeScript SDK, and Pi extension as platform-specific release artifacts. You do not need Rust, Python, a TinyMem account, a model service for TinyMem, or network access for TinyMem's install/engine operations. Pi may use the model/provider you normally choose for coding; task evidence sent to that provider follows its own data-handling policy.

This beta does not claim support for another coding agent, a web/API service, team authorization, automatic conversation capture, secure deletion, natural-task quality gains, or exact model-token budgets. Read memory/source evidence as untrusted data. The database is plaintext and unencrypted; scopes are namespacing, not access control.

## Qualification matrix

Only the checked cells below are qualified. A successful emulated Linux container is not evidence from bare-metal Linux hardware. Other operating systems, architectures, libc baselines, Pi versions, and Node versions are **unqualified**, not implicitly supported.

| Host / binary | Required Pi / Node cell | Qualification gate |
| --- | --- | --- |
| macOS 26.5.1, Apple Silicon (`darwin-arm64`) | Pi 1.0.0 + Node 26.1.0 | Qualified only when an exact passing run is recorded in [`PI_BETA_CHECKS.md`](PI_BETA_CHECKS.md) |
| macOS 26.5.1, Apple Silicon (`darwin-arm64`) | Pi 1.0.0 + Node 22.19.0 | Qualified only when an exact passing run is recorded in [`PI_BETA_CHECKS.md`](PI_BETA_CHECKS.md) |
| Debian 12, Linux x64 (`linux-x64-gnu`, glibc 2.36), Docker amd64 emulation on ARM host | Pi 1.0.0 + Node 22.19.0 | Qualified only when an exact passing run is recorded in [`PI_BETA_CHECKS.md`](PI_BETA_CHECKS.md); container/emulation only |
| Any OS | Pi 1.0.0 + Node 24.x | **Unqualified** until an exact passing run is recorded |
| Windows, Intel macOS, Linux other than the exact listed cell, musl/Alpine | Any | **Unqualified / no artifact claim** |

The maintainer build is pinned to Rust/Cargo 1.93.1 and `--locked`; Rust is a build-time tool only. End users run the bundled native engine and do not need Rust or a C compiler. `Cargo.toml`'s Rust minimum is not a qualification of arbitrary builder versions.

The maintainer [qualification report](PI_BETA_CHECKS.md) is the evidence ledger for executed final-artifact runs, manifests, checksums, and environment details. A test configured in CI or a local trial artifact is not an executed final-artifact qualification. The test harness uses Pi's real CLI/package manager and runtime with a deterministic offline model-provider fixture; it makes no model requests and does not exercise natural coding quality.

## Download and verify

> **Not yet published.** There is no download URL. `scripts/release.mjs` only
> writes a local directory, and the archives below exist only in
> `dist/beta/0.1.0-beta.1/aarch64-apple-darwin/` and
> `dist/beta/0.1.0-beta.1/x86_64-unknown-linux-gnu/`. Do not announce this beta
> as downloadable until tag `0.1.0-beta.1` is pushed and those exact files are
> attached to the release. See [`PI_BETA_CHECKS.md`](PI_BETA_CHECKS.md) for hashes
> and publication status.

Once published, obtain the five files for **your exact artifact target** from the
same tag `0.1.0-beta.1` release:

- `tinymem-pi-0.1.0-beta.1-<platform>-<arch>.tgz` — the Pi package and bundled native executable.
- `tinymem-native-0.1.0-beta.1-<platform>-<arch>.tar.gz` — standalone native engine archive.
- `tinymem-source-0.1.0-beta.1.tar.gz` — source snapshot used by the release builder.
- `release-manifest.json` and `SHA256SUMS` — artifact checksums and build-input metadata.

Check each downloaded file against `SHA256SUMS` before use. The release manifest records the target, tool versions, input-file hashes/modes, source digest, binary digest, artifact hashes and byte counts. `sourceDigest` is a canonical input-inventory digest, **not a Git commit identifier**. Beta artifacts are not code-signed or notarized; checksums detect accidental/cross-download corruption but do not authenticate a publisher.

### macOS Gatekeeper (unsigned beta)

The Apple Silicon engine is adhoc/linker-signed only, not Developer-ID signed or
notarized. After verifying `SHA256SUMS`, clear the quarantine flag on the
downloaded package directory before first use:

```sh
xattr -dr com.apple.quarantine "$install_root"
"$package_root/native/tinymem" --version  # expect: tinymem 0.1.0-beta.1
```

If macOS still blocks the binary, allow it under System Settings → Privacy &
Security (or right-click → Open once). Do not disable Gatekeeper system-wide.

## Install into one project

Requires Pi **1.0.0** and a Node runtime satisfying the package engine floor of **22.19.0**. Only the exact OS/Pi/Node cells listed above and recorded as passing in the qualification report are qualified; the package engine floor is not a blanket platform/version support claim. The install helper edits only the selected project's `.pi/settings.json` and `.pi/mcp.json`, plus its ownership receipt. Stop Pi before install, upgrade, or uninstall. Use a package directory that remains at a stable, trusted absolute path; do not delete or move it while installed.

Extract the platform-specific package archive and keep its `package/` directory:

```sh
install_root="$HOME/.local/share/tinymem/packages/0.1.0-beta.1"
mkdir -p "$install_root"
tar -xzf tinymem-pi-0.1.0-beta.1-<platform>-<arch>.tgz -C "$install_root"
package_root="$(cd "$install_root/package" && pwd -P)"
```

Prepare an absolute database path. Use a private directory; the installer deliberately does **not** create/chmod database parents or open the database:

```sh
umask 077
mkdir -m 700 -p "$HOME/.local/share/tinymem/data"
db="$HOME/.local/share/tinymem/data/my-project.db"
```

Choose the exact user/project values you intend to keep. Identifiers must be 1–128 ASCII characters matching `[A-Za-z0-9][A-Za-z0-9_.:@-]*`. An optional agent is another exact scope dimension, not a permission. Canonicalize the project directory (no symlinked path components) before installing:

```sh
project="$(cd /absolute/path/to/your/project && pwd -P)"
node "$package_root/scripts/pi-package.mjs" install \
  --root "$project" --db "$db" --user alice --project my-project --agent pi
```

Omit `--agent pi` only when you deliberately want to use the exact null-agent namespace; clients that also omit agent use that same namespace, while every named agent remains separate. The helper verifies its bundled package manifest and atomically registers its absolute package path and native MCP server in that project's settings. It records `.pi/tinymem-install.json` ownership metadata. It does not open, create, migrate, copy, or delete a database. The supported install path is this helper, not `pi install`: do not also add a second TinyMem server or independently manage this same package declaration with `pi install`; the helper owns both local Pi declarations for this workflow. The acceptance harness exercises real `pi install/list/remove` only as a separate package-manager check, not as the install path.

Verify the project config diff, then start Pi in the project and approve its trusted project resources:

```sh
cd "$project"
pi --approve
```

Project trust allows Pi to execute its configured extension/MCP server with the operating-system permissions of the Pi process; it is **not a sandbox**. Once approved, run `/memory status`. The native server creates its database on first connection. To save a note intentionally, use `/memory remember <durable note>`. A new Pi process in the same project/scope can retrieve it automatically for a related task or inspect it with `/memory recall <task>`.

The extension assembles current task evidence before model requests, including tool follow-ups. It injects only the bounded prompt and checks source freshness for each request. The default is 4096 conservative UTF-8-byte units (not model tokenizer tokens); the default subprocess deadline is 5000 ms. A zero budget injects nothing. It does not write model context into the transcript. `/memory correct <id> <version> <note>` and `/memory delete <id> <version>` are explicit, optimistic-version writes. Deletion hides the memory but retains audit history and is not secure erasure. Model-issued memory writes are blocked by default.

Automatic context can be constrained per invocation with `--tinymem-context-sources memory`, `repository`, or the empty string, and `--tinymem-context-exclude '["generated/**"]'`. These options affect automatic assembly only, not explicit `/memory recall` or source dependency freshness. Current personal evidence is scoped exactly to the configured user/agent/project tuple. The local engine itself has no model/network endpoint; selected evidence is nevertheless included in the coding model's request when Pi calls your configured provider.

## Back up and restore

Stop Pi for the restore operation. Back up using the **bundled** native engine, with absolute paths and an existing destination parent:

```sh
backup_dir="$HOME/.local/share/tinymem/backups"
mkdir -m 700 -p "$backup_dir"
"$package_root/native/tinymem" --db "$db" backup --output "$backup_dir/my-project.db"
```

SQLite's online backup API makes a consistent snapshot, including committed WAL data; it validates the finished image and refuses to overwrite an existing destination. The source database/WAL is not checkpointed or modified by the backup operation. Backups are sensitive plaintext and retain deleted audit observations.

Restore only to a **new, absent** database path whose parent already exists:

```sh
restore_db="$HOME/.local/share/tinymem/data/my-project-restored.db"
"$package_root/native/tinymem" --db "$restore_db" restore --input "$backup_dir/my-project.db"
```

The backup input must be a valid compatible TinyMem snapshot; the restore destination itself must be a new absent path. Backups contain the **whole database**, including any user/project/agent scopes, repository indexes, audit history, and deleted observations; they are not per-project exports. Never restore over a live database or point an old engine at a database written by a newer schema. Keep independent backups before upgrades or manual storage operations.

## Upgrade and uninstall

Install the new package archive into a new stable package directory first. Stop Pi, then run the **new** helper with the exact same database and scope options plus `--upgrade`:

```sh
node "$new_package_root/scripts/pi-package.mjs" install \
  --root "$project" --db "$db" --user alice --project my-project --agent pi --upgrade
```

The helper verifies its ownership receipt and refuses if the existing TinyMem MCP/package declaration was edited, or if the database/scope values differ. It replaces only its own package/config entries and never opens or migrates the database. Re-review the diff and start Pi again. Beta acceptance qualifies preservation while relocating the helper-managed package root; it does not establish a separate old-schema migration or a schema-version change (both tested engine versions currently use native schema v4). The Linux x64 cell additionally does **not** claim genuine prior-version (`0.1.0` → beta) upgrade preservation: its runs left `TINYMEM_PREVIOUS_BINARY` unset (see [`PI_BETA_CHECKS.md`](PI_BETA_CHECKS.md)). Only the macOS cells passed that preservation check.

To uninstall while leaving all user data intact:

```sh
node "$package_root/scripts/pi-package.mjs" uninstall --root "$project"
```

It removes only the exactly owned TinyMem package/MCP entries and receipt, preserving unrelated Pi settings and servers. It deliberately leaves the package files and database in place. Deleting database files is a separate manual decision; SQLite/WAL/backups may retain data even after ordinary file deletion.

## Maintainer acceptance

From a prepared checkout with release builder Rust/Cargo 1.93.1 and a real Pi 1.0.0 installation, build and test as follows:

```sh
node scripts/release.mjs --out /absolute/empty/release-dir --offline
TINYMEM_RELEASE_DIR=/absolute/empty/release-dir \
TINYMEM_PI_BIN=/absolute/path/to/pi \
TINYMEM_ACCEPT_ARTIFACTS=/absolute/outside/source/acceptance \
TINYMEM_PREVIOUS_BINARY=/absolute/path/to/previous/tinymem \
TINYMEM_PREVIOUS_SHA256=<64-hex-digest> \
node --experimental-strip-types scripts/accept-pi-beta.mjs
```

The harness requires a matching native artifact and Pi 1.0.0; it fails rather than skipping missing coverage. It extracts and verifies all release archives, performs an offline npm install of the package tarball, exercises actual Pi package install/list/remove and MCP discovery, checks project-helper install/upgrade/uninstall behavior, and uses separate real Pi CLI/RPC processes with an in-process deterministic provider for remember → restart/recall → correction → dependency invalidation → delete. It also tests scoped provenance/budgets, backup/restore, and real Pi host-runtime nested-write gating. All HOME, Pi config/session directories, projects, receipts, databases, restores, and logs are temporary; never set those variables to real user paths. The optional previous binary is checksum-checked and must be built for the test host. Its database test verifies preservation, not a schema migration.

Linux x64 qualification uses [`scripts/fixtures/linux-beta/`](../scripts/fixtures/linux-beta/), a disposable, pinned amd64 container. Dependency preparation is the only networked phase; acceptance/release building runs with `--network none`. Docker is not a user/runtime requirement.
