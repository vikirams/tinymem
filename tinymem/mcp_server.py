"""MCP Server for TinyMem - Integration with coding agents."""

import asyncio
import json
import os
from typing import Any, Optional
from uuid import UUID

from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp.types import Tool, TextContent

from tinymem import TinyMem, MemoryType, EntityType


# Initialize TinyMem instance
_tinymem_instance: Optional[TinyMem] = None


def get_tinymem() -> TinyMem:
    """Get or create TinyMem instance."""
    global _tinymem_instance
    if _tinymem_instance is None:
        db_path = os.environ.get("TINYMEM_DB_PATH", "tinymem_mcp.db")
        embedding_model = os.environ.get("TINYMEM_EMBEDDING_MODEL", "all-MiniLM-L6-v2")
        _tinymem_instance = TinyMem(db_path=db_path, embedding_model=embedding_model)
    return _tinymem_instance


# Create MCP server
server = Server("tinymem")


@server.list_tools()
async def list_tools() -> list[Tool]:
    """List available MCP tools."""
    return [
        Tool(
            name="add_memory",
            description="Add a memory to TinyMem. Use this to remember important information about the user, project, or context.",
            inputSchema={
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "The content to remember",
                    },
                    "user_id": {
                        "type": "string",
                        "description": "Optional user ID for scoping",
                    },
                    "agent_id": {
                        "type": "string",
                        "description": "Optional agent ID for scoping",
                    },
                    "project_id": {
                        "type": "string",
                        "description": "Optional project ID for scoping",
                    },
                    "file_id": {
                        "type": "string",
                        "description": "Optional file ID for scoping",
                    },
                    "memory_type": {
                        "type": "string",
                        "description": "Type of memory (fact, preference, decision, plan, relationship, code_context, architectural_decision, issue_solution)",
                        "enum": ["fact", "preference", "decision", "plan", "relationship", "code_context", "architectural_decision", "issue_solution"],
                        "default": "fact",
                    },
                    "metadata": {
                        "type": "object",
                        "description": "Optional additional metadata as JSON object",
                    },
                    "infer": {
                        "type": "boolean",
                        "description": "Whether to extract facts using LLM (true) or store raw (false)",
                        "default": True,
                    },
                },
                "required": ["content"],
            },
        ),
        Tool(
            name="search_memories",
            description="Search for relevant memories. Use this to retrieve context before making decisions or answering questions.",
            inputSchema={
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query",
                    },
                    "user_id": {
                        "type": "string",
                        "description": "Optional user ID filter",
                    },
                    "agent_id": {
                        "type": "string",
                        "description": "Optional agent ID filter",
                    },
                    "project_id": {
                        "type": "string",
                        "description": "Optional project ID filter",
                    },
                    "file_id": {
                        "type": "string",
                        "description": "Optional file ID filter",
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results",
                        "default": 5,
                    },
                    "min_score": {
                        "type": "number",
                        "description": "Minimum similarity score (0-1)",
                        "default": 0.0,
                    },
                },
                "required": ["query"],
            },
        ),
        Tool(
            name="get_memory",
            description="Get a specific memory by ID",
            inputSchema={
                "type": "object",
                "properties": {
                    "memory_id": {
                        "type": "string",
                        "description": "Memory ID",
                    },
                },
                "required": ["memory_id"],
            },
        ),
        Tool(
            name="delete_memory",
            description="Soft delete a memory by ID",
            inputSchema={
                "type": "object",
                "properties": {
                    "memory_id": {
                        "type": "string",
                        "description": "Memory ID",
                    },
                },
                "required": ["memory_id"],
            },
        ),
        Tool(
            name="add_code_context",
            description="Index a code file and extract entities. Use this when working with code files to maintain context.",
            inputSchema={
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Path to the file",
                    },
                    "content": {
                        "type": "string",
                        "description": "File content",
                    },
                    "language": {
                        "type": "string",
                        "description": "Programming language (python, javascript, typescript, java, go, etc.)",
                    },
                    "project_id": {
                        "type": "string",
                        "description": "Optional project ID",
                    },
                    "metadata": {
                        "type": "object",
                        "description": "Optional additional metadata",
                    },
                },
                "required": ["file_path", "content", "language"],
            },
        ),
        Tool(
            name="search_code",
            description="Search code context. Use this to find relevant code in the project.",
            inputSchema={
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query",
                    },
                    "project_id": {
                        "type": "string",
                        "description": "Optional project ID filter",
                    },
                    "file_path": {
                        "type": "string",
                        "description": "Optional file path filter",
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results",
                        "default": 5,
                    },
                },
                "required": ["query"],
            },
        ),
        Tool(
            name="get_file_dependencies",
            description="Get dependencies for a file. Use this to understand what a file depends on.",
            inputSchema={
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "File path",
                    },
                },
                "required": ["file_path"],
            },
        ),
        Tool(
            name="get_file_usages",
            description="Get files that depend on this file. Use this to understand the impact of changes.",
            inputSchema={
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "File path",
                    },
                },
                "required": ["file_path"],
            },
        ),
        Tool(
            name="track_code_change",
            description="Track a code change and create relationships. Use this when modifying files.",
            inputSchema={
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "File path",
                    },
                    "old_content": {
                        "type": "string",
                        "description": "Old file content",
                    },
                    "new_content": {
                        "type": "string",
                        "description": "New file content",
                    },
                    "language": {
                        "type": "string",
                        "description": "Programming language",
                    },
                    "project_id": {
                        "type": "string",
                        "description": "Optional project ID",
                    },
                },
                "required": ["file_path", "old_content", "new_content", "language"],
            },
        ),
        Tool(
            name="search_code_entities",
            description="Search for code entities (functions, classes, etc.). Use this to find specific code elements.",
            inputSchema={
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query",
                    },
                    "entity_type": {
                        "type": "string",
                        "description": "Entity type filter (person, organization, concept, file, function, class, variable, module)",
                        "enum": ["person", "organization", "concept", "file", "function", "class", "variable", "module"],
                    },
                    "project_id": {
                        "type": "string",
                        "description": "Optional project ID filter",
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results",
                        "default": 10,
                    },
                },
                "required": ["query"],
            },
        ),
        Tool(
            name="add_to_session",
            description="Add a memory to a session. Use this for short-term memory during a coding session.",
            inputSchema={
                "type": "object",
                "properties": {
                    "memory_id": {
                        "type": "string",
                        "description": "Memory ID",
                    },
                    "session_id": {
                        "type": "string",
                        "description": "Session identifier",
                    },
                },
                "required": ["memory_id", "session_id"],
            },
        ),
        Tool(
            name="get_session_context",
            description="Get all memories in a session. Use this to retrieve session context.",
            inputSchema={
                "type": "object",
                "properties": {
                    "session_id": {
                        "type": "string",
                        "description": "Session identifier",
                    },
                },
                "required": ["session_id"],
            },
        ),
        Tool(
            name="clear_session",
            description="Clear session memory. Use this to clean up after a session ends.",
            inputSchema={
                "type": "object",
                "properties": {
                    "session_id": {
                        "type": "string",
                        "description": "Session identifier",
                    },
                },
                "required": ["session_id"],
            },
        ),
        Tool(
            name="consolidate_memories",
            description="Consolidate multiple related memories into one. Use this to combine related information.",
            inputSchema={
                "type": "object",
                "properties": {
                    "memory_ids": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "List of memory IDs to consolidate",
                    },
                    "user_id": {
                        "type": "string",
                        "description": "Optional user ID",
                    },
                },
                "required": ["memory_ids"],
            },
        ),
        Tool(
            name="create_workspace",
            description="Create a workspace for multi-tenancy. Use this for company brain features.",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Workspace name",
                    },
                    "org_id": {
                        "type": "string",
                        "description": "Organization ID",
                    },
                    "metadata": {
                        "type": "object",
                        "description": "Optional additional metadata",
                    },
                },
                "required": ["name", "org_id"],
            },
        ),
        Tool(
            name="share_memory",
            description="Share a memory with a workspace. Use this for team collaboration.",
            inputSchema={
                "type": "object",
                "properties": {
                    "memory_id": {
                        "type": "string",
                        "description": "Memory ID",
                    },
                    "workspace_id": {
                        "type": "string",
                        "description": "Workspace ID",
                    },
                    "access_level": {
                        "type": "string",
                        "description": "Access level (read, write, admin)",
                        "enum": ["read", "write", "admin"],
                        "default": "read",
                    },
                },
                "required": ["memory_id", "workspace_id"],
            },
        ),
        Tool(
            name="get_workspace_analytics",
            description="Get analytics for a workspace. Use this to understand workspace usage.",
            inputSchema={
                "type": "object",
                "properties": {
                    "workspace_id": {
                        "type": "string",
                        "description": "Workspace ID",
                    },
                },
                "required": ["workspace_id"],
            },
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: Any) -> list[TextContent]:
    """Handle tool calls."""
    tinymem = get_tinymem()

    try:
        if name == "add_memory":
            memory_id = tinymem.add(
                content=arguments["content"],
                user_id=arguments.get("user_id"),
                agent_id=arguments.get("agent_id"),
                project_id=arguments.get("project_id"),
                file_id=arguments.get("file_id"),
                memory_type=MemoryType(arguments.get("memory_type", "fact")),
                metadata=arguments.get("metadata"),
                infer=arguments.get("infer", True),
            )
            return [TextContent(type="text", text=json.dumps({"memory_id": str(memory_id), "success": True}))]

        elif name == "search_memories":
            results = tinymem.search(
                query=arguments["query"],
                user_id=arguments.get("user_id"),
                agent_id=arguments.get("agent_id"),
                project_id=arguments.get("project_id"),
                file_id=arguments.get("file_id"),
                limit=arguments.get("limit", 5),
                min_score=arguments.get("min_score", 0.0),
            )
            result_dicts = [
                {
                    "memory_id": str(r.memory.id),
                    "content": r.memory.content,
                    "memory_type": r.memory.memory_type.value,
                    "score": r.score,
                    "metadata": r.memory.metadata,
                    "created_at": r.memory.created_at.isoformat(),
                }
                for r in results
            ]
            return [TextContent(type="text", text=json.dumps({"results": result_dicts, "count": len(result_dicts)}))]

        elif name == "get_memory":
            memory = tinymem.get(UUID(arguments["memory_id"]))
            if memory:
                return [TextContent(type="text", text=json.dumps({
                    "memory_id": str(memory.id),
                    "content": memory.content,
                    "memory_type": memory.memory_type.value,
                    "metadata": memory.metadata,
                    "importance_score": memory.importance_score,
                    "access_count": memory.access_count,
                    "created_at": memory.created_at.isoformat(),
                }))]
            else:
                return [TextContent(type="text", text=json.dumps({"error": "Memory not found"}))]

        elif name == "delete_memory":
            tinymem.delete(UUID(arguments["memory_id"]))
            return [TextContent(type="text", text=json.dumps({"success": True}))]

        elif name == "add_code_context":
            memory_id = tinymem.add_code_context(
                file_path=arguments["file_path"],
                content=arguments["content"],
                language=arguments["language"],
                project_id=arguments.get("project_id"),
                metadata=arguments.get("metadata"),
            )
            return [TextContent(type="text", text=json.dumps({"memory_id": str(memory_id), "success": True}))]

        elif name == "search_code":
            results = tinymem.search_code(
                query=arguments["query"],
                project_id=arguments.get("project_id"),
                file_path=arguments.get("file_path"),
                limit=arguments.get("limit", 5),
            )
            result_dicts = [
                {
                    "memory_id": str(r.memory.id),
                    "content": r.memory.content,
                    "score": r.score,
                    "metadata": r.memory.metadata,
                }
                for r in results
            ]
            return [TextContent(type="text", text=json.dumps({"results": result_dicts, "count": len(result_dicts)}))]

        elif name == "get_file_dependencies":
            deps = tinymem.get_file_dependencies(arguments["file_path"])
            return [TextContent(type="text", text=json.dumps({"file_path": arguments["file_path"], "dependencies": deps}))]

        elif name == "get_file_usages":
            usages = tinymem.get_file_usages(arguments["file_path"])
            return [TextContent(type="text", text=json.dumps({"file_path": arguments["file_path"], "usages": usages}))]

        elif name == "track_code_change":
            memory_id = tinymem.track_code_change(
                file_path=arguments["file_path"],
                old_content=arguments["old_content"],
                new_content=arguments["new_content"],
                language=arguments["language"],
                project_id=arguments.get("project_id"),
            )
            return [TextContent(type="text", text=json.dumps({"memory_id": str(memory_id), "success": True}))]

        elif name == "search_code_entities":
            entity_type = EntityType(arguments["entity_type"]) if arguments.get("entity_type") else None
            entities = tinymem.search_code_entities(
                query=arguments["query"],
                entity_type=entity_type,
                project_id=arguments.get("project_id"),
                limit=arguments.get("limit", 10),
            )
            entity_dicts = [
                {
                    "entity_id": str(e.id),
                    "name": e.name,
                    "entity_type": e.entity_type.value,
                    "metadata": e.metadata,
                }
                for e in entities
            ]
            return [TextContent(type="text", text=json.dumps({"entities": entity_dicts, "count": len(entity_dicts)}))]

        elif name == "add_to_session":
            tinymem.add_to_session(UUID(arguments["memory_id"]), arguments["session_id"])
            return [TextContent(type="text", text=json.dumps({"success": True}))]

        elif name == "get_session_context":
            memories = tinymem.get_session_context(arguments["session_id"])
            memory_dicts = [
                {
                    "memory_id": str(m.id),
                    "content": m.content,
                    "memory_type": m.memory_type.value,
                }
                for m in memories
            ]
            return [TextContent(type="text", text=json.dumps({"memories": memory_dicts, "count": len(memory_dicts)}))]

        elif name == "clear_session":
            tinymem.clear_session(arguments["session_id"])
            return [TextContent(type="text", text=json.dumps({"success": True}))]

        elif name == "consolidate_memories":
            memory_ids = [UUID(mid) for mid in arguments["memory_ids"]]
            consolidated_id = tinymem.consolidate_memories(memory_ids, arguments.get("user_id"))
            if consolidated_id:
                return [TextContent(type="text", text=json.dumps({"consolidated_id": str(consolidated_id), "success": True}))]
            else:
                return [TextContent(type="text", text=json.dumps({"error": "Could not consolidate memories"}))]

        elif name == "create_workspace":
            workspace_id = tinymem.create_workspace(
                name=arguments["name"],
                org_id=arguments["org_id"],
                metadata=arguments.get("metadata"),
            )
            return [TextContent(type="text", text=json.dumps({"workspace_id": str(workspace_id), "success": True}))]

        elif name == "share_memory":
            tinymem.share_memory(
                memory_id=UUID(arguments["memory_id"]),
                workspace_id=UUID(arguments["workspace_id"]),
                access_level=arguments.get("access_level", "read"),
            )
            return [TextContent(type="text", text=json.dumps({"success": True}))]

        elif name == "get_workspace_analytics":
            analytics = tinymem.get_workspace_analytics(UUID(arguments["workspace_id"]))
            return [TextContent(type="text", text=json.dumps(analytics))]

        else:
            return [TextContent(type="text", text=json.dumps({"error": f"Unknown tool: {name}"}))]

    except Exception as e:
        return [TextContent(type="text", text=json.dumps({"error": str(e)}))]


async def main():
    """Main entry point for MCP server."""
    async with stdio_server() as (read_stream, write_stream):
        await server.run(
            read_stream,
            write_stream,
            server.create_initialization_options(),
        )


if __name__ == "__main__":
    asyncio.run(main())
