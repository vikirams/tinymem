import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';
import { POLICY } from '../policy.mjs';
import { createGatewayRuntime } from '../pi-model.mjs';
import { createManagedSession, shutdownManagedSession } from '../pi-session.mjs';
import { fixture, sse, responseEvents, environment } from './helpers.mjs';

function directory(t) {
  const root = mkdtempSync(join(tmpdir(), 'tinymem-pi-routing-'));
  const agent = join(root, 'agent'); mkdirSync(agent);
  t.after(() => rmSync(root, { recursive: true, force: true }));
  return { root, agent };
}
const context = { messages: [{ role: 'user', content: 'Hello 漢字', timestamp: 0 }] };

test('Pi native Responses transport streams Unicode and records reported usage/cache/cost', async t => {
  const { agent } = directory(t);
  const f = await fixture((_, response) => sse(response)); t.after(() => f.close());
  const { runtime, model } = await createGatewayRuntime(f.gateway, agent);
  const stream = runtime.streamSimple(model, context);
  const events = [];
  for await (const event of stream) events.push(event.type);
  const message = await stream.result();
  assert.equal(message.stopReason, 'stop');
  assert.equal(message.content[0].text, 'Hello 漢字');
  assert.ok(events.includes('text_delta'));
  assert.equal(f.requests[0].path.endsWith('/openai/responses'), true);
  assert.equal(f.requests[0].body.model, POLICY.chatModel);
  assert.equal(f.requests[0].body.store, false);
  assert.equal(f.entries[0].usage.input, 10);
  assert.equal(f.entries[0].usage.cache_read, 2);
  assert.equal(f.entries[0].usage.output, 4);
  assert.ok(Math.abs(f.entries[0].usage.estimated_cost_usd - (10 * .4 + 2 * .1 + 4 * 1.6) / 1e6) < 1e-15);
  assert.equal(JSON.stringify(f.entries).includes(environment().OPENAI_API_KEY), false);
  assert.deepEqual(runtime.getModels().map(model => model.id), [POLICY.chatModel]);
});

test('SDK alternate-model, direct-route, classifiers and external provider registration cannot bypass policy', async t => {
  const { agent } = directory(t);
  const f = await fixture((_, response) => sse(response)); t.after(() => f.close());
  const { runtime, model } = await createGatewayRuntime(f.gateway, agent);
  for (const candidate of [{ ...model, baseUrl: 'https://api.openai.com/v1' }, { ...model, id: 'gpt-4o' }, { ...model, api: 'openai-completions' }]) {
    assert.throws(() => runtime.streamSimple(candidate, context), /model_not_allowed/);
    assert.throws(() => runtime.complete(candidate, context), /model_not_allowed/);
  }
  assert.throws(() => runtime.getProvider(POLICY.provider).streamSimple({ ...model, baseUrl: 'https://api.openai.com/v1' }, context), /model_not_allowed/);
  assert.throws(() => runtime.classify({}, {}), /operation_not_allowed/);
  assert.throws(() => runtime.registerProvider('direct', {}), /operation_not_allowed/);
  assert.equal(runtime.getModel('openai', 'gpt-4.1-mini'), undefined);
  assert.deepEqual(runtime.getModelsOfType('classifier'), []);
  assert.equal(f.requests.length, 0);
});

test('a completed chat without provider-reported usage cannot pass as a measurable baseline', async t => {
  const { agent } = directory(t);
  const f = await fixture((_, response) => sse(response, responseEvents({ usage: false }))); t.after(() => f.close());
  const { runtime, model } = await createGatewayRuntime(f.gateway, agent);
  const message = await runtime.completeSimple(model, context);
  assert.equal(message.stopReason, 'error');
  assert.match(message.errorMessage, /missing_chat_usage/);
  assert.equal(f.entries[0].usage, null);
  assert.equal(f.entries[0].cost_basis, 'unknown-until-reconciled');
});

test('Pi failures, partial streams, rate limits and aborts never fall back or leak remote diagnostics in ledger', async t => {
  for (const handler of [
    (_, response) => { response.statusCode = 401; response.end(environment().OPENAI_API_KEY); },
    (_, response) => { response.statusCode = 429; response.end('retry ' + environment().CF_AIG_TOKEN); },
    (_, response) => { response.statusCode = 503; response.end('direct fallback'); },
    (_, response) => { response.statusCode = 307; response.setHeader('location', 'https://api.openai.com/v1/responses'); response.end(); },
    (_, response) => sse(response, responseEvents().slice(0, -1)),
    (_, response) => sse(response, responseEvents({ model: 'gpt-4o' })),
    (_, response) => sse(response, [{ type: 'response.failed', response: { status: 'failed', error: { code: 'fake', message: environment().CF_AIG_TOKEN } } }]),
  ]) {
    const { agent } = directory(t);
    const f = await fixture(handler); t.after(() => f.close());
    const { runtime, model } = await createGatewayRuntime(f.gateway, agent);
    const message = await runtime.completeSimple(model, context);
    assert.equal(message.stopReason, 'error');
    assert.equal(f.requests.length, 1);
    assert.equal(f.entries.length, 1);
    assert.ok(!JSON.stringify(f.entries).includes(environment().CF_AIG_TOKEN));
    assert.ok(!JSON.stringify(f.entries).includes(environment().OPENAI_API_KEY));
    assert.ok(!message.errorMessage.includes(environment().CF_AIG_TOKEN));
    assert.ok(!message.errorMessage.includes(environment().OPENAI_API_KEY));
  }
  const { agent } = directory(t);
  const f = await fixture((_, response) => { response.writeHead(200, { 'content-type': 'text/event-stream' }); response.write(': waiting\n\n'); });
  t.after(() => f.close());
  const { runtime, model } = await createGatewayRuntime(f.gateway, agent);
  const controller = new AbortController();
  const result = runtime.completeSimple(model, context, { signal: controller.signal });
  const timer = setTimeout(() => controller.abort(), 50);
  try { assert.equal((await result).stopReason, 'aborted'); } finally { clearTimeout(timer); }
  assert.equal(f.requests.length, 1);
});

test('managed Pi sessions have no prior transcript/discovered resources/shell; explicit compaction uses same gateway', async t => {
  const { root, agent } = directory(t);
  const f = await fixture((_, response) => sse(response)); t.after(() => f.close());
  const first = await createManagedSession(f.gateway, { cwd: root, agentDir: agent });
  t.after(() => shutdownManagedSession(first));
  let settled = false;
  first.subscribe(event => { if (event.type === 'agent_settled') settled = true; });
  assert.deepEqual(first.getActiveToolNames(), []);
  await first.prompt('Remember nothing; test transport.');
  assert.equal(settled, true);
  await first.prompt('Test second turn. ' + 'fixture '.repeat(500));
  await first.compact();
  assert.ok(f.requests.length >= 3);
  assert.ok(f.requests.every(request => request.path.endsWith('/openai/responses')));
  const secondAgent = join(root, 'fresh-agent'); mkdirSync(secondAgent);
  const fresh = await createManagedSession(f.gateway, { cwd: root, agentDir: secondAgent });
  t.after(() => shutdownManagedSession(fresh));
  assert.equal(fresh.messages.length, 0);
  await fresh.prompt('Fresh transcript.');
  const body = JSON.stringify(f.requests.at(-1).body);
  assert.ok(!body.includes('Remember nothing'));
  await Promise.all(f.gateway.accounting);
  assert.equal(f.entries.length, f.requests.length);
});

test('scripted Pi save and fresh-session native MCP recall both use the gateway transport', async t => {
  const { root, agent } = directory(t);
  const binary = fileURLToPath(new URL('../../../target/debug/tinymem', import.meta.url));
  // CI builds this first. The integration test is mandatory, not silently skipped.
  const f = await fixture((_, response, body, turn) => {
    if (turn === 1) {
      assert.ok(body.tools.some(tool => tool.name === 'mcp__tinymem__add_memory'));
      sse(response, responseEvents({ tool: { name: 'mcp__tinymem__add_memory', args: {
        content: 'Owner-approved: prefer explicit UTC timestamps.', kind: 'preference' } } }));
    } else if (turn === 3) {
      assert.ok(!JSON.stringify(body.input).includes('Owner-approved: prefer explicit UTC timestamps.'));
      sse(response, responseEvents({ tool: { name: 'mcp__tinymem__search_memories', args: { query: 'UTC', mode: 'lexical' } } }));
    } else {
      assert.ok(JSON.stringify(body.input).includes('function_call_output'));
      if (turn === 4) assert.ok(JSON.stringify(body.input).includes('Owner-approved: prefer explicit UTC timestamps.'));
      sse(response);
    }
  }); t.after(() => f.close());
  const memory = { binary, db: join(root, 'memory.db'), user: 'fixture', project: 'test', embeddings: false };
  const session = await createManagedSession(f.gateway, { cwd: root, agentDir: agent, memory });
  t.after(() => shutdownManagedSession(session));
  await session.prompt('Save this owner-approved note: prefer explicit UTC timestamps.');
  assert.equal(session.messages.some(message => message.role === 'toolResult' && message.isError), false);
  assert.equal(f.requests.length, 2);
  await shutdownManagedSession(session);
  const freshAgent = join(root, 'fresh'); mkdirSync(freshAgent);
  const fresh = await createManagedSession(f.gateway, { cwd: root, agentDir: freshAgent, memory });
  t.after(() => shutdownManagedSession(fresh));
  assert.equal(fresh.messages.length, 0);
  await fresh.prompt('Recall the project timestamp preference.');
  assert.equal(f.requests.length, 4);
  assert.equal(f.entries.length, 4);
});
