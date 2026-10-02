# Linux x64 qualification fixture

Maintainer recipe, not an installation dependency or a support claim. It pins
Rust 1.93.1 and Node 22.19.0 base images by digest and installs Pi 1.0.0 with
lifecycle scripts disabled. Apt package revisions are not snapshot-pinned;
retain the resulting image ID, environment log and release manifests. Rebuilds
are compared within the same prepared environment, not claimed universally
bit-identical across every compiler/libc image.

Prefer an extracted, verified **release-source archive** and empty
**disposable** cache and output directories. A checkout is allowed only when it
contains no private configuration or data: the whole source tree is mounted
read-only, even though the release builder later copies only its allowlist.
The guard refuses root-level `.pi`, `.tinymem`, `.env*`, and SQLite database or
sidecar filenames (including dangling symlinks). It is not a recursive secret
scanner; inspect the entire checkout yourself or use the clean source archive.
The three paths must not overlap. Root, HOME and `.pi` paths are rejected.
Never pass any real user config/data directory:

```sh
sh scripts/fixtures/linux-beta/qualify.sh prepare /absolute/source /absolute/fixture-cargo-cache /absolute/fixture-output
sh scripts/fixtures/linux-beta/qualify.sh run /absolute/source /absolute/fixture-cargo-cache /absolute/fixture-output > /absolute/fixture-output/run.log 2>&1
```

`prepare` explicitly uses network for Docker, apt, npm and locked Cargo inputs.
`run` uses `--network none`, mounts source and the populated Cargo cache
read-only, and gives Pi a disposable HOME. It builds twice in separate native
target directories, compares all archive/manifest hashes, and runs the real Pi
CLI clean-room acceptance suite against the first artifact set. No Rust or
compiler is invoked by TinyMem installation or Pi runtime. Archives bundle the
engine, TypeScript SDK and extension; there are no TinyMem npm dependencies or
install hooks. The container nevertheless contains builder tools for the
separate rebuild proof.

For a genuine version-upgrade check, point to a previously built **Linux x64**
TinyMem 0.1.0 executable, not a renamed beta binary:

```sh
TINYMEM_PREVIOUS_BINARY=/absolute/fixtures/tinymem-0.1.0 \
TINYMEM_PREVIOUS_SHA256=<64-hex-sha256-of-that-file> \
sh scripts/fixtures/linux-beta/qualify.sh run /absolute/source /absolute/fixture-cargo-cache /absolute/new-output
```

The file and digest are checked before a read-only mount. Without them, the
acceptance report explicitly leaves genuine prior-version upgrade unqualified;
it still exercises beta installation relocation and database preservation.

The suite writes logs under `fixture-output/acceptance`; native/source/npm
archives are in `build-one` and `build-two`. A pre-existing output build
subdirectory is refused rather than overwritten. For another run choose a new
output directory. `TINYMEM_QUALIFIER_IMAGE` may select an already prepared image;
record its exact ID. On ARM hosts this is Docker's emulated amd64 Linux, not
bare-metal x64 hardware evidence. Docker is not needed by beta users.
