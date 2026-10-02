import { preflight, POLICY, GatewayError } from './policy.mjs';
import { openLedger } from './gateway.mjs';

try {
  const config = preflight();
  const ledger = openLedger(config.ledgerPath);
  ledger.close();
  console.log(JSON.stringify({ status: 'ready-for-live-check', policy: POLICY.id,
    chat_model: POLICY.chatModel, embedding_model: POLICY.embeddingModel,
    embedding_identity: POLICY.embeddingIdentity, dimensions: POLICY.dimensions,
    privacy: 'selected text leaves machine; metadata-only gateway logs; no response cache',
    live_verified: false }));
} catch (error) {
  console.error(error instanceof GatewayError ? error.message : 'TinyMem gateway: preflight_failed');
  process.exitCode = 1;
}
