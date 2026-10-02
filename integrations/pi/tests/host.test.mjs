// Runs the real Pi SDK with an in-process deterministic provider. No accounts,
// model requests, private agent config, persistent sessions or network required.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { existsSync, mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import test from 'node:test';
import { Client } from '../../../sdks/typescript/index.ts';

process.env.PI_OFFLINE = '1';
process.env.PI_SKIP_VERSION_CHECK = '1';
process.env.PI_TELEMETRY = '0';

const require = createRequire(import.meta.url);
let piPath = process.env.TINYMEM_PI_MODULE;
if (!piPath) {
  const local = require.resolve.paths('@earendil-works/pi-coding-agent')
    .map(directory => join(directory, '@earendil-works/pi-coding-agent/dist/index.js')).find(existsSync);
  piPath = local;
}
if (!piPath) {
  try {
    piPath = join(execFileSync('npm', ['root', '-g'], { encoding: 'utf8' }).trim(), '@earendil-works/pi-coding-agent/dist/index.js');
    if (!existsSync(piPath)) piPath = undefined;
  } catch { piPath = undefined; }
}
if (!piPath && process.env.TINYMEM_REQUIRE_PI === '1') throw new Error('Install Pi 1.0.0 or set TINYMEM_PI_MODULE to its module entry');
const binary = resolve(process.env.TINYMEM_BINARY ?? 'target/debug/tinymem');

test('real Pi runtime: new-session recall, request-local evidence, freshness and command lifecycle', { skip: !piPath, timeout: 30000 }, async () => {
  const pi = await import(pathToFileURL(piPath).href);
  const fromPi = createRequire(piPath);
  // Pi AI is import-only, so require.resolve cannot resolve its main export.
  const aiPackage = fromPi.resolve.paths('@earendil-works/pi-ai')
    .map(directory => join(directory, '@earendil-works/pi-ai/package.json')).find(existsSync);
  assert.ok(aiPackage, 'Pi AI must be available beside the installed Pi runtime');
  const ai = await import(pathToFileURL(join(dirname(aiPackage), 'dist/index.js')).href);
  const root = mkdtempSync(join(tmpdir(), 'tinymem-pi-host-'));
  const cwd = join(root, 'repository');
  const agentDir = join(root, 'agent');
  mkdirSync(join(cwd, '.pi'), { recursive: true });
  mkdirSync(agentDir);
  const db = join(root, 'memory.db');
  writeFileSync(join(cwd, '.pi/mcp.json'), JSON.stringify({ mcpServers: { tinymem: {
    command: binary, args: ['--db', db, 'mcp', cwd, '--user', 'fixture', '--project', 'fixture'], exposure: 'deferred',
  } } }));
  const dependency = join(cwd, '.pi', 'dependency.txt');
  writeFileSync(dependency, 'UTC\n');
  const native = new Client({ binary, db, repository: cwd, userId: 'fixture', projectId: 'fixture' });
  const errors = [];
  const requests = [];
  let mutationExecutions = 0;
  let nestedBlocked = false;
  let runtime;

  async function open() {
    const settingsManager = pi.SettingsManager.inMemory({ compaction: { enabled: false }, retry: { enabled: false }, cacheWarming: 'off', defaultTools: [] });
    settingsManager.setProjectTrusted(true);
    const resourceLoader = new pi.DefaultResourceLoader({
      cwd, agentDir, settingsManager,
      additionalExtensionPaths: [resolve(process.env.TINYMEM_PI_EXTENSION ?? 'integrations/pi/index.ts')],
      extensionFactories: [extension => {
        // A deferred native-shaped tool proves the gate runs on nested calls,
        // before execution, without relying on a second MCP connection.
        extension.registerTool({ name: 'mcp__tinymem__add_memory', label: 'Fixture write', description: 'Fixture only', exposure: 'deferred', parameters: ai.Type.Object({}),
          async execute() { mutationExecutions++; return { content: [{ type: 'text', text: 'write executed' }], details: undefined }; } });
        extension.registerTool({ name: 'memory_gate_fixture', label: 'Fixture parent', description: 'Fixture only', parameters: ai.Type.Object({}),
          async execute(_id, _args, _signal, _update, ctx) {
            const outcome = await ctx.executeTool('mcp__tinymem__add_memory', {});
            nestedBlocked = outcome.isError;
            return { content: [{ type: 'text', text: outcome.isError ? 'write blocked' : 'write executed' }], details: undefined };
          } });
      }],
      noSkills: true, noPromptTemplates: true, noContextFiles: true, noThemes: true,
    });
    await resourceLoader.reload({ resolveProjectTrust: async () => true });
    assert.deepEqual(resourceLoader.getExtensions().errors, []);
    const modelRuntime = await pi.ModelRuntime.create({ authPath: join(agentDir, 'auth.json'), modelsPath: join(agentDir, 'models.json') });
    modelRuntime.registerProvider('tinymem-fixture', {
      baseUrl: 'http://127.0.0.1:1/not-used', apiKey: 'fixture-not-a-secret', api: 'tinymem-fixture',
      models: [{ id: 'deterministic', name: 'Offline fixture', reasoning: false, input: ['text'], cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 }, contextWindow: 100000, maxTokens: 100 }],
      streamSimple(model, context) {
        requests.push(structuredClone(context.messages));
        const stream = ai.createAssistantMessageEventStream();
        const gate = context.messages.at(-1)?.role === 'user' && JSON.stringify(context.messages.at(-1).content).includes('gate_fixture');
        const message = {
          role: 'assistant', content: gate ? [{ type: 'toolCall', id: 'fixture-call', name: 'memory_gate_fixture', arguments: {} }] : [{ type: 'text', text: 'fixture done' }],
          api: model.api, provider: model.provider, model: model.id, stopReason: gate ? 'toolUse' : 'stop', timestamp: Date.now(),
          usage: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, totalTokens: 0, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } },
        };
        queueMicrotask(() => { stream.push({ type: 'start', partial: message }); stream.push({ type: 'done', reason: message.stopReason, message }); stream.end(message); });
        return stream;
      },
    });
    const { session } = await pi.createAgentSession({
      cwd, agentDir, settingsManager, resourceLoader, modelRuntime,
      model: modelRuntime.getModel('tinymem-fixture', 'deterministic'), thinkingLevel: 'off',
      sessionManager: pi.SessionManager.inMemory(cwd), noTools: true,
    });
    await session.bindExtensions({ mode: 'print', onError: error => errors.push(error) });
    return session;
  }
  function evidence(request) {
    return request.filter(message => {
      const text = typeof message.content === 'string' ? message.content : Array.isArray(message.content) ? message.content.filter(block => block.type === 'text').map(block => block.text).join('') : '';
      return text.startsWith('Task context:');
    });
  }
  try {
    // Intentional write through real Pi's slash-command pipeline.
    runtime = await open();
    await runtime.prompt('/memory remember clock uses UTC, never local time.');
    assert.equal(requests.length, 0, 'slash commands must not issue model requests');
    const saved = (await native.searchMemories('clock'))[0];
    assert.ok(saved);
    runtime.dispose();
    runtime = await open();
    await runtime.prompt('Implement clock timestamps.');
    assert.equal(evidence(requests.at(-1)).length, 1);
    assert.ok(JSON.stringify(evidence(requests.at(-1))).includes('UTC'));
    assert.ok(!runtime.messages.some(message => message.customType === 'tinymem:context'), 'injection must not enter persisted session messages');
    await runtime.prompt(`/memory correct ${saved.id} 1 clock uses ISO UTC notation.`);
    await runtime.prompt('Implement clock timestamps.');
    assert.equal(evidence(requests.at(-1)).length, 1);
    assert.ok(JSON.stringify(evidence(requests.at(-1))).includes('ISO UTC'));
    assert.ok(!JSON.stringify(requests.at(-1)).includes('never local time.'), 'old injected context must not accumulate');
    await runtime.prompt(`/memory delete ${saved.id} 2`);
    await runtime.prompt('Implement clock timestamps.');
    assert.equal(evidence(requests.at(-1)).length, 0);
    const dependent = await native.addMemory('clock uses UTC from the fixture dependency.', { source_refs: [{ kind: 'file', role: 'dependency', path: '.pi/dependency.txt' }] });
    await runtime.prompt('Implement clock timestamps.');
    assert.equal(evidence(requests.at(-1)).length, 1);
    writeFileSync(dependency, 'local\n');
    await runtime.prompt('Implement clock timestamps.');
    assert.equal(evidence(requests.at(-1)).length, 0);
    const history = await native.memoryHistory(dependent.id);
    assert.equal(history.length, 1);
    assert.equal(history[0].freshness.status, 'stale');
    await runtime.prompt('/memory off');
    writeFileSync(dependency, 'UTC\n');
    await runtime.prompt('Implement clock timestamps.');
    assert.equal(evidence(requests.at(-1)).length, 0);
    await runtime.prompt('gate_fixture');
    assert.equal(nestedBlocked, true, 'nested calls must pass the intentional-write gate');
    assert.equal(mutationExecutions, 0, 'deferred write must never execute');
    assert.deepEqual(errors, []);
  } finally { runtime?.dispose(); rmSync(root, { recursive: true, force: true }); }
});
