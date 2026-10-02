import { POLICY, GatewayError } from './policy.mjs';
import { chatUsage } from './gateway.mjs';

// Restrict SDK model access, not just defaults/cycling. Reuse Pi's native
// Responses transport, conversion, tools, streaming, cancellation and usage.
export async function createGatewayRuntime(gateway, agentDir) {
  const { ModelRuntime } = await import('@earendil-works/pi-coding-agent');
  const runtime = await ModelRuntime.create({ authPath: `${agentDir}/auth.json`,
    modelsPath: null, modelsStorePath: `${agentDir}/catalog.json`,
    allowModelNetwork: false, refreshOnCreate: false });
  runtime.registerProvider(POLICY.provider, {
    baseUrl: gateway.config.baseUrl, api: 'openai-responses',
    models: [{ id: POLICY.chatModel, name: 'TinyMem approved gateway chat',
      reasoning: false, input: ['text'], contextWindow: 128000, maxTokens: 8192,
      cost: { input: POLICY.prices.input, output: POLICY.prices.output,
        cacheRead: POLICY.prices.cacheRead, cacheWrite: POLICY.prices.cacheWrite } }],
  });
  await runtime.setRuntimeApiKey(POLICY.provider, gateway.config.providerKey);
  const model = Object.freeze(runtime.getModel(POLICY.provider, POLICY.chatModel));
  const assertModel = candidate => {
    if (!candidate || candidate.id !== model.id || candidate.provider !== model.provider ||
      candidate.api !== model.api || candidate.baseUrl !== model.baseUrl) throw new GatewayError('model_not_allowed');
  };
  const call = (simple, candidate, context, options = {}) => {
    assertModel(candidate);
    const operation = gateway.operation('chat');
    const method = simple ? 'streamSimple' : 'stream';
    const stream = runtime[method](model, context, { ...options,
      transport: 'sse', cacheRetention: 'none', maxRetries: 0,
      timeoutMs: gateway.timeoutMs, fetch: operation.fetch,
      onPayload: async payload => {
        const replaced = await options.onPayload?.(payload, model);
        return { ...(replaced ?? payload), model: POLICY.chatModel, stream: true, store: false, service_tier: 'default' };
      },
      onProviderStreamEvent: async (event, currentModel) => {
        if (['response.completed', 'response.incomplete'].includes(event?.type) && event.response?.model !== POLICY.chatModel) {
          throw new GatewayError('response_model_not_allowed');
        }
        const usage = event?.response?.usage;
        if (['response.completed', 'response.incomplete'].includes(event?.type) && usage &&
          ['input_tokens', 'output_tokens', 'total_tokens'].every(key => Number.isSafeInteger(usage[key]) && usage[key] >= 0) &&
          usage.total_tokens === usage.input_tokens + usage.output_tokens) operation.markUsageReported();
        await options.onProviderStreamEvent?.(event, currentModel);
      },
    });
    // result() observes completion without competing with the event consumer.
    // Do not log model text, errors, headers or tool arguments.
    const accounting = stream.result().then(message => {
      const usage = operation.usageReported ? chatUsage(message.usage) : null;
      if (!usage && !['error', 'aborted'].includes(message.stopReason)) {
        message.stopReason = 'error';
        message.errorMessage = 'TinyMem gateway: missing_chat_usage';
      } else if (['error', 'aborted'].includes(message.stopReason)) {
        // The native error may echo a provider response. Keep it out of model
        // history/compaction as well as stdout and the usage ledger.
        message.errorMessage = 'TinyMem gateway: model_request_failed';
      }
      operation.finish(usage,
        message.stopReason === 'stop' || message.stopReason === 'toolUse' ? 'ok' : message.stopReason);
      return message;
    });
    gateway.accounting.push(accounting);
    accounting.catch(() => {}); // The managed runner awaits these after settling.
    // complete/completeSimple consumers await accounting before returning.
    stream.result = () => accounting;
    return stream;
  };
  const blocked = new Set(['classify', 'generateImages', 'streamDeferred', 'fetchDeferred', 'cancelDeferred',
    'registerProvider', 'registerNativeProvider', 'registerVirtualModel', 'unregisterProvider', 'unregisterVirtualModel',
    'getRegisteredNativeProvider', 'setRuntimeApiKey', 'removeRuntimeApiKey', 'login', 'logout', 'refresh']);
  const provider = { id: POLICY.provider, name: POLICY.provider, baseUrl: model.baseUrl,
    getModels: () => [model], getAllModels: () => [model],
    stream: (candidate, context, options) => call(false, candidate, context, options),
    streamSimple: (candidate, context, options) => call(true, candidate, context, options) };
  const restricted = new Proxy(runtime, {
    get(target, property) {
      if (blocked.has(property)) return () => { throw new GatewayError('operation_not_allowed'); };
      if (['stream', 'streamSimple', 'complete', 'completeSimple'].includes(property)) {
        return (candidate, context, options) => {
          const stream = call(property.endsWith('Simple'), candidate, context, options);
          return property.startsWith('complete') ? stream.result() : stream;
        };
      }
      if (['getModels', 'getAllModels', 'getAvailable', 'getAllAvailable', 'getAvailableSnapshot'].includes(property)) {
        return provider => provider && provider !== POLICY.provider ? [] : [model];
      }
      if (['getModelsOfType', 'getAllAvailableOfType', 'getAvailableOfType'].includes(property)) {
        return (type, provider) => type === 'chat' && (!provider || provider === POLICY.provider) ? [model] : [];
      }
      if (property === 'getModelOfType') {
        return (type, provider, id) => type === 'chat' && provider === model.provider && id === model.id ? model : undefined;
      }
      if (property === 'getProviders') return () => [provider];
      if (property === 'getProvider') return id => id === POLICY.provider ? provider : undefined;
      if (['getModel', 'getPhysicalModel'].includes(property)) {
        return (provider, id) => provider === model.provider && id === model.id ? model : undefined;
      }
      if (property === 'resolveModel') return async candidate => { assertModel(candidate); return { model, thinkingLevel: 'off' }; };
      const value = Reflect.get(target, property, target);
      return typeof value === 'function' ? value.bind(target) : value;
    },
  });
  return { runtime: restricted, model };
}
