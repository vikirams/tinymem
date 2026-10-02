import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { once } from 'node:events';
import { mkdtempSync, writeFileSync, chmodSync, readFileSync, realpathSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';
import { POLICY, childEnvironment } from '../policy.mjs';
import { fixture, vectorResponse, environment } from './helpers.mjs';

// Minimal RPC fixture client, only for exercising the real Rust subprocess seam.
function native(root, adapter, env) {
  const args = ['--db', join(root, 'native.db'), 'mcp', root, '--user', 'fixture', '--project', 'test'];
  if (adapter) args.push('--embedding-command', adapter, '--embedding-model', POLICY.embeddingIdentity,
    '--embedding-dimensions', String(POLICY.dimensions));
  const binary = realpathSync(fileURLToPath(new URL('../../../target/debug/tinymem', import.meta.url)));
  const child = spawn(binary, args, { env, stdio: ['pipe', 'pipe', 'pipe'] });
  const pending = new Map();
  let id = 0;
  createInterface({ input: child.stdout }).on('line', line => {
    const message = JSON.parse(line);
    if (pending.has(message.id)) { pending.get(message.id).resolve(message); pending.delete(message.id); }
  });
  child.stderr.resume();
  child.on('error', error => { for (const p of pending.values()) p.reject(error); });
  child.on('exit', () => { for (const p of pending.values()) p.reject(new Error('native fixture exited')); });
  function rpc(method, params) {
    const requestId = ++id;
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => { child.kill(); reject(new Error('native fixture deadline')); }, 5000);
      pending.set(requestId, { resolve: message => { clearTimeout(timeout); resolve(message); },
        reject: error => { clearTimeout(timeout); reject(error); } });
      child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id: requestId, method, params }) + '\n');
    });
  }
  return {
    async initialize() {
      await rpc('initialize', { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 'gateway-fixture', version: '1' } });
      child.stdin.write('{"jsonrpc":"2.0","method":"notifications/initialized"}\n');
    },
    async call(name, args = {}) {
      const response = await rpc('tools/call', { name, arguments: args });
      assert.equal(response.error, undefined);
      return response.result;
    },
    async close() {
      const exited = once(child, 'exit');
      child.stdin.end();
      const timer = setTimeout(() => child.kill(), 2000);
      try { await exited; } finally { clearTimeout(timer); }
    },
  };
}

function data(result) { return JSON.parse(result.content[0].text); }

test('Rust CommandEmbedder: gateway write/query/backfill/update; failed inference commits no evidence/cache', async t => {
  const root = mkdtempSync(join(tmpdir(), 'tinymem-native-gateway-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  let failure = false;
  const f = await fixture((_, response) => {
    if (failure) { response.statusCode = 503; response.end('secret upstream diagnostic'); }
    else vectorResponse(response);
  }); t.after(() => f.close());
  // A trusted test executable injects fake HTTP at the module's internal seam.
  // Production embed.mjs has no origin override, test flag or local fallback.
  const adapter = join(root, 'adapter.mjs');
  const module = new URL('../embed.mjs', import.meta.url).href;
  const gatewayModule = new URL('../gateway.mjs', import.meta.url).href;
  const policyModule = new URL('../policy.mjs', import.meta.url).href;
  writeFileSync(adapter, `#!/usr/bin/env node
import { runEmbedding } from ${JSON.stringify(module)};
import { Gateway } from ${JSON.stringify(gatewayModule)};
import { preflight } from ${JSON.stringify(policyModule)};
const gateway = new Gateway(preflight(), { fetch: (url, init) => fetch(${JSON.stringify(f.httpBaseUrl)} + new URL(url).pathname, init) });
try { await runEmbedding(gateway); }
catch { process.stderr.write('TinyMem gateway: test_provider_failed\\n'); process.exitCode = 1; }
finally { gateway.ledger.close(); }
`);
  chmodSync(adapter, 0o755);
  const ledger = join(root, 'usage.jsonl');
  const env = { ...childEnvironment(process.env, root), ...environment(ledger) };
  const offline = native(root, undefined, childEnvironment(process.env, root));
  await offline.initialize();
  await offline.call('add_memory', { content: 'Approved offline evidence to explicitly backfill.' });
  await offline.close();
  const engine = native(root, adapter, env); t.after(() => engine.close());
  await engine.initialize();
  const approved = data(await engine.call('add_memory', { content: 'Approved: prefer explicit UTC timestamps.', kind: 'preference' })).memory;
  assert.ok(approved.id);
  assert.equal(f.requests.length, 1);
  const search = data(await engine.call('search_memories', { query: 'UTC', mode: 'hybrid' }));
  assert.ok(JSON.stringify(search).includes(approved.id));
  assert.equal(f.requests.length, 2);
  assert.equal((await engine.call('index_memories')).isError, false);
  assert.equal(f.requests.length, 3); // Explicit backfill of the model-free write.
  assert.equal((await engine.call('update_memory', { id: approved.id, content: 'Approved: explicit UTC and ISO format.', expected_version: 1 })).isError, false);
  const beforeFailure = f.requests.length;
  failure = true;
  const rejected = await engine.call('add_memory', { content: 'Do not commit this failed write.' });
  assert.equal(rejected.isError, true);
  assert.ok(!JSON.stringify(rejected).includes('secret upstream diagnostic'));
  assert.equal(f.requests.length, beforeFailure + 1); // One attempt; no fallback.
  const update = await engine.call('update_memory', { id: approved.id, content: 'Do not replace existing evidence.', expected_version: 2 });
  assert.equal(update.isError, true);
  const current = data(await engine.call('get_memory', { id: approved.id })).memory;
  assert.equal(current.version, 2);
  assert.equal(current.content, 'Approved: explicit UTC and ISO format.');
  const lexical = data(await engine.call('search_memories', { query: 'failed write', mode: 'lexical' }));
  assert.ok(!JSON.stringify(lexical).includes('Do not commit'));
  failure = false;
  const restarted = native(root, adapter, env); t.after(() => restarted.close());
  await restarted.initialize();
  assert.equal(data(await restarted.call('get_memory', { id: approved.id })).memory.id, approved.id);
  const entries = readFileSync(ledger, 'utf8').trim().split('\n').map(line => JSON.parse(line));
  assert.equal(entries.length, f.requests.length);
  assert.ok(entries.every(entry => entry.operation === 'embedding'));
  for (const secret of [env.OPENAI_API_KEY, env.CF_AIG_TOKEN, approved.content, 'secret upstream diagnostic']) {
    assert.ok(!JSON.stringify(entries).includes(secret));
  }
});
