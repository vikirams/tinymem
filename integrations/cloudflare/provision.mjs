// Explicit management-plane operation; never used by the model/embedding runner.
// The operator supplies a separate AI Gateway Write token, never an agent tool.
import { GatewayError } from './policy.mjs';

try {
  if (process.argv.slice(2).join(' ') !== '--create-private-test-gateway') {
    throw new GatewayError('explicit_provisioning_required', '--create-private-test-gateway');
  }
  const { CF_ACCOUNT_ID: account, CF_GATEWAY_ID: id, CF_AIG_ADMIN_TOKEN: token } = process.env;
  if (!/^[a-f0-9]{32}$/.test(account ?? '') || !/^[a-z0-9][a-z0-9-]{0,63}$/.test(id ?? '') ||
    !/^[A-Za-z0-9_.-]{16,512}$/.test(token ?? '')) {
    throw new GatewayError('missing_or_invalid_provisioning_configuration', 'CF_ACCOUNT_ID, CF_GATEWAY_ID, CF_AIG_ADMIN_TOKEN');
  }
  const response = await fetch(`https://api.cloudflare.com/client/v4/accounts/${account}/ai-gateway/gateways`, {
    method: 'POST', redirect: 'manual', signal: AbortSignal.timeout(15000),
    headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' },
    body: JSON.stringify({ id, authentication: true, collect_logs: false, cache_ttl: 0,
      cache_invalidate_on_update: true, rate_limiting_interval: 60, rate_limiting_limit: 60,
      rate_limiting_technique: 'fixed', retry_max_attempts: 1 }),
  });
  if (!response.ok) { await response.body?.cancel(); throw new GatewayError(`provisioning_http_${response.status}`); }
  const reader = response.body.getReader();
  const chunks = [];
  let size = 0;
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    size += value.length;
    if (size > 128 * 1024) { await reader.cancel(); throw new GatewayError('provisioning_response_too_large'); }
    chunks.push(value);
  }
  const data = JSON.parse(Buffer.concat(chunks).toString('utf8'));
  const gateway = data.result;
  if (data.success !== true || gateway?.id !== id || gateway.authentication !== true ||
    gateway.collect_logs !== false || gateway.cache_ttl !== 0 || gateway.retry_max_attempts !== 1) {
    throw new GatewayError('provisioning_policy_not_confirmed', 'inspect gateway settings in dashboard');
  }
  console.log(JSON.stringify({ status: 'private-test-gateway-created', authentication: true,
    gateway_default_logs: false, cache_ttl: 0, retry_max_attempts: 1,
    next: 'Create a gateway Run token; approve the local model/data-transfer policy; run preflight and live smoke.' }));
} catch (error) {
  console.error(error instanceof GatewayError ? error.message : 'TinyMem gateway: provisioning_failed');
  process.exitCode = 1;
}
