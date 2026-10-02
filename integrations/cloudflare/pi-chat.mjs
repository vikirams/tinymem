import { mkdtempSync, mkdirSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, isAbsolute } from 'node:path';
import { parseArgs } from 'node:util';
import { preflight, childEnvironment, GatewayError } from './policy.mjs';
import { Gateway } from './gateway.mjs';
import { createManagedSession, shutdownManagedSession } from './pi-session.mjs';

let gateway, session, work;
const originalFetch = globalThis.fetch;
try {
  const { values } = parseArgs({ options: { binary: { type: 'string' }, db: { type: 'string' },
    root: { type: 'string' }, user: { type: 'string' }, project: { type: 'string' },
    embeddings: { type: 'boolean', default: false }, compact: { type: 'boolean', default: false } } });
  const config = preflight(); // Required live smoke: no skip, no direct fallback.
  const memoryNames = ['binary', 'db', 'root', 'user', 'project'];
  const withMemory = memoryNames.some(name => values[name]);
  if ((withMemory && memoryNames.some(name => !values[name])) || (!withMemory && values.embeddings)) {
    throw new GatewayError('invalid_memory_configuration', 'supply binary, db, root, user and project together');
  }
  if (withMemory && (!isAbsolute(values.db) || !isAbsolute(values.binary) || !isAbsolute(values.root))) {
    throw new GatewayError('invalid_memory_configuration', 'use absolute paths');
  }
  if (withMemory) { values.binary = realpathSync(values.binary); values.root = realpathSync(values.root); }
  work = mkdtempSync(join(tmpdir(), 'tinymem-gateway-'));
  const cwd = withMemory ? values.root : join(work, 'fixture');
  const agentDir = join(work, 'agent');
  mkdirSync(agentDir, { mode: 0o700 });
  if (!withMemory) mkdirSync(cwd);
  // Load credentials into the trusted transport closure, then remove *all*
  // ambient secrets and personal config from tool/MCP child environments.
  const safeEnv = childEnvironment(process.env, agentDir);
  for (const name of Object.keys(process.env)) delete process.env[name];
  Object.assign(process.env, safeEnv);
  gateway = new Gateway(config, { fetch: originalFetch });
  globalThis.fetch = async () => { throw new GatewayError('route_not_allowed'); };
  let bytes = 0;
  const chunks = [];
  for await (const chunk of process.stdin) {
    bytes += chunk.length;
    if (bytes > 65536) throw new GatewayError('prompt_too_large');
    chunks.push(chunk);
  }
  const prompt = Buffer.concat(chunks).toString('utf8').trim() || 'Reply with: gateway transport ready.';
  session = await createManagedSession(gateway, { cwd, agentDir,
    memory: withMemory ? { ...values } : undefined });
  let settled = false, failed = false, messages = 0;
  const tools = new Set();
  const unsubscribe = session.subscribe(event => {
    if (event.type === 'agent_settled') settled = true;
    if (event.type === 'tool_execution_start') tools.add(event.toolName);
    if (event.type === 'tool_execution_end' && event.isError) failed = true;
    if (event.type === 'message_end' && event.message.role === 'assistant') {
      messages++;
      if (['error', 'aborted', 'length'].includes(event.message.stopReason)) failed = true;
      if (messages >= 8) { failed = true; void session.abort(); }
    }
  });
  const deadline = setTimeout(() => { failed = true; void session.abort(); }, 90000);
  try {
    await session.prompt(prompt);
    if (values.compact && !failed) await session.compact();
    await Promise.all(gateway.accounting);
    if (!settled || failed || !messages) throw new GatewayError('pi_run_failed');
    console.log(JSON.stringify({ status: 'ok', session_id: session.sessionManager.getSessionId(),
      assistant_messages: messages, tool_names: [...tools], transcript_persisted: false,
      note: 'Transport smoke only; consult usage ledger. No code-change effectiveness claim.' }));
  } finally { clearTimeout(deadline); unsubscribe(); }
} catch (error) {
  console.error(error instanceof GatewayError ? error.message : 'TinyMem gateway: pi_smoke_failed');
  process.exitCode = 1;
} finally {
  if (session) await shutdownManagedSession(session).catch(() => { process.exitCode = 1; });
  globalThis.fetch = originalFetch;
  gateway?.ledger.close();
  if (work) rmSync(work, { recursive: true, force: true });
}
