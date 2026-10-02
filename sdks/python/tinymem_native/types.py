"""Native wire types; no legacy Python memory or model imports."""
from typing import Literal, TypedDict

SearchMode = Literal["lexical", "semantic", "hybrid"]


class EmbeddingIndexReport(TypedDict):
    indexed: int
    stale: int
    remaining: int
    model: str
    dimensions: int


MemoryKind = Literal["fact", "preference", "decision", "plan", "relationship", "architectural_decision", "issue_solution"]


class Scope(TypedDict):
    user_id: str
    agent_id: str | None
    project_id: str | None


SourceRole = Literal["origin", "dependency"]
FreshnessStatus = Literal["current", "stale", "unknown"]
FreshnessReason = Literal["no_dependencies", "dependencies_current", "hash_matches", "content_changed", "file_missing", "source_unavailable"]


class _FileSourceOptions(TypedDict, total=False):
    expected_hash: str


class FileSourceInput(_FileSourceOptions):
    kind: Literal["file"]
    role: SourceRole
    path: str


class FileSourceReference(TypedDict):
    kind: Literal["file"]
    role: SourceRole
    repository: str
    path: str
    content_hash: str
    observed_at_ms: int


class FileDependencyCheck(TypedDict):
    source_index: int
    expected_hash: str
    observed_hash: str | None
    status: FreshnessStatus
    reason: FreshnessReason


class Freshness(TypedDict):
    status: FreshnessStatus
    reason: FreshnessReason
    checked_at_ms: int | None
    checks: list[FileDependencyCheck]


class MemoryRecord(TypedDict):
    id: str
    scope: Scope
    content: str
    kind: MemoryKind
    source: str | None
    source_refs: list[FileSourceReference]
    expires_at_ms: int | None
    version: int
    created_at_ms: int
    updated_at_ms: int


class MemoryEvent(TypedDict):
    event: Literal["added", "updated", "deleted"]
    version: int
    at_ms: int
    memory: MemoryRecord
    freshness: Freshness


class Profile(TypedDict):
    stable: list[MemoryRecord]
    dynamic: list[MemoryRecord]


class RefreshReport(TypedDict):
    repository: str
    indexed: int
    unchanged: int
    removed: int
    skipped: int
    indexed_at_ms: int


class Symbol(TypedDict):
    name: str
    kind: str
    start_line: int
    end_line: int


class RepositoryHit(TypedDict):
    path: str
    start_line: int
    end_line: int
    content: str
    content_hash: str
    language: str
    symbols: list[Symbol]
    syntax_errors: bool
    indexed_at_ms: int


class RepositorySearchResult(TypedDict):
    refresh: RefreshReport
    hits: list[RepositoryHit]


class MemoryProvenance(TypedDict):
    type: Literal["memory"]
    id: str
    scope: Scope
    kind: MemoryKind
    source: str | None
    source_refs: list[FileSourceReference]
    version: int
    created_at_ms: int
    updated_at_ms: int
    expires_at_ms: int | None


class RepositoryProvenance(TypedDict):
    type: Literal["repository"]
    repository: str
    path: str
    start_line: int
    end_line: int
    content_hash: str
    language: str
    syntax_errors: bool
    indexed_at_ms: int


class ContextEntry(TypedDict):
    category: Literal["preference", "decision", "repository", "memory"]
    content: str
    provenance: list[MemoryProvenance | RepositoryProvenance]


ContextSource = Literal["memory", "repository"]


class AssembledContext(TypedDict):
    prompt: str
    entries: list[ContextEntry]
    token_budget: int
    tokens_used: int
    token_count_method: Literal["utf8_bytes_upper_bound", "custom"]
