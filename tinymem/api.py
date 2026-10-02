"""FastAPI REST API for TinyMem."""

from typing import Any, Dict, List, Optional
from uuid import UUID

from fastapi import FastAPI, HTTPException
from pydantic import BaseModel

from tinymem import TinyMem, MemoryType
from tinymem.models import MemorySearchResult

# Initialize FastAPI app
app = FastAPI(
    title="TinyMem API",
    description="A lightweight, efficient memory system for AI agents",
    version="0.1.0",
)

# Global TinyMem instance
tinymem: Optional[TinyMem] = None


# Request/Response models
class AddMemoryRequest(BaseModel):
    content: str
    user_id: Optional[str] = None
    agent_id: Optional[str] = None
    project_id: Optional[str] = None
    file_id: Optional[str] = None
    memory_type: str = "fact"
    metadata: Optional[Dict[str, Any]] = None
    infer: bool = True


class AddMemoryResponse(BaseModel):
    memory_id: str
    success: bool


class SearchRequest(BaseModel):
    query: str
    user_id: Optional[str] = None
    agent_id: Optional[str] = None
    project_id: Optional[str] = None
    file_id: Optional[str] = None
    limit: int = 5
    min_score: float = 0.0


class SearchResponse(BaseModel):
    results: List[Dict[str, Any]]
    count: int


class CodeContextRequest(BaseModel):
    file_path: str
    content: str
    language: str
    project_id: Optional[str] = None
    metadata: Optional[Dict[str, Any]] = None


class WorkspaceRequest(BaseModel):
    name: str
    org_id: str
    metadata: Optional[Dict[str, Any]] = None


class ShareMemoryRequest(BaseModel):
    memory_id: str
    workspace_id: str
    access_level: str = "read"


# Startup event
@app.on_event("startup")
async def startup_event():
    """Initialize TinyMem on startup."""
    global tinymem
    tinymem = TinyMem(db_path="tinymem.db")


# Shutdown event
@app.on_event("shutdown")
async def shutdown_event():
    """Close TinyMem on shutdown."""
    global tinymem
    if tinymem:
        tinymem.close()


# Health check
@app.get("/health")
async def health_check():
    """Health check endpoint."""
    return {"status": "healthy", "tinymem_initialized": tinymem is not None}


# Memory endpoints
@app.post("/memories", response_model=AddMemoryResponse)
async def add_memory(request: AddMemoryRequest):
    """Add a memory to the system."""
    if not tinymem:
        raise HTTPException(status_code=500, detail="TinyMem not initialized")

    try:
        memory_type = MemoryType(request.memory_type)
    except ValueError:
        raise HTTPException(status_code=400, detail=f"Invalid memory type: {request.memory_type}")

    memory_id = tinymem.add(
        content=request.content,
        user_id=request.user_id,
        agent_id=request.agent_id,
        project_id=request.project_id,
        file_id=request.file_id,
        memory_type=memory_type,
        metadata=request.metadata,
        infer=request.infer,
    )

    return AddMemoryResponse(memory_id=str(memory_id), success=True)


@app.post("/memories/search", response_model=SearchResponse)
async def search_memories(request: SearchRequest):
    """Search for relevant memories."""
    if not tinymem:
        raise HTTPException(status_code=500, detail="TinyMem not initialized")

    results = tinymem.search(
        query=request.query,
        user_id=request.user_id,
        agent_id=request.agent_id,
        project_id=request.project_id,
        file_id=request.file_id,
        limit=request.limit,
        min_score=request.min_score,
    )

    # Convert to dict for JSON response
    result_dicts = [
        {
            "memory_id": str(result.memory.id),
            "content": result.memory.content,
            "memory_type": result.memory.memory_type.value,
            "score": result.score,
            "metadata": result.memory.metadata,
            "created_at": result.memory.created_at.isoformat(),
        }
        for result in results
    ]

    return SearchResponse(results=result_dicts, count=len(result_dicts))


@app.get("/memories/{memory_id}")
async def get_memory(memory_id: str):
    """Get a memory by ID."""
    if not tinymem:
        raise HTTPException(status_code=500, detail="TinyMem not initialized")

    memory = tinymem.get(UUID(memory_id))
    if not memory:
        raise HTTPException(status_code=404, detail="Memory not found")

    return {
        "memory_id": str(memory.id),
        "content": memory.content,
        "memory_type": memory.memory_type.value,
        "metadata": memory.metadata,
        "importance_score": memory.importance_score,
        "access_count": memory.access_count,
        "created_at": memory.created_at.isoformat(),
    }


@app.delete("/memories/{memory_id}")
async def delete_memory(memory_id: str):
    """Delete a memory by ID."""
    if not tinymem:
        raise HTTPException(status_code=500, detail="TinyMem not initialized")

    tinymem.delete(UUID(memory_id))
    return {"success": True}


# Code context endpoints
@app.post("/code/context")
async def add_code_context(request: CodeContextRequest):
    """Add code context to memory."""
    if not tinymem:
        raise HTTPException(status_code=500, detail="TinyMem not initialized")

    memory_id = tinymem.add_code_context(
        file_path=request.file_path,
        content=request.content,
        language=request.language,
        project_id=request.project_id,
        metadata=request.metadata,
    )

    return {"memory_id": str(memory_id), "success": True}


@app.get("/code/dependencies/{file_path:path}")
async def get_file_dependencies(file_path: str):
    """Get dependencies for a file."""
    if not tinymem:
        raise HTTPException(status_code=500, detail="TinyMem not initialized")

    dependencies = tinymem.get_file_dependencies(file_path)
    return {"file_path": file_path, "dependencies": dependencies}


@app.get("/code/usages/{file_path:path}")
async def get_file_usages(file_path: str):
    """Get files that depend on this file."""
    if not tinymem:
        raise HTTPException(status_code=500, detail="TinyMem not initialized")

    usages = tinymem.get_file_usages(file_path)
    return {"file_path": file_path, "usages": usages}


# Workspace endpoints
@app.post("/workspaces")
async def create_workspace(request: WorkspaceRequest):
    """Create a new workspace."""
    if not tinymem:
        raise HTTPException(status_code=500, detail="TinyMem not initialized")

    workspace_id = tinymem.create_workspace(
        name=request.name,
        org_id=request.org_id,
        metadata=request.metadata,
    )

    return {"workspace_id": str(workspace_id), "success": True}


@app.get("/workspaces/{workspace_id}")
async def get_workspace(workspace_id: str):
    """Get a workspace by ID."""
    if not tinymem:
        raise HTTPException(status_code=500, detail="TinyMem not initialized")

    workspace = tinymem.get_workspace(UUID(workspace_id))
    if not workspace:
        raise HTTPException(status_code=404, detail="Workspace not found")

    return {
        "workspace_id": str(workspace.id),
        "name": workspace.name,
        "org_id": workspace.org_id,
        "metadata": workspace.metadata,
        "created_at": workspace.created_at.isoformat(),
    }


@app.get("/workspaces/org/{org_id}")
async def get_workspaces_by_org(org_id: str):
    """Get all workspaces for an organization."""
    if not tinymem:
        raise HTTPException(status_code=500, detail="TinyMem not initialized")

    workspaces = tinymem.get_workspaces_by_org(org_id)
    return {
        "org_id": org_id,
        "workspaces": [
            {
                "workspace_id": str(ws.id),
                "name": ws.name,
                "created_at": ws.created_at.isoformat(),
            }
            for ws in workspaces
        ],
    }


@app.post("/memories/share")
async def share_memory(request: ShareMemoryRequest):
    """Share a memory with a workspace."""
    if not tinymem:
        raise HTTPException(status_code=500, detail="TinyMem not initialized")

    tinymem.share_memory(
        memory_id=UUID(request.memory_id),
        workspace_id=UUID(request.workspace_id),
        access_level=request.access_level,
    )

    return {"success": True}


@app.get("/workspaces/{workspace_id}/shared")
async def get_shared_memories(workspace_id: str, access_level: Optional[str] = None, limit: int = 50):
    """Get memories shared with a workspace."""
    if not tinymem:
        raise HTTPException(status_code=500, detail="TinyMem not initialized")

    memories = tinymem.get_shared_memories(
        workspace_id=UUID(workspace_id),
        access_level=access_level,
        limit=limit,
    )

    return {
        "workspace_id": workspace_id,
        "memories": [
            {
                "memory_id": str(mem.id),
                "content": mem.content,
                "memory_type": mem.memory_type.value,
            }
            for mem in memories
        ],
        "count": len(memories),
    }


@app.get("/workspaces/{workspace_id}/analytics")
async def get_workspace_analytics(workspace_id: str):
    """Get analytics for a workspace."""
    if not tinymem:
        raise HTTPException(status_code=500, detail="TinyMem not initialized")

    analytics = tinymem.get_workspace_analytics(UUID(workspace_id))
    return analytics


@app.get("/org/{org_id}/analytics")
async def get_org_analytics(org_id: str):
    """Get analytics for an organization."""
    if not tinymem:
        raise HTTPException(status_code=500, detail="TinyMem not initialized")

    analytics = tinymem.get_org_analytics(org_id)
    return analytics


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8000)
