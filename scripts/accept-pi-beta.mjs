#!/usr/bin/env node
/** Clean-room acceptance for the Pi-only beta release artifacts.
 * Requires Node 22.19+ and Pi 1.0.0. Uses only local artifacts and a deterministic
 * in-process Pi provider; every Pi process/config/database lives in a temp root.
 */
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import { spawn, spawnSync } from 'node:child_process';
import {
  chmodSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync,
  readdirSync, realpathSync, rmSync, statSync, writeFileSync,
} from 'node:fs';
import { release as osRelease, tmpdir } from 'node:os';
import { dirname, join, resolve, sep } from 'node:path';
import { pathToFileURL } from 'node:url';

const report = {
  started_at: new Date().toISOString(),
  node: process.version,
  platform: `${process.platform}-${process.arch}`,
  os_release: osRelease(),
  steps: [],
  status: 'running',
};
let artifactDir = process.env.TINYMEM_ACCEPT_ARTIFACTS;
let workRoot;
let logSequence = 0;

function logName(label, suffix) {
  const safe = label.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
  return join(artifactDir, 'logs', `${String(++logSequence).padStart(3, '0')}-${safe}.${suffix}`);
}
function record(name, detail = {}) {
  report.steps.push({ name, status: 'passed', ...detail });
  console.log(`PASS ${name}`);
}
function command(label, executable, args, options = {}) {
  const result = spawnSync(executable, args, {
    cwd: options.cwd,
    env: options.env ?? process.env,
    input: options.input,
    encoding: 'utf8',
    timeout: options.timeout ?? 120_000,
    maxBuffer: options.maxBuffer ?? 24 * 1024 * 1024,
    windowsHide: true,
  });
  const stdout = result.stdout ?? '';
  const stderr = result.stderr ?? '';
  if (artifactDir) {
    mkdirSync(join(artifactDir, 'logs'), { recursive: true });
    writeFileSync(logName(label, 'command.json'), JSON.stringify({ executable, args, cwd: options.cwd ?? null, status: result.status, signal: result.signal }, null, 2) + '\n');
    writeFileSync(logName(label, 'stdout.txt'), stdout);
    writeFileSync(logName(label, 'stderr.txt'), stderr);
  }
  if (result.error) throw new Error(`${label}: ${result.error.message}`);
  return { ...result, stdout, stderr };
}
function run(label, executable, args, options = {}) {
  const result = command(label, executable, args, options);
  assert.equal(result.status, 0, `${label} exited ${result.status ?? result.signal}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`);
  return result;
}
function withUmask(mask, callback) {
  const previous = process.umask(mask);
  try { return callback(); } finally { process.umask(previous); }
}
function assertMode(path, expected, label) {
  assert.equal(statSync(path).mode & 0o777, expected, `${label} changed existing config file permissions`);
}
function findFiles(root, predicate) {
  return readdirSync(root, { withFileTypes: true }).flatMap(entry => {
    const path = join(root, entry.name);
    return entry.isDirectory() ? findFiles(path, predicate) : predicate(entry.name, path) ? [path] : [];
  });
}
function sha256(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}
function safeExtract(archive, destination) {
  const listing = run(`list-${basename(archive)}`, 'tar', ['-tzf', archive]).stdout;
  for (const entry of listing.split(/\r?\n/).filter(Boolean)) {
    assert.ok(!entry.startsWith('/') && !/^[A-Za-z]:[\\/]/.test(entry), `absolute archive member: ${entry}`);
    assert.ok(!entry.split(/[\\/]/).includes('..'), `traversal archive member: ${entry}`);
  }
  mkdirSync(destination, { recursive: true });
  run(`extract-${basename(archive)}`, 'tar', ['-xzf', archive, '-C', destination]);
}
function basename(path) { return path.split(/[\\/]/).at(-1); }
function readJson(path) { return JSON.parse(readFileSync(path, 'utf8')); }
function writeJson(path, value) { mkdirSync(dirname(path), { recursive: true }); writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`); }
function ensureDir(path) { mkdirSync(path, { recursive: true }); return path; }
function versionTuple(version) { return version.replace(/^v/, '').split('.').map(Number); }
function versionAtLeast(actual, required) {
  const a = versionTuple(actual); const b = versionTuple(required);
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    if ((a[i] ?? 0) !== (b[i] ?? 0)) return (a[i] ?? 0) > (b[i] ?? 0);
  }
  return true;
}
function toolEnv(base, extra = {}) {
  return { ...base, ...extra, PI_OFFLINE: '1', PI_SKIP_VERSION_CHECK: '1', PI_TELEMETRY: '0' };
}
async function rpcProcess(label, executable, args, request, options = {}) {
  const stdoutPath = logName(label, 'stdout.txt');
  const stderrPath = logName(label, 'stderr.txt');
  const commandPath = logName(label, 'command.json');
  writeFileSync(commandPath, JSON.stringify({ executable, args, cwd: options.cwd ?? null, request }, null, 2) + '\n');
  const timeoutMs = options.timeout ?? 60_000;
  return await new Promise((resolvePromise, rejectPromise) => {
    const child = spawn(executable, args, { cwd: options.cwd, env: options.env ?? process.env, stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
    const records = [];
    let stdout = '';
    let stderr = '';
    let pending = '';
    let response;
    let settled = !options.waitForSettlement;
    let closedInput = false;
    let finished = false;
    const finishInput = () => {
      if (!closedInput && response && settled) { closedInput = true; child.stdin.end(); }
    };
    const timer = setTimeout(() => {
      child.kill('SIGKILL');
      fail(new Error(`${label}: Pi RPC timed out after ${timeoutMs} ms`));
    }, timeoutMs);
    const fail = error => {
      if (finished) return;
      finished = true; clearTimeout(timer);
      try { child.kill('SIGKILL'); } catch { /* process already ended */ }
      writeFileSync(stdoutPath, stdout); writeFileSync(stderrPath, stderr);
      rejectPromise(error);
    };
    child.on('error', fail);
    child.stdout.on('data', chunk => {
      stdout += chunk.toString('utf8');
      if (Buffer.byteLength(stdout) > 24 * 1024 * 1024) return fail(new Error(`${label}: Pi RPC stdout exceeded 24 MiB`));
      pending += chunk.toString('utf8');
      let newline;
      while ((newline = pending.indexOf('\n')) >= 0) {
        const line = pending.slice(0, newline).replace(/\r$/, ''); pending = pending.slice(newline + 1);
        if (!line) continue;
        let record;
        try { record = JSON.parse(line); } catch (error) { return fail(new Error(`${label}: invalid Pi RPC JSONL: ${error}\n${line}`)); }
        records.push(record);
        if (record.type === 'response' && record.id === request.id) response = record;
        if (record.type === 'agent_settled') settled = true;
        // Fire-and-forget extension UI records (notably setStatus) need no reply.
        if (record.type === 'extension_ui_request' && ['select', 'confirm', 'input', 'editor'].includes(record.method)) {
          child.stdin.write(`${JSON.stringify({ type: 'extension_ui_response', id: record.id, cancelled: true })}\n`);
        }
        finishInput();
      }
    });
    child.stderr.on('data', chunk => {
      stderr += chunk.toString('utf8');
      if (Buffer.byteLength(stderr) > 24 * 1024 * 1024) return fail(new Error(`${label}: Pi RPC stderr exceeded 24 MiB`));
    });
    child.on('close', (code, signal) => {
      if (finished) return;
      finished = true; clearTimeout(timer);
      writeFileSync(stdoutPath, stdout); writeFileSync(stderrPath, stderr);
      if (code !== 0) return rejectPromise(new Error(`${label}: Pi exited ${code ?? signal}\n${stdout}\n${stderr}`));
      if (!response?.success) return rejectPromise(new Error(`${label}: missing/failed RPC response\n${stdout}\n${stderr}`));
      if (!settled) return rejectPromise(new Error(`${label}: Pi exited before agent_settled\n${stdout}\n${stderr}`));
      resolvePromise({ records, stdout, stderr, response });
    });
    child.stdin.on('error', error => { if (error.code !== 'EPIPE') fail(error); });
    child.stdin.write(`${JSON.stringify(request)}\n`);
  });
}

async function main() {
  const releaseDir = resolve(process.env.TINYMEM_RELEASE_DIR ?? process.argv[2] ?? '');
  assert.ok(process.env.TINYMEM_RELEASE_DIR || process.argv[2], 'set TINYMEM_RELEASE_DIR or pass the release output directory');
  assert.ok(existsSync(releaseDir), `release directory not found: ${releaseDir}`);
  artifactDir = resolve(artifactDir ?? join(releaseDir, `acceptance-${process.platform}-${process.arch}`));
  mkdirSync(artifactDir, { recursive: true });
  workRoot = mkdtempSync(join(realpathSync(tmpdir()), 'tinymem-pi-beta-'));
  report.release_dir = releaseDir;
  report.artifact_dir = artifactDir;
  report.temporary_root = workRoot;
  report.pi_executable = process.env.TINYMEM_PI_BIN ?? 'pi';
  const agentDir = ensureDir(join(workRoot, 'pi-agent'));
  const home = ensureDir(join(workRoot, 'home'));
  const sessionDir = ensureDir(join(workRoot, 'sessions'));
  const env = toolEnv(process.env, {
    HOME: home,
    USERPROFILE: home,
    PI_CODING_AGENT_DIR: agentDir,
    PI_CODING_AGENT_SESSION_DIR: sessionDir,
    XDG_CONFIG_HOME: ensureDir(join(workRoot, 'xdg-config')),
    XDG_CACHE_HOME: ensureDir(join(workRoot, 'xdg-cache')),
  });

  assert.ok(versionAtLeast(process.versions.node, '22.19.0'), `Node 22.19.0+ required, got ${process.versions.node}`);
  const piVersion = run('pi-version', report.pi_executable, ['--version']).stdout.trim();
  assert.match(piVersion, /\b1\.0\.0\b/, `Pi 1.0.0 required, got ${piVersion}`);
  assert.equal(piVersion, '1.0.0', `This qualification cell is pinned to Pi 1.0.0, got ${piVersion}`);
  report.pi_version = piVersion;
  record('runtime-pins', { node: process.versions.node, pi: piVersion });

  const manifestPath = join(releaseDir, 'release-manifest.json');
  const sumsPath = join(releaseDir, 'SHA256SUMS');
  assert.ok(existsSync(manifestPath), 'release-manifest.json is required');
  assert.ok(existsSync(sumsPath), 'SHA256SUMS is required');
  const manifest = readJson(manifestPath);
  report.release_manifest = manifest;
  const files = readdirSync(releaseDir);
  const arch = process.arch === 'x64' ? 'x64' : process.arch;
  const target = `${process.platform}-${arch}`;
  const nativeArchive = files.find(name => name.startsWith('tinymem-native-') && name.includes(target) && name.endsWith('.tar.gz'));
  const packageArchive = files.find(name => name.startsWith('tinymem-pi-') && name.includes(target) && name.endsWith('.tgz'));
  const sourceArchive = files.find(name => name.startsWith('tinymem-source-') && name.endsWith('.tar.gz'));
  assert.ok(nativeArchive, `no native archive for ${target} in ${releaseDir}`);
  assert.ok(packageArchive, `no npm Pi package tarball for ${target} in ${releaseDir}`);
  assert.ok(sourceArchive, 'source archive is required');
  const checksumLines = readFileSync(sumsPath, 'utf8').split(/\r?\n/).filter(Boolean);
  const checksums = new Map(checksumLines.map(line => {
    const match = line.match(/^([a-f\d]{64})\s+[* ]?(.+)$/i);
    assert.ok(match, `malformed SHA256SUMS line: ${line}`);
    return [match[2].replace(/^\*/, ''), match[1].toLowerCase()];
  }));
  assert.ok(!checksums.has('SHA256SUMS'), 'SHA256SUMS must not claim to hash itself');
  for (const [name, expected] of checksums) {
    assert.equal(name, basename(name), `checksum entry must be one release-directory filename: ${name}`);
    const path = join(releaseDir, name);
    assert.ok(existsSync(path), `checksummed release file missing: ${name}`);
    assert.equal(sha256(path), expected, `checksum mismatch for ${name}`);
  }
  for (const archiveName of [nativeArchive, packageArchive, sourceArchive]) {
    const actual = sha256(join(releaseDir, archiveName));
    assert.equal(actual, checksums.get(archiveName), `checksum missing for ${archiveName}`);
    report[`sha256_${archiveName}`] = actual;
  }
  record('release-artifacts-and-checksums', { target, native_archive: nativeArchive, package_archive: packageArchive, source_archive: sourceArchive });

  const nativeExtract = join(workRoot, 'native-extract');
  const packageExtract = join(workRoot, 'package-extract');
  const sourceExtract = join(workRoot, 'source-extract');
  safeExtract(join(releaseDir, nativeArchive), nativeExtract);
  safeExtract(join(releaseDir, packageArchive), packageExtract);
  safeExtract(join(releaseDir, sourceArchive), sourceExtract);
  const binaries = findFiles(nativeExtract, name => name === (process.platform === 'win32' ? 'tinymem.exe' : 'tinymem'));
  assert.equal(binaries.length, 1, `expected exactly one engine in native archive, found ${binaries.length}`);
  const binary = realpathSync(binaries[0]);
  if (process.platform !== 'win32') chmodSync(binary, statSync(binary).mode | 0o111);
  const engineVersion = run('engine-version', binary, ['--version']).stdout.trim();
  report.engine_version = engineVersion;
  const packageRoots = findFiles(packageExtract, name => name === 'package.json').map(dirname).filter(path => {
    try { return readJson(join(path, 'package.json')).name === 'tinymem-pi'; } catch { return false; }
  });
  assert.equal(packageRoots.length, 1, `expected one tinymem-pi package root, found ${packageRoots.length}`);
  const packageRoot = realpathSync(packageRoots[0]);
  const packageJson = readJson(join(packageRoot, 'package.json'));
  report.package_name = packageJson.name;
  report.package_version = packageJson.version;
  assert.ok(existsSync(join(packageRoot, 'integrations/pi/index.ts')), 'Pi extension entry missing from npm tarball');
  assert.ok(existsSync(join(packageRoot, 'sdks/typescript/index.ts')), 'native SDK source missing from npm tarball');
  assert.ok(existsSync(join(packageRoot, 'scripts/pi-package.mjs')), 'project installer missing from npm tarball');
  const packageManifest = readJson(join(packageRoot, 'release-manifest.json'));
  assert.equal(packageManifest.version, packageJson.version);
  assert.equal(packageManifest.platform, process.platform);
  assert.equal(packageManifest.arch, arch);
  for (const [path, digest] of Object.entries(packageManifest.files ?? {})) assert.equal(sha256(join(packageRoot, path)), digest, `package manifest mismatch: ${path}`);
  const nativeManifestPath = findFiles(nativeExtract, name => name === 'release-manifest.json')[0];
  assert.ok(nativeManifestPath, 'native archive release manifest missing');
  const nativeManifest = readJson(nativeManifestPath);
  assert.equal(nativeManifest.files?.['bin/tinymem'], sha256(binary));
  assert.equal(manifest.binarySha256, sha256(binary));
  for (const [name, metadata] of Object.entries(manifest.artifacts ?? {})) {
    assert.equal(sha256(join(releaseDir, name)), metadata.sha256, `release manifest mismatch: ${name}`);
    assert.equal(statSync(join(releaseDir, name)).size, metadata.bytes, `release manifest byte count mismatch: ${name}`);
  }
  const sourceFiles = findFiles(sourceExtract, name => name === 'release.mjs');
  assert.ok(sourceFiles.length > 0, 'source archive does not contain release tooling');
  record('extract-native-npm-source-archives', { engine_version: engineVersion, package: `${packageJson.name}@${packageJson.version}` });

  const npmPrefix = ensureDir(join(workRoot, 'npm-prefix'));
  const npmCache = ensureDir(join(workRoot, 'npm-cache'));
  const npmEnv = toolEnv(env, {
    npm_config_cache: npmCache,
    npm_config_userconfig: join(workRoot, 'npmrc'),
    npm_config_update_notifier: 'false',
  });
  run('offline-npm-install-artifact', process.env.NPM_BIN ?? 'npm', [
    'install', '--offline', '--ignore-scripts', '--no-audit', '--no-fund',
    '--prefix', npmPrefix, join(releaseDir, packageArchive),
  ], { env: npmEnv, timeout: 120_000 });
  const npmInstalledRoot = join(npmPrefix, 'node_modules', packageJson.name);
  assert.equal(readJson(join(npmInstalledRoot, 'package.json')).version, packageJson.version);
  for (const [path, digest] of Object.entries(packageManifest.files ?? {})) assert.equal(sha256(join(npmInstalledRoot, path)), digest, `npm-installed package mismatch: ${path}`);
  for (const hostPackage of ['@earendil-works/pi-coding-agent', '@earendil-works/pi-ai', '@earendil-works/pi-agent-core', '@earendil-works/pi-tui', 'typebox']) {
    assert.ok(!existsSync(join(npmPrefix, 'node_modules', hostPackage)), `host peer ${hostPackage} was physically bundled/installed`);
  }
  record('offline-npm-install-of-release-tarball', { npm_package: `${packageJson.name}@${packageJson.version}` });

  const piReal = resolvePiExecutable(report.pi_executable);
  const piRequire = createRequire(piReal);
  const aiPackageJson = piRequire.resolve.paths('@earendil-works/pi-ai')
    .map(directory => join(directory, '@earendil-works/pi-ai/package.json')).find(existsSync);
  assert.ok(aiPackageJson, 'installed Pi must provide @earendil-works/pi-ai');
  const aiEntry = join(dirname(aiPackageJson), 'dist/index.js');
  assert.ok(existsSync(aiEntry), `Pi AI module not found: ${aiEntry}`);
  const piRoot = findPackageRoot(piReal, '@earendil-works/pi-coding-agent');
  const piEntry = process.env.TINYMEM_PI_MODULE ?? join(piRoot, 'dist/index.js');
  assert.ok(existsSync(piEntry), `Pi module entry not found: ${piEntry}`);
  const fixtureExtension = join(workRoot, 'fixture-provider.mjs');
  writeFileSync(fixtureExtension, fixtureProviderSource(pathToFileURL(aiEntry).href));

  const root = ensureDir(join(workRoot, 'fixture-repository'));
  const piDir = ensureDir(join(root, '.pi'));
  const db = join(workRoot, 'data', 'memory.db');
  const backup = join(workRoot, 'backup', 'memory.backup.db');
  const userId = 'beta-fixture-user';
  const projectId = 'beta-fixture-project';
  const agentId = 'beta-fixture-agent';
  const settingsPath = join(piDir, 'settings.json');
  const mcpPath = join(piDir, 'mcp.json');
  const receiptPath = join(piDir, 'tinymem-install.json');
  const unrelatedSettings = { defaultTools: ['read'], enableAnalytics: false, quietStartup: true };
  const unrelatedServer = { command: process.execPath, args: ['-e', 'process.exit(0)'], enabled: false };
  writeJson(settingsPath, unrelatedSettings);
  writeJson(mcpPath, { mcpServers: { 'unrelated-disabled': unrelatedServer }, unrelatedTopLevel: { keep: 'yes' } });
  chmodSync(settingsPath, 0o660);
  chmodSync(mcpPath, 0o660);

  const priorPackageRoot = join(workRoot, 'prior-install-package');
  cpSync(packageRoot, priorPackageRoot, { recursive: true, dereference: false });
  const helper = join(packageRoot, 'scripts/pi-package.mjs');
  const priorHelper = join(priorPackageRoot, 'scripts/pi-package.mjs');
  const installArgs = ['install', '--root', root, '--db', db, '--user', userId, '--project', projectId, '--agent', agentId];
  withUmask(0o077, () => run('owned-project-install', process.execPath, [priorHelper, ...installArgs], { cwd: root, env }));
  assertMode(settingsPath, 0o660, 'install/settings.json');
  assertMode(mcpPath, 0o660, 'install/mcp.json');
  assert.ok(existsSync(receiptPath), 'installer ownership receipt missing');
  assert.ok(!existsSync(db), 'installation must not open/create the memory database');
  const installedSettings = readJson(settingsPath);
  assert.equal(installedSettings.defaultTools.join(','), unrelatedSettings.defaultTools.join(','));
  assert.equal(installedSettings.enableAnalytics, false);
  assert.equal(installedSettings.quietStartup, true);
  const configured = readJson(mcpPath);
  assert.deepEqual(configured.mcpServers['unrelated-disabled'], unrelatedServer);
  assert.deepEqual(configured.unrelatedTopLevel, { keep: 'yes' });
  const serverEntry = configured.mcpServers.tinymem;
  assert.ok(serverEntry, 'installer did not configure native TinyMem MCP server');
  assert.ok(serverEntry.args.includes(db), 'MCP config must point to requested database');
  assert.ok(serverEntry.args.includes(root), 'MCP config must point to current project');
  record('owned-install-preserves-unrelated-config-permissions-and-does-not-open-db', { preserved_config_mode_under_umask_077: '0660' });

  const originalMcp = readJson(mcpPath);
  const originalReceipt = readJson(receiptPath);
  const driftedMcp = structuredClone(originalMcp);
  driftedMcp.mcpServers.tinymem.args[1] = join(workRoot, 'unexpected.db');
  writeJson(mcpPath, driftedMcp);
  const refusedUpgrade = command('owned-upgrade-refuses-drift', process.execPath,
    [helper, 'install', '--root', root, '--db', db, '--user', userId, '--project', projectId, '--agent', agentId, '--upgrade'], { cwd: root, env });
  assert.notEqual(refusedUpgrade.status, 0, 'upgrade overwrote a user-modified owned MCP entry');
  assert.deepEqual(readJson(mcpPath), driftedMcp);
  assert.deepEqual(readJson(settingsPath), installedSettings);
  assert.deepEqual(readJson(receiptPath), originalReceipt);
  assert.ok(!existsSync(db), 'refused upgrade unexpectedly opened the database');
  writeJson(mcpPath, originalMcp);

  const tamperedRoot = join(workRoot, 'tampered-package');
  cpSync(packageRoot, tamperedRoot, { recursive: true, dereference: false });
  writeFileSync(join(tamperedRoot, 'integrations/pi/index.ts'), `${readFileSync(join(tamperedRoot, 'integrations/pi/index.ts'), 'utf8')}\\n// tampered\\n`);
  const tamperedProject = ensureDir(join(workRoot, 'tampered-project'));
  const tamperedPiDir = ensureDir(join(tamperedProject, '.pi'));
  const tamperedSettingsPath = join(tamperedPiDir, 'settings.json');
  const tamperedMcpPath = join(tamperedPiDir, 'mcp.json');
  writeJson(tamperedSettingsPath, { defaultTools: ['read'], enableAnalytics: false });
  writeJson(tamperedMcpPath, { mcpServers: { 'unrelated-disabled': unrelatedServer } });
  const beforeTamperedSettings = readFileSync(tamperedSettingsPath);
  const beforeTamperedMcp = readFileSync(tamperedMcpPath);
  const tampered = command('tampered-package-refused', process.execPath,
    [join(tamperedRoot, 'scripts/pi-package.mjs'), 'install', '--root', tamperedProject, '--db', join(workRoot, 'tampered.db'), '--user', userId, '--project', projectId], { cwd: tamperedProject, env });
  assert.notEqual(tampered.status, 0, 'installer accepted a package that failed its file manifest');
  assert.deepEqual(readFileSync(tamperedSettingsPath), beforeTamperedSettings);
  assert.deepEqual(readFileSync(tamperedMcpPath), beforeTamperedMcp);
  assert.ok(!existsSync(join(tamperedPiDir, 'tinymem-install.json')));
  record('installer-refuses-config-drift-and-package-tampering-without-writes');

  const piCwd = root;
  const rpcBase = ['--mode', 'rpc', '--no-session', '--approve'];
  async function piRpc(label, commandObject, { model = false, traceName = `${label}.jsonl` } = {}) {
    const trace = join(workRoot, traceName);
    rmSync(trace, { force: true });
    const args = [...rpcBase, '--no-skills', '--no-prompt-templates', '--no-themes', '--no-context-files', '--extension', fixtureExtension];
    if (model) args.push('--provider', 'tinymem-fixture', '--model', 'tinymem-fixture/deterministic');
    const processResult = await rpcProcess(label, report.pi_executable, args, commandObject, {
      cwd: piCwd,
      env: toolEnv(env, { TINYMEM_FIXTURE_TRACE: trace }),
      waitForSettlement: model,
      timeout: 60_000,
    });
    return { ...processResult, trace };
  }
  let commandId = 0;
  async function slash(commandText) {
    const current = ++commandId;
    const result = await piRpc(`pi-${String(current).padStart(2, '0')}-${commandText.split(/\s+/)[0].slice(1)}`, {
      id: `command-${current}`, type: 'prompt', message: commandText,
    });
    assert.equal(result.response.data?.disposition, 'handled', `${commandText} was not consumed as a Pi extension command`);
    assert.ok(!existsSync(result.trace), `${commandText} unexpectedly made a model request`);
    return result;
  }
  async function prompt(label, text) {
    const result = await piRpc(label, { id: `prompt-${++commandId}`, type: 'prompt', message: text }, { model: true, traceName: `${label}.jsonl` });
    assert.equal(result.response.data?.disposition, 'started');
    assert.ok(existsSync(result.trace), `${label} did not reach the offline fixture provider`);
    const requests = readFileSync(result.trace, 'utf8').trim().split(/\r?\n/).filter(Boolean).map(JSON.parse);
    assert.ok(requests.length >= 1, `${label} did not record a provider request`);
    const request = requests.at(-1);
    const evidence = request.filter(message => {
      const text = typeof message.content === 'string' ? message.content : Array.isArray(message.content)
        ? message.content.filter(block => block.type === 'text').map(block => block.text).join('\n') : '';
      return text.startsWith('Task context:');
    });
    return { ...result, request, evidence };
  }

  // This shell command is real Pi's MCP client, not a test double. It verifies the
  // unpacked binary starts, negotiates MCP and discovers every advertised tool.
  writeJson(join(agentDir, 'mcp.json'), { mcpServers: { tinymem: serverEntry } });
  const mcpList = run('pi-mcp-list', report.pi_executable, ['mcp', 'list', '--json'], { cwd: root, env, timeout: 60_000 });
  for (const name of ['add_memory', 'search_memories', 'get_memory', 'update_memory', 'delete_memory', 'memory_history', 'get_profile', 'assemble_context', 'refresh_repository', 'search_repository', 'index_memories']) {
    assert.ok(mcpList.stdout.includes(name), `Pi MCP list did not report ${name}\n${mcpList.stdout}\n${mcpList.stderr}`);
  }
  record('real-pi-mcp-discovery', { tool_count: 11 });

  await slash('/memory remember beta acceptance marker prefers UTC clock notation; first phrasing only.');
  const { Client } = await import(pathToFileURL(join(packageRoot, 'sdks/typescript/index.ts')).href);
  const clientOptions = { binary, db, repository: root, userId, projectId, agentId, timeoutMs: 10_000 };
  const native = new Client(clientOptions);
  const initial = (await native.searchMemories('beta acceptance marker'))[0];
  assert.ok(initial, 'intentional /memory remember did not persist');
  assert.equal(initial.version, 1);
  assert.deepEqual(initial.scope, { user_id: userId, agent_id: agentId, project_id: projectId });
  assert.equal(initial.source, 'explicit Pi /memory remember');
  const isolated = new Client({ ...clientOptions, userId: 'beta-other-user' });
  assert.deepEqual(await isolated.searchMemories('beta acceptance marker'), [], 'memory escaped exact user scope');
  const projectIsolated = new Client({ ...clientOptions, projectId: 'beta-other-project' });
  assert.deepEqual(await projectIsolated.searchMemories('beta acceptance marker'), [], 'memory escaped exact project scope');
  const agentIsolated = new Client({ ...clientOptions, agentId: 'beta-other-agent' });
  assert.deepEqual(await agentIsolated.searchMemories('beta acceptance marker'), [], 'memory escaped exact agent scope');
  const assembled = await native.assembleContext('beta acceptance marker UTC clock', 4096, { sources: ['memory'] });
  assert.ok(assembled.prompt.includes('UTC clock notation'));
  assert.ok(assembled.tokens_used <= 4096);
  assert.equal(assembled.tokens_used, Buffer.byteLength(assembled.prompt));
  assert.equal(assembled.token_count_method, 'utf8_bytes_upper_bound');
  assert.ok(assembled.entries.some(entry => entry.provenance.some(ref => ref.type === 'memory' && ref.id === initial.id && ref.scope.user_id === userId)));
  const zeroBudget = await native.assembleContext('beta acceptance marker UTC clock', 0, { sources: ['memory'] });
  assert.deepEqual(zeroBudget.entries, []);
  assert.equal(zeroBudget.tokens_used, 0);
  record('exact-scope-provenance-and-budget');

  const recalled = await prompt('new-process-recall', 'Implement the beta acceptance marker UTC clock change.');
  assert.equal(recalled.evidence.length, 1, 'recall should inject exactly one request-local evidence message');
  const recalledText = recalled.evidence.map(message => typeof message.content === 'string' ? message.content : JSON.stringify(message.content)).join('\n');
  assert.ok(recalledText.includes('UTC clock notation'));
  assert.ok(Buffer.byteLength(recalledText) <= 4096, 'Pi request context exceeded its default byte budget');
  const afterRecall = await native.getMemory(initial.id);
  assert.equal(afterRecall.version, 1, 'automatic recall mutated memory version');
  assert.equal((await native.memoryHistory(initial.id)).length, 1, 'automatic recall created observations');
  record('new-pi-process-recall-is-request-local-and-read-only');

  await slash(`/memory correct ${initial.id} 1 beta acceptance marker now requires ISO UTC timestamp notation.`);
  const corrected = await native.getMemory(initial.id);
  assert.equal(corrected.version, 2);
  const correctedRun = await prompt('new-process-corrected-recall', 'Implement the beta acceptance marker UTC clock change.');
  const correctedText = JSON.stringify(correctedRun.evidence);
  assert.ok(correctedText.includes('ISO UTC timestamp notation'));
  assert.ok(!correctedText.includes('first phrasing only'));
  assert.equal((await native.memoryHistory(initial.id)).length, 2);
  record('explicit-correction-visible-after-process-restart');

  const dependency = join(root, '.pi', 'fixture-dependency.txt');
  writeFileSync(dependency, 'the beta fixture dependency says use monotonic UTC clocks\n');
  const dependent = await native.addMemory('beta freshness marker depends on monotonic UTC clock input.', {
    kind: 'decision', source: 'acceptance-fixture',
    source_refs: [{ kind: 'file', role: 'dependency', path: '.pi/fixture-dependency.txt' }],
  });
  assert.ok(dependent.source_refs.length === 1 && dependent.source_refs[0].role === 'dependency');
  assert.equal(dependent.source_refs[0].repository, realpathSync(root));
  const freshnessTask = 'Implement monotonic dependency behavior.';
  const freshRun = await prompt('source-current-recall', freshnessTask);
  assert.equal(freshRun.evidence.length, 1);
  assert.ok(JSON.stringify(freshRun.evidence).includes('beta freshness marker'));
  writeFileSync(dependency, 'the beta fixture dependency now says use local time\n');
  const staleRun = await prompt('source-changed-recall', freshnessTask);
  assert.equal(staleRun.evidence.length, 0, 'changed dependency remained in current request context');
  const history = await native.memoryHistory(dependent.id);
  assert.equal(history.length, 1, 'freshness checks must not append an audit observation');
  assert.equal(history[0].freshness.status, 'stale');
  assert.equal(history[0].freshness.reason, 'content_changed');
  record('file-provenance-invalidates-on-change-without-history-mutation');

  ensureDir(dirname(backup));
  run('database-backup', binary, ['--db', db, 'backup', '--output', backup]);
  assert.ok(existsSync(backup), 'consistent backup output missing');
  const restoredDb = join(ensureDir(join(workRoot, 'restore')), 'memory.db');
  run('database-restore', binary, ['--db', restoredDb, 'restore', '--input', backup]);
  const restored = new Client({ ...clientOptions, db: restoredDb });
  const restoredMemory = await restored.getMemory(initial.id);
  assert.equal(restoredMemory.version, 2);
  assert.equal(restoredMemory.content, corrected.content);
  assert.equal((await restored.memoryHistory(dependent.id))[0].freshness.status, 'stale');
  record('database-backup-restore-retains-memory-history-and-provenance');

  const beforeUpgradeMcp = readJson(mcpPath);
  const beforeUpgradeSettings = readJson(settingsPath);
  withUmask(0o077, () => run('owned-project-upgrade', process.execPath, [helper, 'install', '--root', root, '--db', db, '--user', userId, '--project', projectId, '--agent', agentId, '--upgrade'], { cwd: root, env }));
  assertMode(settingsPath, 0o660, 'upgrade/settings.json');
  assertMode(mcpPath, 0o660, 'upgrade/mcp.json');
  const afterUpgradeMcp = readJson(mcpPath);
  assert.deepEqual(afterUpgradeMcp.mcpServers['unrelated-disabled'], beforeUpgradeMcp.mcpServers['unrelated-disabled']);
  assert.deepEqual(afterUpgradeMcp.unrelatedTopLevel, beforeUpgradeMcp.unrelatedTopLevel);
  assert.deepEqual(afterUpgradeMcp.mcpServers.tinymem.args, beforeUpgradeMcp.mcpServers.tinymem.args, 'upgrade changed database or exact user/project/agent scope');
  assert.equal(afterUpgradeMcp.mcpServers.tinymem.command, join(packageRoot, 'native/tinymem'), 'upgrade did not move MCP entry to the new package root');
  const afterUpgradeSettings = readJson(settingsPath);
  assert.deepEqual({ defaultTools: afterUpgradeSettings.defaultTools, enableAnalytics: afterUpgradeSettings.enableAnalytics, quietStartup: afterUpgradeSettings.quietStartup }, { defaultTools: beforeUpgradeSettings.defaultTools, enableAnalytics: beforeUpgradeSettings.enableAnalytics, quietStartup: beforeUpgradeSettings.quietStartup }, 'upgrade changed unrelated Pi settings');
  assert.deepEqual(afterUpgradeSettings.packages, [packageRoot], 'upgrade did not replace only the prior TinyMem package source');
  assert.equal((await native.getMemory(initial.id)).version, 2, 'upgrade changed memory database contents');
  assert.ok(existsSync(db));
  record('upgrade-preserves-database-scope-and-unrelated-settings');

  await slash(`/memory delete ${initial.id} 2`);
  assert.equal(await native.getMemory(initial.id), null);
  const deletedHistory = await native.memoryHistory(initial.id);
  assert.deepEqual(deletedHistory.map(event => event.event), ['added', 'updated', 'deleted']);
  const afterDelete = await prompt('new-process-after-delete', 'Implement the beta acceptance marker UTC clock change.');
  assert.equal(afterDelete.evidence.length, 0, 'deleted memory remained in automatic context');
  record('explicit-delete-retains-audit-and-stops-current-recall');

  // Qualify Pi's own project package commands separately from TinyMem's ownership
  // helper, so install/list/discovery/remove are actually exercised as user sees them.
  const piPackageProject = ensureDir(join(workRoot, 'pi-package-cli-project'));
  ensureDir(join(piPackageProject, '.pi'));
  const piPackageSettings = join(piPackageProject, '.pi', 'settings.json');
  const piPackageSentinel = { defaultTools: ['read'], enableAnalytics: false };
  writeJson(piPackageSettings, piPackageSentinel);
  run('pi-package-cli-install', report.pi_executable, ['install', '--local', packageRoot, '--approve'], { cwd: piPackageProject, env, timeout: 120_000 });
  const listed = run('pi-package-cli-list-installed', report.pi_executable, ['list', '--approve'], { cwd: piPackageProject, env });
  assert.ok(listed.stdout.includes(packageJson.name), `Pi list did not show installed package\n${listed.stdout}`);
  const commands = await piRpcForProject('pi-package-command-discovery', piPackageProject, env, fixtureExtension, { type: 'get_commands', id: 'commands' });
  assert.ok(commands.some(command => command.name === 'memory' && command.source === 'extension'), 'installed package did not load the /memory extension through real Pi');
  assert.ok(readJson(piPackageSettings).enableAnalytics === false);
  run('pi-package-cli-remove', report.pi_executable, ['remove', '--local', packageRoot, '--approve'], { cwd: piPackageProject, env, timeout: 120_000 });
  const removedSettings = readJson(piPackageSettings);
  assert.equal(removedSettings.enableAnalytics, false, 'Pi remove damaged unrelated project settings');
  const afterRemove = await piRpcForProject('pi-package-command-removed', piPackageProject, env, fixtureExtension, { type: 'get_commands', id: 'commands-after' });
  assert.ok(!afterRemove.some(command => command.name === 'memory'), 'Pi remove left TinyMem extension loaded');
  record('actual-pi-install-list-load-remove');

  const hostTest = findFiles(sourceExtract, name => name === 'host.test.mjs')
    .find(path => path.includes(`${sep}integrations${sep}pi${sep}tests${sep}`));
  assert.ok(hostTest, 'source archive must contain the real-Pi host acceptance test');
  const sourceRoot = resolve(hostTest, '..', '..', '..', '..');
  const hostEnv = toolEnv(env, {
    TINYMEM_BINARY: binary,
    TINYMEM_PI_MODULE: piEntry,
    TINYMEM_PI_EXTENSION: join(packageRoot, 'integrations/pi/index.ts'),
    TINYMEM_REQUIRE_PI: '1',
  });
  run('packaged-extension-real-pi-host-test', process.execPath,
    ['--experimental-strip-types', '--test', hostTest], { cwd: sourceRoot, env: hostEnv, timeout: 120_000 });
  record('packaged-extension-real-pi-process-runtime-test');

  // Verify data created by the previous native binary. Both schemas are v4,
  // so this is upgrade preservation, not a schema-migration claim.
  if (process.env.TINYMEM_PREVIOUS_BINARY) {
    const previous = resolve(process.env.TINYMEM_PREVIOUS_BINARY);
    assert.ok(existsSync(previous), `previous binary not found: ${previous}`);
    if (process.env.TINYMEM_PREVIOUS_SHA256) assert.equal(sha256(previous), process.env.TINYMEM_PREVIOUS_SHA256.toLowerCase());
    const previousDb = join(workRoot, 'upgrade', 'previous.db');
    const oldClient = new Client({ ...clientOptions, binary: previous, db: previousDb });
    const old = await oldClient.addMemory('prior release schema beta migration marker.');
    assert.ok(old);
    const currentClient = new Client({ ...clientOptions, db: previousDb });
    const retained = await currentClient.getMemory(old.id);
    assert.equal(retained?.content, old.content);
    assert.equal((await currentClient.memoryHistory(old.id)).length, 1);
    record('previous-native-database-upgrade-preservation', { previous_binary_sha256: sha256(previous) });
  } else {
    report.unqualified_checks = [...(report.unqualified_checks ?? []), 'previous-native-database upgrade preservation not run (TINYMEM_PREVIOUS_BINARY unset)'];
  }

  const dbBytesBeforeUninstall = sha256(db);
  const dbSidecarsBeforeUninstall = readdirSync(dirname(db)).filter(name => name.startsWith(basename(db))).sort().map(name => [name, sha256(join(dirname(db), name))]);
  withUmask(0o077, () => run('owned-project-uninstall', process.execPath, [helper, 'uninstall', '--root', root], { cwd: root, env }));
  assertMode(settingsPath, 0o660, 'uninstall/settings.json');
  assertMode(mcpPath, 0o660, 'uninstall/mcp.json');
  assert.ok(existsSync(db), 'uninstall deleted the database');
  assert.equal(sha256(db), dbBytesBeforeUninstall, 'uninstall changed database bytes');
  const dbSidecarsAfterUninstall = readdirSync(dirname(db)).filter(name => name.startsWith(basename(db))).sort().map(name => [name, sha256(join(dirname(db), name))]);
  assert.deepEqual(dbSidecarsAfterUninstall, dbSidecarsBeforeUninstall, 'uninstall changed database sidecars');
  assert.ok(existsSync(packageRoot), 'uninstall removed the separately unpacked package files');
  assert.ok(!existsSync(receiptPath), 'uninstall left an ownership receipt');
  const uninstalledMcp = readJson(mcpPath);
  assert.deepEqual(uninstalledMcp, { mcpServers: { 'unrelated-disabled': unrelatedServer }, unrelatedTopLevel: { keep: 'yes' } });
  const uninstalledSettings = readJson(settingsPath);
  assert.equal(uninstalledSettings.defaultTools.join(','), unrelatedSettings.defaultTools.join(','));
  assert.equal(uninstalledSettings.enableAnalytics, false);
  assert.equal(uninstalledSettings.quietStartup, true);
  assert.deepEqual(await native.memoryHistory(initial.id), deletedHistory, 'uninstall changed retained database history');
  record('owned-uninstall-removes-only-tinymem-config-and-preserves-database');

  report.status = 'passed';
  report.completed_at = new Date().toISOString();
  if (process.env.TINYMEM_ACCEPT_KEEP !== '1') {
    rmSync(workRoot, { recursive: true, force: true });
    report.temporary_root = 'removed';
  }
}

function findPackageRoot(path, expectedName) {
  for (let directory = dirname(path); ; directory = dirname(directory)) {
    const packageJson = join(directory, 'package.json');
    if (existsSync(packageJson)) {
      try { if (readJson(packageJson).name === expectedName) return directory; } catch { /* keep walking */ }
    }
    const parent = dirname(directory);
    if (parent === directory) break;
  }
  throw new Error(`cannot find ${expectedName} package root above ${path}`);
}
function resolvePiExecutable(input) {
  if (input.includes(sep) || input.includes('/')) return realpathSync(input);
  const commandName = process.platform === 'win32' ? 'where' : 'which';
  const found = spawnSync(commandName, [input], { encoding: 'utf8' });
  assert.equal(found.status, 0, `cannot find Pi executable ${input}`);
  return realpathSync(found.stdout.split(/\r?\n/).find(Boolean));
}
function fixtureProviderSource(aiUrl) {
  return `import { appendFileSync } from 'node:fs';\nimport { createAssistantMessageEventStream } from ${JSON.stringify(aiUrl)};\n\n` +
    `export default function fixtureProvider(pi) {\n` +
    `  pi.registerProvider('tinymem-fixture', {\n` +
    `    api: 'tinymem-fixture', baseUrl: 'http://127.0.0.1:1/not-used', apiKey: 'offline-fixture',\n` +
    `    models: [{ id: 'deterministic', name: 'Offline deterministic fixture', reasoning: false, input: ['text'], cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 }, contextWindow: 100000, maxTokens: 128 }],\n` +
    `    streamSimple(model, context) {\n` +
    `      appendFileSync(process.env.TINYMEM_FIXTURE_TRACE, JSON.stringify(context.messages) + '\\n');\n` +
    `      const stream = createAssistantMessageEventStream();\n` +
    `      const message = { role: 'assistant', content: [{ type: 'text', text: 'offline deterministic acceptance response' }], api: model.api, provider: model.provider, model: model.id, stopReason: 'stop', timestamp: Date.now(), usage: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, totalTokens: 0, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } } };\n` +
    `      queueMicrotask(() => { stream.push({ type: 'start', partial: message }); stream.push({ type: 'done', reason: 'stop', message }); stream.end(message); });\n` +
    `      return stream;\n` +
    `    },\n` +
    `  });\n` +
    `}\n`;
}
async function piRpcForProject(label, cwd, env, fixtureExtension, request) {
  const args = ['--mode', 'rpc', '--no-session', '--approve', '--no-skills', '--no-prompt-templates', '--no-themes', '--no-context-files', '--extension', fixtureExtension];
  const result = await rpcProcess(label, process.env.TINYMEM_PI_BIN ?? 'pi', args, request, { cwd, env, timeout: 60_000 });
  return result.response.data?.commands ?? [];
}

main().catch(error => {
  report.status = 'failed';
  report.error = error?.stack ?? String(error);
  report.completed_at = new Date().toISOString();
  if (artifactDir) {
    mkdirSync(artifactDir, { recursive: true });
    writeFileSync(join(artifactDir, 'release-acceptance.json'), `${JSON.stringify(report, null, 2)}\n`);
  }
  console.error(`FAIL ${error?.stack ?? error}`);
  process.exitCode = 1;
}).finally(() => {
  if (artifactDir && report.status !== 'running') {
    mkdirSync(artifactDir, { recursive: true });
    writeFileSync(join(artifactDir, 'release-acceptance.json'), `${JSON.stringify(report, null, 2)}\n`);
  }
  if (workRoot && process.env.TINYMEM_ACCEPT_KEEP !== '1' && report.status === 'failed') {
    rmSync(workRoot, { recursive: true, force: true });
  }
});
