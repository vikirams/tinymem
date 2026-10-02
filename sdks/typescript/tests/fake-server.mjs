#!/usr/bin/env node
// Controllable system boundary, not a native memory emulator.
import assert from 'node:assert/strict';
import { createInterface } from 'node:readline';
import { writeFileSync } from 'node:fs';
const args = process.argv.slice(2);
const mode = args[args.indexOf('--user') + 1];
if (args.includes('--db')) writeFileSync(args[args.indexOf('--db') + 1], String(process.pid));
if (mode === 'ignore-term') process.on('SIGTERM', () => {});
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
if (mode === 'init-hang') await sleep(60_000);
async function emit(value) {
  const frame = Buffer.from(JSON.stringify(value) + '\n');
  if (mode === 'fragmented') {
    for (let start = 0; start < frame.length; start += 7) {
      process.stdout.write(frame.subarray(start, start + 7));
      await sleep(1);
    }
  } else process.stdout.write(frame);
}
const messages = [];
for await (const line of createInterface({ input: process.stdin })) {
  const request = JSON.parse(line);
  messages.push(request);
  let result;
  if (request.method === 'initialize') {
    assert.equal(request.id, 1);
    assert.equal(request.params.protocolVersion, '2025-06-18');
    assert.deepEqual(request.params.capabilities, {});
    if (mode === 'stderr-flood') process.stderr.write('e'.repeat(8192));
    if (mode === 'startup-exit') { process.stderr.write('intentional startup failure\n'); process.exit(7); }
    result = { protocolVersion: mode === 'wrong-version' ? 'wrong' : '2025-06-18', capabilities: { tools: {} }, serverInfo: { name: 'fixture', version: '1' } };
    if (mode === 'bad-server-info') result.serverInfo = { name: 7 };
  } else if (request.method === 'notifications/initialized') {
    assert.ok(!('id' in request));
    if (mode === 'no-read') { process.stdin.pause(); await sleep(60_000); }
    continue;
  } else {
    assert.equal(request.method, 'tools/call');
    assert.equal(request.id, 2);
    assert.equal(messages.at(-2).method, 'notifications/initialized');
    if (mode === 'tool-hang') await sleep(60_000);
    if (['invalid-json', 'invalid-utf8', 'frame-flood', 'partial-frame'].includes(mode)) {
      const frame = mode === 'invalid-utf8' ? Buffer.from([255, 10]) : mode === 'invalid-json' ? 'not-json\n' : mode === 'frame-flood' ? 'x'.repeat(8192) + '\n' : '{"jsonrpc":';
      process.stdout.write(frame);
      continue;
    }
    if (['rpc-error', 'bad-rpc-error'].includes(mode)) {
      await emit({ jsonrpc: '2.0', id: 2, error: { code: mode === 'bad-rpc-error' ? 'invalid' : -32602, message: 'invalid arguments', data: { detail: 1 } } });
      continue;
    }
    if (mode === 'notification-flood') for (let i = 0; i < 100; i++) await emit({ jsonrpc: '2.0', method: 'notifications/progress', params: { v: 'x'.repeat(100) } });
    if (['notifications', 'malformed-notification'].includes(mode)) {
      const notification = { jsonrpc: '2.0', method: 'notifications/message', params: { message: 'hello' } };
      if (mode === 'malformed-notification') notification.result = {};
      await emit(notification);
    }
    let structuredContent = { arguments: request.params.arguments, argv: args, pid: process.pid };
    if (mode === 'wrappers') {
      const record = { id: 'fixture-id', scope: { user_id: mode, agent_id: null, project_id: null },
        content: JSON.stringify(request.params.arguments), kind: 'fact', source: request.params.name,
        expires_at_ms: null, version: 1, created_at_ms: 1, updated_at_ms: 1 };
      const report = { repository: '/fixture', indexed: 0, unchanged: 0, removed: 0, skipped: 0, indexed_at_ms: 1 };
      const event = { event: 'added', version: 1, at_ms: 1, memory: record };
      structuredContent = { memory: record, memories: [record], history: [event], deleted: true,
        stable: [record], dynamic: [], ...report, refresh: report, hits: [] };
      if (request.params.name === 'assemble_context') {
        const prompt = JSON.stringify(request.params.arguments);
        structuredContent = { prompt, entries: [], token_budget: request.params.arguments.token_budget,
          tokens_used: Buffer.byteLength(prompt), token_count_method: 'utf8_bytes_upper_bound' };
      }
    }
    if (mode === 'null-memory') structuredContent = { memory: null };
    if (mode === 'large-result') structuredContent = { payload: 'x'.repeat(1024 * 1024 + 64) };
    const text = request.params.name === 'assemble_context' && 'prompt' in structuredContent
      ? structuredContent.prompt : JSON.stringify(structuredContent);
    result = { content: [{ type: 'text', text }], structuredContent, isError: false };
    if (mode === 'business-error') result = { content: [{ type: 'text', text: 'business failure' }], isError: true };
    if (mode === 'malformed-result') result.structuredContent = [];
    if (mode === 'wrong-id') request.id = 9;
  }
  const response = { jsonrpc: '2.0', id: request.id, result };
  await emit(response);
  if (mode === 'duplicate-id' && request.id === 2) await emit(response);
  if (mode === 'unsolicited-zero' && request.id === 2) await emit({ ...response, id: 0 });
}
if (mode === 'ignore-term') await sleep(60_000);
if (mode === 'exit-nonzero') { process.stderr.write('intentional exit after response\n'); process.exit(9); }
if (!['wrong-version', 'bad-server-info', 'startup-exit'].includes(mode)) assert.deepEqual(messages.map(message => message.method), ['initialize', 'notifications/initialized', 'tools/call']);
