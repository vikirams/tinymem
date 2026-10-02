# TinyMem Cloudflare model access

Restricted Pi 0.99.2 native Responses transport and a gateway embedding executable for Rust `CommandEmbedder`. One explicit chat/embedding policy; no direct/local fallback, no transcript ingestion or TinyMem summary trees.

See [setup, privacy policy, approval, credentials and live acceptance](../../docs/PI_GATEWAY_TESTING.md). No live gateway was provisioned/verified during implementation because credentials were absent. General coding tools are intentionally disabled until OS isolation is supplied.

From the repository root:

```sh
cargo build --locked -p tinymem-engine
npm ci --prefix integrations/cloudflare --ignore-scripts
npm test --prefix integrations/cloudflare
```

- `provision.mjs`: explicit private test gateway creation, separate Write token.
- `preflight.mjs`: local configuration/approval/private-ledger checks; no inference.
- `pi-chat.mjs`: required, opt-in paid/live Pi transport/MCP smoke; fresh in-memory session.
- `embed.mjs`: executable JSON stdin/vector stdout adapter (Node built-ins only).
- `policy.mjs`, `gateway.mjs`, `pi-model.mjs`, `pi-session.mjs`: checked policy, HTTP/usage, existing Pi transport and trusted MCP access.

Tests use fake HTTP plus the real native/Pi transports. The scripted save/fresh-session recall check is a protocol test, **not** the tested-code-change milestone or evidence of natural memory effectiveness. Ledger costs are explicit estimates; failures with unreported usage remain unknown until reconciled.
