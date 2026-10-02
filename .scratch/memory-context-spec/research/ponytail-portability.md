# Research: Ponytail portability

Question: How does [DietrichGebert/ponytail](https://github.com/DietrichGebert/ponytail) make one core behavior load in many coding agents (adapters, hooks, `AGENTS.md`), as stated by its own docs?

Sources: that repo only. Snapshotted at commit `6c97ffa48710714672214915ad092e3fc6e85516` (package version `4.10.1` in `package.json`). Raw files: `https://raw.githubusercontent.com/DietrichGebert/ponytail/main/<path>`.

This note is for later TinyMem product decisions (adapter surface, injection). It is not a spec and does not implement TinyMem.

---

## 1. Product one-liner

**Ponytail is a coding-taste skill, not a memory store.** It puts a “lazy senior developer” persona into coding agents so they climb a YAGNI / stdlib / native / one-line ladder before writing code. ([README.md](https://github.com/DietrichGebert/ponytail/blob/main/README.md); [docs/agent-portability.md](https://github.com/DietrichGebert/ponytail/blob/main/docs/agent-portability.md))

It stores no user facts, has no retrieval, and has no SQLite. Its MCP server only *serves the same instruction text* as a prompt or a tool — explicitly not always-on memory. ([ponytail-mcp/README.md](https://github.com/DietrichGebert/ponytail/blob/main/ponytail-mcp/README.md))

TinyMem should steal the **distribution pattern** (one core, thin host adapters). It should not steal the **ruleset**.

---

## 2. Core vs adapter split — where the real behavior lives

Ponytail’s own rule, in [docs/agent-portability.md](https://github.com/DietrichGebert/ponytail/blob/main/docs/agent-portability.md):

> Ponytail is an agent-portable skill distribution. The skills in `skills/` hold the core behavior; host-specific files are adapters that make that behavior easy to load in a given agent.
>
> Keep adapters thin. When a host supports skills or hooks, point it at the existing `skills/` and `hooks/` files. When a host only supports project instructions, keep its copied rule text aligned with `AGENTS.md`.

### Core (one source of truth)

| Artifact | Role |
|----------|------|
| [`skills/ponytail/SKILL.md`](https://github.com/DietrichGebert/ponytail/blob/main/skills/ponytail/SKILL.md) | Runtime ruleset: ladder, persistence, intensity table (`lite`/`full`/`ultra`), output format, safety carve-outs. Frontmatter `description` is written so skill-capable hosts can auto-invoke it on coding tasks. |
| [`hooks/ponytail-instructions.js`](https://github.com/DietrichGebert/ponytail/blob/main/hooks/ponytail-instructions.js) | Shared instruction builder. Reads `skills/ponytail/SKILL.md`, strips frontmatter, filters intensity-table rows and quoted examples to the active mode, prefixes `PONYTAIL MODE ACTIVE — level: …`. Fallback inline copy if the skill file is missing. Comment at top: “Shared Ponytail instruction builder for Claude hooks and Pi extension.” OpenCode and Cursor reuse it too. |
| [`hooks/ponytail-config.js`](https://github.com/DietrichGebert/ponytail/blob/main/hooks/ponytail-config.js) | Default mode from `PONYTAIL_DEFAULT_MODE` then `~/.config/ponytail/config.json` (`defaultMode`), else `full`. |
| [`hooks/ponytail-runtime.js`](https://github.com/DietrichGebert/ponytail/blob/main/hooks/ponytail-runtime.js) | Host detection + **stdout shape**. Same Node scripts; JSON vs raw text differs per host. |
| [`AGENTS.md`](https://github.com/DietrichGebert/ponytail/blob/main/AGENTS.md) | Compact always-on instruction set for hosts without skills. Same ladder and safety carve-outs, shorter than `SKILL.md` (no intensity table, no Persistence/Output/Boundaries sections). |
| Sibling skills under `skills/` | Optional commands, not the always-on persona: `ponytail-review`, `ponytail-audit`, `ponytail-debt`, `ponytail-gain`, `ponytail-help`. Listed as “Portable Behavior” in [docs/agent-portability.md](https://github.com/DietrichGebert/ponytail/blob/main/docs/agent-portability.md). |

`check-rule-copies.js` states the split in code: “SKILL.md is the runtime source of truth and is longer than the compact body, so it cannot be byte-compared.” Compact copies must match `AGENTS.md` (minus a repo-self note). Load-bearing phrases must appear in both `SKILL.md` and `AGENTS.md`. ([scripts/check-rule-copies.js](https://github.com/DietrichGebert/ponytail/blob/main/scripts/check-rule-copies.js))

### Adapters (thin; point, copy, or wrap)

Host files do **not** re-implement the ladder. They either:

1. **Point** a plugin manifest at `skills/` and `hooks/` (Claude, Codex, Copilot CLI, Qoder, Grok, Devin).
2. **Call** `getPonytailInstructions()` from a host plugin (OpenCode, pi).
3. **Copy** the `AGENTS.md` body into a host-shaped rule file (Cursor `.mdc`, Windsurf, Cline, Copilot Chat, Kiro, Qoder rules, `.agents/rules/`).
4. **Rewrite frontmatter only**, keep the skill body (OpenClaw: `scripts/build-openclaw-skills.js`).

Exception: Hermes [`__init__.py`](https://github.com/DietrichGebert/ponytail/blob/main/__init__.py) reimplements `build_injected_context` / `_filter_skill_body_for_mode` in Python instead of calling the JS builder. It still *reads* `skills/ponytail/SKILL.md`. That is the thickest adapter.

### Not core

- [`commands/*.toml`](https://github.com/DietrichGebert/ponytail/tree/main/commands) — slash-command wrappers that paraphrase the ladder for Gemini/Claude command menus. Not the source of truth.
- [`docs/platform-native.md`](https://github.com/DietrichGebert/ponytail/blob/main/docs/platform-native.md) — cheat sheet for rung 4 of the ladder (HTML/CSS/stdlib substitutes). Product content, not a loader.
- [`benchmarks/`](https://github.com/DietrichGebert/ponytail/tree/main/benchmarks) — marketing measurement of the ruleset.
- Statusline scripts under `hooks/ponytail-statusline.*` — Claude-only UX.

---

## 3. How each host class loads it

Ponytail groups hosts into **plugin-tier** (skills + hooks or native injection) vs **instruction-tier** (always-on rule file, no `/ponytail` levels). README: “Commands need a skill-capable host… The instruction-only adapters load the always-on ruleset without the commands.” ([README.md](https://github.com/DietrichGebert/ponytail/blob/main/README.md) § Commands)

### Class A — Plugin manifest + shared Node hooks

Manifest points at existing `skills/` and a **host-named hook map**. Hook *scripts* are shared; the JSON event names are the adapter.

| Host | Manifest | Hook map | What the manifest does |
|------|----------|----------|------------------------|
| Claude Code | [`.claude-plugin/plugin.json`](https://github.com/DietrichGebert/ponytail/blob/main/.claude-plugin/plugin.json) | [`hooks/claude-codex-hooks.json`](https://github.com/DietrichGebert/ponytail/blob/main/hooks/claude-codex-hooks.json) | `"hooks": "./hooks/claude-codex-hooks.json"`. Skills are auto-discovered from `skills/` by Claude’s plugin layout. Install: `/plugin marketplace add DietrichGebert/ponytail` then `/plugin install ponytail@ponytail`. |
| Codex | [`.codex-plugin/plugin.json`](https://github.com/DietrichGebert/ponytail/blob/main/.codex-plugin/plugin.json) | same `claude-codex-hooks.json` | Explicit `"skills": "./skills/"`, `"hooks": "./hooks/claude-codex-hooks.json"`. Same two Node scripts. |
| Copilot CLI | [`.github/plugin/plugin.json`](https://github.com/DietrichGebert/ponytail/blob/main/.github/plugin/plugin.json) | [`hooks/copilot-hooks.json`](https://github.com/DietrichGebert/ponytail/blob/main/hooks/copilot-hooks.json) | `"commands": "commands/"`, `"skills": "skills/"`, `"hooks": "hooks/copilot-hooks.json"`. Events are Copilot-named (`sessionStart`, `userPromptSubmitted`) with bash/powershell command strings. Output shape: `{ additionalContext }` on SessionStart only ([`ponytail-runtime.js`](https://github.com/DietrichGebert/ponytail/blob/main/hooks/ponytail-runtime.js)). |
| Qoder | [`.qoder-plugin/plugin.json`](https://github.com/DietrichGebert/ponytail/blob/main/.qoder-plugin/plugin.json) | [`hooks/qoder-hooks.json`](https://github.com/DietrichGebert/ponytail/blob/main/hooks/qoder-hooks.json) (template; user copies into `.qoder/settings.json`) | Points at `skills/`, `.qoder/rules/`, and the template. Qoder has **no SessionStart**; `UserPromptSubmit` both activates and injects every turn ([`ponytail-mode-tracker.js`](https://github.com/DietrichGebert/ponytail/blob/main/hooks/ponytail-mode-tracker.js) comment). Subagents: `PreToolUse` matcher `task\|Task` → `ponytail-subagent.js`. |

Claude/Codex hook map ([`hooks/claude-codex-hooks.json`](https://github.com/DietrichGebert/ponytail/blob/main/hooks/claude-codex-hooks.json)):

- `SessionStart` matcher `startup|resume|clear|compact` → `ponytail-activate.js`
- `SubagentStart` → `ponytail-subagent.js`
- `UserPromptSubmit` → `ponytail-mode-tracker.js`

`${CLAUDE_PLUGIN_ROOT}` (or Copilot `${PLUGIN_ROOT}`) is the only host-specific path token. Cursor’s template uses a `PONYTAIL_DIR` placeholder filled by an installer ([`scripts/cursor-hooks.js`](https://github.com/DietrichGebert/ponytail/blob/main/scripts/cursor-hooks.js)).

### Class B — Native host plugin that calls the shared builder

No Claude-style hook JSON. The adapter is a small plugin that injects `getPonytailInstructions(mode)` into the host’s own prompt pipeline.

| Host | Adapter | Injection seam |
|------|---------|----------------|
| OpenCode | [`.opencode/plugins/ponytail.mjs`](https://github.com/DietrichGebert/ponytail/blob/main/.opencode/plugins/ponytail.mjs) (`package.json` `"main"`) | V2: `ctx.session.hook('context', …)` pushes a system text part every turn. V1: `experimental.chat.system.transform` appends to `output.system`. Also registers commands/skills by *reading* `skills/*/SKILL.md` and `.opencode/command/*.md`. Checkout auto-load: `.opencode/plugins/index.js` re-exports the same module. |
| pi | [`pi-extension/index.js`](https://github.com/DietrichGebert/ponytail/blob/main/pi-extension/index.js); `package.json` `"pi": { "extensions": ["./pi-extension/index.js"], "skills": ["./skills"] }` | `pi.on("before_agent_start")` prepends instructions to `event.systemPrompt`. Mode persisted as a session custom entry, not a flag file. |
| Hermes | [`plugin.yaml`](https://github.com/DietrichGebert/ponytail/blob/main/plugin.yaml) + [`__init__.py`](https://github.com/DietrichGebert/ponytail/blob/main/__init__.py) | `pre_llm_call` returns `{ context }`. `pre_gateway_dispatch` rewrites `/ponytail-*` into “load skill `ponytail:…`” prompts. Registers `skills/*/SKILL.md` as `ponytail:<name>`. Python duplicate of the filter, still file-backed on `SKILL.md`. |

### Class C — Skills-only plugin (hooks cannot inject)

**Grok Build.** Root [`plugin.json`](https://github.com/DietrichGebert/ponytail/blob/main/plugin.json) is `{ "name": "ponytail" }`. [`.grok-plugin/marketplace.json`](https://github.com/DietrichGebert/ponytail/blob/main/.grok-plugin/marketplace.json) points `"source": "./"`. [docs/agent-portability.md](https://github.com/DietrichGebert/ponytail/blob/main/docs/agent-portability.md): “Grok can auto-invoke ponytail from its coding-task skill description; `/ponytail` makes activation explicit. **Grok lifecycle hooks are not used because passive hook output cannot inject instructions.**” README repeats: SessionStart output cannot inject. Fallback: `AGENTS.md` from a checkout.

**Devin CLI.** [`.devin-plugin/plugin.json`](https://github.com/DietrichGebert/ponytail/blob/main/.devin-plugin/plugin.json) is metadata only (no hooks key). Skills invoked as `/ponytail:ponytail`, etc. ([README.md](https://github.com/DietrichGebert/ponytail/blob/main/README.md))

**Gemini CLI / Antigravity.** [`gemini-extension.json`](https://github.com/DietrichGebert/ponytail/blob/main/gemini-extension.json): `"contextFileName": "AGENTS.md"`. Reuses `commands/*.toml` and `skills/`. Intentionally **does not** ship `hooks/hooks.json` because Gemini auto-loads that path, while Ponytail’s lifecycle hooks use Claude/Codex event names. ([README.md](https://github.com/DietrichGebert/ponytail/blob/main/README.md); [docs/agent-portability.md](https://github.com/DietrichGebert/ponytail/blob/main/docs/agent-portability.md))

**Swival / OpenClaw.** Skill directories. Swival: `swival skills add` copies the six skills into `.swival/skills/`. OpenClaw: generated copies under `.openclaw/skills/` with shorter `description` (≤160 chars); body copied verbatim ([`scripts/build-openclaw-skills.js`](https://github.com/DietrichGebert/ponytail/blob/main/scripts/build-openclaw-skills.js)). Instruction-tier fallback: `AGENTS.md`.

### Class D — Instruction-tier copies of `AGENTS.md`

Byte-aligned (after stripping host frontmatter) by [`scripts/check-rule-copies.js`](https://github.com/DietrichGebert/ponytail/blob/main/scripts/check-rule-copies.js):

| Host | File |
|------|------|
| Cursor (rule-only alternative) | [`.cursor/rules/ponytail.mdc`](https://github.com/DietrichGebert/ponytail/blob/main/.cursor/rules/ponytail.mdc) (`alwaysApply: true`) |
| Windsurf | `.windsurf/rules/ponytail.md` |
| Cline | `.clinerules/ponytail.md` |
| Antigravity workspace rules | `.agents/rules/ponytail.md` |
| Qoder project rules | `.qoder/rules/ponytail.md` |
| GitHub Copilot Chat | `.github/copilot-instructions.md` |
| Kiro | `.kiro/steering/ponytail.md` |

Hosts that just **read repo-root `AGENTS.md`** with zero extra files: Antigravity, CodeWhale, VS Code Codex extension, Amp, Jules, Zed, generic agents, Copilot CLI fallback, Junie (after pointing Guidelines Path). ([docs/agent-portability.md](https://github.com/DietrichGebert/ponytail/blob/main/docs/agent-portability.md) table)

Instruction-tier = always-on compact ruleset, **no** lite/full/ultra switches, **no** hooks.

### Class E — Cursor native hooks (plugin-tier without a Cursor plugin)

Cursor has no Ponytail plugin package. Installer merges [`hooks/cursor-hooks.json`](https://github.com/DietrichGebert/ponytail/blob/main/hooks/cursor-hooks.json) into `~/.cursor/hooks.json` (or project `.cursor/hooks.json`). Events: `sessionStart` → activate, `beforeSubmitPrompt` → mode tracker. Same Node scripts as Claude. ([docs/cursor-hooks.md](https://github.com/DietrichGebert/ponytail/blob/main/docs/cursor-hooks.md); [docs/agent-portability.md](https://github.com/DietrichGebert/ponytail/blob/main/docs/agent-portability.md))

Hooks and `.cursor/rules/ponytail.mdc` are **alternatives, not layers**. If the rule file exists in the workspace, hooks inject a one-line notice and refuse level switches — a rule cannot be removed from context, so `off`/`lite`/`ultra` would contradict it. ([docs/cursor-hooks.md](https://github.com/DietrichGebert/ponytail/blob/main/docs/cursor-hooks.md); [`ponytail-activate.js`](https://github.com/DietrichGebert/ponytail/blob/main/hooks/ponytail-activate.js))

### Class F — MCP prompt/tool (not always-on)

[`ponytail-mcp/README.md`](https://github.com/DietrichGebert/ponytail/blob/main/ponytail-mcp/README.md):

> It is not a replacement for the always-on adapters. Ponytail normally lives in the system context every turn. MCP prompts are user-invoked, and **there is no portable MCP primitive for "inject this into every turn" across hosts.** So this server is the clean option for MCP hosts whose only injection point is the prompt menu, or that pull context through tools.

Exposes prompt `ponytail` and tool `ponytail_instructions`. Thin map onto `buildInstructions`. Reuses `hooks/ponytail-config.js` for default mode. Directly relevant to TinyMem: Ponytail’s authors already decided MCP-alone cannot be the always-on loader.

---

## 4. SessionStart / SubagentStart (and equivalents)

Goal: the ruleset is in context **without the user invoking a skill every turn**. Hosts differ in whether they can do that, and whether **subagents** inherit it.

### Claude Code / Codex (canonical)

| Event | Script | Injects |
|-------|--------|---------|
| `SessionStart` | [`hooks/ponytail-activate.js`](https://github.com/DietrichGebert/ponytail/blob/main/hooks/ponytail-activate.js) | Writes `~/.claude/.ponytail-active` (or Codex `PLUGIN_DATA`) with default mode; stdout = mode-filtered `SKILL.md`. `off` → no flag, no rules. Claude also gets a one-time statusline nudge. |
| `SubagentStart` | [`hooks/ponytail-subagent.js`](https://github.com/DietrichGebert/ponytail/blob/main/hooks/ponytail-subagent.js) | Same instructions if mode flag is active. Comment: “SessionStart context is parent-thread only and never reaches subagents” (issue #252). Optional `PONYTAIL_SUBAGENT_MATCHER` regex on `agent_type`; unset → all subagents; bad regex / missing type **fail open** (inject). |
| `UserPromptSubmit` | [`hooks/ponytail-mode-tracker.js`](https://github.com/DietrichGebert/ponytail/blob/main/hooks/ponytail-mode-tracker.js) | Parses `/ponytail lite|full|ultra|off` (also `@`/`$` prefixes). Writes flag. Emits `PONYTAIL MODE CHANGED` / `PONYTAIL MODE OFF`. Does **not** re-inject the full ruleset on Claude (skill body already loaded); Cursor branch does re-inject because Cursor has no `/ponytail` skill command. |

Stdout is host-shaped in [`writeHookOutput`](https://github.com/DietrichGebert/ponytail/blob/main/hooks/ponytail-runtime.js):

- Claude SessionStart: **raw text**.
- Claude `SubagentStart`: JSON `{ hookSpecificOutput: { hookEventName, additionalContext } }` — “or the context is dropped.”
- Codex: `{ systemMessage, hookSpecificOutput: { additionalContext } }`.
- Copilot: `{ additionalContext }` on SessionStart only; empty object otherwise.
- Qoder: Codex-like `hookSpecificOutput` without `systemMessage`.
- Cursor: `{ additional_context }` ; on prompt-submit also `{ continue: true }`. Empty stdout = nothing. Raw text is a parse error.

### Cursor equivalents and gaps

Documented in [docs/cursor-hooks.md](https://github.com/DietrichGebert/ponytail/blob/main/docs/cursor-hooks.md) (Cursor 3.20.17 client, 2026-09-14):

| Cursor event | Ponytail | Delivery |
|--------------|----------|----------|
| `sessionStart` | yes | `additional_context` stored on the conversation (`hooksAdditionalContext`) and sent with every later request of that chat. |
| `beforeSubmitPrompt` | yes | `additional_context` as a system reminder for that turn. **Undocumented** in Cursor’s public hook docs (docs list only `continue` / `user_message`); client accepts it. Inline cap 10k chars. |
| `subagentStart` | **not registered** | Output schema is `permission` / `user_message` only. Client protobuf has an unused `additional_context` slot the hook path never fills. **Subagents never get the ruleset.** `PONYTAIL_SUBAGENT_MATCHER` has no effect. |
| Cloud agents | `sessionStart` does not fire | No startup injection. Project-level `beforeSubmitPrompt` still runs, so a typed `/ponytail <level>` can set the rest of that conversation. |

### Other equivalents

| Host | Seam | Subagents |
|------|------|-----------|
| OpenCode | every-turn system transform / `context` hook | Not a separate SubagentStart; injection is per chat turn. |
| pi | `before_agent_start` → `systemPrompt` | Same prepend whenever an agent starts. |
| Hermes | `pre_llm_call` → `{ context }` | Every LLM turn in that process. Mode is process-local ([`after-install.md`](https://github.com/DietrichGebert/ponytail/blob/main/after-install.md)). |
| Qoder | `UserPromptSubmit` (no SessionStart) + `PreToolUse` `task\|Task` | Subagent injection via PreToolUse, not SubagentStart. |
| Grok | none | Skill auto-invoke / explicit `/ponytail` only. |
| Gemini | `contextFileName: AGENTS.md` | Always-on file, not a lifecycle hook. |
| Instruction-tier | host reads the file | Whatever that host does for project rules; no Ponytail subagent logic. |

---

## 5. Pattern TinyMem could copy (without the lazy-senior ruleset)

TinyMem’s core is already a **stdio MCP server + SQLite**, not a persona. Ponytail’s portable object is **instruction text**. Map the *loader* pattern onto TinyMem’s *store*, not the YAGNI ladder.

### Copy

1. **One core artifact, many pointers.** Ponytail: `skills/` + `getPonytailInstructions()`. TinyMem: the Python MCP process and (separately) a single **discipline/skill file** that teaches when to `get_profile` / `search_memories` / `add_memory`. Host adapters should point at those, not fork tool semantics per agent. ([docs/agent-portability.md](https://github.com/DietrichGebert/ponytail/blob/main/docs/agent-portability.md) Adapter Rule)

2. **Three loader tiers, not one.** Plugin+hooks (always-on), native plugin (system-prompt seam), instruction-tier (`AGENTS.md`-class file). TinyMem already has a fourth that Ponytail treats as weaker: **MCP config writers** (`tinymem connect` → `.cursor/mcp.json` etc.). Keep MCP as the *tool* surface. Do not expect MCP registration alone to inject memories every turn — Ponytail’s own MCP README says there is no portable “inject every turn” MCP primitive. ([ponytail-mcp/README.md](https://github.com/DietrichGebert/ponytail/blob/main/ponytail-mcp/README.md))

3. **Thin hook maps, shared scripts, host-shaped stdout.** One activate / one “on prompt” / one subagent script; JSON templates only change event names and path tokens (`CLAUDE_PLUGIN_ROOT`, `PONYTAIL_DIR`, Copilot bash vs powershell). A TinyMem hook, if ticket 07 wants injection, should emit **retrieved profile/memories**, not coding taste, and isolate Cursor vs Claude JSON in a `writeHookOutput`-style branch. ([`hooks/claude-codex-hooks.json`](https://github.com/DietrichGebert/ponytail/blob/main/hooks/claude-codex-hooks.json), [`hooks/ponytail-runtime.js`](https://github.com/DietrichGebert/ponytail/blob/main/hooks/ponytail-runtime.js))

4. **Subagent is a separate event.** Parent SessionStart does not reach Task-spawned agents on Claude; Cursor cannot inject on `subagentStart` at all. If TinyMem injects, decide explicitly: parent only, Claude `SubagentStart` / Qoder `PreToolUse`, or accept Cursor subagents are blind. ([`ponytail-subagent.js`](https://github.com/DietrichGebert/ponytail/blob/main/hooks/ponytail-subagent.js); [docs/cursor-hooks.md](https://github.com/DietrichGebert/ponytail/blob/main/docs/cursor-hooks.md))

5. **Instruction copies stay aligned with a checker.** Compact `AGENTS.md` vs host files (`.mdc`, `.clinerules`, copilot-instructions) are the same body plus host frontmatter. `check-rule-copies.js` fails CI on drift; OpenClaw copies are *generated* from `skills/`. TinyMem’s “when to read/write” skill should have one canonical file and generated/checked copies — not six hand-edited README fragments. ([`scripts/check-rule-copies.js`](https://github.com/DietrichGebert/ponytail/blob/main/scripts/check-rule-copies.js); [`scripts/build-openclaw-skills.js`](https://github.com/DietrichGebert/ponytail/blob/main/scripts/build-openclaw-skills.js))

6. **Do not stack always-on rule + hooks.** Cursor adapter treats them as alternatives because a project rule cannot be turned off. If TinyMem ships both an `AGENTS.md`/`SKILL.md` and SessionStart injection, define coexistence (Ponytail: hooks stand down and say so). ([docs/cursor-hooks.md](https://github.com/DietrichGebert/ponytail/blob/main/docs/cursor-hooks.md) § Coexistence)

7. **Installer merges, does not clobber.** Cursor installer keeps unrelated `hooks.json` entries, replaces only commands matching `ponytail-*.js`, refuses malformed JSON and unsafe paths. A TinyMem hook installer should merge the same way. ([`scripts/cursor-hooks.js`](https://github.com/DietrichGebert/ponytail/blob/main/scripts/cursor-hooks.js))

8. **Skip hosts whose hooks cannot inject.** Grok: no hook map. Gemini: don’t drop Claude-shaped `hooks/hooks.json` on a path the host auto-loads. Prefer `AGENTS.md` / skill description auto-invoke there. ([docs/agent-portability.md](https://github.com/DietrichGebert/ponytail/blob/main/docs/agent-portability.md))

### Decision inputs for ticket 11 (adapter surface)

Ponytail’s surface is **not** “connect config writers only.” It is:

- **Core behavior file(s)** (`skills/`, compact `AGENTS.md`)
- **Optional lifecycle hooks** where the host can inject context
- **Copied instruction files** where it cannot
- **MCP as a last-resort prompt/tool**, documented as insufficient for always-on

TinyMem today is closer to Ponytail’s Class F (MCP) plus config writers. Stealing Ponytail portability means adding Classes A–D **around** the MCP core (skill/instruction that *calls tools*, optional hooks that *pre-fill* search results), not replacing the server with a ruleset.

---

## 6. What to skip

TinyMem’s map already lists “Ponytail's lazy-senior coding ruleset (portability pattern only)” as out of destination. Concretely skip:

| Skip | Why / where it lives |
|------|----------------------|
| The ladder and persona | `skills/ponytail/SKILL.md`, `AGENTS.md`, all rule copies. Product is coding taste, not memory. |
| Intensity levels (`lite`/`full`/`ultra`/`off`) and `/ponytail` mode flag files | Mode tracker, `PONYTAIL_DEFAULT_MODE`, `~/.config/ponytail/config.json`. TinyMem has no analogous “how aggressive is memory” knob in this pattern. |
| Review / audit / debt / gain / help skills and `commands/*.toml` | Over-engineering review, repo audit, `ponytail:` comment harvest, benchmark scoreboard. Unrelated to a memory store. |
| `ponytail:` ceiling comments | Ruleset convention, not a storage schema. |
| Benchmarks and “~54% less code” claims | [`benchmarks/`](https://github.com/DietrichGebert/ponytail/tree/main/benchmarks), README numbers. Measure a coding skill, not retrieval. |
| `docs/platform-native.md` | HTML/CSS/stdlib substitution table for the ladder. |
| Statusline badge and Claude `settings.json` nudge | [`ponytail-activate.js`](https://github.com/DietrichGebert/ponytail/blob/main/hooks/ponytail-activate.js), `hooks/ponytail-statusline.*`. |
| Pairing with “caveman” terse-prose | README FAQ. |
| Hermes Python reimplementation of the filter | Anti-pattern if TinyMem wants one core; keep one builder. |
| Treating MCP as always-on injection | Ponytail already tried this and documented the gap ([ponytail-mcp/README.md](https://github.com/DietrichGebert/ponytail/blob/main/ponytail-mcp/README.md), issue #70). |

---

## Source index

- Portability contract: https://github.com/DietrichGebert/ponytail/blob/main/docs/agent-portability.md
- Cursor hook contract: https://github.com/DietrichGebert/ponytail/blob/main/docs/cursor-hooks.md
- README install/commands: https://github.com/DietrichGebert/ponytail/blob/main/README.md
- Core skill: https://github.com/DietrichGebert/ponytail/blob/main/skills/ponytail/SKILL.md
- Compact instructions: https://github.com/DietrichGebert/ponytail/blob/main/AGENTS.md
- Shared builder: https://github.com/DietrichGebert/ponytail/blob/main/hooks/ponytail-instructions.js
- Hook maps: `hooks/claude-codex-hooks.json`, `hooks/cursor-hooks.json`, `hooks/copilot-hooks.json`, `hooks/qoder-hooks.json`
- MCP caveat: https://github.com/DietrichGebert/ponytail/blob/main/ponytail-mcp/README.md
- Copy checker: https://github.com/DietrichGebert/ponytail/blob/main/scripts/check-rule-copies.js
