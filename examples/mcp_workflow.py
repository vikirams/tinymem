"""
Example MCP workflow for coding agents.

This demonstrates how a coding agent would use TinyMem MCP tools
during a typical development workflow.
"""

# This is a conceptual example showing the tool calls a coding agent would make
# In practice, these would be invoked by the agent's MCP client

EXAMPLE_WORKFLOW = """
## Onboarding a New Project

### Step 1: Index the codebase
```json
{
  "tool": "add_code_context",
  "arguments": {
    "file_path": "src/auth.py",
    "content": "def authenticate_user(username, password): ...",
    "language": "python",
    "project_id": "my_project"
  }
}
```

### Step 2: Remember architectural decisions
```json
{
  "tool": "add_memory",
  "arguments": {
    "content": "Project uses JWT for authentication with Redis session storage",
    "memory_type": "architectural_decision",
    "project_id": "my_project",
    "metadata": {
      "category": "security",
      "priority": "high"
    }
  }
}
```

### Step 3: Remember coding patterns
```json
{
  "tool": "add_memory",
  "arguments": {
    "content": "Always use async/await for database operations in this project",
    "memory_type": "preference",
    "project_id": "my_project"
  }
}
```

## Implementing a Feature

### Step 1: Search for context
```json
{
  "tool": "search_memories",
  "arguments": {
    "query": "authentication database patterns",
    "project_id": "my_project",
    "limit": 5
  }
}
```

### Step 2: Search for similar code
```json
{
  "tool": "search_code",
  "arguments": {
    "query": "database connection",
    "project_id": "my_project",
    "limit": 3
  }
}
```

### Step 3: Track the implementation decision
```json
{
  "tool": "add_memory",
  "arguments": {
    "content": "Implemented user profile update endpoint with optimistic locking",
    "memory_type": "decision",
    "project_id": "my_project",
    "file_id": "src/api/profile.py"
  }
}
```

### Step 4: Index the new code
```json
{
  "tool": "add_code_context",
  "arguments": {
    "file_path": "src/api/profile.py",
    "content": "def update_profile(user_id, data): ...",
    "language": "python",
    "project_id": "my_project"
  }
}
```

## Debugging an Issue

### Step 1: Search for similar issues
```json
{
  "tool": "search_memories",
  "arguments": {
    "query": "database timeout connection pool",
    "memory_type": "issue_solution",
    "project_id": "my_project"
  }
}
```

### Step 2: Find related code
```json
{
  "tool": "search_code_entities",
  "arguments": {
    "query": "connection pool",
    "entity_type": "class",
    "project_id": "my_project"
  }
}
```

### Step 3: Track the solution
```json
{
  "tool": "add_memory",
  "arguments": {
    "content": "Fixed database timeout by increasing connection pool size and adding retry logic",
    "memory_type": "issue_solution",
    "project_id": "my_project",
    "metadata": {
      "category": "bugfix",
      "severity": "high"
    }
  }
}
```

### Step 4: Track the code change
```json
{
  "tool": "track_code_change",
  "arguments": {
    "file_path": "src/db/connection.py",
    "old_content": "pool = ConnectionPool(size=10)",
    "new_content": "pool = ConnectionPool(size=20, retry=True)",
    "language": "python",
    "project_id": "my_project"
  }
}
```

## Session-Based Workflow

### Step 1: Start a session
```json
{
  "tool": "add_to_session",
  "arguments": {
    "memory_id": "memory-uuid-1",
    "session_id": "session-123"
  }
}
```

### Step 2: Add more context to session
```json
{
  "tool": "add_to_session",
  "arguments": {
    "memory_id": "memory-uuid-2",
    "session_id": "session-123"
  }
}
```

### Step 3: Retrieve session context
```json
{
  "tool": "get_session_context",
  "arguments": {
    "session_id": "session-123"
  }
}
```

### Step 4: Consolidate important session memories
```json
{
  "tool": "consolidate_memories",
  "arguments": {
    "memory_ids": ["uuid-1", "uuid-2", "uuid-3"],
    "user_id": "user123"
  }
}
```

### Step 5: Clear session
```json
{
  "tool": "clear_session",
  "arguments": {
    "session_id": "session-123"
  }
}
```

## Understanding Code Impact

### Step 1: Check file dependencies
```json
{
  "tool": "get_file_dependencies",
  "arguments": {
    "file_path": "src/auth.py"
  }
}
```

### Step 2: Check file usages
```json
{
  "tool": "get_file_usages",
  "arguments": {
    "file_path": "src/auth.py"
  }
}
```

### Step 3: Make decision based on impact
```json
{
  "tool": "add_memory",
  "arguments": {
    "content": "Auth module is used by 5 other files, changes require careful testing",
    "memory_type": "decision",
    "project_id": "my_project"
  }
}
```

## Team Collaboration

### Step 1: Create workspace
```json
{
  "tool": "create_workspace",
  "arguments": {
    "name": "Backend Team",
    "org_id": "company-123"
  }
}
```

### Step 2: Share important decision
```json
{
  "tool": "share_memory",
  "arguments": {
    "memory_id": "decision-uuid",
    "workspace_id": "workspace-uuid",
    "access_level": "read"
  }
}
```

### Step 3: Check workspace analytics
```json
{
  "tool": "get_workspace_analytics",
  "arguments": {
    "workspace_id": "workspace-uuid"
  }
}
```
"""

if __name__ == "__main__":
    print("TinyMem MCP Workflow Examples")
    print("=" * 60)
    print(EXAMPLE_WORKFLOW)
