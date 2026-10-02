import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync, chmodSync, symlinkSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { POLICY, preflight, childEnvironment, embeddingEnvironment } from '../policy.mjs';
import { Gateway, openLedger } from '../gateway.mjs';
import { environment, fixture, vectorResponse } from './helpers.mjs';

const input = { model: POLICY.embeddingIdentity, text: 'private memory 漢字\n"UTC"' };

test('preflight rejects absent credentials, placeholders, unapproved policy, overrides without exposing values', () => {
  assert.throws(() => preflight({}), /missing_configuration.*CF_AIG_TOKEN.*OPENAI_API_KEY/);
  for (const [key, value] of [['CF_ACCOUNT_ID', 'https://api.openai.com'], ['CF_GATEWAY_ID', '../openai'],
    ['CF_AIG_TOKEN', 'Bearer secret\nheader'], ['OPENAI_API_KEY', 'REPLACE_PROVIDER_KEY'],
    ['TINYMEM_GATEWAY_APPROVAL', 'not-approved'], ['HTTPS_PROXY', 'http://private-proxy'],
    ['NODE_OPTIONS', '--require evil.mjs'], ['TINYMEM_USAGE_LEDGER', 'relative']]) {
    assert.throws(() => preflight({ ...environment(), [key]: value }), error => !error.message.includes(value));
  }
  assert.equal(preflight(environment()).baseUrl, `https://gateway.ai.cloudflare.com/v1/${'a'.repeat(32)}/tinymem-private-test/openai`);
});

test('child environments are allowlisted; only trusted embedding child receives necessary credentials', () => {
  const env = { ...environment(), PATH: '/bin', HOME: '/personal', UNRELATED_SECRET: 'secret', CF_AIG_ADMIN_TOKEN: 'admin',
    ANTHROPIC_API_KEY: 'other-secret', PI_CODING_AGENT_DIR: '/personal/pi' };
  const child = childEnvironment(env, '/isolated');
  assert.equal(child.HOME, '/isolated');
  for (const name of ['CF_AIG_TOKEN', 'OPENAI_API_KEY', 'UNRELATED_SECRET', 'CF_AIG_ADMIN_TOKEN', 'ANTHROPIC_API_KEY', 'PI_CODING_AGENT_DIR']) assert.equal(child[name], undefined);
  const embedding = embeddingEnvironment(preflight(env), env, '/isolated');
  assert.equal(embedding.CF_AIG_TOKEN, env.CF_AIG_TOKEN);
  assert.equal(embedding.CF_AIG_ADMIN_TOKEN, undefined);
});

test('real fake HTTP verifies embedding path, model, auth, Unicode, privacy, metadata and cost', async t => {
  const f = await fixture((_, response) => vectorResponse(response));
  t.after(() => f.close());
  const result = await f.gateway.embed(input);
  assert.equal(result.embedding.length, POLICY.dimensions);
  const request = f.requests[0];
  assert.equal(request.path, `/v1/${'a'.repeat(32)}/tinymem-private-test/openai/embeddings`);
  assert.deepEqual(request.body, { model: POLICY.embeddingModel, input: input.text, dimensions: POLICY.dimensions, encoding_format: 'float' });
  assert.equal(request.headers.authorization, `Bearer ${environment().OPENAI_API_KEY}`);
  assert.equal(request.headers['cf-aig-authorization'], `Bearer ${environment().CF_AIG_TOKEN}`);
  assert.equal(request.headers['cf-aig-skip-cache'], 'true');
  assert.equal(request.headers['cf-aig-max-attempts'], '1');
  assert.equal(request.headers['cf-aig-request-timeout'], '25000');
  assert.equal(request.headers['cf-aig-collect-log-payload'], 'false');
  const metadata = JSON.parse(request.headers['cf-aig-metadata']);
  assert.deepEqual(Object.keys(metadata).sort(), ['operation', 'request_id']);
  const entry = f.entries[0];
  assert.equal(entry.request_id, metadata.request_id);
  assert.equal(entry.gateway_log_id, '12345678-1234-1234-1234-123456789012');
  assert.equal(entry.usage.total_tokens, 3);
  assert.equal(entry.usage.estimated_cost_usd, 3 * 0.02 / 1e6);
  for (const secret of [input.text, environment().CF_AIG_TOKEN, environment().OPENAI_API_KEY]) assert.ok(!JSON.stringify(f.entries).includes(secret));
});

test('caller metadata/privacy overrides and unsafe correlation headers cannot leak into logs', async t => {
  const f = await fixture((_, response) => {
    response.setHeader('cf-aig-log-id', environment().CF_AIG_TOKEN);
    response.end('{}');
  }); t.after(() => f.close());
  const operation = f.gateway.operation('embedding');
  const response = await operation.fetch(`${f.gateway.config.baseUrl}/embeddings`, {
    method: 'POST', body: JSON.stringify({ model: POLICY.embeddingModel }),
    headers: { 'cf-aig-collect-log-payload': 'true', 'cf-aig-metadata': '{"owner":"private-owner"}',
      'cf-aig-max-attempts': '5', 'cf-aig-authorization': 'stolen', authorization: 'wrong' },
  });
  await response.text(); operation.finish();
  assert.equal(f.requests[0].headers['cf-aig-collect-log-payload'], 'false');
  assert.equal(f.requests[0].headers['cf-aig-max-attempts'], '1');
  assert.ok(!f.requests[0].headers['cf-aig-metadata'].includes('private-owner'));
  assert.equal(f.entries[0].gateway_log_id, null);
  assert.ok(!JSON.stringify(f.entries).includes(environment().CF_AIG_TOKEN));
});

test('redirects and 401/429/5xx never retry or fall back and never echo upstream error bodies', async t => {
  for (const status of [301, 307, 401, 429, 500, 503]) {
    const f = await fixture((_, response) => {
      response.statusCode = status;
      response.setHeader('location', 'https://api.openai.com/v1/embeddings');
      response.end(environment().CF_AIG_TOKEN + input.text);
    });
    t.after(() => f.close());
    await assert.rejects(f.gateway.embed(input), error => !error.message.includes(input.text) && !error.message.includes(environment().CF_AIG_TOKEN));
    assert.equal(f.requests.length, 1);
    assert.equal(f.entries.length, 1);
    assert.equal(f.entries[0].usage, null);
  }
});

test('direct hosts, other gateway paths/models/operations are blocked before network', async () => {
  let calls = 0;
  const gateway = new Gateway(preflight(environment()), { ledger: { append() {} }, fetch() { calls++; throw new Error(); } });
  const good = gateway.config.baseUrl + '/embeddings';
  for (const url of ['https://api.openai.com/v1/embeddings', 'http://localhost:11434/v1/embeddings',
    good + '?provider=openai', good.replace('/openai/', '/anthropic/'), good.replace('tinymem-private-test', 'other')]) {
    await assert.rejects(gateway.operation('embedding').fetch(url, { method: 'POST', body: JSON.stringify({ model: POLICY.embeddingModel }) }), /route_not_allowed/);
  }
  await assert.rejects(gateway.operation('embedding').fetch(good, { method: 'POST', body: '{"model":"not-approved"}' }), /model_not_allowed/);
  assert.throws(() => gateway.operation('judge'), /operation_not_allowed/);
  assert.equal(calls, 0);
});

test('malformed vectors, mismatched identities and missing usage cannot return a cacheable vector', async t => {
  const bodies = [ { data: [] }, { model: 'another-model' }, { usage: undefined },
    { data: [{ index: 1, embedding: Array(POLICY.dimensions).fill(1) }] },
    { data: [{ index: 0, embedding: [1, 2] }] },
    { data: [{ index: 0, embedding: Array(POLICY.dimensions).fill(0) }] },
    { data: [{ index: 0, embedding: Array(POLICY.dimensions).fill(1e100) }] } ];
  for (const body of bodies) {
    const f = await fixture((_, response) => vectorResponse(response, body));
    t.after(() => f.close());
    await assert.rejects(f.gateway.embed(input), /embedding/);
    assert.equal(f.entries[0].outcome.startsWith('invalid_') || f.entries[0].outcome.startsWith('missing_'), true);
  }
  for (const request of [{ ...input, model: POLICY.embeddingModel }, { ...input, extra: 'bad' }, { ...input, text: 'x'.repeat(65537) }]) {
    const f = await fixture((_, response) => vectorResponse(response));
    t.after(() => f.close());
    await assert.rejects(f.gateway.embed(request), /invalid_embedding_request/);
    assert.equal(f.requests.length, 0);
  }
});

test('finite header/body deadlines, cancellation, overflow and broken JSON fail without fallback', async t => {
  for (const handler of [() => {}, (_, response) => { response.writeHead(200, { 'content-type': 'application/json' }); response.write('{'); }]) {
    const f = await fixture(handler, { timeoutMs: 40 });
    t.after(() => f.close());
    await assert.rejects(f.gateway.embed(input));
    assert.equal(f.requests.length, 1);
    assert.equal(f.entries.length, 1);
  }
  const overflow = await fixture((_, response) => response.end('x'.repeat(4 * 1024 * 1024 + 1)));
  t.after(() => overflow.close());
  await assert.rejects(overflow.gateway.embed(input));
  assert.equal(overflow.entries[0].outcome, 'response_too_large');
  const malformed = await fixture((_, response) => response.end('not JSON'));
  t.after(() => malformed.close());
  await assert.rejects(malformed.gateway.embed(input), /invalid_embedding_response/);
  const controller = new AbortController(); controller.abort();
  await assert.rejects(malformed.gateway.embed(input, controller.signal));
});

test('ledger is mandatory, private, symlink-safe and payload-free', () => {
  const directory = mkdtempSync(join(tmpdir(), 'tinymem-ledger-'));
  try {
    const path = join(directory, 'usage.jsonl');
    const ledger = openLedger(path);
    ledger.append({ operation: 'embedding', usage: null }); ledger.close();
    assert.deepEqual(JSON.parse(readFileSync(path, 'utf8')), { operation: 'embedding', usage: null });
    chmodSync(path, 0o644);
    assert.throws(() => openLedger(path), /ledger_unavailable/);
    writeFileSync(join(directory, 'target'), '');
    symlinkSync(join(directory, 'target'), join(directory, 'link'));
    assert.throws(() => openLedger(join(directory, 'link')), /ledger_unavailable/);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});

test('embedding executable fails clearly before network with no stdout or leaked secrets', () => {
  const result = spawnSync(process.execPath, [new URL('../embed.mjs', import.meta.url).pathname], {
    env: { PATH: process.env.PATH, OPENAI_API_KEY: environment().OPENAI_API_KEY },
    input: JSON.stringify(input), encoding: 'utf8', timeout: 5000 });
  assert.equal(result.status, 1);
  assert.equal(result.stdout, '');
  assert.match(result.stderr, /missing_configuration.*CF_AIG_TOKEN/);
  assert.ok(!result.stderr.includes(environment().OPENAI_API_KEY));
});
