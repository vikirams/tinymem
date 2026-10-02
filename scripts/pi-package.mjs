#!/usr/bin/env node
// Explicit local configuration only. No subprocesses, database I/O, package
// downloads, global settings, implicit identities, or destructive data cleanup.
import { accessSync, constants, closeSync, existsSync, fchmodSync, fsyncSync, linkSync, lstatSync, mkdirSync, openSync, readFileSync, realpathSync, renameSync, rmdirSync, unlinkSync, writeFileSync } from 'node:fs';
import { basename, dirname, isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { isDeepStrictEqual } from 'node:util';
import { absolute, json, noSymlinks, object, parseOptions, readRegular, sha256 } from './lib/release-common.mjs';

const packageRoot = realpathSync(join(dirname(fileURLToPath(import.meta.url)), '..'));
const RECEIPT = 'tinymem-install.json';
function token(value, label) {
  if (typeof value !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9_.:@-]{0,127}$/.test(value)) throw new Error(`${label} requires an explicit 1-128 character ASCII scope token`);
  return value;
}
function document(path) {
  const previous = readRegular(path, true);
  const value = previous ? JSON.parse(previous.data.toString('utf8')) : {};
  if (!object(value)) throw new Error(`Expected JSON object: ${path}`);
  return { path, previous, value };
}
function verifyPackage() {
  const pkg = JSON.parse(readRegular(join(packageRoot, 'package.json')).data);
  const manifest = JSON.parse(readRegular(join(packageRoot, 'release-manifest.json'), false, 1024 * 1024).data);
  if (pkg.name !== 'tinymem-pi' || manifest.version !== pkg.version || !object(manifest.files) || manifest.platform !== process.platform || manifest.arch !== process.arch) throw new Error('Package version/platform mismatch; obtain the correct native release');
  for (const [path, digest] of Object.entries(manifest.files)) {
    if (!/^[A-Za-z0-9_.\/-]+$/.test(path) || path.startsWith('/') || path.split('/').includes('..') || !/^[0-9a-f]{64}$/.test(digest)) throw new Error('Invalid package file manifest');
    if (sha256(readRegular(join(packageRoot, path), false, 128 * 1024 * 1024).data) !== digest) throw new Error(`Package digest mismatch: ${path}`);
  }
  for (const required of ['native/tinymem', 'package.json', 'integrations/pi/index.ts', 'integrations/pi/config.ts', 'sdks/typescript/index.ts', 'scripts/pi-package.mjs', 'scripts/lib/release-common.mjs']) {
    if (!Object.hasOwn(manifest.files, required)) throw new Error(`Package manifest missing ${required}`);
  }
  accessSync(join(packageRoot, 'native', 'tinymem'), constants.X_OK);
  return { pkg, manifest };
}
function sameBytes(path, previous) {
  const current = readRegular(path, true);
  return current === null ? previous === null : previous !== null && current.data.equals(previous.data);
}
function replace(path, data, previous, mode = 0o600) {
  if (!sameBytes(path, previous)) throw new Error(`Configuration changed during operation: ${path}`);
  if (data === null) { if (previous) unlinkSync(path); return; }
  const temp = `${path}.tinymem-${process.pid}-${Math.random().toString(16).slice(2)}.tmp`;
  let fd;
  try {
    fd = openSync(temp, 'wx', previous?.mode ?? mode);
    if (previous) fchmodSync(fd, previous.mode); // open's mode is filtered by the caller's umask.
    writeFileSync(fd, data); fsyncSync(fd); closeSync(fd); fd = undefined;
    if (!sameBytes(path, previous)) throw new Error(`Configuration changed during operation: ${path}`);
    if (previous === null) { linkSync(temp, path); unlinkSync(temp); }
    else renameSync(temp, path);
  } finally {
    if (fd !== undefined) closeSync(fd);
    if (existsSync(temp)) unlinkSync(temp);
  }
}
function commit(changes) {
  const done = [];
  try {
    for (const change of changes) {
      if (change.previous?.data.equals(change.data ?? Buffer.alloc(0))) continue;
      replace(change.path, change.data, change.previous);
      done.push(change);
    }
  } catch (error) {
    const rollbackErrors = [];
    for (const change of done.reverse()) {
      try { replace(change.path, change.previous?.data ?? null, change.data === null ? null : { data: change.data, mode: change.previous?.mode ?? 0o600 }); }
      catch (rollback) { rollbackErrors.push(rollback.message); }
    }
    if (rollbackErrors.length) throw new Error(`${error.message}; rollback incomplete: ${rollbackErrors.join('; ')}. Stop Pi and inspect configuration.`);
    throw error;
  }
}
function source(entry) { return typeof entry === 'string' ? entry : object(entry) ? entry.source : undefined; }
function normalized(name) { return name.replaceAll('-', '_'); }
function foreignTinyMem(sourcePath, configDir) {
  if (typeof sourcePath !== 'string') return false;
  if (/^npm:tinymem-pi(?:@|$)/.test(sourcePath)) return true;
  // Local declarations resolve from their settings file, never process.cwd().
  // Inspect only local manifests; never resolve/download npm, git or URL sources.
  if (!isAbsolute(sourcePath) && !sourcePath.startsWith('.')) return false;
  try { return JSON.parse(readRegular(join(resolve(configDir, sourcePath), 'package.json')).data).name === 'tinymem-pi'; }
  catch { return false; }
}
function run(command, options) {
  const root = absolute(options.root, '--root');
  noSymlinks(root);
  if (realpathSync(root) !== root) throw new Error('--root must be canonical');
  const configDir = join(root, '.pi');
  noSymlinks(configDir);
  const paths = ['settings.json', 'mcp.json', RECEIPT].map(name => join(configDir, name));
  const [settings, mcp, receipt] = paths.map(document);
  const packages = settings.value.packages ?? [];
  const servers = mcp.value.mcpServers ?? {};
  if (!Array.isArray(packages) || !object(servers)) throw new Error('Invalid Pi packages/mcpServers shape');
  let owned = receipt.previous ? receipt.value : null;
  if (owned) {
    if (owned.format !== 1 || owned.root !== root || !object(owned.entry) || typeof owned.package !== 'string' || typeof owned.packageOwned !== 'boolean' || !object(owned.scope)) throw new Error('Invalid TinyMem ownership receipt');
    if (!isDeepStrictEqual(servers.tinymem, owned.entry)) throw new Error('Owned TinyMem MCP configuration was modified; refusing to overwrite/remove it');
    if (packages.filter(entry => source(entry) === owned.package).length !== 1 || !packages.includes(owned.package)) throw new Error('Owned TinyMem package configuration was modified; refusing to overwrite/remove it');
  }
  for (const [name, entry] of Object.entries(servers)) {
    if (owned && name === 'tinymem') continue;
    if (normalized(name) === 'tinymem' || (object(entry) && typeof entry.command === 'string' && ['tinymem', 'tinymem.exe'].includes(basename(entry.command)))) throw new Error(`TinyMem MCP collision: ${name}`);
  }
  let result;
  if (command === 'install') {
    const { pkg, manifest } = verifyPackage();
    const db = absolute(options.db, '--db'); noSymlinks(db);
    if (existsSync(db) && !lstatSync(db).isFile()) throw new Error('--db must name a regular database file, not a directory or special file');
    if (paths.includes(db) || db === configDir || db === root) throw new Error('Database path collides with configuration');
    const scope = { db, user: token(options.user, '--user'), project: token(options.project, '--project'), agent: options.agent === undefined ? null : token(options.agent, '--agent') };
    if (owned && !isDeepStrictEqual(owned.scope, scope)) throw new Error('Upgrade must retain the exact database and user/project/agent scope');
    if (owned && !options.upgrade && (owned.package !== packageRoot || owned.version !== pkg.version || owned.sourceDigest !== manifest.sourceDigest)) throw new Error('Existing installation differs; review it and use --upgrade');
    if (!owned && options.upgrade) throw new Error('--upgrade requires an owned existing installation');
    for (const entry of packages) {
      if (owned && source(entry) === owned.package) continue;
      if (source(entry) === packageRoot && entry === packageRoot) continue;
      if (source(entry) === packageRoot || foreignTinyMem(source(entry), configDir)) throw new Error('TinyMem package collision');
    }
    if (packages.filter(entry => source(entry) === packageRoot).length > 1) throw new Error('Duplicate TinyMem package entry');
    const binary = join(packageRoot, 'native', 'tinymem');
    const args = ['--db', db, 'mcp', root, '--user', scope.user, '--project', scope.project];
    if (scope.agent !== null) args.push('--agent', scope.agent);
    const entry = { command: binary, args, exposure: 'deferred', description: 'Local, project-scoped memory and repository context' };
    const packageOwned = owned ? owned.packageOwned : !packages.includes(packageRoot);
    if (owned && !owned.packageOwned && owned.package !== packageRoot) throw new Error('Pi installed this package separately; update its package declaration explicitly before reconfiguring');
    settings.value.packages = packages.filter(entry => !owned || source(entry) !== owned.package);
    if (!settings.value.packages.includes(packageRoot)) settings.value.packages.push(packageRoot);
    mcp.value.mcpServers = { ...servers, tinymem: entry };
    receipt.value = {
      format: 1, root, package: packageRoot, packageOwned, version: pkg.version, sourceDigest: manifest.sourceDigest,
      scope, entry, originalSettingsAbsent: owned?.originalSettingsAbsent ?? !settings.previous,
      originalMcpAbsent: owned?.originalMcpAbsent ?? !mcp.previous,
      originalPackagesAbsent: owned?.originalPackagesAbsent ?? !Object.hasOwn(settings.previous ? JSON.parse(settings.previous.data) : {}, 'packages'),
      originalServersAbsent: owned?.originalServersAbsent ?? !Object.hasOwn(mcp.previous ? JSON.parse(mcp.previous.data) : {}, 'mcpServers'),
    };
    result = { action: options.upgrade ? 'upgrade' : 'install', root, package: packageRoot, binary, scope, databaseOpened: false };
  } else {
    if (!owned) throw new Error('No owned TinyMem installation; nothing was removed');
    if (owned.packageOwned) {
      settings.value.packages = packages.filter(entry => entry !== owned.package);
      if (!settings.value.packages.length && owned.originalPackagesAbsent) delete settings.value.packages;
    }
    delete servers.tinymem;
    mcp.value.mcpServers = servers;
    if (!Object.keys(servers).length && owned.originalServersAbsent) delete mcp.value.mcpServers;
    receipt.value = null;
    result = { action: 'uninstall', root, databasePreserved: owned.scope.db, packageFilesPreserved: owned.package, packageDeclarationPreserved: !owned.packageOwned, databaseOpened: false };
  }
  const changes = [settings, mcp, receipt].map(doc => ({
    ...doc,
    data: doc.value === null || (command === 'uninstall' && Object.keys(doc.value).length === 0 && (doc === settings ? owned.originalSettingsAbsent : owned.originalMcpAbsent)) ? null : Buffer.from(json(doc.value)),
  }));
  for (const change of changes) if (change.data && change.data.length > 65536) throw new Error('Merged configuration exceeds 64 KiB');
  if (!options['dry-run']) {
    mkdirSync(configDir, { recursive: true, mode: 0o700 }); noSymlinks(configDir);
    const lock = join(configDir, '.tinymem-install.lock');
    try { mkdirSync(lock, { mode: 0o700 }); } catch { throw new Error('TinyMem configuration operation already active (or interrupted); stop Pi and inspect .pi/.tinymem-install.lock'); }
    try { commit(changes); } finally { rmdirSync(lock); }
  }
  console.log(json({ ...result, dryRun: !!options['dry-run'] }).trim());
}
try {
  const [command, ...args] = process.argv.slice(2);
  if (command === '--help' || command === 'help') console.log('Usage: node scripts/pi-package.mjs install --root ABS --db ABS --user TOKEN --project TOKEN [--agent TOKEN] [--upgrade] [--dry-run]\n       node scripts/pi-package.mjs uninstall --root ABS [--dry-run]\nStop Pi before changing configuration. Never removes databases or extracted package files.');
  else if (command === '--version') console.log(JSON.parse(readFileSync(join(packageRoot, 'package.json'))).version);
  else if (command === 'install') run(command, parseOptions(args, ['root', 'db', 'user', 'project', 'agent', 'upgrade', 'dry-run'], ['upgrade', 'dry-run']));
  else if (command === 'uninstall') run(command, parseOptions(args, ['root', 'dry-run'], ['dry-run']));
  else throw new Error('Expected install or uninstall; use --help');
} catch (error) { console.error(`tinymem-pi: ${error.message}`); process.exitCode = 1; }
