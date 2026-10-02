#!/usr/bin/env node
// Maintainer-only, offline-capable when the locked Cargo cache is populated.
// Consumers need only Pi + Node; the resulting package has no lifecycle hooks.
import { execFileSync, spawnSync } from 'node:child_process';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { homedir, release, tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';
import { absolute, collectFiles, json, noSymlinks, parseOptions, readRegular, sha256 } from './lib/release-common.mjs';

const ROOT = realpathSync(join(dirname(fileURLToPath(import.meta.url)), '..'));
const TARGETS = {
  'aarch64-apple-darwin': { platform: 'darwin', arch: 'arm64' },
  'x86_64-unknown-linux-gnu': { platform: 'linux', arch: 'x64' },
};
const PIN = '1.93.1';
function command(program, args, options = {}) {
  return execFileSync(program, args, { encoding: 'utf8', maxBuffer: 8 * 1024 * 1024, ...options }).trim();
}
function inventory() {
  const roots = ['Cargo.toml', 'Cargo.lock', 'package.json', 'LICENSE', 'README.md', 'ARCHITECTURE.md', 'CONTRIBUTING.md', 'CONTEXT.md', 'QUICKSTART.md', 'sdks/typescript/index.ts'];
  for (const directory of ['crates', 'integrations/pi', 'scripts', 'docs', '.github']) {
    if (!existsSync(join(ROOT, directory))) continue;
    for (const path of collectFiles(join(ROOT, directory))) {
      if (path.split('/').some(part => part.startsWith('.') || part === '__pycache__' || part === 'node_modules' || part === 'beta-runs')) continue;
      if (directory === 'docs' && /(?:CHECKS|REPORT|RESULTS)\.md$/.test(path)) continue;
      if (/\.(rs|toml|ts|mjs|py|sh|md|yml|yaml)$/.test(path) || path.split('/').at(-1) === 'Dockerfile') roots.push(`${directory}/${path}`);
    }
  }
  return [...new Set(roots)].filter(path => existsSync(join(ROOT, path))).sort().map(path => {
    const data = readRegular(join(ROOT, path), false, 16 * 1024 * 1024).data;
    return { path, data, sha256: sha256(data), mode: /\.(sh|mjs)$/.test(path) ? 0o755 : 0o644 };
  });
}
// Minimal POSIX ustar writer: no host tar/npm version, owner IDs, source mtimes,
// absolute paths, PAX metadata, or gzip timestamps can leak into output.
function archive(files, epoch) {
  const chunks = [];
  const text = (buffer, offset, size, value) => { const bytes = Buffer.from(value); if (bytes.length > size) throw new Error(`tar field overflow: ${value}`); bytes.copy(buffer, offset); };
  for (const file of [...files].sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0)) {
    const header = Buffer.alloc(512);
    let name = file.path, prefix = '';
    if (Buffer.byteLength(name) > 100) {
      let split = name.lastIndexOf('/');
      while (split > 0 && (Buffer.byteLength(name.slice(split + 1)) > 100 || Buffer.byteLength(name.slice(0, split)) > 155)) split = name.lastIndexOf('/', split - 1);
      if (split < 1) throw new Error(`tar path too long: ${name}`);
      prefix = name.slice(0, split); name = name.slice(split + 1);
    }
    text(header, 0, 100, name);
    for (const [offset, size, value] of [[100, 8, file.mode ?? 0o644], [108, 8, 0], [116, 8, 0], [124, 12, file.data.length], [136, 12, epoch]]) text(header, offset, size, value.toString(8).padStart(size - 1, '0') + '\0');
    header.fill(32, 148, 156); header[156] = 48;
    text(header, 257, 6, 'ustar\0'); text(header, 263, 2, '00'); text(header, 345, 155, prefix);
    const sum = header.reduce((total, byte) => total + byte, 0);
    text(header, 148, 8, sum.toString(8).padStart(6, '0') + '\0 ');
    chunks.push(header, file.data, Buffer.alloc((512 - file.data.length % 512) % 512));
  }
  chunks.push(Buffer.alloc(1024));
  return gzipSync(Buffer.concat(chunks), { level: 9, mtime: 0 });
}
function main() {
  const args = parseOptions(process.argv.slice(2), ['out', 'target', 'offline'], ['offline']);
  const out = absolute(args.out, '--out'); noSymlinks(out);
  if (existsSync(out) && readdirSync(out).length) throw new Error('--out must be absent or empty (releases are never overwritten)');
  const rustc = command('rustc', ['--version']);
  const cargo = command('cargo', ['--version']);
  if (!rustc.startsWith(`rustc ${PIN} `) || !cargo.startsWith(`cargo ${PIN} `)) throw new Error(`Release requires installed rustc/cargo ${PIN}; no toolchain is downloaded automatically`);
  const rustVerbose = command('rustc', ['-vV']);
  const host = rustVerbose.match(/^host: (.+)$/m)?.[1];
  const target = args.target ?? host;
  if (!TARGETS[target] || target !== host) throw new Error('Only native aarch64-apple-darwin or x86_64-unknown-linux-gnu builders are supported');
  const platform = TARGETS[target];
  if (process.platform !== platform.platform || process.arch !== platform.arch) throw new Error('Node and Rust builder architecture must match');
  const epoch = Number(process.env.SOURCE_DATE_EPOCH ?? '0');
  if (!Number.isSafeInteger(epoch) || epoch < 0 || epoch > 0xffffffff) throw new Error('SOURCE_DATE_EPOCH must be an integer in 0..4294967295');
  const source = inventory();
  const inputs = source.map(({ path, sha256, mode }) => ({ path, sha256, mode }));
  const sourceDigest = sha256(json(inputs));
  const pkg = JSON.parse(source.find(file => file.path === 'package.json').data);
  const version = pkg.version;
  if (!source.find(file => file.path === 'crates/tinymem-engine/Cargo.toml').data.toString().includes(`version = "${version}"`)) throw new Error('Engine and package versions must agree');
  const work = mkdtempSync(join(realpathSync(tmpdir()), 'tinymem-release-'));
  try {
    const snapshot = join(work, 'source');
    for (const file of source) { const path = join(snapshot, file.path); mkdirSync(dirname(path), { recursive: true }); writeFileSync(path, file.data, { mode: file.mode }); }
    const cargoHome = realpathSync(process.env.CARGO_HOME ?? join(homedir(), '.cargo'));
    const rustflags = `--remap-path-prefix=${work}=/tinymem-build --remap-path-prefix=${cargoHome}=/cargo-home`;
    const environment = {
      ...process.env, CARGO_INCREMENTAL: '0', CARGO_TARGET_DIR: join(work, 'target'), SOURCE_DATE_EPOCH: String(epoch),
      RUSTFLAGS: rustflags, CARGO_ENCODED_RUSTFLAGS: '',
      // C dependencies include bundled SQLite and Tree-sitter. Normalize their paths too.
      CFLAGS: `-ffile-prefix-map=${work}=/tinymem-build -ffile-prefix-map=${cargoHome}=/cargo-home`,
      CXXFLAGS: `-ffile-prefix-map=${work}=/tinymem-build -ffile-prefix-map=${cargoHome}=/cargo-home`,
      RUSTUP_AUTO_INSTALL: '0',
    };
    // Empty CARGO_ENCODED_RUSTFLAGS takes precedence over RUSTFLAGS; remove it.
    delete environment.CARGO_ENCODED_RUSTFLAGS;
    const build = spawnSync('cargo', ['build', '--release', '--locked', '-p', 'tinymem-engine', '--target', target, ...(args.offline ? ['--offline'] : [])], { cwd: snapshot, env: environment, stdio: 'inherit' });
    if (build.error) throw build.error;
    if (build.status !== 0) throw new Error(`Cargo release build failed (${build.status})`);
    const binaryPath = join(work, 'target', target, 'release', 'tinymem');
    const binary = readFileSync(binaryPath);
    const engineVersion = command(binaryPath, ['--version']);
    if (engineVersion !== `tinymem ${version}`) throw new Error('Built engine version mismatch');
    const builder = { rustc, cargo, rustVerbose, node: process.version, osRelease: release(), cc: command(process.env.CC ?? 'cc', ['--version']).split('\n')[0], cargoLocked: true, sourceDateEpoch: epoch, cargoIncremental: false, rustPathRemapping: true, cPathRemapping: true };
    const base = { format: 1, version, target, ...platform, engineVersion, sourceDigest, sourceIdentity: 'sha256 of canonical release-input inventory (not a git commit)', builder };
    const sourceManifest = { format: 1, version, sourceDigest, sourceIdentity: base.sourceIdentity, inputs };
    const runtimePaths = ['LICENSE', 'README.md', 'integrations/pi/index.ts', 'integrations/pi/config.ts', 'sdks/typescript/index.ts', 'scripts/pi-package.mjs', 'scripts/lib/release-common.mjs', ...source.filter(file => /^docs\/(PI_LOCAL_SETUP|PI_BETA_RELEASE|BETA_RELEASE|RELEASE)\.md$/.test(file.path)).map(file => file.path)];
    const runtime = runtimePaths.map(path => ({ ...source.find(file => file.path === path) }));
    const packageMetadata = { ...pkg, os: [platform.platform], cpu: [platform.arch], files: ['integrations/pi/index.ts', 'integrations/pi/config.ts', 'sdks/typescript/index.ts', 'scripts/pi-package.mjs', 'scripts/lib/release-common.mjs', 'native/tinymem', 'release-manifest.json', 'docs', 'LICENSE', 'README.md'] };
    // No npm lifecycle hooks or undeclared runtime dependencies in consumer archive.
    delete packageMetadata.scripts;
    runtime.push({ path: 'package.json', data: Buffer.from(json(packageMetadata)), mode: 0o644 }, { path: 'native/tinymem', data: binary, mode: 0o755 });
    const files = Object.fromEntries(runtime.sort((a,b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0).map(file => [file.path, sha256(file.data)]));
    const packageManifest = { ...base, files };
    runtime.push({ path: 'release-manifest.json', data: Buffer.from(json(packageManifest)), mode: 0o644 });
    const label = `${platform.platform}-${platform.arch}`;
    const nativeRoot = `tinymem-native-${version}-${label}`;
    const native = [
      { path: `${nativeRoot}/bin/tinymem`, data: binary, mode: 0o755 },
      { path: `${nativeRoot}/LICENSE`, data: source.find(file => file.path === 'LICENSE').data, mode: 0o644 },
      { path: `${nativeRoot}/release-manifest.json`, data: Buffer.from(json({ ...base, files: { 'bin/tinymem': sha256(binary) } })), mode: 0o644 },
    ];
    const artifacts = new Map([
      [`tinymem-pi-${version}-${label}.tgz`, archive(runtime.map(file => ({ ...file, path: `package/${file.path}` })), epoch)],
      [`${nativeRoot}.tar.gz`, archive(native, epoch)],
      [`tinymem-source-${version}.tar.gz`, archive([...source.map(file => ({ ...file, path: `tinymem-source-${version}/${file.path}` })), { path: `tinymem-source-${version}/source-manifest.json`, data: Buffer.from(json(sourceManifest)), mode: 0o644 }], epoch)],
    ]);
    const manifest = { ...base, inputs, binarySha256: sha256(binary), artifacts: Object.fromEntries([...artifacts].map(([name, data]) => [name, { sha256: sha256(data), bytes: data.length }])) };
    artifacts.set('release-manifest.json', Buffer.from(json(manifest)));
    artifacts.set('SHA256SUMS', Buffer.from([...artifacts].sort(([a],[b]) => a < b ? -1 : 1).map(([name,data]) => `${sha256(data)}  ${name}\n`).join('')));
    mkdirSync(out, { recursive: true, mode: 0o755 });
    for (const [name, data] of artifacts) writeFileSync(join(out, name), data, { flag: 'wx', mode: 0o644 });
    console.log(json({ out, version, target, sourceDigest, binarySha256: manifest.binarySha256, artifacts: [...artifacts.keys()] }).trim());
  } finally { rmSync(work, { recursive: true, force: true }); }
}
try { main(); } catch (error) { console.error(`release: ${error.message}`); process.exitCode = 1; }
