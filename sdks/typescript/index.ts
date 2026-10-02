import { spawn } from 'node:child_process';

export const PROTOCOL_VERSION = '2025-06-18';
export const DEFAULT_MAX_FRAME_BYTES = 3 * 1024 * 1024;
export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
export type JsonObject = { [key: string]: Json };
export type MemoryKind = 'fact' | 'preference' | 'decision' | 'plan' | 'relationship' | 'architectural_decision' | 'issue_solution';
export interface Scope { user_id: string; agent_id: string | null; project_id: string | null }
export type SourceRole = 'origin' | 'dependency';
export type FreshnessStatus = 'current' | 'stale' | 'unknown';
export type FreshnessReason = 'no_dependencies' | 'dependencies_current' | 'hash_matches' | 'content_changed' | 'file_missing' | 'source_unavailable';
export interface FileSourceInput { kind: 'file'; role: SourceRole; path: string; expected_hash?: string }
export interface FileSourceReference { kind: 'file'; role: SourceRole; repository: string; path: string; content_hash: string; observed_at_ms: number }
export interface FileDependencyCheck { source_index: number; expected_hash: string; observed_hash: string | null; status: FreshnessStatus; reason: FreshnessReason }
export interface Freshness { status: FreshnessStatus; reason: FreshnessReason; checked_at_ms: number | null; checks: FileDependencyCheck[] }
export interface MemoryRecord {
  id: string; scope: Scope; content: string; kind: MemoryKind; source: string | null;
  expires_at_ms: number | null; version: number; created_at_ms: number; updated_at_ms: number;
  source_refs: FileSourceReference[];
}
export interface MemoryEvent {
  event: 'added' | 'updated' | 'deleted'; version: number; at_ms: number; memory: MemoryRecord; freshness: Freshness;
}
export interface AddMemoryOptions { kind?: MemoryKind; source?: string; expires_at_ms?: number; source_refs?: FileSourceInput[] }
export interface VersionOptions { expected_version?: number }
export interface SearchOptions { limit?: number; max_bytes?: number }
export type SearchMode = 'lexical' | 'semantic' | 'hybrid';
export interface MemorySearchOptions extends SearchOptions { mode?: SearchMode; min_similarity?: number }
export interface EmbeddingIndexReport { indexed: number; stale: number; remaining: number; model: string; dimensions: number }
export interface Profile { stable: MemoryRecord[]; dynamic: MemoryRecord[] }
export interface RefreshReport {
  repository: string; indexed: number; unchanged: number; removed: number; skipped: number; indexed_at_ms: number;
}
export interface Symbol { name: string; kind: string; start_line: number; end_line: number }
export interface RepositoryHit {
  path: string; start_line: number; end_line: number; content: string; content_hash: string;
  language: string; symbols: Symbol[]; syntax_errors: boolean; indexed_at_ms: number;
}
export interface RepositorySearchResult { refresh: RefreshReport; hits: RepositoryHit[] }
export interface MemoryProvenance {
  type: 'memory'; id: string; scope: Scope; kind: MemoryKind; source: string | null;
  version: number; created_at_ms: number; updated_at_ms: number; expires_at_ms: number | null;
  source_refs: FileSourceReference[];
}
export interface RepositoryProvenance {
  type: 'repository'; repository: string; path: string; start_line: number; end_line: number;
  content_hash: string; language: string; syntax_errors: boolean; indexed_at_ms: number;
}
export interface ContextEntry {
  category: 'preference' | 'decision' | 'repository' | 'memory'; content: string;
  provenance: (MemoryProvenance | RepositoryProvenance)[];
}
export type ContextSource = 'memory' | 'repository';
export interface AssembleOptions { sources?: ContextSource[]; exclude_paths?: string[] }
export interface AssembledContext {
  prompt: string; entries: ContextEntry[]; token_budget: number; tokens_used: number;
  token_count_method: 'utf8_bytes_upper_bound' | 'custom';
}
export interface ClientOptions {
  userId: string; binary?: string; repository?: string; db?: string; agentId?: string; projectId?: string;
  timeoutMs?: number; closeTimeoutMs?: number; maxFrameBytes?: number; maxOutputBytes?: number; maxStderrBytes?: number;
  embeddingCommand?: string; embeddingModel?: string; embeddingDimensions?: number; embeddingTimeoutMs?: number;
}
export class NativeError extends Error { stderr = ''; returncode: number | null = null; }
export class TransportError extends NativeError {}
export class ProtocolError extends TransportError {}
export class OutputLimitError extends TransportError {}
export class RPCTimeoutError extends TransportError {}
export class RPCError extends NativeError {
  readonly code: number;
  readonly data: Json | undefined;
  constructor(error: JsonObject) {
    super(error.message as string);
    this.code = error.code as number;
    this.data = error.data;
  }
}
export class ToolError extends NativeError {
  readonly result: JsonObject;
  constructor(result: JsonObject) {
    super((result.content as JsonObject[]).filter(item => item.type === 'text').map(item => item.text).join('\n') || 'native tool failed');
    this.result = result;
  }
}
function object(value: unknown): value is JsonObject {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
function integer(value: number, name: string, minimum = 1): number {
  if (!Number.isSafeInteger(value) || value < minimum) throw new RangeError(`${name} must be an integer >= ${minimum}`);
  return value;
}
function duration(value: number, name: string): number {
  integer(value, name);
  if (value > 2 ** 31 - 1) throw new RangeError(`${name} exceeds the Node timer limit`);
  return value;
}
function token(value: string, name: string): string {
  if (typeof value !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9_.:@-]{0,127}$/.test(value)) throw new TypeError(`${name} must be a 1-128 character ASCII identifier token`);
  return value;
}
function result(message: JsonObject): JsonObject {
  if ('error' in message) {
    const error = message.error;
    if (!object(error) || typeof error.code !== 'number' || !Number.isSafeInteger(error.code) || typeof error.message !== 'string') throw new ProtocolError('malformed JSON-RPC error');
    throw new RPCError(error);
  }
  if (!object(message.result)) throw new ProtocolError('MCP result must be an object');
  return message.result;
}
const scopeKeys = new Set(['scope', 'user', 'agent', 'project', 'user_id', 'agent_id', 'project_id', 'userId', 'agentId', 'projectId', 'repository', 'db', 'binary']);

/** Immutable configuration; every operation owns a fresh child through EOF/exit. */
export class Client {
  private readonly args: readonly string[];
  private readonly binary: string;
  private readonly timeoutMs: number;
  private readonly closeTimeoutMs: number;
  private readonly maxFrameBytes: number;
  private readonly maxOutputBytes: number;
  private readonly maxStderrBytes: number;

  constructor(options: ClientOptions) {
    token(options.userId, 'userId');
    this.timeoutMs = duration(options.timeoutMs ?? 10_000, 'timeoutMs');
    this.closeTimeoutMs = duration(options.closeTimeoutMs ?? 500, 'closeTimeoutMs');
    this.maxFrameBytes = integer(options.maxFrameBytes ?? DEFAULT_MAX_FRAME_BYTES, 'maxFrameBytes');
    this.maxOutputBytes = integer(options.maxOutputBytes ?? 4 * 1024 * 1024, 'maxOutputBytes');
    this.maxStderrBytes = integer(options.maxStderrBytes ?? 64 * 1024, 'maxStderrBytes', 0);
    const args: string[] = [];
    if (options.db !== undefined) args.push('--db', options.db);
    args.push('mcp', options.repository ?? '.', '--user', options.userId);
    if (options.agentId !== undefined) args.push('--agent', token(options.agentId, 'agentId'));
    if (options.projectId !== undefined) args.push('--project', token(options.projectId, 'projectId'));
    if ([options.embeddingCommand, options.embeddingModel, options.embeddingDimensions].some(value => value !== undefined)) {
      if (typeof options.embeddingCommand !== 'string' || !options.embeddingCommand || typeof options.embeddingModel !== 'string' || !options.embeddingModel.trim() || options.embeddingModel.includes('\0') || Buffer.byteLength(options.embeddingModel) > 4096) throw new TypeError('embeddingCommand and a nonblank embeddingModel are required together');
      const dimensions = integer(options.embeddingDimensions ?? 0, 'embeddingDimensions');
      if (dimensions > 4096) throw new RangeError('embeddingDimensions exceeds 4096');
      const timeout = integer(options.embeddingTimeoutMs ?? 30000, 'embeddingTimeoutMs');
      if (timeout > 120000) throw new RangeError('embeddingTimeoutMs exceeds 120000');
      args.push('--embedding-command', options.embeddingCommand, '--embedding-model', options.embeddingModel, '--embedding-dimensions', String(dimensions), '--embedding-timeout-ms', String(timeout));
    }
    this.args = Object.freeze(args);
    this.binary = options.binary ?? 'tinymem';
  }

  private encode(message: JsonObject): Buffer {
    const text = JSON.stringify(message, (_key, value: unknown) => {
      if (typeof value === 'number' && !Number.isFinite(value)) throw new TypeError('JSON numbers must be finite');
      return value;
    });
    if (Buffer.byteLength(text, 'utf8') > this.maxFrameBytes) throw new RangeError('request frame exceeds maxFrameBytes');
    return Buffer.from(text + '\n', 'utf8');
  }

  async callTool(name: string, args: JsonObject = {}): Promise<JsonObject> {
    if (typeof name !== 'string' || !name) throw new TypeError('tool name must be nonempty');
    if (!object(args)) throw new TypeError('arguments must be an object');
    if (Object.keys(args).some(key => scopeKeys.has(key))) throw new TypeError('scope and repository are fixed at process startup');
    const initialize = this.encode({ jsonrpc: '2.0', id: 1, method: 'initialize', params: {
      protocolVersion: PROTOCOL_VERSION, capabilities: {}, clientInfo: { name: 'tinymem-native-typescript', version: '0.1.0' },
    } });
    const notification = this.encode({ jsonrpc: '2.0', method: 'notifications/initialized' });
    const tool = this.encode({ jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name, arguments: args } });
    const response = await this.operation(initialize, notification, tool);
    if (typeof response.isError !== 'boolean' || !Array.isArray(response.content)) throw new ProtocolError('malformed MCP tool result');
    for (const item of response.content) {
      if (!object(item) || typeof item.type !== 'string' || (item.type === 'text' && typeof item.text !== 'string')) throw new ProtocolError('malformed MCP content');
    }
    if (response.isError) throw new ToolError(response);
    return this.field(response, 'structuredContent', 'object') as JsonObject;
  }

  private operation(initialize: Buffer, notification: Buffer, tool: Buffer): Promise<JsonObject> {
    return new Promise((resolve, reject) => {
      const child = spawn(this.binary, [...this.args], { stdio: ['pipe', 'pipe', 'pipe'], shell: false });
      let failure: NativeError | undefined;
      let buffer = Buffer.alloc(0);
      let stderr = Buffer.alloc(0);
      let stdoutBytes = 0;
      let stderrBytes = 0;
      let expectedId = 1;
      let toolResult: JsonObject | undefined;
      let processExited = false;
      let termTimer: ReturnType<typeof setTimeout> | undefined;
      let killTimer: ReturnType<typeof setTimeout> | undefined;
      const deadline = setTimeout(() => fail(new RPCTimeoutError(`MCP operation exceeded ${this.timeoutMs}ms deadline`)), this.timeoutMs);
      const terminate = (): void => {
        if (processExited) {
          child.stdin.destroy();
          child.stdout.destroy();
          child.stderr.destroy();
          return;
        }
        if (termTimer) return;
        child.stdin.end();
        termTimer = setTimeout(() => {
          if (processExited) return;
          child.kill('SIGTERM');
          killTimer = setTimeout(() => { if (!processExited) child.kill('SIGKILL'); }, this.closeTimeoutMs);
        }, this.closeTimeoutMs);
      };
      const fail = (error: NativeError): void => {
        failure ??= error;
        terminate();
      };
      const receive = (value: unknown): void => {
        if (!object(value) || value.jsonrpc !== '2.0') throw new ProtocolError('expected JSON-RPC 2.0 object');
        if (!('id' in value) && typeof value.method === 'string' && value.method.startsWith('notifications/')) {
          if ('result' in value || 'error' in value || ('params' in value && !object(value.params))) throw new ProtocolError('malformed server notification');
          return;
        }
        if ((expectedId !== 1 && expectedId !== 2) || value.id !== expectedId || !Number.isSafeInteger(value.id) || 'method' in value) throw new ProtocolError('unexpected or duplicate response id');
        if (('result' in value) === ('error' in value)) throw new ProtocolError('response must contain exactly one result/error');
        const response = result(value);
        if (expectedId === 1) {
          if (response.protocolVersion !== PROTOCOL_VERSION) throw new ProtocolError('server negotiated unsupported MCP version');
          if (!object(response.capabilities) || !object(response.serverInfo) || typeof response.serverInfo.name !== 'string' || typeof response.serverInfo.version !== 'string') throw new ProtocolError('malformed initialize result');
          expectedId = 2;
          child.stdin.write(notification);
          child.stdin.end(tool); // EOF is part of each operation's shutdown contract.
        } else {
          toolResult = response;
          expectedId = 0;
        }
      };
      child.stdout.on('data', (chunk: Buffer) => {
        if (failure) return; // Continue draining, but never retain more failed output.
        try {
          stdoutBytes += chunk.length;
          if (stdoutBytes > this.maxOutputBytes) throw new OutputLimitError('stdout exceeds maxOutputBytes');
          let start = 0;
          while (start < chunk.length) {
            const newline = chunk.indexOf(10, start);
            const end = newline === -1 ? chunk.length : newline;
            const part = chunk.subarray(start, end);
            if (buffer.length + part.length > this.maxFrameBytes) throw new OutputLimitError('stdout frame exceeds maxFrameBytes');
            buffer = Buffer.concat([buffer, part]);
            if (newline === -1) break;
            const frame = buffer;
            buffer = Buffer.alloc(0);
            receive(JSON.parse(new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(frame)));
            start = newline + 1;
          }
        } catch (error) {
          fail(error instanceof NativeError ? error : new ProtocolError(`invalid stdout: ${String(error)}`));
        }
      });
      child.stderr.on('data', (chunk: Buffer) => {
        stderrBytes += chunk.length;
        const room = this.maxStderrBytes - stderr.length;
        if (room > 0) stderr = Buffer.concat([stderr, chunk.subarray(0, room)]);
        if (stderrBytes > this.maxStderrBytes) fail(new OutputLimitError('stderr exceeds maxStderrBytes'));
      });
      child.stdout.on('end', () => { if (buffer.length) fail(new ProtocolError('stdout ended with incomplete JSON frame')); });
      for (const [name, stream] of [['stdin', child.stdin], ['stdout', child.stdout], ['stderr', child.stderr]] as const) {
        stream.on('error', error => fail(new TransportError(`${name}: ${error.message}`)));
      }
      child.once('error', error => fail(new TransportError(`cannot start tinymem: ${error.message}`)));
      child.once('exit', () => { processExited = true; });
      child.once('close', (code, signal) => {
        clearTimeout(deadline);
        clearTimeout(termTimer);
        clearTimeout(killTimer);
        child.stdin.destroy();
        child.stdout.destroy();
        child.stderr.destroy();
        if (!failure && code !== 0) failure = new TransportError(`tinymem exited (code ${code}, signal ${signal})`);
        if (!failure && !toolResult) failure = new TransportError('server exited without the tool response');
        if (failure) {
          failure.stderr = stderr.toString('utf8');
          failure.returncode = code;
          if (failure.stderr) failure.message += `\nstderr: ${failure.stderr}`;
          reject(failure);
        } else resolve(toolResult!);
      });
      child.stdin.write(initialize);
    });
  }

  private field(response: JsonObject, key: string, kind: 'object' | 'array' | 'boolean', nullable = false): Json {
    const value = response[key];
    if (!(key in response) || !((nullable && value === null) || (kind === 'array' ? Array.isArray(value) : kind === 'object' ? object(value) : typeof value === 'boolean'))) throw new ProtocolError(`malformed tool result: expected ${key}`);
    return value;
  }

  async addMemory(content: string, options: AddMemoryOptions = {}): Promise<MemoryRecord | null> {
    const args: JsonObject = { content };
    for (const key of ['kind', 'source', 'expires_at_ms'] as const) if (options[key] !== undefined) args[key] = options[key]!;
    if (options.source_refs !== undefined) args.source_refs = options.source_refs.map(reference => ({ ...reference }));
    return this.field(await this.callTool('add_memory', args), 'memory', 'object', true) as unknown as MemoryRecord | null;
  }
  async getMemory(id: string): Promise<MemoryRecord | null> {
    return this.field(await this.callTool('get_memory', { id: token(id, 'id') }), 'memory', 'object', true) as unknown as MemoryRecord | null;
  }
  async searchMemories(query: string, options: MemorySearchOptions = {}): Promise<MemoryRecord[]> {
    const args: JsonObject = { query, ...this.budget(options) };
    if (options.mode !== undefined) args.mode = options.mode;
    if (options.min_similarity !== undefined) args.min_similarity = options.min_similarity;
    return this.field(await this.callTool('search_memories', args), 'memories', 'array') as unknown as MemoryRecord[];
  }
  async indexMemories(options: { limit?: number } = {}): Promise<EmbeddingIndexReport> {
    const result = await this.callTool('index_memories', options.limit === undefined ? {} : { limit: options.limit });
    for (const key of ['indexed', 'stale', 'remaining', 'dimensions']) {
      if (typeof result[key] !== 'number' || !Number.isSafeInteger(result[key]) || (result[key] as number) < 0) throw new ProtocolError(`malformed tool result: expected ${key}`);
    }
    if (typeof result.model !== 'string') throw new ProtocolError('malformed tool result: expected model');
    return result as unknown as EmbeddingIndexReport;
  }
  async updateMemory(id: string, content: string, options: VersionOptions = {}): Promise<MemoryRecord | null> {
    const args: JsonObject = { id: token(id, 'id'), content };
    if (options.expected_version !== undefined) args.expected_version = options.expected_version;
    return this.field(await this.callTool('update_memory', args), 'memory', 'object', true) as unknown as MemoryRecord | null;
  }
  async deleteMemory(id: string, options: VersionOptions = {}): Promise<boolean> {
    const args: JsonObject = { id: token(id, 'id') };
    if (options.expected_version !== undefined) args.expected_version = options.expected_version;
    return this.field(await this.callTool('delete_memory', args), 'deleted', 'boolean') as boolean;
  }
  async memoryHistory(id: string, options: SearchOptions = {}): Promise<MemoryEvent[]> {
    return this.field(await this.callTool('memory_history', { id: token(id, 'id'), ...this.budget(options) }), 'history', 'array') as unknown as MemoryEvent[];
  }
  async getProfile(options: SearchOptions = {}): Promise<Profile> {
    const response = await this.callTool('get_profile', this.budget(options));
    this.field(response, 'stable', 'array');
    this.field(response, 'dynamic', 'array');
    return response as unknown as Profile;
  }
  /** Pack relevant evidence in the fixed scope. Insert only result.prompt.
   * Budget includes provenance using a conservative UTF-8-byte token bound.
   * Refreshes repository cache, never changes personal memory.
   */
  async assembleContext(query: string, tokenBudget: number, options: AssembleOptions = {}): Promise<AssembledContext> {
    integer(tokenBudget, 'tokenBudget', 0);
    const args: JsonObject = { query, token_budget: tokenBudget };
    if (options.sources !== undefined) {
      if (!Array.isArray(options.sources) || options.sources.length > 2 ||
          options.sources.some(source => source !== 'memory' && source !== 'repository') ||
          new Set(options.sources).size !== options.sources.length) throw new TypeError('sources must be unique memory/repository values');
      args.sources = [...options.sources];
    }
    if (options.exclude_paths !== undefined) {
      if (!Array.isArray(options.exclude_paths) || options.exclude_paths.length > 64 ||
          options.exclude_paths.some(pattern => typeof pattern !== 'string' || !pattern || Buffer.byteLength(pattern) > 1024 ||
            /[\u0000-\u001f\u007f-\u009f\\:]/u.test(pattern) || pattern.split('/').some(part => !part || part === '.' || part === '..')) ||
          options.exclude_paths.reduce((bytes, pattern) => bytes + Buffer.byteLength(pattern), 0) > 16384) {
        throw new TypeError('exclude_paths must contain at most 64 normalized relative globs (1024 bytes each, 16 KiB total)');
      }
      args.exclude_paths = [...options.exclude_paths];
    }
    const response = await this.callTool('assemble_context', args);
    this.field(response, 'entries', 'array');
    if (typeof response.prompt !== 'string' || typeof response.token_count_method !== 'string') throw new ProtocolError('malformed context prompt or count method');
    for (const key of ['token_budget', 'tokens_used']) {
      if (typeof response[key] !== 'number' || !Number.isSafeInteger(response[key]) || (response[key] as number) < 0) throw new ProtocolError(`malformed tool result: expected ${key}`);
    }
    return response as unknown as AssembledContext;
  }
  async refreshRepository(): Promise<RefreshReport> {
    return await this.callTool('refresh_repository') as unknown as RefreshReport;
  }
  async searchRepository(query: string, options: SearchOptions = {}): Promise<RepositorySearchResult> {
    const response = await this.callTool('search_repository', { query, ...this.budget(options) });
    this.field(response, 'refresh', 'object');
    this.field(response, 'hits', 'array');
    return response as unknown as RepositorySearchResult;
  }
  private budget(options: SearchOptions): JsonObject {
    const args: JsonObject = {};
    if (options.limit !== undefined) args.limit = options.limit;
    if (options.max_bytes !== undefined) args.max_bytes = options.max_bytes;
    return args;
  }
}
