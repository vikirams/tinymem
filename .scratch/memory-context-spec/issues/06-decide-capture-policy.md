# Decide capture policy

Type: grilling
Status: resolved
Blocked by: 01, 02, 05

## Question

What may be written into the store without the agent calling a Tool: nothing, opt-in session capture, or something else? If capture exists, what does it write (a Memory, or another noun)?

## Answer

**Nothing.** Capture is refused. Only Tools write (`add_memory`, `update_memory`, `set_profile`). Optional Hooks may inject Context or remind the agent to save; they must not persist. No LLM observer, no scraping of tool output. A Memory still only exists if the agent (or a human via the same Tools) writes it.
