#!/bin/sh
# Maintainer-only. Explicit fixture directories, read-only source mount.
set -eu
if [ "$#" -ne 4 ]; then
  echo 'Usage: sh qualify.sh prepare|run ABS_SOURCE ABS_CARGO_CACHE ABS_OUTPUT' >&2
  exit 2
fi
phase=$1 source=$2 cache=$3 output=$4
case "$phase" in prepare|run) ;; *) echo 'expected prepare or run' >&2; exit 2 ;; esac
for path in "$source" "$cache" "$output"; do
  case "$path" in /*) ;; *) echo 'all paths must be absolute' >&2; exit 2 ;; esac
  case "$path" in *:*) echo 'colon paths are unsupported by Docker bind syntax' >&2; exit 2 ;; esac
done
[ -f "$source/Cargo.lock" ] && [ -f "$source/scripts/release.mjs" ] || { echo 'expected release source checkout or extracted source archive' >&2; exit 2; }
# Fail before mkdir/mount when a fixture path could expose or overwrite a broad
# home/config/source directory. Resolve existing ancestors, including /tmp aliases.
node --input-type=module - "$source" "$cache" "$output" <<'JS'
import { existsSync, readdirSync, realpathSync } from 'node:fs';
import { homedir } from 'node:os';
import { basename, dirname, join, resolve, sep } from 'node:path';
function canonical(input) {
  let path = resolve(input); const missing = [];
  while (!existsSync(path)) { missing.unshift(basename(path)); path = dirname(path); }
  return join(realpathSync(path), ...missing);
}
const paths = process.argv.slice(2).map(canonical);
// The whole source directory is mounted, not just the builder's allowlist.
// Refuse obvious private root entries, including dangling symlinks by name.
const privateEntry = readdirSync(paths[0]).find(name =>
  name === '.pi' || name === '.tinymem' || name === '.env' || name.startsWith('.env.') ||
  /\.(?:db|sqlite|sqlite3)(?:-(?:wal|shm|journal))?$/i.test(name));
if (privateEntry) throw new Error(`Refusing private source entry ${privateEntry}; extract a clean release-source archive`);
for (const path of paths) {
  if (path === sep || path === canonical(homedir()) || path.split(sep).includes('.pi')) throw new Error('Refusing root/home/.pi fixture path');
}
for (let i = 0; i < paths.length; i++) for (let j = i + 1; j < paths.length; j++) {
  if (paths[i] === paths[j] || paths[i].startsWith(paths[j] + sep) || paths[j].startsWith(paths[i] + sep)) throw new Error('Source/cache/output paths must not overlap');
}
JS
image=${TINYMEM_QUALIFIER_IMAGE:-tinymem-beta-qualifier:local}
mkdir -p "$cache" "$output"
if [ "$phase" = prepare ]; then
  # Network is confined to this explicit dependency preparation phase.
  docker build --platform linux/amd64 -t "$image" -f "$source/scripts/fixtures/linux-beta/Dockerfile" "$source/scripts/fixtures/linux-beta"
  docker run --rm --platform linux/amd64 -e CARGO_HOME=/cargo-cache \
    -v "$source:/source:ro" -v "$cache:/cargo-cache" "$image" \
    sh -ec 'cd /source; cargo fetch --locked --target x86_64-unknown-linux-gnu'
  docker image inspect "$image" > "$output/qualifier-image.json"
  exit 0
fi
# A genuine previous native release is optional, never synthesized. Pin its
# bytes before mounting it, and let acceptance record an absent prior as skipped.
set --
if [ -n "${TINYMEM_PREVIOUS_BINARY:-}" ]; then
  node --input-type=module - "$TINYMEM_PREVIOUS_BINARY" "${TINYMEM_PREVIOUS_SHA256:-}" <<'JS'
import { createHash } from 'node:crypto';
import { lstatSync, readFileSync } from 'node:fs';
import { isAbsolute } from 'node:path';
const [path, expected] = process.argv.slice(2);
if (!isAbsolute(path) || path.includes(':') || !lstatSync(path).isFile() || !/^[a-f0-9]{64}$/.test(expected)) throw new Error('Prior binary needs an absolute regular file path and TINYMEM_PREVIOUS_SHA256');
if (createHash('sha256').update(readFileSync(path)).digest('hex') !== expected) throw new Error('Prior binary checksum mismatch');
JS
  set -- -v "$TINYMEM_PREVIOUS_BINARY:/fixtures/previous-tinymem:ro" \
    -e TINYMEM_PREVIOUS_BINARY=/fixtures/previous-tinymem \
    -e "TINYMEM_PREVIOUS_SHA256=$TINYMEM_PREVIOUS_SHA256"
elif [ -n "${TINYMEM_PREVIOUS_SHA256:-}" ]; then
  echo 'TINYMEM_PREVIOUS_SHA256 requires TINYMEM_PREVIOUS_BINARY' >&2; exit 2
fi
# No network even for dependency fetching or Pi startup. Never mount the user's
# HOME or .pi. The preflight requires a clean source tree; the builder copies
# only its allowlist from that read-only mount into private build directories.
docker run --rm --platform linux/amd64 --network none "$@" \
  -e CARGO_HOME=/work/cargo-cache -e SOURCE_DATE_EPOCH=0 \
  -v "$source:/source:ro" -v "$cache:/prepared-cargo-cache:ro" -v "$output:/output" "$image" \
  sh -ec '
    mkdir -p /work/home /work/cargo-cache
    cp -R /prepared-cargo-cache/. /work/cargo-cache/
    export HOME=/work/home
    node --version; pi --version; rustc --version; cargo --version; cc --version; ldd --version
    node /source/scripts/release.mjs --out /output/build-one --offline
    node /source/scripts/release.mjs --out /output/build-two --offline
    cmp /output/build-one/SHA256SUMS /output/build-two/SHA256SUMS
    TINYMEM_ACCEPT_ARTIFACTS=/output/acceptance node /source/scripts/accept-pi-beta.mjs /output/build-one
  '
