import { readFileSync, realpathSync, statSync } from 'node:fs';
import { basename, isAbsolute, join } from 'node:path';
import { homedir } from 'node:os';
import { Client, type AssembleOptions, type ClientOptions, type ContextSource } from '../../sdks/typescript/index.ts';

export const DEFAULT_BUDGET = 4096;
export const MAX_BUDGET = 262144;
export const MAX_TASK_BYTES = 65536;

export function boundedInteger(value: unknown, fallback: number, maximum: number): number {
  if (value === undefined) return fallback;
  if (typeof value !== 'string' || !/^\d+$/.test(value)) throw new Error('TinyMem option must be a non-negative integer');
  const number = Number(value);
  if (!Number.isSafeInteger(number) || number > maximum) throw new Error('TinyMem option exceeds its limit');
  return number;
}

/** These flags affect request-local automatic assembly, never the native index or
 * explicit /memory recall and repository-search operations. Native assembly also
 * validates bounded glob syntax; invalid settings fail closed, without fallback.
 */
export function automaticContextOptions(sources: unknown, excludes: unknown): AssembleOptions {
  if (sources === undefined) sources = 'memory,repository';
  if (excludes === undefined) excludes = '[]';
  if (typeof sources !== 'string' || typeof excludes !== 'string' || Buffer.byteLength(excludes) > 65536) {
    throw new Error('TinyMem context sources must be comma-separated; exclusions must be a JSON string array');
  }
  const selected = sources === '' ? [] : sources.split(',');
  if (selected.some(source => source !== 'memory' && source !== 'repository') || new Set(selected).size !== selected.length) {
    throw new Error('TinyMem context sources must be unique memory,repository values (empty selects none)');
  }
  let paths: unknown;
  try { paths = JSON.parse(excludes); } catch { throw new Error('TinyMem context exclusions must be a JSON string array'); }
  if (!Array.isArray(paths) || paths.some(path => typeof path !== 'string')) {
    throw new Error('TinyMem context exclusions must be a JSON string array');
  }
  return { sources: selected as ContextSource[], exclude_paths: paths };
}

function absolute(value: string): string {
  const path = value.startsWith('~/') ? join(homedir(), value.slice(2)) : value;
  if (!isAbsolute(path) || path.includes('\0')) throw new Error('TinyMem requires absolute executable, repository and database paths');
  return path;
}

function object(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** Deliberately supports only project-local, native, model-free configurations.
 * No shell, environment expansion, HTTP, wrappers or implicit scope selection.
 */
export function loadClientOptions(cwd: string, server: string, timeoutMs: number): ClientOptions {
  if (!/^[A-Za-z0-9_-]+$/.test(server)) throw new Error('Invalid TinyMem server name');
  const path = join(cwd, '.pi', 'mcp.json');
  const metadata = statSync(path);
  if (!metadata.isFile() || metadata.size > 65536) throw new Error('MCP configuration must be a regular file of at most 64 KiB');
  const config: unknown = JSON.parse(readFileSync(path, 'utf8'));
  const servers = object(config) && object(config.mcpServers) ? config.mcpServers : {};
  const entry = servers[server];
  if (!object(entry) || (entry.enabled !== undefined && entry.enabled !== true) || typeof entry.command !== 'string' || !Array.isArray(entry.args)) {
    throw new Error('Configure an enabled native TinyMem server in project .pi/mcp.json');
  }
  if ('url' in entry || 'env' in entry || 'cwd' in entry || (entry.type !== undefined && entry.type !== 'stdio')) {
    throw new Error('TinyMem automatic recall requires plain native stdio without env/cwd overrides');
  }
  const binary = absolute(entry.command);
  if (!['tinymem', 'tinymem.exe'].includes(basename(binary))) throw new Error('TinyMem automatic recall requires the native tinymem executable');
  const args: unknown[] = entry.args;
  if (args.length < 7 || args.some(arg => typeof arg !== 'string') || args[0] !== '--db' || args[2] !== 'mcp') {
    throw new Error('Expected --db <path> mcp <repository> --user <owner> --project <project>');
  }
  const db = absolute(args[1] as string);
  const repository = realpathSync(absolute(args[3] as string));
  if (repository !== realpathSync(cwd)) throw new Error('TinyMem repository must match the current Pi working directory');
  const options: ClientOptions = { binary, db, repository, userId: '', timeoutMs };
  const seen = new Set<string>();
  for (let i = 4; i < args.length; i += 2) {
    const flag = args[i] as string;
    const value = args[i + 1];
    if (!['--user', '--project', '--agent'].includes(flag) || typeof value !== 'string' || seen.has(flag)) {
      throw new Error('Unsupported or repeated TinyMem option; automatic recall is lexical only');
    }
    seen.add(flag);
    if (flag === '--user') options.userId = value;
    if (flag === '--project') options.projectId = value;
    if (flag === '--agent') options.agentId = value;
  }
  if (!seen.has('--user') || !seen.has('--project')) throw new Error('Explicit TinyMem user and project scopes are required');
  // Reuse the SDK's scope/deadline validation, rather than inventing another contract.
  new Client(options);
  return options;
}
