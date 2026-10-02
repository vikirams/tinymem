# TinyMem Architecture

## Overview

TinyMem is an MIT-licensed, local-first memory layer for coding agents. The native Rust core is the primary product; stdio MCP and thin agent integrations keep it independent of any agent or cloud vendor. Pi remains the initial end-to-end evaluation runtime. The optional controlled Cloudflare integration retains its gateway-only policy; its live provisioning and coding-tool egress verification remain pending. Core lexical operation requires neither Pi nor Cloudflare.

The [coding-agent roadmap](docs/MEMORY_SYSTEM_PLAN.md) defines source freshness, write quality, token efficiency, portability, integration gates and the ordered backlog. The [coding-memory benchmark protocol](docs/CODING_MEMORY_BENCHMARK.md) defines how to measure outcomes without claiming unrun results. [ADR 0003](docs/adr/0003-open-source-coding-agent-memory.md) narrows the prior gateway-first product policy. [Domain language](CONTEXT.md) distinguishes sources, validity dependencies, memories and context.

## Design Principles

1. **Lightweight**: Minimal required dependencies and embedded storage; no provider required for lexical operation
2. **Efficient**: Low resource use and fast retrieval; latency targets require benchmarks
3. **Coding-First**: Optimized for code context, file relationships, and development workflows
4. **Evidence-First**: Exact scopes and provenance; namespace isolation is not authentication
5. **Simple**: Easy to integrate, minimal configuration required

## Native agent-memory MVP

The experimental Rust engine in `crates/tinymem-engine` runs alongside (not replacing) the
Python implementation below. One native SQLite/FTS5 database has two separate evidence stores:

- **Repository context:** canonical-worktree file hashes, Python declarations/unresolved imports
  and source chunks. Transactional refresh reuses unchanged files and removes obsolete chunks;
  retrieval checks source hashes and bounds serialized results. The filesystem is not frozen.
- **Personal memory:** exact `(user_id, agent_id, project_id)` namespaces, scope-local deduplication,
  stable memory IDs and immutable added/updated/deleted audit events. Immediate write transactions
  serialize deduplication and optimistic version checks. Expired/deleted records are hidden from
  current retrieval; audit evidence persists. Explicit file origins/dependencies retain captured hashes;
  stale/unknown dependencies are excluded across all current reads. Freshness inspection is separate
  from immutable observations. Reads do not record access or change importance.

Native schema v1/v2/v3 databases migrate transactionally to v4 without losing repository evidence,
personal memories or audit history. Optional embeddings live in a separate versioned derived-cache
table, with model/dimension isolation and transactional invalidation on all memory updates.
Opening validates database identity/version before setup and again under the schema writer lock;
foreign and newer databases are rejected. Legacy Python databases are not migrated.

The library exposes repository operations and explicit memory CRUD/search/audit/profile operations.
A serialized stdio MCP adapter fixes scope/root at startup and exposes eleven bounded tools; standalone
Python/TypeScript SDKs own one subprocess per operation with deadlines and bounded streams.
The optional `integrations/pi` extension reuses the TypeScript SDK for fresh, budgeted request-local
context and intentional `/memory` writes. Pi's built-in MCP remains the persistent model-tool connection;
deferred exposure avoids declaring every memory tool up front. See [local Pi checks](docs/PI_LOCAL_CHECKS.md).
There are no required models, provider services, network listeners or autonomous extraction.

See [native memory/MCP setup](docs/NATIVE_MEMORY.md), [repository behavior](docs/RUST_ENGINE.md), and
[the implementation contract](docs/NATIVE_MVP.md). Opt-in [semantic/hybrid personal-memory retrieval](docs/SEMANTIC_RETRIEVAL.md)
uses a trusted embedding adapter, exact cosine ranking and reciprocal-rank fusion; reads do not backfill.
[Context assembly](docs/CONTEXT_ASSEMBLY.md) selects scoped task evidence, merges duplicates retaining
provenance, and packs whole entries into a budgeted prompt. Default counting uses a conservative
UTF-8-byte token bound; Rust callers can supply a target tokenizer without adding engine dependencies.
See [file provenance and read-time freshness](docs/FILE_PROVENANCE.md) for the shipped file-only slice.
Semantic repository search, inferred code dependencies, remote source resolvers, OS-event watching, enterprise auth
and verified assistant-specific installation remain future work.
The Python descriptions below are historical design intent, not the target architecture or verification of every proposal. New summaries, if justified by evaluation, will be rebuildable derived caches over scoped/versioned evidence, never replacements for raw memory. Missing summaries must not block startup. See [ADR 0001](docs/adr/0001-reuse-pi-and-route-inference-through-cloudflare.md) and [proposed ADR 0002](docs/adr/0002-derived-summary-cache-not-memory-replacement.md).

## Legacy Python Core Architecture

**Not the current product contract.** This implementation uses local inference and has unscoped hash deduplication and unauthenticated HTTP interfaces. Do not reuse it as the enterprise/security foundation or for the gateway-only evaluation path. Company/graph/ML proposals below remain historical or deferred.

### Storage Layer

**SQLite as Primary Storage**
- Embedded, zero-configuration database
- ACID compliant, fast reads/writes
- Easy backup and migration
- Supports full-text search (FTS5)

**Three Storage Tables:**

1. **memories** - Primary memory storage
   - id (UUID)
   - content (text)
   - embedding (vector - stored as blob)
   - metadata (JSON)
   - importance_score (float)
   - access_count (int)
   - last_accessed (timestamp)
   - created_at (timestamp)
   - hash (MD5 for deduplication)
   - scope (user_id, agent_id, project_id, file_id)

2. **entities** - Entity extraction and linking
   - id (UUID)
   - name (text)
   - type (person, organization, concept, file, function, class)
   - embedding (vector)
   - metadata (JSON)

3. **relationships** - Memory relationships
   - id (UUID)
   - source_memory_id (UUID)
   - target_memory_id (UUID)
   - target_entity_id (UUID)
   - relationship_type (updates, extends, derives, references, contains)
   - created_at (timestamp)

### Memory Processing Pipeline

**ADD-Only Architecture (inspired by mem0)**
- No UPDATE/DELETE operations on memories
- New information adds new memories with relationships
- Simplifies conflict resolution and consistency
- Enables full audit trail

**Pipeline Stages:**

1. **Content Extraction**
   - Extract text from various sources (chat, files, code)
   - Handle code-specific extraction (functions, classes, comments)

2. **Context Lookup**
   - Search for related existing memories
   - Avoid duplicates using hash-based deduplication

3. **Fact Extraction (Optional)**
   - LLM-based extraction for unstructured text
   - Extract preferences, decisions, relationships
   - Can be disabled for raw storage mode

4. **Entity Extraction**
   - Identify proper nouns, technical terms
   - Code-specific entities (function names, class names, file paths)
   - Extract relationships between entities

5. **Embedding Generation**
   - Use sentence-transformers (local, no API calls)
   - Configurable model selection
   - Batch embedding for efficiency

6. **Relationship Inference**
   - Detect "updates" relationships (changes in facts)
   - Detect "extends" relationships (additional context)
   - Detect "references" relationships (code dependencies)

7. **Storage**
   - Store in SQLite with proper indexing
   - Update FTS index for text search
   - Update entity relationships

### Retrieval Pipeline

**Multi-Stage Retrieval:**

1. **Query Processing**
   - Extract entities from query
   - Generate query embedding
   - Parse scope filters (user_id, project_id, etc.)

2. **Vector Search**
   - Cosine similarity search
   - Configurable top-k results
   - Similarity threshold filtering

3. **Entity-Based Boost**
   - Boost memories sharing entities with query
   - Traverse entity relationships

4. **Relationship Expansion**
   - Follow "extends" relationships for additional context
   - Follow "references" relationships for code context
   - Follow "updates" relationships for current facts

5. **Importance Scoring**
   - Combine similarity with importance_score
   - Decay factor based on time and access patterns
   - Boost recently accessed memories

6. **Ranking and Filtering**
   - Re-rank based on combined score
   - Apply scope filters
   - Return top-k results

### Memory Lifecycle

**Importance Scoring**
- Initial score based on extraction confidence
- Increases with access frequency
- Decreases over time (decay)
- Manual boost for critical memories

**Memory Decay**
- Exponential decay over time
- Configurable decay rate
- Preserve high-importance memories
- Archive old, low-importance memories

**Forgetting**
- Automatic cleanup of low-score memories
- Configurable retention policies
- Soft delete with audit trail

## Coding Agent Features

### Code Context Awareness

**File-Level Scoping**
- Memories scoped to specific files
- Track file modifications and dependencies
- Cross-file relationship tracking

**Code Entity Extraction**
- Function names and signatures
- Class definitions and inheritance
- Variable names and types
- Import statements and dependencies

**Code Relationships**
- "contains" - file contains function/class
- "calls" - function calls another function
- "imports" - file imports module
- "extends" - class extends another class

### Development Workflow Integration

**Session Memory**
- Short-term memory for current coding session
- Auto-cleanup after session ends
- Configurable retention period

**Project Memory**
- Long-term memory for project context
- Architectural decisions
- Design patterns used
- Common issues and solutions

**Agent Memory**
- Agent-specific learning
- Preferences and configurations
- Performance metrics

## Company Brain Features

### Multi-Tenancy

**Workspace/Container Isolation**
- Separate memory spaces per organization
- User-level scoping within workspaces
- No cross-workspace leakage

**Access Control**
- Role-based permissions (read, write, admin)
- Memory-level visibility settings
- Audit logging for sensitive operations

### Organization

**Hierarchical Structure**
- Organization → Projects → Files
- Tags and categories
- Custom metadata schemas

**Sharing**
- Share memories across users
- Shared project memory
- Knowledge base integration

### Analytics

**Memory Statistics**
- Memory growth over time
- Access patterns
- Entity relationships
- Retrieval performance

**Insights**
- Most accessed memories
- Entity clusters
- Knowledge gaps

## Performance Optimizations

### Caching
- LRU cache for recent queries
- Embedding cache for repeated content
- Entity lookup cache

### Indexing
- FTS5 full-text search index
- Vector similarity index (via sqlite-vec or external)
- B-tree indexes on metadata fields

### Batch Operations
- Batch embedding generation
- Bulk memory insertion
- Batch relationship updates

### Lazy Loading
- Load embeddings on demand
- Lazy relationship expansion
- Pagination for large result sets

## Integration API

### Core Operations

```python
# Add memory
memory_id = tinymem.add(
    content="User prefers dark mode",
    user_id="user123",
    agent_id="agent456",
    metadata={"type": "preference"}
)

# Search memories
results = tinymem.search(
    query="What are the user's preferences?",
    user_id="user123",
    limit=5
)

# Get memory by ID
memory = tinymem.get(memory_id)

# Delete memory (soft delete)
tinymem.delete(memory_id)
```

### Coding Agent API

```python
# Add code context
tinymem.add_code_context(
    file_path="src/auth.py",
    content="...",
    language="python",
    project_id="project789"
)

# Search code context
results = tinymem.search_code(
    query="authentication function",
    project_id="project789",
    file_path="src/auth.py"
)

# Get file relationships
relationships = tinymem.get_file_relationships("src/auth.py")
```

### Company Brain API

```python
# Create workspace
workspace_id = tinymem.create_workspace(
    name="Acme Corp",
    org_id="org123"
)

# Share memory
tinymem.share_memory(
    memory_id="mem456",
    workspace_id="ws789",
    access_level="read"
)

# Get workspace analytics
analytics = tinymem.get_workspace_analytics(workspace_id)
```

## Technology Stack

### Core
- **Python 3.8+**
- **SQLite 3.35+** (with FTS5)
- **sentence-transformers** (embeddings)
- **numpy** (vector operations)

### Optional
- **sqlite-vec** (vector similarity in SQLite)
- **fastapi** (REST API server)
- **pydantic** (data validation)

### Development
- **pytest** (testing)
- **black** (code formatting)
- **mypy** (type checking)

## Comparison with Alternatives

The [primary-source research](docs/research/memory-layer-options.md) records useful
mem0/Supermemory ideas and open-source versus hosted capability differences. The old
setup-time/latency comparison was not a measured benchmark and has been removed.
Performance and resource budgets must be measured against fixed local workloads.

## Next architecture slices

Follow the [coding-agent backlog](docs/MEMORY_SYSTEM_PLAN.md#6-ordered-implementation-backlog): baseline measurement and install scaffolding → structured provenance/freshness → write quality → context efficiency → portable lifecycle → verified agent packages and public results. Team/org authorization is a separate later phase. Summary caches need measured justification; another agent loop, distributed/graph infrastructure and the legacy company-brain product are not the direction.
