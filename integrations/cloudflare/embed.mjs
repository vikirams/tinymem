#!/usr/bin/env node
// CommandEmbedder protocol: one bounded JSON request on stdin, one JSON vector
// response on stdout. No payloads, headers or raw exceptions on stderr.
import { preflight, GatewayError } from './policy.mjs';
import { Gateway } from './gateway.mjs';
import { pathToFileURL } from 'node:url';

// The test seam changes the HTTP dependency, never production routing policy.
export async function runEmbedding(gateway, input = process.stdin, output = process.stdout) {
  let size = 0;
  const chunks = [];
  for await (const chunk of input) {
    size += chunk.length;
    if (size > 128 * 1024) throw new GatewayError('stdin_too_large');
    chunks.push(chunk);
  }
  let request;
  try { request = JSON.parse(Buffer.concat(chunks).toString('utf8')); }
  catch { throw new GatewayError('invalid_embedding_request'); }
  output.write(JSON.stringify(await gateway.embed(request)) + '\n');
}

async function main() {
  let gateway;
  const timeout = setTimeout(() => {
    process.stderr.write('TinyMem gateway: adapter_deadline\n');
    process.exit(1);
  }, 27000);
  try {
    gateway = new Gateway(preflight());
    await runEmbedding(gateway);
  } catch (error) {
    process.stderr.write((error instanceof GatewayError ? error.message : 'TinyMem gateway: adapter_failed') + '\n');
    process.exitCode = 1;
  } finally {
    clearTimeout(timeout);
    gateway?.ledger.close();
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) await main();
