# Decide identity and scope

Type: grilling
Status: resolved
Blocked by: 03, 05

## Question

What is the unit of isolation: Namespace only, or also user / session / agent / turn? Who is the writer of a Memory, and is that provenance in the spec?

## Answer

**Namespace only.** Every agent on a project shares that bucket. Cross-project read is an explicit `namespace` on search, not a User/Global noun. Session, agent, and turn are not scopes. Writer provenance is not in the spec.
