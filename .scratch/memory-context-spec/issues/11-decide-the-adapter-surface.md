# Decide the adapter surface

Type: grilling
Status: resolved
Blocked by: 04

## Question

How does TinyMem express “one core, thin adapters” for every coding agent: `connect` config writers only, plus Hooks, plus instruction files, or another surface — without taking Ponytail's coding ruleset?

## Answer

**Three tiers, thin adapters.** Core is MCP + Store + one discipline file (read/write rules, not Ponytail taste). `connect` always writes MCP config. SessionStart Hooks and the discipline file are opt-in. v1 names the five hosts `connect` already writes: Pi, Cursor, Claude Code, VS Code, Codex. Further hosts are later adapters. Adapters must not fork Tools.
