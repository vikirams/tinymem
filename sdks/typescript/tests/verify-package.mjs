import assert from 'node:assert/strict';
import { chmodSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { Client } from '../dist/index.js';

const binary = fileURLToPath(new URL('./fake-server.mjs', import.meta.url));
chmodSync(binary, 0o755);
const client = new Client({ binary, userId: 'packaging' });
const result = await client.callTool('echo', { content: 'compiled' });
assert.deepEqual(result.arguments, { content: 'compiled' });
assert.throws(() => process.kill(result.pid, 0), { code: 'ESRCH' });
console.log('Compiled Node package import, public API and EOF reaping: PASS');
