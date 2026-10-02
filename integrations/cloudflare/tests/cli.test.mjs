import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { environment } from './helpers.mjs';

function run(script, env = {}, args = []) {
  return spawnSync(process.execPath, [new URL(`../${script}.mjs`, import.meta.url).pathname, ...args], {
    env: { PATH: process.env.PATH, ...env }, input: '', encoding: 'utf8', timeout: 5000 });
}

test('required live entry points fail closed before inference when credentials/approval are absent', () => {
  for (const script of ['pi-chat', 'preflight', 'embed']) {
    const result = run(script, { OPENAI_API_KEY: environment().OPENAI_API_KEY });
    assert.equal(result.status, 1);
    assert.equal(result.stdout, '');
    assert.match(result.stderr, /missing_configuration.*CF_AIG_TOKEN/);
    assert.ok(!result.stderr.includes(environment().OPENAI_API_KEY));
  }
  const unapproved = run('pi-chat', { ...environment(), TINYMEM_GATEWAY_APPROVAL: 'pending' });
  assert.equal(unapproved.status, 1);
  assert.match(unapproved.stderr, /policy_not_approved/);
});

test('preflight checks local readiness only, never reports live validation or zero-cost success', () => {
  const root = mkdtempSync(join(tmpdir(), 'tinymem-preflight-'));
  try {
    const env = environment(join(root, 'usage.jsonl'));
    const result = run('preflight', env);
    assert.equal(result.status, 0);
    const report = JSON.parse(result.stdout);
    assert.equal(report.live_verified, false);
    assert.match(report.status, /ready-for-live-check/);
    assert.ok(!result.stdout.includes(env.CF_AIG_TOKEN));
    assert.ok(!result.stdout.includes(env.OPENAI_API_KEY));
    assert.equal(readFileSync(env.TINYMEM_USAGE_LEDGER, 'utf8'), '');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('gateway provisioning requires an explicit action and separate admin credentials', () => {
  const noConsent = run('provision');
  assert.equal(noConsent.status, 1);
  assert.match(noConsent.stderr, /explicit_provisioning_required/);
  const missingAdmin = run('provision', environment(), ['--create-private-test-gateway']);
  assert.equal(missingAdmin.status, 1);
  assert.match(missingAdmin.stderr, /CF_AIG_ADMIN_TOKEN/);
  assert.ok(!missingAdmin.stderr.includes(environment().CF_AIG_TOKEN));
});
