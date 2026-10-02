"""TinyMem - A lightweight, efficient memory system for AI agents."""

from tinymem.code import CodeMemoryManager
from tinymem.company import CompanyBrain
from tinymem.context import ContextManager
from tinymem.core import TinyMem
from tinymem.models import (
    CodeContext,
    Entity,
    EntityType,
    Memory,
    MemorySearchResult,
    MemoryType,
    Relationship,
    RelationshipType,
)
from tinymem.scoring import ScoringManager
from tinymem.storage import Storage

# MCP server is available but requires mcp package
try:
    from tinymem import mcp_server
    _mcp_available = True
except ImportError:
    _mcp_available = False

__version__ = "0.1.0"
__all__ = [
    "TinyMem",
    "Storage",
    "ScoringManager",
    "ContextManager",
    "CodeMemoryManager",
    "CompanyBrain",
    "Memory",
    "Entity",
    "Relationship",
    "MemoryType",
    "EntityType",
    "RelationshipType",
    "MemorySearchResult",
    "CodeContext",
]
