# Pi beta run evidence

These are retained outputs from the actual `0.1.0-beta.1` artifact/runtime qualification runs. Acceptance was performed with isolated temporary Pi homes, project configs, sessions, databases and synthetic fixture memories; the provider fixture was deterministic/offline. No real user Pi configuration, HOME or database was mounted.

- `macos-arm64-node26/`, `macos-arm64-node22/`, `linux-x64-node22/`: full harness JSON, console output and per-command logs for each matrix cell.
- `builds/macos-arm64/`, `builds/linux-x64/`: release manifests/checksums from both independent builds, build logs, host/image metadata and the serial regression-test record.
- `linux-runtime-only/`: a supplemental repeat against the Linux release archive in an official Node 22.19 container with Pi copied from the prepared image. It ran `--network none --read-only`, and records the checked absence of Rust/Cargo, C/C++ compilers and Python.

The Linux x64 result is Docker amd64 emulation on an Apple Silicon host, not bare-metal x64 hardware evidence. The qualification ledger is [`../PI_BETA_CHECKS.md`](../PI_BETA_CHECKS.md). The report tree is intentionally excluded from the release-source input inventory; adding these logs therefore does not alter the frozen release `sourceDigest`.
