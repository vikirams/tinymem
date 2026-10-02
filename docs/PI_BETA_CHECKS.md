# Pi beta qualification ledger (`0.1.0-beta.1`)

This file is the evidence ledger referenced by the [install guide](PI_BETA_RELEASE.md).
A matrix cell is qualified **only** when this file records its exact passing run,
release manifest, checksums, and environment. Until this file existed, every cell
was unqualified. Raw run outputs live in [`beta-runs/`](beta-runs/); this file is
the qualification verdict over them, not a duplicate of every log line.

Verify locally (from the checkout root):

```sh
shasum -a 256 dist/beta/0.1.0-beta.1/aarch64-apple-darwin/* dist/beta/0.1.0-beta.1/x86_64-unknown-linux-gnu/*
node scripts/release.mjs --help
```

## Source identity

- `sourceDigest`: `8e459a0e32bbf42a95ae4e666a0345bbccc25b7b7a2b438fd34f5b9ecedd120f`
  (identical in both target manifests; see below).
- `sourceDigest` is a SHA-256 over the canonical release-input inventory
  (`release-manifest.json`: `sourceIdentity: "sha256 of canonical release-input
  inventory (not a git commit)"`). It is **not** a Git commit id.
- This checkout now has local git metadata: commit `ace4236`, local tag
  `0.1.0-beta.1`, and workflow `.github/workflows/release-beta.yml`. There is
  **no remote configured and no published GitHub release**, so there is still no
  download URL. Do not cite a commit id as the source identity.
- The `beta-runs/` tree is excluded from the release-source input inventory, so
  adding run logs does not alter the frozen `sourceDigest`.
- The `sourceDigest` above freezes the pre-announcement doc set. Post-freeze doc
  fixes (`PI_BETA_RELEASE.md` download/Gatekeeper/scope notes, `README.md`,
  `MCP_SETUP.md`, this ledger, `.github/workflows/release-beta.yml`) are
  intentionally **not** rebuilt into `0.1.0-beta.1` archives; publish the exact
  committed `dist/beta/0.1.0-beta.1/` files.

## Publication status (announcement blocker)

- `scripts/release.mjs` only writes a **local directory** (`--out`). It does not
  upload, publish, or print a download URL.
- The exact shippable archives exist only locally at:
  - `dist/beta/0.1.0-beta.1/aarch64-apple-darwin/` (darwin-arm64 set)
  - `dist/beta/0.1.0-beta.1/x86_64-unknown-linux-gnu/` (linux-x64 set)
  They are force-added to the local tag (`dist/` stays ignored for other build
  debris). Pushing the tag with no remote configured is still pending.
- Each set ships 5 files: package tarball (`tinymem-pi-*.tgz`), native archive
  (`tinymem-native-*.tar.gz`), source archive (`tinymem-source-*.tar.gz`),
  `release-manifest.json`, and `SHA256SUMS`. Hashes are recorded per cell below.
- The install guide currently has **no download URL**. Do not announce the beta
  as downloadable until the tag is pushed and those exact 10 files
  (5 per target) are attached to the release. Required remaining steps:
  1. Add a remote, push commit + tag `0.1.0-beta.1`
     (`git push origin main 0.1.0-beta.1`, or `gh release create 0.1.0-beta.1`
     with the 10 files below).
  2. Confirm `.github/workflows/release-beta.yml` attached the exact committed
     files (it verifies `SHA256SUMS`, never rebuilds).
  3. Fill the download URL into `docs/PI_BETA_RELEASE.md` and `README.md`.
- Supported install path is the **downloaded package plus
  `node <package>/scripts/pi-package.mjs install ...`**, not `pi install`. The
  harness exercises real `pi install/list/remove` only as a separate package-manager
  check (`actual-pi-install-list-load-remove`); it is not the supported install path.

## Code signing status (macOS announcement blocker)

- The Apple Silicon engine is **not Developer-ID signed or notarized**.
  The bundled binary is adhoc/linker-signed only (`Signature=adhoc`,
  `CDHash=331b16b2cebefc018c60348fee3bea5561f798f9`). Checksums detect
  accidental/cross-download corruption but do **not** authenticate a publisher.
- Until signed/notarized, macOS users must clear the Gatekeeper quarantine after
  verifying `SHA256SUMS`. With the extracted package at `$package_root`:
  ```sh
  xattr -dr com.apple.quarantine "$install_root"
  "$package_root/native/tinymem" --version  # expect: tinymem 0.1.0-beta.1
  ```
  If macOS still refuses the binary, open System Settings → Privacy & Security and
  allow it, or right-click → Open once. Do not disable Gatekeeper system-wide.
- Either sign and notarize the Apple Silicon engine before announcing, or keep this
  Gatekeeper step in the install guide. This ledger records the unsigned state; it
  does not grant a signing claim.

## Verdicts

| Host / binary | Pi / Node cell | Verdict |
| --- | --- | --- |
| macOS 26.5.1, Apple Silicon (`darwin-arm64`) | Pi 1.0.0 + Node 26.1.0 | **Qualified** for local-artifact acceptance (run 1 below). Download URL, tag, and signing still pending per sections above. |
| macOS 26.5.1, Apple Silicon (`darwin-arm64`) | Pi 1.0.0 + Node 22.19.0 | **Qualified** for local-artifact acceptance (run 2 below). Download URL, tag, and signing still pending per sections above. |
| Debian 12, Linux x64 (`linux-x64-gnu`, glibc 2.36), Docker amd64 emulation on ARM host | Pi 1.0.0 + Node 22.19.0 | **Qualified** for local-artifact acceptance **without prior-version upgrade** (runs 3–4 below). Container/emulation only. |
| Any OS | Pi 1.0.0 + Node 24.x | **Unqualified** — no passing run recorded. |
| Windows, Intel macOS, Linux other than the exact listed cell, musl/Alpine | Any | **Unqualified / no artifact claim**. |
| Bare-metal Linux x64 hardware | Any | **Unqualified** — the Linux evidence is emulated-container only, not bare-metal. |
| Any other Pi or Node version | Any | **Unqualified** — engine floor `>=22.19.0` is not a support claim. |

Out of scope for this beta (not blockers, not claimed): Node 24, Windows, Intel
macOS, musl/Alpine, bare-metal Linux, other Pi versions, an npm/`pi.dev`/packages
listing, and coding-quality results. The clean-room acceptance verifies
integration/lifecycle behavior, not whether memory improves natural coding outcomes.

## Run 1 — macOS arm64, Node 26.1.0 (qualified)

- Run: `docs/beta-runs/macos-arm64-node26/release-acceptance.json`
  (`console.log`, `logs/` alongside). Status `passed`,
  `2026-10-02T12:15:25.234Z` → `2026-10-02T12:15:33.733Z`.
- Runtime: `darwin-arm64`, macOS 26.5.1 (`os_release 25.5.0`), Node `v26.1.0`,
  Pi `1.0.0` (`/Users/apple/.nvm/versions/node/v26.1.0/bin/pi`), engine
  `tinymem 0.1.0-beta.1`, package `tinymem-pi@0.1.0-beta.1`.
- Release dir tested: `dist/beta/0.1.0-beta.1/aarch64-apple-darwin`
  (byte-identical to `docs/beta-runs/builds/macos-arm64/` manifest + `SHA256SUMS`).
- Manifest: target `aarch64-apple-darwin`, `engineVersion tinymem 0.1.0-beta.1`,
  `sourceDigest 8e459a0e…edd120f` (full value in "Source identity"),
  `binarySha256 bce24a60380036e25dd8302460b51beb7d8839a092436f327ee5087866d85275`.
  Builder: `rustc/cargo 1.93.1`, `node v26.1.0`, `osRelease 25.5.0`,
  `Apple clang 17.0.0 (clang-1700.0.13.3)`, `cargoLocked true`,
  `sourceDateEpoch 0`, path remapping on, incremental off.
- Artifacts (also `SHA256SUMS` in that dir):
  - `tinymem-pi-0.1.0-beta.1-darwin-arm64.tgz`: `d70e402a980e180f044549f3b5a31f228a29270a2e668bf798ae497a03831e53` (2562494 bytes)
  - `tinymem-native-0.1.0-beta.1-darwin-arm64.tar.gz`: `c0cf9efa944ac4d26e623ad83d92778bb14e5a064ad7d7fabd43e3780f5a371e` (2532348 bytes)
  - `tinymem-source-0.1.0-beta.1.tar.gz`: `40ed27c52fd159745e54fb01bb106a0cf79a5e0e698f7d3e2205d0911e9d0d5c` (233247 bytes)
  - `release-manifest.json`: `487bc12b505db334742de0ed7deb26ec68514b053410be25945d1fc70aad0704`
- Host env: `docs/beta-runs/builds/macos-arm64/system.txt` (macOS 26.5.1 build
  25F80, Darwin 25.5.0 arm64, Apple clang 17.0.0). Rebuild proof:
  `build-and-reproducibility.log` plus `rebuild-release-manifest.json` /
  `rebuild-SHA256SUMS` (identical hashes, `cmp` clean).
- Steps passed (18/18, no `unqualified_checks`): `runtime-pins`,
  `release-artifacts-and-checksums`, `extract-native-npm-source-archives`,
  `offline-npm-install-of-release-tarball`,
  `owned-install-preserves-unrelated-config-permissions-and-does-not-open-db`,
  `installer-refuses-config-drift-and-package-tampering-without-writes`,
  `real-pi-mcp-discovery` (11 tools), `exact-scope-provenance-and-budget`,
  `new-pi-process-recall-is-request-local-and-read-only`,
  `explicit-correction-visible-after-process-restart`,
  `file-provenance-invalidates-on-change-without-history-mutation`,
  `database-backup-restore-retains-memory-history-and-provenance`,
  `upgrade-preserves-database-scope-and-unrelated-settings`,
  `explicit-delete-retains-audit-and-stops-current-recall`,
  `actual-pi-install-list-load-remove`,
  `packaged-extension-real-pi-process-runtime-test`,
  `previous-native-database-upgrade-preservation`
  (`previous_binary_sha256 0bb0ee2aebe6406119ed8c9622f49ae00229b4ea8d2617ae7c4a8f7570c8cb81`,
  prior `tinymem 0.1.0` darwin-arm64; preservation on native schema v4, not a
  schema migration), `owned-uninstall-removes-only-tinymem-config-and-preserves-database`.

## Run 2 — macOS arm64, Node 22.19.0 (qualified)

- Run: `docs/beta-runs/macos-arm64-node22/release-acceptance.json`
  (`console.log`, `logs/` alongside). Status `passed`,
  `2026-10-02T12:14:35.054Z` → `2026-10-02T12:14:44.483Z`.
- Runtime: `darwin-arm64`, macOS 26.5.1 (`os_release 25.5.0`), Node `v22.19.0`,
  Pi `1.0.0`, engine `tinymem 0.1.0-beta.1`, package `tinymem-pi@0.1.0-beta.1`.
- Release dir, manifest, checksums, builder, host env, and rebuild proof:
  identical to Run 1 (same `dist/beta/0.1.0-beta.1/aarch64-apple-darwin/` set;
  builder `node` field in the manifest records the build-time Node `v26.1.0`,
  while this run's test-time Node is `v22.19.0`).
- Steps passed: same 18/18 list as Run 1, including
  `previous-native-database-upgrade-preservation` with the same prior
  `0bb0ee2a…` darwin-arm64 binary (schema-v4 preservation, not migration).
  No `unqualified_checks`.

## Run 3 — Linux x64, Node 22.19.0, qualifier container (qualified without prior-version upgrade)

- Run: `docs/beta-runs/linux-x64-node22/release-acceptance.json`
  (`console.log`, `logs/` alongside). Status `passed`,
  `2026-10-02T12:23:36.734Z` → `2026-10-02T12:24:03.090Z`.
- Runtime: `linux-x64`, `os_release 6.12.76-linuxkit`, Node `v22.19.0`,
  Pi `1.0.0`, engine `tinymem 0.1.0-beta.1`, package `tinymem-pi@0.1.0-beta.1`.
  **Docker amd64 emulation on an ARM host, not bare-metal x64 hardware.**
- Fixture: `scripts/fixtures/linux-beta/` pinned qualifier image
  `tinymem-beta-qualifier:local` (`Id aba94e44825b68ab7dc3bb2845f7b213b3166ce4f7685b67489e53d6b9db2f13`);
  prepared env `docs/beta-runs/builds/linux-x64/linux-prepared-environment.txt`
  (Node v22.19.0, npm 10.9.3, Pi 1.0.0, rustc/cargo 1.93.1,
  `cc (Debian 12.2.0-14+deb12u1) 12.2.0`, `ldd (Debian GLIBC 2.36-9+deb12u13) 2.36`,
  Python 3.11.2). Acceptance ran with `--network none`.
- Release set: `dist/beta/0.1.0-beta.1/x86_64-unknown-linux-gnu`
  (byte-identical to `docs/beta-runs/builds/linux-x64/` manifest + `SHA256SUMS`).
  Manifest target `x86_64-unknown-linux-gnu`, `sourceDigest 8e459a0e…edd120f`
  (same as macOS), `binarySha256 5ece3e15858323ad7b9e5a6a100f96e695803062df35054eb44438887342f633`.
  Builder: `rustc/cargo 1.93.1`, `node v22.19.0`, `osRelease 6.12.76-linuxkit`,
  `cc (Debian 12.2.0-14+deb12u1) 12.2.0`, `cargoLocked true`, `sourceDateEpoch 0`.
- Artifacts (also `SHA256SUMS` in that dir):
  - `tinymem-pi-0.1.0-beta.1-linux-x64.tgz`: `232976834d84b58ee3522741076d33b3367e6405380a8d31b64a661ffdbac103` (2837137 bytes)
  - `tinymem-native-0.1.0-beta.1-linux-x64.tar.gz`: `56dcf4a7b95619b5fe1c1ff46332757d90e853a13e5d2c335d5619caef2c5c62` (2808750 bytes)
  - `tinymem-source-0.1.0-beta.1.tar.gz`: `066b16a16d13603d56c5e7c4e23d857d8a207c67d03eb682a1254c921b7bd96d` (233247 bytes)
  - `release-manifest.json`: `ad8561a8ad0149040a02f63fa77dd550b11cf5c947e68efaa8f1b83479023aeb`
- Rebuild proof: `docs/beta-runs/builds/linux-x64/two-builds-and-acceptance.log`
  builds twice into separate dirs, `cmp`s both `SHA256SUMS`, then runs acceptance
  against the first set.
- Steps passed (17/17): same list as macOS **except**
  `previous-native-database-upgrade-preservation` did **not** run.
  `unqualified_checks: ["previous-native-database upgrade preservation not run
  (TINYMEM_PREVIOUS_BINARY unset)"]`.
- **Linux upgrade scope:** this cell qualifies beta install relocation and
  same-schema database preservation (`upgrade-preserves-database-scope-and-unrelated-settings`
  on schema v4). It does **not** claim genuine prior-version (`0.1.0` → beta)
  upgrade preservation. That preservation, if claimed later, is still schema-v4
  preservation, not a schema migration. A candidate prior Linux x64 binary exists
  locally at `dist/beta-validation/prior-engines/linux-x64/tinymem`
  (`tinymem 0.1.0`, `sha256 c9311937ad087e2efeafe45a5e54b42c2358716149747f99d82ed06ffc8050ad`)
  for a future rerun:
  ```sh
  TINYMEM_PREVIOUS_BINARY=/absolute/fixtures/tinymem-0.1.0 \
  TINYMEM_PREVIOUS_SHA256=c9311937ad087e2efeafe45a5e54b42c2358716149747f99d82ed06ffc8050ad \
  sh scripts/fixtures/linux-beta/qualify.sh run /absolute/source /absolute/fixture-cargo-cache /absolute/new-output
  ```
  Until such a rerun passes and is recorded here, the Linux cell carries no
  prior-version upgrade claim.

## Run 4 — Linux x64 runtime-only repeat (supplemental, same scope)

- Run: `docs/beta-runs/linux-runtime-only/release-acceptance.json`
  (`console.log`, `preflight.txt`, `artifact-checksums.txt`,
  `runtime-environment.txt`, `runtime-image.json`, `pi-source-image.json`, `logs/`).
  Status `passed`, `2026-10-02T12:27:43.030Z` → `2026-10-02T12:28:09.845Z`.
- Same `linux-x64` release set, manifest, and artifact hashes as Run 3.
- Runtime image: official `node:22.19.0-bookworm-slim`
  (`Id daa74c183f7d8c1ba55ed79c76e912f50be8782ca9d2da640645f906dce474b8`,
  `node@sha256:4a4884e8a44826194dff92ba316264f392056cbe243dcc9fd3551e71cea02b90`),
  run `--network none --read-only` with Pi copied from the prepared image.
  Records absence of `rustc/cargo/cc/gcc/clang/python/python3` and
  `ldd (Debian GLIBC 2.36-9+deb12u13) 2.36`. Same 17/17 steps and same
  `unqualified_checks` (no prior-binary preservation) as Run 3.
- This repeat strengthens the "no toolchain at install/runtime" evidence; it does
  not add a new matrix cell or a prior-version claim.

## Explicitly unqualified

- `Pi 1.0.0 + Node 24.x` on any OS: no passing run recorded.
- Windows, Intel macOS, non-Debian-12/non-glibc-2.36 Linux, musl/Alpine: no
  artifacts and no runs; **no artifact claim**.
- Bare-metal Linux x64: Runs 3–4 are emulated-container evidence only.
- Other Pi/Node versions: untested; the package `engines.node >=22.19.0` floor is
  not a qualification.
- npm registry, `pi.dev`, or packages-listing presence: not claimed and not required.
- Coding-quality/task-effectiveness: not measured by this harness (deterministic
  offline fixture, no model requests, no natural coding tasks).
