# Coding-agent memory

TinyMem preserves useful evidence across coding tasks and sessions. It distinguishes what was recorded, where it came from, and whether it still applies.

## Language

**Memory**:
An explicitly retained fact, preference, decision, plan, relationship, or solution attributable to an owner.
_Avoid_: Transcript, summary

**Evidence**:
Recorded information with an identifiable origin; it may be mistaken, outdated, or untrusted.
_Avoid_: Truth, instruction

**Memory scope**:
The exact owner, optional agent, and optional project to which a memory belongs. An unspecified dimension denotes a distinct global scope, not every scope.
_Avoid_: Tenant, permission

**Observation**:
One recorded version of a memory, including a recorded change or deletion.
_Avoid_: Access, inference

**Provenance**:
The origin and identity of evidence, sufficient to distinguish it from another observation or source.
_Avoid_: Confidence

**Source reference**:
An identified origin of evidence, including the version observed, such as a file, commit, pull request, or conversation.
_Avoid_: Source label, proof

**Validity dependency**:
Evidence whose continued applicability is required for a memory to be used as current. A memory's recorded origin is not necessarily a validity dependency.
_Avoid_: Origin, citation

**Freshness**:
Whether a memory's validity dependencies have been checked and remain applicable: current, stale, or unknown. Freshness does not establish factual correctness.
_Avoid_: Truth, recency

**Memory proposal**:
A candidate durable memory awaiting an explicit acceptance decision or policy. It is not yet accepted evidence for future tasks.
_Avoid_: Memory, automatic truth

**Correction**:
An explicit replacement of an identified prior memory's claim, retaining the relationship to what was corrected.
_Avoid_: Newest fact, silent overwrite

**Memory grant**:
Permission for an authenticated actor to perform specified operations on scoped evidence. A scope label alone does not confer a grant.
_Avoid_: Namespace, ownership label

**Profile**:
A bounded selection of current stable and dynamic memories, not a generated account of the owner.
_Avoid_: Biography, summary

**Summary**:
A lossy, derived description of identified evidence. It is not a new independent fact and does not replace its supporting observations.
_Avoid_: Memory, truth

**Startup context**:
A bounded selection of durable preferences, important decisions, and recent relevant evidence used when a session begins.
_Avoid_: Entire history

**Task context**:
A bounded selection of evidence relevant to a particular task, retaining its provenance.
_Avoid_: System instructions

**Maintenance work**:
Explicit work that refreshes derived representations of evidence without silently changing the retained memories.
_Avoid_: Learning, forgetting
