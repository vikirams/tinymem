import assert from 'node:assert/strict';
import { test } from 'node:test';
import { chmodSync, mkdtempSync, mkdirSync, rmSync, unlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Client, NativeError, ToolError, TransportError } from '../index.ts';
import type { ClientOptions, MemoryEvent } from '../index.ts';

const binary = process.env.TINYMEM_BINARY ?? fileURLToPath(new URL('../../../target/debug/tinymem', import.meta.url));
let pending: string | false = false;
const probeRoot = mkdtempSync(join(tmpdir(), 'tinymem probe '));
try {
  await new Client({ binary, userId: 'probe', repository: probeRoot, db: join(probeRoot, 'probe.db'), timeoutMs: 5000 }).getProfile();
} catch (error) {
  const unavailable = error instanceof TransportError && (error.message.includes('cannot start tinymem') || error.stderr.includes("unrecognized subcommand 'mcp'"));
  if (!unavailable || process.env.TINYMEM_BINARY !== undefined || process.env.TINYMEM_REQUIRE_INTEGRATION === '1') throw error;
  pending = `PENDING: native mcp not available at ${binary}: ${String(error)}`;
} finally { rmSync(probeRoot, { recursive: true, force: true }); }
const config = { skip: pending };

function setup(): { options: ClientOptions; client: Client; cleanup: () => void } {
  const directory = mkdtempSync(join(tmpdir(), 'tinymem sdk real '));
  const repository = join(directory, 'repository');
  mkdirSync(repository);
  const options = { binary, repository, db: join(directory, 'memory.db'), userId: 'alice', agentId: 'assistant', projectId: 'project' };
  return { options, client: new Client(options), cleanup: () => rmSync(directory, { recursive: true, force: true }) };
}

test('file provenance, freshness and immutable history across SDK restarts', config, async () => {
  const { options, client, cleanup } = setup();
  try {
    const path = join(options.repository!, 'clock.txt');
    writeFileSync(path, 'UTC');
    const refs = [{ kind: 'file', role: 'dependency', path: 'clock.txt' }] as const;
    const saved = await client.addMemory('clock correction', { source: 'user correction', source_refs: [...refs] });
    assert.ok(saved);
    assert.equal(saved.source_refs[0]!.path, 'clock.txt');
    assert.match(saved.source_refs[0]!.content_hash, /^blake3:/);
    assert.deepEqual(await client.getMemory(saved.id), saved);
    assert.deepEqual(await client.searchMemories('correction'), [saved]);
    assert.equal((await client.memoryHistory(saved.id))[0]!.freshness.status, 'current');
    assert.equal((await client.addMemory('clock correction', { source: 'user correction', source_refs: [...refs] }))!.id, saved.id);
    const origin = await client.addMemory('clock origin', { source_refs: [{ kind: 'file', role: 'origin', path: 'clock.txt' }] });
    assert.ok(origin);
    const provenance = (await client.assembleContext('correction', 8000)).entries[0]!.provenance[0]!;
    assert.equal(provenance.type, 'memory');
    if (provenance.type === 'memory') assert.deepEqual(provenance.source_refs, saved.source_refs);
    const other = new Client({ ...options, projectId: undefined });
    assert.equal(await other.getMemory(saved.id), null);
    assert.deepEqual(await other.memoryHistory(saved.id), []);
    writeFileSync(path, 'monotonic');
    for (const reason of ['content_changed', 'file_missing', 'source_unavailable']) {
      if (reason === 'file_missing') unlinkSync(path);
      else if (reason === 'source_unavailable') mkdirSync(path);
      assert.equal(await client.getMemory(saved.id), null);
      assert.deepEqual(await client.searchMemories('correction'), []);
      assert.deepEqual((await client.getProfile()).stable, [origin]);
      assert.deepEqual((await client.assembleContext('correction', 8000)).entries, []);
      const event: MemoryEvent = (await client.memoryHistory(saved.id))[0]!;
      assert.deepEqual(event.memory, saved);
      assert.equal(event.freshness.reason, reason);
      assert.equal(event.freshness.checks[0]!.expected_hash, saved.source_refs[0]!.content_hash);
      assert.equal(event.freshness.status, reason === 'source_unavailable' ? 'unknown' : 'stale');
    }
    assert.deepEqual(await client.getMemory(origin.id), origin);
    assert.equal(await client.deleteMemory(saved.id, { expected_version: 1 }), true);
    assert.deepEqual((await client.memoryHistory(saved.id)).map(event => event.event), ['added', 'deleted']);
  } finally { cleanup(); }
});

test('native CRUD, dedup, restart, version conflicts, history and deletion', config, async () => {
  const { options, client, cleanup } = setup();
  try {
    const memory = await client.addMemory('Saffron tea is preferred', { kind: 'preference', source: 'sdk test' });
    assert.ok(memory);
    assert.deepEqual(memory.scope, { user_id: 'alice', agent_id: 'assistant', project_id: 'project' });
    assert.equal(memory.version, 1);
    assert.equal((await client.addMemory('Saffron tea is preferred', { kind: 'preference', source: 'sdk test' }))!.id, memory.id);
    assert.deepEqual(await client.getMemory(memory.id), memory);
    assert.ok((await client.searchMemories('Saffron')).some(record => record.id === memory.id));
    const updated = await client.updateMemory(memory.id, 'Saffron coffee is preferred', { expected_version: 1 });
    assert.ok(updated);
    assert.equal(updated.id, memory.id);
    assert.equal(updated.version, 2);
    assert.equal(updated.kind, 'preference');
    assert.equal(updated.source, 'sdk test');
    await assert.rejects(client.updateMemory(memory.id, 'stale write must fail', { expected_version: 1 }), ToolError);
    await assert.rejects(client.deleteMemory(memory.id, { expected_version: 1 }), ToolError);
    const restarted = new Client(options); // Every call already exited/reaped its server.
    assert.deepEqual(await restarted.getMemory(memory.id), updated);
    const history: MemoryEvent[] = await restarted.memoryHistory(memory.id, { limit: 100, max_bytes: 64000 });
    assert.deepEqual(history.map(event => event.event), ['added', 'updated']);
    assert.deepEqual(history.map(event => event.version), [1, 2]);
    assert.deepEqual(history.map(event => event.memory.id), [memory.id, memory.id]);
    assert.deepEqual(history.map(event => event.memory.content), ['Saffron tea is preferred', 'Saffron coffee is preferred']);
    assert.ok(history.every(event => event.at_ms > 0));
    assert.equal(await restarted.deleteMemory(memory.id, { expected_version: 2 }), true);
    assert.equal(await restarted.getMemory(memory.id), null);
    assert.deepEqual(await restarted.searchMemories('Saffron'), []);
    assert.equal(await restarted.deleteMemory(memory.id), false);
    const deletedHistory = await restarted.memoryHistory(memory.id, { limit: 100, max_bytes: 64000 });
    assert.deepEqual(deletedHistory.map(event => event.event), ['added', 'updated', 'deleted']);
    assert.deepEqual(deletedHistory.map(event => event.version), [1, 2, 3]);
    assert.deepEqual(deletedHistory.slice(0, 2), history);
    assert.equal(deletedHistory[2]!.memory.id, memory.id);
    assert.equal(deletedHistory[2]!.memory.content, 'Saffron coffee is preferred');
    await assert.rejects(restarted.updateMemory(memory.id, 'must not resurrect a deleted record'), ToolError);
  } finally { cleanup(); }
});

test('native exact scopes and scope-local duplicate content', config, async () => {
  const { options, client, cleanup } = setup();
  try {
    const original = await client.addMemory('Juniper common content', { source: 'explicit' });
    assert.ok(original);
    for (const change of [{ userId: 'bob' }, { agentId: 'other' }, { projectId: 'other' }, { agentId: undefined }, { projectId: undefined }, { agentId: undefined, projectId: undefined }]) {
      const other = new Client({ ...options, ...change });
      assert.equal(await other.getMemory(original.id), null);
      assert.deepEqual(await other.searchMemories('Juniper'), []);
      assert.deepEqual(await other.memoryHistory(original.id), []);
      assert.equal(await other.deleteMemory(original.id), false);
      const profile = await other.getProfile();
      assert.deepEqual([...profile.stable, ...profile.dynamic], []);
      await assert.rejects(other.updateMemory(original.id, 'foreign scope must not write'), ToolError);
      const duplicate = await other.addMemory('Juniper common content', { source: 'explicit' });
      assert.ok(duplicate);
      assert.notEqual(duplicate.id, original.id);
      assert.equal(await client.getMemory(duplicate.id), null);
    }
    assert.deepEqual(await client.getMemory(original.id), original);
  } finally { cleanup(); }
});

test('native profiles, expiration and read requests do not add or update memory', config, async () => {
  const { client, cleanup } = setup();
  try {
    const stable = await client.addMemory('Prefer tea', { kind: 'preference' });
    const dynamic = await client.addMemory('Plan a release', { kind: 'plan' });
    const expired = await client.addMemory('Expired saffron fact', { expires_at_ms: Date.now() - 1000 });
    assert.ok(stable && dynamic && expired);
    const history = await client.memoryHistory(stable.id, { limit: 100, max_bytes: 64000 });
    await client.getMemory(stable.id);
    await client.searchMemories('tea', { limit: 1, max_bytes: 16000 });
    const profile = await client.getProfile({ limit: 100, max_bytes: 64000 });
    assert.ok(profile.stable.some(record => record.id === stable.id));
    assert.ok(profile.dynamic.some(record => record.id === dynamic.id));
    assert.ok(![...profile.stable, ...profile.dynamic].some(record => record.id === expired.id));
    assert.deepEqual(await client.searchMemories('Expired saffron'), []);
    assert.ok((await client.memoryHistory(expired.id)).length > 0);
    await client.refreshRepository();
    await client.searchRepository('no_source_match');
    assert.deepEqual(await client.memoryHistory(stable.id, { limit: 100, max_bytes: 64000 }), history);
    assert.deepEqual(await client.getMemory(stable.id), stable);
    const matches = await client.searchMemories('tea', { limit: 1, max_bytes: 16000 });
    assert.ok(matches.length <= 1);
    assert.ok(Buffer.byteLength(JSON.stringify(matches)) <= 16000);
    assert.ok((await client.memoryHistory(stable.id, { limit: 1, max_bytes: 16000 })).length <= 1);
  } finally { cleanup(); }
});

test('native repository refresh follows edits/deletions without personal writes', config, async () => {
  const { options, client, cleanup } = setup();
  try {
    const source = join(options.repository!, 'login.py');
    writeFileSync(source, 'def obsolete_login():\n    pass\n');
    assert.equal((await client.refreshRepository()).indexed, 1);
    assert.equal((await client.searchRepository('obsolete_login')).hits[0]!.path, 'login.py');
    writeFileSync(source, 'def current_login():\n    return True\n');
    const edited = await client.searchRepository('current_login');
    assert.equal(edited.refresh.indexed, 1);
    assert.match(edited.hits[0]!.content, /current_login/);
    assert.deepEqual((await client.searchRepository('obsolete_login')).hits, []);
    unlinkSync(source);
    const deleted = await client.searchRepository('current_login');
    assert.deepEqual(deleted.hits, []);
    assert.equal(deleted.refresh.removed, 1);
    const profile = await client.getProfile();
    assert.deepEqual([...profile.stable, ...profile.dynamic], []);
  } finally { cleanup(); }
});

test('invalid native input and startup failures propagate', config, async () => {
  const { options, client, cleanup } = setup();
  try {
    await assert.rejects(client.addMemory(''), ToolError);
    await assert.rejects(new Client({ ...options, repository: join(options.repository!, 'missing') }).getProfile(), NativeError);
    assert.ok(await client.addMemory('Still usable after failed operations'));
  } finally { cleanup(); }
});

test('native opt-in semantic backfill, hybrid ranking and invalidation', config, async () => {
  const { options, client, cleanup } = setup();
  try {
    const embeddingCommand = fileURLToPath(new URL('../../../scripts/fixtures/embed.py', import.meta.url));
    chmodSync(embeddingCommand, 0o755);
    const original = await client.addMemory('morning drink');
    assert.ok(original);
    const configured = { ...options, embeddingCommand, embeddingModel: 'fixture-v1', embeddingDimensions: 3 };
    const semantic = new Client(configured);
    assert.equal((await semantic.indexMemories({ limit: 1 })).indexed, 1);
    assert.deepEqual(await semantic.searchMemories('hot beverage', { mode: 'semantic' }), [original]);
    const assembled = await semantic.assembleContext('hot beverage', 4000);
    assert.equal(assembled.entries[0]!.content, 'morning drink');
    assert.equal(assembled.entries[0]!.provenance[0]!.type, 'memory');
    assert.equal((await semantic.memoryHistory(original.id)).length, 1);
    const source = join(options.repository!, 'drink.txt');
    writeFileSync(source, 'espresso');
    const exact = await semantic.addMemory('espresso', { source_refs: [{ kind: 'file', role: 'dependency', path: 'drink.txt' }] });
    const keyword = await semantic.addMemory('compiler');
    assert.ok(exact && keyword);
    const hybrid = await semantic.searchMemories('espresso compiler', { mode: 'hybrid', min_similarity: 0.5 });
    assert.deepEqual(hybrid[0], exact);
    assert.deepEqual(new Set(hybrid.map(record => record.id)), new Set([original.id, exact.id, keyword.id]));
    assert.deepEqual(await new Client(configured).searchMemories('hot beverage', { mode: 'semantic' }), await semantic.searchMemories('hot beverage', { mode: 'semantic' }));
    assert.deepEqual(await new Client({ ...configured, userId: 'bob' }).searchMemories('hot beverage', { mode: 'semantic' }), []);
    await client.updateMemory(original.id, 'compiler updated', { expected_version: 1 });
    assert.ok(!(await semantic.searchMemories('hot beverage', { mode: 'semantic' })).some(record => record.id === original.id));
    assert.equal((await semantic.indexMemories()).indexed, 1);
    assert.equal((await semantic.memoryHistory(exact.id)).length, 1);
    await assert.rejects(client.searchMemories('espresso', { mode: 'hybrid' }), ToolError);
    writeFileSync(source, 'different recommendation');
    for (const mode of ['semantic', 'hybrid'] as const) {
      assert.ok(!(await semantic.searchMemories('espresso', { mode })).some(record => record.id === exact.id));
    }
    assert.ok(!(await semantic.assembleContext('hot beverage', 8000)).prompt.includes('espresso'));
    assert.equal((await semantic.memoryHistory(exact.id))[0]!.freshness.reason, 'content_changed');
  } finally { cleanup(); }
});

test('native context assembly budgets, deduplication, provenance, refresh and scope', config, async () => {
  const { options, client, cleanup } = setup();
  try {
    const source = join(options.repository!, 'auth.py');
    writeFileSync(source, 'auth = True\n');
    const memory = await client.addMemory('Prefer auth tests', { kind: 'preference', source: 'user' });
    assert.ok(memory);
    await client.addMemory('auth uses cookies', { kind: 'decision', source: 'ADR-1' });
    await client.addMemory('auth uses cookies', { kind: 'decision', source: 'review' });
    await client.addMemory('auth expired', { expires_at_ms: 0 });
    const history = await client.memoryHistory(memory.id);
    const result = await client.assembleContext('Please update the auth flow', 8000);
    assert.equal(result.token_count_method, 'utf8_bytes_upper_bound');
    assert.equal(result.tokens_used, Buffer.byteLength(result.prompt));
    assert.ok(result.tokens_used <= 8000);
    assert.deepEqual(result.entries.map(entry => entry.category), ['preference', 'decision', 'repository']);
    assert.deepEqual(JSON.parse(result.prompt.slice(result.prompt.indexOf('\n') + 1)), result.entries);
    const personal = result.entries[0]!.provenance[0]!;
    assert.equal(personal.type, 'memory');
    if (personal.type === 'memory') assert.equal(personal.id, memory.id);
    assert.equal(result.entries[1]!.provenance.length, 2);
    const evidence = result.entries[2]!.provenance[0]!;
    assert.equal(evidence.type, 'repository');
    if (evidence.type === 'repository') assert.equal(evidence.path, 'auth.py');
    assert.ok(!result.prompt.includes('expired'));
    assert.equal((await client.assembleContext('auth', result.tokens_used)).prompt, result.prompt);
    assert.equal((await client.assembleContext('auth', 0)).prompt, '');
    const memoryOnly = await client.assembleContext('auth', 8000, {
      sources: ['memory'], exclude_paths: ['auth.py'],
    });
    assert.ok(memoryOnly.entries.length > 0);
    assert.ok(memoryOnly.entries.every(entry => entry.category !== 'repository'));
    const repositoryOnly = await client.assembleContext('auth', 8000, {
      sources: ['repository'], exclude_paths: ['auth.py'],
    });
    assert.deepEqual(repositoryOnly.entries, []);
    assert.ok((await client.searchRepository('auth')).hits.some(hit => hit.path === 'auth.py'));
    unlinkSync(source);
    assert.ok((await client.assembleContext('auth', 8000)).entries.every(entry => entry.category !== 'repository'));
    assert.deepEqual((await new Client({ ...options, userId: 'bob' }).assembleContext('auth', 8000)).entries, []);
    assert.deepEqual(await client.memoryHistory(memory.id), history);
    assert.deepEqual(await client.getMemory(memory.id), memory);
  } finally { cleanup(); }
});
