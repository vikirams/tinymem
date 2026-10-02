import http from 'node:http';
import { once } from 'node:events';
import { POLICY, preflight } from '../policy.mjs';
import { Gateway } from '../gateway.mjs';

export function environment(ledgerPath = '/tmp/unused-tinymem-ledger.jsonl') {
  return { CF_ACCOUNT_ID: 'a'.repeat(32), CF_GATEWAY_ID: 'tinymem-private-test',
    CF_AIG_TOKEN: 'cf-test-secret-12345678', OPENAI_API_KEY: 'sk-test-secret-12345678',
    TINYMEM_GATEWAY_APPROVAL: POLICY.id, TINYMEM_USAGE_LEDGER: ledgerPath };
}

export async function fixture(handler, options = {}) {
  const requests = [], entries = [], errors = [];
  const server = http.createServer(async (request, response) => {
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    const body = JSON.parse(Buffer.concat(chunks).toString('utf8'));
    requests.push({ path: request.url, headers: request.headers, body });
    try { await handler(request, response, body, requests.length); }
    catch (error) { errors.push(error); response.statusCode = 599; response.end('fixture failed'); }
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const config = preflight(environment());
  const gateway = new Gateway(config, { ledger: { append: entry => entries.push(entry) },
    fetch: (url, init) => {
      if (!url.startsWith(config.baseUrl + '/')) throw new Error('unexpected logical URL');
      return fetch(`http://127.0.0.1:${server.address().port}${new URL(url).pathname}`, init);
    }, ...options });
  return { gateway, requests, entries, httpBaseUrl: `http://127.0.0.1:${server.address().port}`,
    async close() {
      server.closeAllConnections(); await new Promise(resolve => server.close(resolve));
      if (errors.length) throw errors[0];
    } };
}

export function vectorResponse(response, overrides = {}) {
  response.setHeader('content-type', 'application/json');
  response.setHeader('cf-aig-log-id', '12345678-1234-1234-1234-123456789012');
  response.end(JSON.stringify({ model: POLICY.embeddingModel,
    data: [{ index: 0, embedding: Array(POLICY.dimensions).fill(0.5) }],
    usage: { prompt_tokens: 3, total_tokens: 3 }, ...overrides }));
}

export function responseEvents({ text = 'Hello 漢字', tool, usage = true, status = 'completed', model = POLICY.chatModel } = {}) {
  const item = tool
    ? { id: 'fc_test', type: 'function_call', call_id: 'call_test', name: tool.name, arguments: JSON.stringify(tool.args) }
    : { id: 'msg_test', type: 'message', role: 'assistant', content: [{ type: 'output_text', text, annotations: [] }] };
  return [
    { type: 'response.created', response: { id: 'resp_test', status: 'in_progress' } },
    { type: 'response.output_item.added', output_index: 0, item: { ...item, content: tool ? undefined : [], arguments: tool ? '' : undefined } },
    ...(tool ? [{ type: 'response.function_call_arguments.delta', output_index: 0, delta: item.arguments }]
      : [{ type: 'response.output_text.delta', output_index: 0, delta: text }]),
    { type: 'response.output_item.done', output_index: 0, item },
    { type: status === 'incomplete' ? 'response.incomplete' : 'response.completed', response: {
      id: 'resp_test', model, status, output: [item], incomplete_details: status === 'incomplete' ? { reason: 'max_output_tokens' } : undefined,
      usage: usage ? { input_tokens: 12, output_tokens: 4, total_tokens: 16,
        input_tokens_details: { cached_tokens: 2 } } : undefined } },
  ];
}

export function sse(response, events = responseEvents()) {
  response.setHeader('content-type', 'text/event-stream');
  response.setHeader('cf-aig-log-id', '12345678-1234-1234-1234-123456789012');
  // Deliberately split UTF-8 bytes to exercise the *existing Pi* stream parser.
  const bytes = Buffer.from(events.map(event => `event: ${event.type}\ndata: ${JSON.stringify(event)}\n\n`).join(''));
  for (let index = 0; index < bytes.length; index += 7) response.write(bytes.subarray(index, index + 7));
  response.end();
}
