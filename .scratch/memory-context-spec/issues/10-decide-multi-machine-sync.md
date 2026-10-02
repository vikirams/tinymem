# Decide multi-machine sync

Type: grilling
Status: resolved
Blocked by: 01

## Question

Does v1 of the spec include multi-machine sync (for example git-backed markdown), later, or never? If not never, what is the source of truth: the SQLite file, files, or both?

## Answer

**The Store is the source of truth** (one local file, today SQLite). v1 has **no sync protocol**. Copying the file is allowed and unspecified. Git-backed markdown and dual-write are out of this spec. Sync, if ever, is a later spec of the same Store, not a second record.
