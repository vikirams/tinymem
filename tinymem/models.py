"""Data models for TinyMem."""

from datetime import datetime
from enum import Enum
from typing import Any, Dict, List, Optional
from uuid import UUID, uuid4

from pydantic import BaseModel, Field


class MemoryType(str, Enum):
    """Types of memories."""

    FACT = "fact"
    PREFERENCE = "preference"
    DECISION = "decision"
    PLAN = "plan"
    RELATIONSHIP = "relationship"
    CODE_CONTEXT = "code_context"
    ARCHITECTURAL_DECISION = "architectural_decision"
    ISSUE_SOLUTION = "issue_solution"


class EntityType(str, Enum):
    """Types of entities."""

    PERSON = "person"
    ORGANIZATION = "organization"
    CONCEPT = "concept"
    FILE = "file"
    FUNCTION = "function"
    CLASS = "class"
    VARIABLE = "variable"
    MODULE = "module"


class RelationshipType(str, Enum):
    """Types of relationships between memories."""

    UPDATES = "updates"
    EXTENDS = "extends"
    DERIVES = "derives"
    REFERENCES = "references"
    CONTAINS = "contains"
    CALLS = "calls"
    IMPORTS = "imports"


class Memory(BaseModel):
    """A memory in the system."""

    id: UUID = Field(default_factory=uuid4)
    content: str
    embedding: Optional[List[float]] = None
    memory_type: MemoryType = MemoryType.FACT
    metadata: Dict[str, Any] = Field(default_factory=dict)
    importance_score: float = 0.5
    access_count: int = 0
    last_accessed: Optional[datetime] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)
    hash: Optional[str] = None

    # Scoping
    user_id: Optional[str] = None
    agent_id: Optional[str] = None
    project_id: Optional[str] = None
    file_id: Optional[str] = None

    # Soft delete
    is_deleted: bool = False
    deleted_at: Optional[datetime] = None


class Entity(BaseModel):
    """An entity extracted from memories."""

    id: UUID = Field(default_factory=uuid4)
    name: str
    entity_type: EntityType
    embedding: Optional[List[float]] = None
    metadata: Dict[str, Any] = Field(default_factory=dict)
    created_at: datetime = Field(default_factory=datetime.utcnow)


class Relationship(BaseModel):
    """A relationship between memories or entities."""

    id: UUID = Field(default_factory=uuid4)
    source_memory_id: Optional[UUID] = None
    target_memory_id: Optional[UUID] = None
    target_entity_id: Optional[UUID] = None
    relationship_type: RelationshipType
    metadata: Dict[str, Any] = Field(default_factory=dict)
    created_at: datetime = Field(default_factory=datetime.utcnow)


class MemorySearchResult(BaseModel):
    """Result of a memory search."""

    memory: Memory
    score: float
    matched_entities: List[str] = Field(default_factory=list)


class Workspace(BaseModel):
    """A workspace for multi-tenancy."""

    id: UUID = Field(default_factory=uuid4)
    name: str
    org_id: str
    metadata: Dict[str, Any] = Field(default_factory=dict)
    created_at: datetime = Field(default_factory=datetime.utcnow)


class CodeContext(BaseModel):
    """Code-specific context information."""

    file_path: str
    language: str
    content: str
    functions: List[str] = Field(default_factory=list)
    classes: List[str] = Field(default_factory=list)
    imports: List[str] = Field(default_factory=list)
    metadata: Dict[str, Any] = Field(default_factory=dict)
