// This policy is deliberately not an arbitrary provider/model/URL registry.
export const POLICY = Object.freeze({
  id: 'tinymem-cloudflare-openai-v1',
  provider: 'tinymem-cloudflare-openai',
  chatModel: 'gpt-4.1-mini-2025-04-14',
  embeddingModel: 'text-embedding-3-small',
  embeddingIdentity: 'openai/text-embedding-3-small/d1536/raw-utf8-v1',
  dimensions: 1536,
  // USD per million tokens. Estimates, not a reconciled invoice.
  prices: Object.freeze({ input: 0.40, output: 1.60, cacheRead: 0.10, cacheWrite: 0.40, embedding: 0.02 }),
});

export class GatewayError extends Error {
  constructor(code, detail = '') {
    super(`TinyMem gateway: ${code}${detail ? ` (${detail})` : ''}`);
    this.name = 'GatewayError';
    this.code = code;
  }
}

export function preflight(env = process.env) {
  const required = ['CF_ACCOUNT_ID', 'CF_GATEWAY_ID', 'CF_AIG_TOKEN', 'OPENAI_API_KEY',
    'TINYMEM_GATEWAY_APPROVAL', 'TINYMEM_USAGE_LEDGER'];
  // Error details contain only names, never values (even for invalid configuration).
  const missing = required.filter(name => !env[name]?.trim());
  if (missing.length) throw new GatewayError('missing_configuration', missing.join(', '));
  if (!/^[a-f0-9]{32}$/.test(env.CF_ACCOUNT_ID)) throw new GatewayError('invalid_configuration', 'CF_ACCOUNT_ID');
  if (!/^[a-z0-9][a-z0-9-]{0,63}$/.test(env.CF_GATEWAY_ID)) throw new GatewayError('invalid_configuration', 'CF_GATEWAY_ID');
  if (env.TINYMEM_GATEWAY_APPROVAL !== POLICY.id) throw new GatewayError('policy_not_approved', 'TINYMEM_GATEWAY_APPROVAL');
  for (const name of ['CF_AIG_TOKEN', 'OPENAI_API_KEY']) {
    if (!/^[A-Za-z0-9_.-]{16,512}$/.test(env[name]) || /REPLACE|YOUR_|placeholder/i.test(env[name])) {
      throw new GatewayError('invalid_credentials', name);
    }
  }
  if (!env.TINYMEM_USAGE_LEDGER.startsWith('/') || env.TINYMEM_USAGE_LEDGER.includes('\0')) {
    throw new GatewayError('invalid_configuration', 'TINYMEM_USAGE_LEDGER must be an absolute path');
  }
  if (['HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY', 'http_proxy', 'https_proxy', 'all_proxy',
    'NODE_USE_ENV_PROXY', 'NODE_OPTIONS', 'NODE_EXTRA_CA_CERTS'].some(name => env[name])) {
    throw new GatewayError('unsupported_environment', 'proxy or Node transport overrides');
  }
  return Object.freeze({
    baseUrl: `https://gateway.ai.cloudflare.com/v1/${env.CF_ACCOUNT_ID}/${env.CF_GATEWAY_ID}/openai`,
    gatewayToken: env.CF_AIG_TOKEN,
    providerKey: env.OPENAI_API_KEY,
    ledgerPath: env.TINYMEM_USAGE_LEDGER,
  });
}

// Children receive an allowlist, not a blacklist of known provider credentials.
export function childEnvironment(env = process.env, home = env.HOME) {
  const result = { PATH: env.PATH || '/usr/bin:/bin', HOME: home, LANG: 'en_US.UTF-8',
    PI_OFFLINE: '1', PI_SKIP_VERSION_CHECK: '1', PI_TELEMETRY: '0' };
  if (env.SystemRoot) result.SystemRoot = env.SystemRoot;
  return result;
}

export function embeddingEnvironment(config, env = process.env, home = env.HOME) {
  const parts = new URL(config.baseUrl).pathname.split('/');
  return { ...childEnvironment(env, home), CF_ACCOUNT_ID: parts[2], CF_GATEWAY_ID: parts[3],
    CF_AIG_TOKEN: config.gatewayToken, OPENAI_API_KEY: config.providerKey,
    TINYMEM_GATEWAY_APPROVAL: POLICY.id, TINYMEM_USAGE_LEDGER: config.ledgerPath };
}
