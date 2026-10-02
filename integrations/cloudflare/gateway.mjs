import { constants, openSync, closeSync, fstatSync, writeSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
import { POLICY, GatewayError } from './policy.mjs';

const REQUEST_LIMIT = 1024 * 1024;
const RESPONSE_LIMIT = 4 * 1024 * 1024;

// Open before inference; never follow a symlink or silently lose accounting.
export function openLedger(path) {
  let fd;
  try {
    fd = openSync(path, constants.O_WRONLY | constants.O_APPEND | constants.O_CREAT | constants.O_NOFOLLOW, 0o600);
    const stat = fstatSync(fd);
    if (!stat.isFile() || (stat.mode & 0o077)) throw new Error('not private');
  } catch {
    if (fd !== undefined) closeSync(fd);
    throw new GatewayError('ledger_unavailable', 'use a private regular file (mode 0600)');
  }
  return {
    append(entry) {
      // Only module-constructed entries reach this sink; never accept raw events.
      const bytes = Buffer.from(JSON.stringify(entry) + '\n');
      try {
        if (writeSync(fd, bytes) !== bytes.length) throw new Error('partial write');
      } catch { throw new GatewayError('ledger_write_failed'); }
    },
    close() { closeSync(fd); },
  };
}

function correlation(headers, secrets) {
  const id = headers.get('cf-aig-log-id');
  return id && /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/i.test(id) &&
    !secrets.some(secret => id.includes(secret)) ? id : null;
}

function validCount(value) { return Number.isSafeInteger(value) && value >= 0; }

export function chatUsage(raw) {
  if (!raw || !['input', 'output', 'cacheRead', 'cacheWrite'].every(key => validCount(raw[key]))) return null;
  const { input, output, cacheRead, cacheWrite } = raw;
  const rates = POLICY.prices;
  return { input, output, cache_read: cacheRead, cache_write: cacheWrite,
    total_tokens: input + output + cacheRead + cacheWrite,
    estimated_cost_usd: (input * rates.input + output * rates.output + cacheRead * rates.cacheRead + cacheWrite * rates.cacheWrite) / 1e6 };
}

// The only production HTTP dependency is Node's fetch. Tests inject a fake HTTP
// transport here, not an environment-configurable alternate origin.
export class Gateway {
  constructor(config, { fetch = globalThis.fetch, ledger = openLedger(config.ledgerPath), timeoutMs = 25000 } = {}) {
    if (!/^https:\/\/gateway\.ai\.cloudflare\.com\/v1\/[a-f0-9]{32}\/[a-z0-9][a-z0-9-]{0,63}\/openai$/.test(config.baseUrl)) {
      throw new GatewayError('route_not_allowed');
    }
    this.config = config;
    this.accounting = [];
    this.transport = fetch;
    this.ledger = ledger;
    if (!Number.isInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 25000) throw new GatewayError('invalid_deadline');
    this.timeoutMs = timeoutMs;
  }

  operation(kind) {
    if (!['chat', 'embedding'].includes(kind)) throw new GatewayError('operation_not_allowed');
    const config = this.config;
    const expectedUrl = `${config.baseUrl}/${kind === 'chat' ? 'responses' : 'embeddings'}`;
    const expectedModel = kind === 'chat' ? POLICY.chatModel : POLICY.embeddingModel;
    const requestId = randomUUID();
    const started = performance.now();
    let status = null, gatewayId = null, attempted = false, recorded = false;
    let reportedUsage = false;
    const finish = (usage = null, outcome = 'ok') => {
      if (recorded) return;
      recorded = true;
      this.ledger.append({ version: 1, policy: POLICY.id, request_id: requestId, operation: kind,
        model: expectedModel, gateway_log_id: gatewayId, http_status: status, attempted,
        outcome, duration_ms: Math.round(performance.now() - started), usage,
        cost_basis: usage ? 'configured-price-estimate' : 'unknown-until-reconciled' });
    };
    const checkedFetch = async (input, init) => {
      try {
        const request = new Request(input, init);
        // Exact comparison rejects credentials in URL, ports, query strings,
        // another account/gateway/provider, normalization tricks and other paths.
        if (request.url !== expectedUrl || request.method !== 'POST') throw new GatewayError('route_not_allowed');
        const bytes = new Uint8Array(await request.arrayBuffer());
        if (bytes.length > REQUEST_LIMIT) throw new GatewayError('request_too_large');
        let payload;
        try { payload = JSON.parse(Buffer.from(bytes).toString('utf8')); }
        catch { throw new GatewayError('invalid_request'); }
        if (payload.model !== expectedModel) throw new GatewayError('model_not_allowed');
        if (kind === 'chat' && (payload.stream !== true || payload.store !== false ||
          (payload.service_tier && payload.service_tier !== 'default'))) throw new GatewayError('request_policy_violation');
        if (attempted) throw new GatewayError('retry_not_allowed');
        const headers = new Headers(request.headers);
        // Drop caller-defined metadata and overwrite privacy/auth policy.
        for (const name of [...headers.keys()]) if (name.startsWith('cf-aig-') || name.startsWith('cf-')) headers.delete(name);
        headers.set('authorization', `Bearer ${config.providerKey}`);
        headers.set('cf-aig-authorization', `Bearer ${config.gatewayToken}`);
        headers.set('cf-aig-skip-cache', 'true');
        headers.set('cf-aig-max-attempts', '1');
        headers.set('cf-aig-request-timeout', String(this.timeoutMs));
        headers.set('cf-aig-collect-log', 'true');
        headers.set('cf-aig-collect-log-payload', 'false');
        headers.set('cf-aig-metadata', JSON.stringify({ request_id: requestId, operation: kind }));
        const signal = AbortSignal.any([request.signal, AbortSignal.timeout(this.timeoutMs)]);
        attempted = true;
        const response = await this.transport(expectedUrl, { method: 'POST', headers, body: bytes, redirect: 'manual', signal });
        status = response.status;
        gatewayId = correlation(response.headers, [config.gatewayToken, config.providerKey]);
        if (status >= 300 && status < 400) {
          await response.body?.cancel();
          throw new GatewayError('redirect_not_allowed');
        }
        if (!response.ok) {
          await response.body?.cancel(); // Never echo provider/gateway error bodies.
          throw new GatewayError(`http_${status}`);
        }
        const reader = response.body?.getReader();
        if (!reader) throw new GatewayError('empty_response');
        let size = 0;
        // A total-operation deadline includes body consumption, not only headers.
        const aborted = new Promise((_, reject) => {
          const abort = () => { void reader.cancel().catch(() => {}); reject(new GatewayError('request_aborted')); };
          if (signal.aborted) abort();
          else signal.addEventListener('abort', abort, { once: true });
        });
        aborted.catch(() => {});
        const body = new ReadableStream({
          async pull(controller) {
            try {
              const { value, done } = await Promise.race([reader.read(), aborted]);
              if (done) { controller.close(); return; }
              size += value.length;
              if (size > (kind === 'embedding' ? 128 * 1024 : RESPONSE_LIMIT)) throw new GatewayError('response_too_large');
              controller.enqueue(value);
            } catch (error) {
              await reader.cancel().catch(() => {});
              const safe = error instanceof GatewayError ? error : new GatewayError(signal.aborted ? 'request_aborted' : 'response_failed');
              finish(null, safe.code);
              controller.error(safe);
            }
          },
          cancel(reason) { return reader.cancel(reason); },
        });
        return new Response(body, { status, headers: response.headers });
      } catch (error) {
        const safe = error instanceof GatewayError ? error : new GatewayError('request_failed');
        finish(null, safe.code);
        throw safe;
      }
    };
    return { fetch: checkedFetch, finish,
      markUsageReported() { reportedUsage = true; },
      get usageReported() { return reportedUsage; } };
  }

  async embed(request, signal) {
    if (!request || Object.keys(request).sort().join(',') !== 'model,text' ||
      request.model !== POLICY.embeddingIdentity || typeof request.text !== 'string' ||
      !request.text.trim() || Buffer.byteLength(request.text) > 65536) throw new GatewayError('invalid_embedding_request');
    const operation = this.operation('embedding');
    let usage = null;
    try {
      const response = await operation.fetch(`${this.config.baseUrl}/embeddings`, {
        method: 'POST', headers: { 'content-type': 'application/json' }, signal,
        body: JSON.stringify({ model: POLICY.embeddingModel, input: request.text,
          dimensions: POLICY.dimensions, encoding_format: 'float' }),
      });
      let data;
      try { data = await response.json(); } catch { throw new GatewayError('invalid_embedding_response'); }
      if (!data || typeof data !== 'object' || Array.isArray(data)) throw new GatewayError('invalid_embedding_response');
      if (validCount(data.usage?.prompt_tokens) && data.usage.total_tokens === data.usage.prompt_tokens) {
        usage = { input: data.usage.prompt_tokens, output: 0, cache_read: 0, cache_write: 0,
          total_tokens: data.usage.total_tokens, estimated_cost_usd: data.usage.prompt_tokens * POLICY.prices.embedding / 1e6 };
      }
      const vector = data.data?.[0]?.embedding;
      if (data.model !== POLICY.embeddingModel || !Array.isArray(data.data) || data.data.length !== 1 || data.data[0].index !== 0 ||
        !Array.isArray(vector) || vector.length !== POLICY.dimensions ||
        !vector.every(value => typeof value === 'number' && Number.isFinite(value) && Number.isFinite(Math.fround(value))) ||
        !vector.some(value => Math.fround(value) !== 0)) throw new GatewayError('invalid_embedding_response');
      if (!usage) throw new GatewayError('missing_embedding_usage');
      operation.finish(usage);
      return { embedding: vector };
    } catch (error) {
      const safe = error instanceof GatewayError ? error : new GatewayError('embedding_failed');
      operation.finish(usage, safe.code);
      throw safe;
    }
  }
}
