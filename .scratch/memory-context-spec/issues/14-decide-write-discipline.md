# Decide write discipline

Type: grilling
Status: resolved

## Question

What may be written as a Memory now that Session and Observation are not stored types — length, kinds, when a recap or code fact is a Memory vs Profile vs refused?

## Answer

**1–2 sentences** (aim under 500 characters). Kinds stay **fact, preference, decision, plan**. Use `valid_until` for decay; no episode kind.

- **Memory** — a durable sentence that will still matter next week (including a durable code fact).
- **Profile** — stack, conventions, commands, short code map.
- **Refuse** — transcripts, tool logs, file bodies, same-day recaps, duplicates (`update_memory` instead).

The discipline file teaches: `get_profile` first; `search_memories` with 1–3 terms; `add_memory` only for Memories; `set_profile` for Profile.
