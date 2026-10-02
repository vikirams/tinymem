# Research Ponytail portability

Type: research
Status: resolved

## Question

How does [DietrichGebert/ponytail](https://github.com/DietrichGebert/ponytail) make one core behavior load in many coding agents (adapters, hooks, `AGENTS.md`), as stated by its own docs?

## Answer

One skill file plus thin adapters in three loader tiers (hooks, native plugins, byte-aligned `AGENTS.md` copies). MCP cannot inject every turn. Detail: [ponytail-portability.md](../research/ponytail-portability.md).
