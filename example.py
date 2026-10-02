"""Comprehensive example usage of TinyMem."""

from tinymem import TinyMem, MemoryType, EntityType

# Initialize TinyMem
mem = TinyMem(db_path="example.db")

print("=" * 60)
print("TinyMem Example Usage")
print("=" * 60)

# 1. Basic Memory Operations
print("\n1. Basic Memory Operations")
print("-" * 60)

# Add some memories
user_pref_id = mem.add(
    content="User prefers dark mode for all applications",
    user_id="user123",
    memory_type=MemoryType.PREFERENCE,
    metadata={"category": "ui_settings"},
)

decision_id = mem.add(
    content="Decided to use PostgreSQL as the primary database",
    user_id="user123",
    project_id="project456",
    memory_type=MemoryType.DECISION,
    metadata={"category": "architecture"},
)

fact_id = mem.add(
    content="The authentication system uses JWT tokens",
    user_id="user123",
    project_id="project456",
    memory_type=MemoryType.FACT,
)

print(f"✓ Added memory (ID: {user_pref_id})")
print(f"✓ Added memory (ID: {decision_id})")
print(f"✓ Added memory (ID: {fact_id})")

# 2. Search Memories
print("\n2. Search Memories")
print("-" * 60)

print("Search for 'preferences':")
results = mem.search("preferences", user_id="user123", limit=3)
for result in results:
    print(f"  - {result.memory.content} (score: {result.score:.2f})")

print("\nSearch for 'database':")
results = mem.search("database", project_id="project456", limit=3)
for result in results:
    print(f"  - {result.memory.content} (score: {result.score:.2f})")

# 3. Code Context
print("\n3. Code Context")
print("-" * 60)

code_content = """
def authenticate_user(username, password):
    # Authentication logic here
    if verify_credentials(username, password):
        return generate_jwt_token(username)
    return None

class AuthManager:
    def __init__(self):
        self.users = {}
        self.sessions = {}

    def login(self, username, password):
        return authenticate_user(username, password)
"""

code_context_id = mem.add_code_context(
    file_path="src/auth.py",
    content=code_content,
    language="python",
    project_id="project456",
)

print(f"✓ Added code context (ID: {code_context_id})")

print("\nSearch for 'authentication':")
results = mem.search_code("authentication", project_id="project456", limit=3)
for result in results:
    print(f"  - {result.memory.content} (score: {result.score:.2f})")

# 4. File Relationships
print("\n4. File Relationships")
print("-" * 60)

deps = mem.get_file_dependencies("src/auth.py")
print(f"Dependencies for src/auth.py: {deps}")

usages = mem.get_file_usages("src/auth.py")
print(f"Files that use src/auth.py: {usages}")

# 5. Session Management
print("\n5. Session Management")
print("-" * 60)

session_id = "session_123"
mem.add_to_session(user_pref_id, session_id)
mem.add_to_session(decision_id, session_id)

print(f"✓ Added memories to session '{session_id}'")

session_memories = mem.get_session_context(session_id)
print(f"Session contains {len(session_memories)} memories")
for memory in session_memories:
    print(f"  - {memory.content}")

# 6. Company Brain
print("\n6. Company Brain")
print("-" * 60)

# Create workspace
workspace_id = mem.create_workspace(
    name="Engineering Team",
    org_id="acme_corp",
    metadata={"department": "engineering"},
)

print(f"✓ Created workspace (ID: {workspace_id})")

# Add user to workspace
mem.add_user_to_workspace(
    workspace_id=workspace_id,
    user_id="user123",
    role="member",
)

print(f"✓ Added user123 to workspace")

# Share memory
mem.share_memory(
    memory_id=decision_id,
    workspace_id=workspace_id,
    access_level="read",
)

print(f"✓ Shared memory with workspace")

# Check access
has_access = mem.check_access(
    user_id="user123",
    workspace_id=workspace_id,
    required_permission="read",
)

print(f"User has read access: {has_access}")

# Get workspace analytics
analytics = mem.get_workspace_analytics(workspace_id)
print(f"\nWorkspace Analytics:")
print(f"  Total memories: {analytics['total_memories']}")
print(f"  Total users: {analytics['user_count']}")
print(f"  Memory types: {analytics['memory_types']}")

# 7. Memory Scoring and Decay
print("\n7. Memory Scoring and Decay")
print("-" * 60)

# Get memory with importance score
memory = mem.get(decision_id)
print(f"Memory importance score: {memory.importance_score:.2f}")
print(f"Memory access count: {memory.access_count}")

# Apply decay (simulated)
from tinymem import ScoringManager
scoring = ScoringManager()
new_score = scoring.apply_decay(memory)
print(f"Score after decay: {new_score:.2f}")

# 8. Entity Search
print("\n8. Entity Search")
print("-" * 60)

entities = mem.search_code_entities(
    query="authenticate",
    entity_type=EntityType.FUNCTION,
    project_id="project456",
)

print(f"Found {len(entities)} matching entities:")
for entity in entities:
    print(f"  - {entity.name} ({entity.entity_type.value})")

# 9. Cleanup
print("\n9. Cleanup")
print("-" * 60)

# Clear session
mem.clear_session(session_id)
print(f"✓ Cleared session '{session_id}'")

# Close the connection
mem.close()
print("\n✓ Closed TinyMem connection")

print("\n" + "=" * 60)
print("Example completed successfully!")
print("=" * 60)

