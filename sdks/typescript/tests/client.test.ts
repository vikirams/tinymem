import assert from 'node:assert/strict';
import { test } from 'node:test';
import { chmodSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Client, NativeError, OutputLimitError, ProtocolError, RPCError, RPCTimeoutError, ToolError, TransportError } from '../index.ts';
import type { ClientOptions, JsonObject } from '../index.ts';

const binary = fileURLToPath(new URL('./fake-server.mjs', import.meta.url));
chmodSync(binary, 0o755);

test('public wrappers send only the native versioned tool arguments', async () => {
  const client = new Client({ binary, userId: 'wrappers' });
  const added = await client.addMemory('tea', { kind: 'preference', source: 'explicit', expires_at_ms: 123 });
  assert.ok(added);
  assert.equal(added.source, 'add_memory');
  assert.deepEqual(JSON.parse(added.content), { content: 'tea', kind: 'preference', source: 'explicit', expires_at_ms: 123 });
  const refs = [{ kind: 'file', role: 'dependency', path: 'src/clock.rs', expected_hash: 'blake3:fixture' }] as const;
  const referenced = await client.addMemory('clock', { source_refs: [...refs] });
  assert.deepEqual(JSON.parse(referenced!.content), { content: 'clock', source_refs: refs });
  const updated = await client.updateMemory('fixture-id', 'coffee', { expected_version: 1 });
  assert.ok(updated);
  assert.deepEqual(JSON.parse(updated.content), { id: 'fixture-id', content: 'coffee', expected_version: 1 });
  assert.equal((await client.getMemory('fixture-id'))!.source, 'get_memory');
  assert.deepEqual(JSON.parse((await client.searchMemories('tea', { limit: 2, max_bytes: 500 }))[0]!.content), { query: 'tea', limit: 2, max_bytes: 500 });
  assert.deepEqual(JSON.parse((await client.searchMemories('tea', { mode: 'hybrid', min_similarity: 0.4 }))[0]!.content), { query: 'tea', mode: 'hybrid', min_similarity: 0.4 });
  assert.deepEqual(JSON.parse((await client.memoryHistory('fixture-id', { limit: 3, max_bytes: 700 }))[0]!.memory.content), { id: 'fixture-id', limit: 3, max_bytes: 700 });
  assert.equal(await client.deleteMemory('fixture-id', { expected_version: 2 }), true);
  assert.deepEqual((await client.getProfile()).dynamic, []);
  assert.equal((await client.refreshRepository()).repository, '/fixture');
  assert.deepEqual((await client.searchRepository('hello')).hits, []);
  const context = await client.assembleContext('auth', 500);
  assert.deepEqual(JSON.parse(context.prompt), { query: 'auth', token_budget: 500 });
  assert.deepEqual(context.entries, []);
  assert.equal(context.token_count_method, 'utf8_bytes_upper_bound');
  const filtered = await client.assembleContext('auth', 500, {
    sources: ['memory'], exclude_paths: ['src/generated/**'],
  });
  assert.deepEqual(JSON.parse(filtered.prompt), {
    query: 'auth', token_budget: 500, sources: ['memory'], exclude_paths: ['src/generated/**'],
  });
  assert.deepEqual(filtered.entries, []);
  assert.equal(filtered.tokens_used, Buffer.byteLength(filtered.prompt));
  const empty = await client.assembleContext('auth', 500, { sources: [], exclude_paths: [] });
  assert.deepEqual(JSON.parse(empty.prompt), {
    query: 'auth', token_budget: 500, sources: [], exclude_paths: [],
  });
  await assert.rejects(client.assembleContext('auth', 500, { sources: ['memory', 'memory'] }), TypeError);
  await assert.rejects(client.assembleContext('auth', 500, { exclude_paths: ['../secrets/**'] }), TypeError);
});

async function rejectAndReap(mode: string, errorType: new (...args: any[]) => NativeError, extra: Partial<ClientOptions> = {}, args: JsonObject = {}): Promise<NativeError> {
  const directory = mkdtempSync(join(tmpdir(), 'sdk-failure-'));
  try {
    const db = join(directory, 'pid');
    const client = new Client({ binary, db, userId: mode, timeoutMs: 2000, closeTimeoutMs: 50, ...extra });
    const start = Date.now();
    let caught: NativeError | undefined;
    await assert.rejects(client.callTool('echo', args), (error: unknown) => {
      assert.ok(error instanceof errorType, `${mode}: expected ${errorType.name}, got ${String(error)}`);
      caught = error;
      return true;
    });
    assert.ok(Date.now() - start < (extra.timeoutMs ?? 2000) + 1000);
    assert.throws(() => process.kill(Number(readFileSync(db, 'utf8')), 0), { code: 'ESRCH' });
    return caught!;
  } finally { rmSync(directory, { recursive: true, force: true }); }
}

test('invalid handshakes, frames and nonzero exits cannot silently succeed', async () => {
  for (const mode of ['wrong-version', 'bad-server-info', 'wrong-id', 'duplicate-id', 'unsolicited-zero', 'invalid-json', 'invalid-utf8', 'partial-frame', 'bad-rpc-error', 'malformed-result', 'malformed-notification']) await rejectAndReap(mode, ProtocolError);
  const exited = await rejectAndReap('exit-nonzero', TransportError);
  assert.equal(exited.returncode, 9);
  assert.match(exited.stderr, /intentional exit after response/);
  assert.match((await rejectAndReap('startup-exit', TransportError)).stderr, /intentional startup failure/);
});

test('deadline includes handshake, blocked write, tool and EOF exit', async () => {
  for (const mode of ['init-hang', 'tool-hang', 'no-read', 'ignore-term']) await rejectAndReap(mode, RPCTimeoutError, { timeoutMs: 500 }, mode === 'no-read' ? { content: 'x'.repeat(1024 * 1024) } : {});
});

test('caps frames, cumulative notification output and stderr', async () => {
  await rejectAndReap('frame-flood', OutputLimitError, { maxFrameBytes: 512 });
  await rejectAndReap('notification-flood', OutputLimitError, { maxOutputBytes: 1024 });
  const error = await rejectAndReap('stderr-flood', OutputLimitError, { maxStderrBytes: 128 });
  assert.ok(Buffer.byteLength(error.stderr) <= 128);
});

test('business and JSON-RPC errors preserve their distinct types', async () => {
  await rejectAndReap('business-error', ToolError);
  const error = await rejectAndReap('rpc-error', RPCError);
  assert.ok(error instanceof RPCError);
  assert.equal(error.code, -32602);
  assert.deepEqual(error.data, { detail: 1 });
  await assert.rejects(new Client({ binary: '/no/such/tinymem', userId: 'alice' }).getMemory('memory'), TransportError);
});

test('notifications, fragmented UTF-8, null memory and large dual results', async () => {
  for (const userId of ['notifications', 'fragmented']) assert.deepEqual((await new Client({ binary, userId }).callTool('echo', { content: '漢字 café\nline' })).arguments, { content: '漢字 café\nline' });
  assert.equal(await new Client({ binary, userId: 'null-memory' }).getMemory('missing'), null);
  assert.equal((await new Client({ binary, userId: 'large-result' }).callTool('echo')).payload?.toString().length, 1024 * 1024 + 64);
});

test('local validation happens before any child starts', async () => {
  for (const userId of ['', 'alice bob', '$(execute)', '--option', '漢字', 'x'.repeat(129)]) assert.throws(() => new Client({ userId }));
  for (const extra of [{ timeoutMs: Infinity }, { maxFrameBytes: 0 }, { maxStderrBytes: -1 }, { embeddingCommand: binary }, { embeddingModel: 'test' }, { embeddingCommand: binary, embeddingModel: 'test', embeddingDimensions: 0 }, { embeddingCommand: binary, embeddingModel: 'test', embeddingDimensions: 3, embeddingTimeoutMs: 0 }]) assert.throws(() => new Client({ userId: 'alice', ...extra }));
  const directory = mkdtempSync(join(tmpdir(), 'sdk-validation-'));
  try {
    const db = join(directory, 'pid');
    const client = new Client({ binary, db, userId: 'alice', maxFrameBytes: 512 });
    for (const key of ['scope', 'user_id', 'agent_id', 'project_id', 'user', 'repository']) await assert.rejects(client.callTool('get_memory', { id: 'id', [key]: 'other' }), TypeError);
    await assert.rejects(client.callTool('echo', { content: '漢'.repeat(512) }), RangeError);
    for (const budget of [-1, NaN, 1.5, Infinity]) await assert.rejects(client.assembleContext('auth', budget), RangeError);
    await assert.rejects(client.assembleContext('auth', 500, { sources: ['memory', 'memory'] }), TypeError);
    await assert.rejects(client.assembleContext('auth', 500, { exclude_paths: ['../outside/**'] }), TypeError);
    assert.throws(() => readFileSync(db), { code: 'ENOENT' });
  } finally { rmSync(directory, { recursive: true, force: true }); }
});

test('concurrent calls on shared configuration own independent children', async () => {
  const client = new Client({ binary, userId: 'alice' });
  const results = await Promise.all([0, 1, 2, 3].map(n => client.callTool('echo', { n })));
  assert.deepEqual(results.map(result => result.arguments), [{ n: 0 }, { n: 1 }, { n: 2 }, { n: 3 }]);
  assert.equal(new Set(results.map(result => result.pid)).size, 4);
  for (const result of results) assert.throws(() => process.kill(result.pid as number, 0), { code: 'ESRCH' });
});

test('each operation handshakes, uses literal arguments and reaps on EOF', async () => {
  const repository = mkdtempSync(join(tmpdir(), "sdk space ' ; "));
  try {
    const db = join(repository, 'pid');
    const client = new Client({ binary, db, repository, userId: 'alice', agentId: 'assistant', projectId: 'project' });
    const pids: number[] = [];
    for (const content of ['hello', 'café 漢字\n$(do not execute)']) {
      const result = await client.callTool('echo', { content });
      assert.deepEqual(result.arguments, { content });
      assert.deepEqual(result.argv, ['--db', db, 'mcp', repository, '--user', 'alice', '--agent', 'assistant', '--project', 'project']);
      const pid = result.pid as number;
      assert.equal(Number(readFileSync(db, 'utf8')), pid);
      assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' });
      pids.push(pid);
    }
    assert.notEqual(pids[0], pids[1]);
  } finally { rmSync(repository, { recursive: true, force: true }); }
});
