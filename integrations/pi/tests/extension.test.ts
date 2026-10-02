import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import test from 'node:test';
import tinyMem, { CONTEXT_TYPE } from '../index.ts';
import { automaticContextOptions, boundedInteger, loadClientOptions } from '../config.ts';
import { Client } from '../../../sdks/typescript/index.ts';

const binary = resolve(process.env.TINYMEM_BINARY ?? 'target/debug/tinymem');

function harness(cwd: string, overrides: Record<string, unknown> = {}, trusted = true) {
  const handlers = new Map<string, Function>();
  const commands = new Map<string, { handler: Function }>();
  const flags = new Map<string, unknown>();
  const notifications: string[] = [];
  const pi = {
    registerFlag(name: string, options: { default: unknown }) { flags.set(name, overrides[name] ?? options.default); },
    getFlag(name: string) { return flags.get(name); },
    on(name: string, handler: Function) { handlers.set(name, handler); },
    registerCommand(name: string, command: { handler: Function }) { commands.set(name, command); },
  };
  tinyMem(pi as any);
  const ctx = {
    cwd, hasUI: true, signal: undefined,
    isProjectTrusted: () => trusted,
    ui: { setStatus() {}, notify(text: string) { notifications.push(text); } },
  };
  const run = (name: string, event: unknown = {}) => {
    if (name === 'before_agent_start') {
      const start = event as { systemPromptOptions?: { sections?: Record<string, string> } };
      start.systemPromptOptions ??= { sections: {} };
      start.systemPromptOptions.sections ??= {};
    }
    return handlers.get(name)!(event, ctx);
  };
  run('session_start');
  return { ctx, run, flags, notifications, command: (args: string) => commands.get('memory')!.handler(args, ctx) };
}

function fixture() {
  const root = mkdtempSync(join(tmpdir(), 'tinymem-pi-unit-'));
  const repository = join(root, 'repository');
  mkdirSync(join(repository, '.pi'), { recursive: true });
  const db = join(root, 'native.db');
  const entry = { command: binary, args: ['--db', db, 'mcp', repository, '--user', 'tester', '--project', 'fixture'], exposure: 'deferred' };
  const configure = (value = entry) => writeFileSync(join(repository, '.pi', 'mcp.json'), JSON.stringify({ mcpServers: { tinymem: value } }));
  configure();
  return {
    root, repository, db, entry, configure,
    client: new Client({ binary, db, repository, userId: 'tester', projectId: 'fixture' }),
    cleanup: () => rmSync(root, { recursive: true, force: true }),
  };
}

test('configuration requires exact native local scope and bounded options', () => {
  const f = fixture();
  try {
    const options = loadClientOptions(f.repository, 'tinymem', 5000);
    assert.equal(options.projectId, 'fixture');
    assert.equal(options.userId, 'tester');
    for (const args of [
      ['--db', f.db, 'mcp', f.repository, '--user', 'tester'],
      [...f.entry.args, '--user', 'other'],
      [...f.entry.args, '--embedding-command', '/usr/bin/false'],
      [...f.entry.args, '--agent'],
      ['--db', 'relative.db', ...f.entry.args.slice(2)],
      ['--db', f.db, 'mcp', f.root, '--user', 'tester', '--project', 'fixture'],
    ]) {
      f.configure({ ...f.entry, args });
      assert.throws(() => loadClientOptions(f.repository, 'tinymem', 5000));
    }
    f.configure({ ...f.entry, command: '/bin/sh' });
    assert.throws(() => loadClientOptions(f.repository, 'tinymem', 5000));
    f.configure({ ...f.entry, env: {} } as any);
    assert.throws(() => loadClientOptions(f.repository, 'tinymem', 5000));
    f.configure({ ...f.entry, enabled: 'yes' } as any);
    assert.throws(() => loadClientOptions(f.repository, 'tinymem', 5000));
    f.configure({ ...f.entry, args: [...f.entry.args.slice(0, 7), '~'] });
    assert.throws(() => loadClientOptions(f.repository, 'tinymem', 5000));
    assert.equal(boundedInteger('0', 4096, 262144), 0);
    for (const value of ['-1', '1.5', '9999999999999999999', '262145']) assert.throws(() => boundedInteger(value, 4096, 262144));
  } finally { f.cleanup(); }
});

test('automatic context filters are bounded, source-specific and distinct from explicit recall', async () => {
  assert.deepEqual(automaticContextOptions(undefined, undefined), {
    sources: ['memory', 'repository'], exclude_paths: [],
  });
  assert.deepEqual(automaticContextOptions('memory', '["generated/**"]'), {
    sources: ['memory'], exclude_paths: ['generated/**'],
  });
  assert.deepEqual(automaticContextOptions('', '[]'), { sources: [], exclude_paths: [] });
  for (const [sources, excludes] of [
    ['memory,memory', '[]'], ['unknown', '[]'], ['memory', '{}'], ['memory', '[1]'],
  ]) assert.throws(() => automaticContextOptions(sources, excludes));

  const f = fixture();
  try {
    mkdirSync(join(f.repository, 'generated'), { recursive: true });
    writeFileSync(join(f.repository, 'generated', 'clock.txt'), 'clock generated repository marker\n');
    const memory = await f.client.addMemory('clock personal decision marker', { kind: 'decision' });
    assert.ok(memory);
    const h = harness(f.repository, {
      'tinymem-context-sources': 'memory',
      'tinymem-context-exclude': '["generated/**"]',
    });
    const sections: Record<string, string> = { existing: 'keep me' };
    const before = { prompt: 'clock', systemPromptOptions: { sections } };
    h.run('before_agent_start', before);
    assert.equal(sections.existing, 'keep me');
    assert.match(sections.tinymem, /without redundant recall/);
    assert.match(sections.tinymem, /literal AND.*1-3 distinctive terms/);

    const automatic = await h.run('context', { messages: [{ role: 'user', content: 'clock' }] });
    assert.equal(automatic.messages.length, 2);
    assert.match(automatic.messages[0].content, /personal decision marker/);
    assert.doesNotMatch(automatic.messages[0].content, /generated repository marker/);

    await h.command('recall clock');
    assert.ok(h.notifications.some(text => text.includes('generated repository marker')),
      '/memory recall is explicit and is not constrained by automatic-context filters');
    await h.command('status');
    assert.ok(h.notifications.some(text => text.includes('automatic sources memory')));
    assert.ok(h.notifications.some(text => text.includes('generated/**')));

    const invalid = harness(f.repository, { 'tinymem-context-exclude': '{bad json' });
    invalid.run('before_agent_start', { prompt: 'clock' });
    const failedClosed = await invalid.run('context', { messages: [{ role: 'user', content: 'clock' }] });
    assert.equal(failedClosed.messages.length, 1);
    assert.ok(invalid.notifications.some(text => text.includes('JSON string array')));
  } finally { f.cleanup(); }
});

test('disabled/empty/zero-budget/untrusted/invalid configurations never inject old context', async () => {
  const f = fixture();
  try {
    const old = { role: 'custom', customType: CONTEXT_TYPE, content: 'stale', timestamp: 1 };
    for (const flags of [{ 'tinymem-off': true }, { 'tinymem-budget': '0' }, { 'tinymem-budget': 'bad' }]) {
      const h = harness(f.repository, flags);
      h.run('before_agent_start', { prompt: 'clock' });
      const output = await h.run('context', { messages: [old, { role: 'user', content: 'clock' }] });
      assert.equal(output.messages.length, 1);
    }
    const h = harness(f.repository, {}, false);
    h.run('before_agent_start', { prompt: 'clock' });
    const output = await h.run('context', { messages: [old] });
    assert.deepEqual(output.messages, []);
    assert.ok(h.notifications.some(text => text.includes('Approve')));
    assert.deepEqual(await h.run('context', { messages: [old] }), { messages: [] });
  } finally { f.cleanup(); }
});

test('model-issued writes blocked; reads allowed; opt-in is explicit', async () => {
  const h = harness('/unused');
  for (const name of ['add_memory', 'update_memory', 'delete_memory', 'index_memories']) {
    assert.equal(h.run('tool_call', { toolName: `mcp__tinymem__${name}` }).block, true);
  }
  for (const toolName of ['mcp__tinymem__assemble_context', 'mcp__other__add_memory', 'read']) assert.equal(h.run('tool_call', { toolName }), undefined);
  const allowed = harness('/unused', { 'tinymem-allow-agent-writes': true });
  assert.equal(allowed.run('tool_call', { toolName: 'mcp__tinymem__add_memory' }), undefined);
});

test('real native lifecycle: fresh calls, no accumulation, corrections, conflicts, deletion and scope isolation', async () => {
  const f = fixture();
  try {
    const h = harness(f.repository);
    await h.command('remember clock timestamps must use UTC, never local time.');
    const saved = (await f.client.searchMemories('clock'))[0];
    assert.ok(saved);
    assert.equal(saved.version, 1);
    h.run('before_agent_start', { prompt: 'Implement clock timestamps.' });
    const input = [{ role: 'user', content: 'Implement clock timestamps.', timestamp: 1 }];
    const first = await h.run('context', { messages: input });
    assert.equal(input.length, 1, 'must not mutate original messages');
    assert.equal(first.messages[0].customType, CONTEXT_TYPE);
    assert.ok(first.messages[0].content.includes('UTC'));
    assert.ok(first.messages[0].content.startsWith('Task context:'));
    assert.ok(Buffer.byteLength(first.messages[0].content) <= 4096);
    const second = await h.run('context', { messages: first.messages });
    assert.equal(second.messages.filter((m: any) => m.customType === CONTEXT_TYPE).length, 1);
    assert.equal((await f.client.memoryHistory(saved.id)).length, 1, 'reads must not create observations');
    const other = new Client({ binary, db: f.db, repository: f.repository, userId: 'tester', projectId: 'other' });
    assert.deepEqual(await other.searchMemories('clock'), []);
    await h.command(`correct ${saved.id} 1 clock timestamps must use ISO UTC notation.`);
    assert.equal((await f.client.getMemory(saved.id))!.version, 2);
    await h.command(`correct ${saved.id} 1 clock use local time.`);
    assert.equal((await f.client.getMemory(saved.id))!.version, 2, 'stale expected version cannot overwrite');
    const changed = await h.run('context', { messages: first.messages });
    assert.ok(changed.messages[0].content.includes('ISO UTC'));
    assert.ok(!changed.messages[0].content.includes('never local time'));
    await h.command('off');
    assert.equal((await h.run('context', { messages: first.messages })).messages.length, 1);
    await h.command('on');
    await h.command(`delete ${saved.id} 2`);
    assert.equal(await f.client.getMemory(saved.id), null);
    assert.equal((await f.client.memoryHistory(saved.id)).length, 3);
    assert.equal((await h.run('context', { messages: first.messages })).messages.length, 1);
    await h.command('status');
    assert.ok(h.notifications.some(text => text.includes('model writes blocked')));
  } finally { f.cleanup(); }
});

test('file changes invalidate injection on the next model call, without losing history', async () => {
  const f = fixture();
  try {
    const path = join(f.repository, '.pi', 'dependency.txt');
    writeFileSync(path, 'UTC\n');
    const memory = await f.client.addMemory('clock timestamps must use UTC.', {
      kind: 'decision', source_refs: [{ kind: 'file', role: 'dependency', path: '.pi/dependency.txt' }],
    });
    const h = harness(f.repository);
    h.run('before_agent_start', { prompt: 'clock' });
    const input = { messages: [{ role: 'user', content: 'clock', timestamp: 1 }] };
    const first = await h.run('context', input);
    assert.equal(first.messages.length, 2);
    writeFileSync(path, 'local\n');
    assert.equal((await h.run('context', { messages: first.messages })).messages.length, 1);
    writeFileSync(path, 'UTC\n');
    assert.equal((await h.run('context', input)).messages.length, 2);
    rmSync(path);
    assert.equal((await h.run('context', input)).messages.length, 1);
    const history = await f.client.memoryHistory(memory!.id);
    assert.equal(history.length, 1);
    assert.equal(history[0].freshness.status, 'stale');
    h.run('session_tree');
    assert.equal((await h.run('context', { messages: first.messages })).messages.length, 1);
  } finally { f.cleanup(); }
});

test('long tasks recall useful evidence; oversized tasks fail closed without mutation', async () => {
  const f = fixture();
  try {
    const h = harness(f.repository);
    await h.command('remember clock must persist UTC timestamps.');
    const prompt = 'Please implement the following changes safely. '.repeat(500) + 'clock clock clock UTC UTC UTC';
    h.run('before_agent_start', { prompt });
    const input = { messages: [{ role: 'user', content: prompt, timestamp: 1 }] };
    assert.equal((await h.run('context', input)).messages.length, 2);
    const oversized = 'x'.repeat(65537);
    h.run('before_agent_start', { prompt: oversized });
    assert.equal((await h.run('context', { messages: [{ role: 'user', content: oversized }] })).messages.length, 1);
    assert.equal((await f.client.searchMemories('clock')).length, 1);
  } finally { f.cleanup(); }
});

test('queued steering uses latest user text, not the original task', async () => {
  const f = fixture();
  try {
    await f.client.addMemory('clock uses UTC.');
    const h = harness(f.repository);
    h.run('before_agent_start', { prompt: 'clock' });
    const clock = { role: 'user', content: 'clock', timestamp: 1 };
    assert.equal((await h.run('context', { messages: [clock] })).messages.length, 2);
    const steer = { role: 'user', content: [{ type: 'text', text: 'banana recipes' }], timestamp: 2 };
    assert.equal((await h.run('context', { messages: [clock, steer] })).messages.length, 2);
    const next = { role: 'user', content: [{ type: 'text', text: 'clock' }], timestamp: 3 };
    assert.equal((await h.run('context', { messages: [clock, steer, next] })).messages.length, 4);
  } finally { f.cleanup(); }
});

test('off during an in-flight recall omits evidence after completion', async () => {
  const f = fixture();
  const original = Client.prototype.assembleContext;
  let complete: Function;
  let entered: Function;
  const began = new Promise(resolve => { entered = resolve; });
  const pending = new Promise(resolve => { complete = resolve; });
  Client.prototype.assembleContext = async () => { entered(); return await pending as any; };
  try {
    const h = harness(f.repository);
    h.run('before_agent_start', { prompt: 'clock' });
    const result = h.run('context', { messages: [{ role: 'user', content: 'clock' }] });
    await began;
    await h.command('off');
    complete({ prompt: 'clock evidence', entries: [], token_budget: 4096, tokens_used: 14, token_count_method: 'utf8_bytes_upper_bound' });
    assert.equal((await result).messages.length, 1);
  } finally { Client.prototype.assembleContext = original; f.cleanup(); }
});
