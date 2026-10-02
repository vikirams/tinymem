"""Storage layer for TinyMem using SQLite."""

import hashlib
import json
import sqlite3
from datetime import datetime
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple
from uuid import UUID

from tinymem.models import (
    Entity,
    EntityType,
    Memory,
    MemoryType,
    Relationship,
    RelationshipType,
)


class Storage:
    """SQLite-based storage for TinyMem."""

    def __init__(self, db_path: str = "tinymem.db"):
        """Initialize storage with database path."""
        self.db_path = Path(db_path)
        self.db_path.parent.mkdir(parents=True, exist_ok=True)
        self.conn = sqlite3.connect(str(self.db_path), check_same_thread=False)
        self.conn.row_factory = sqlite3.Row
        self._init_schema()

    def _init_schema(self) -> None:
        """Initialize database schema."""
        # Memories table
        self.conn.execute(
            """
            CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                embedding BLOB,
                memory_type TEXT NOT NULL,
                metadata TEXT,
                importance_score REAL DEFAULT 0.5,
                access_count INTEGER DEFAULT 0,
                last_accessed TIMESTAMP,
                created_at TIMESTAMP NOT NULL,
                hash TEXT,
                user_id TEXT,
                agent_id TEXT,
                project_id TEXT,
                file_id TEXT,
                is_deleted BOOLEAN DEFAULT 0,
                deleted_at TIMESTAMP
            )
        """
        )

        # Entities table
        self.conn.execute(
            """
            CREATE TABLE IF NOT EXISTS entities (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                entity_type TEXT NOT NULL,
                embedding BLOB,
                metadata TEXT,
                created_at TIMESTAMP NOT NULL
            )
        """
        )

        # Relationships table
        self.conn.execute(
            """
            CREATE TABLE IF NOT EXISTS relationships (
                id TEXT PRIMARY KEY,
                source_memory_id TEXT,
                target_memory_id TEXT,
                target_entity_id TEXT,
                relationship_type TEXT NOT NULL,
                metadata TEXT,
                created_at TIMESTAMP NOT NULL,
                FOREIGN KEY (source_memory_id) REFERENCES memories(id),
                FOREIGN KEY (target_memory_id) REFERENCES memories(id),
                FOREIGN KEY (target_entity_id) REFERENCES entities(id)
            )
        """
        )

        # Workspaces table
        self.conn.execute(
            """
            CREATE TABLE IF NOT EXISTS workspaces (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                org_id TEXT NOT NULL,
                metadata TEXT,
                created_at TIMESTAMP NOT NULL
            )
        """
        )

        # Create indexes
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memories_hash ON memories(hash)"
        )
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memories_user_id ON memories(user_id)"
        )
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memories_project_id ON memories(project_id)"
        )
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memories_file_id ON memories(file_id)"
        )
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memories_is_deleted ON memories(is_deleted)"
        )
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_entities_name ON entities(name)"
        )
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_entities_type ON entities(entity_type)"
        )
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_relationships_source ON relationships(source_memory_id)"
        )
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_relationships_target ON relationships(target_memory_id)"
        )
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_relationships_entity ON relationships(target_entity_id)"
        )

        # FTS5 for full-text search
        self.conn.execute(
            """
            CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                content,
                tokenize='porter unicode61'
            )
        """
        )

        self.conn.commit()

    def add_memory(self, memory: Memory) -> Memory:
        """Add a memory to storage."""
        # Generate hash if not provided
        if memory.hash is None:
            memory.hash = self._compute_hash(memory.content)

        # Check for duplicates
        existing = self.get_memory_by_hash(memory.hash)
        if existing and not existing.is_deleted:
            return existing

        # Serialize embedding
        embedding_blob = None
        if memory.embedding:
            import numpy as np

            embedding_blob = np.array(memory.embedding, dtype=np.float32).tobytes()

        # Insert memory
        self.conn.execute(
            """
            INSERT INTO memories (
                id, content, embedding, memory_type, metadata,
                importance_score, access_count, last_accessed, created_at,
                hash, user_id, agent_id, project_id, file_id, is_deleted, deleted_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
            (
                str(memory.id),
                memory.content,
                embedding_blob,
                memory.memory_type.value,
                json.dumps(memory.metadata),
                memory.importance_score,
                memory.access_count,
                memory.last_accessed.isoformat() if memory.last_accessed else None,
                memory.created_at.isoformat(),
                memory.hash,
                memory.user_id,
                memory.agent_id,
                memory.project_id,
                memory.file_id,
                memory.is_deleted,
                memory.deleted_at.isoformat() if memory.deleted_at else None,
            ),
        )

        # Add to FTS
        self.conn.execute(
            "INSERT INTO memories_fts (rowid, content) VALUES (?, ?)",
            (str(memory.id), memory.content),
        )

        self.conn.commit()
        return memory

    def get_memory(self, memory_id: UUID) -> Optional[Memory]:
        """Get a memory by ID."""
        cursor = self.conn.execute(
            "SELECT * FROM memories WHERE id = ?", (str(memory_id),)
        )
        row = cursor.fetchone()
        if row:
            return self._row_to_memory(row)
        return None

    def get_memory_by_hash(self, hash: str) -> Optional[Memory]:
        """Get a memory by hash."""
        cursor = self.conn.execute(
            "SELECT * FROM memories WHERE hash = ? ORDER BY created_at DESC LIMIT 1",
            (hash,),
        )
        row = cursor.fetchone()
        if row:
            return self._row_to_memory(row)
        return None

    def update_memory_access(self, memory_id: UUID) -> None:
        """Update memory access statistics."""
        self.conn.execute(
            """
            UPDATE memories
            SET access_count = access_count + 1,
                last_accessed = ?
            WHERE id = ?
        """,
            (datetime.utcnow().isoformat(), str(memory_id)),
        )
        self.conn.commit()

    def update_memory_importance(self, memory_id: UUID, importance_score: float) -> None:
        """Update memory importance score."""
        self.conn.execute(
            """
            UPDATE memories
            SET importance_score = ?
            WHERE id = ?
        """,
            (importance_score, str(memory_id)),
        )
        self.conn.commit()

    def soft_delete_memory(self, memory_id: UUID) -> None:
        """Soft delete a memory."""
        self.conn.execute(
            """
            UPDATE memories
            SET is_deleted = 1, deleted_at = ?
            WHERE id = ?
        """,
            (datetime.utcnow().isoformat(), str(memory_id)),
        )
        self.conn.commit()

    def add_entity(self, entity: Entity) -> Entity:
        """Add an entity to storage."""
        # Serialize embedding
        embedding_blob = None
        if entity.embedding:
            import numpy as np

            embedding_blob = np.array(entity.embedding, dtype=np.float32).tobytes()

        self.conn.execute(
            """
            INSERT INTO entities (id, name, entity_type, embedding, metadata, created_at)
            VALUES (?, ?, ?, ?, ?, ?)
        """,
            (
                str(entity.id),
                entity.name,
                entity.entity_type.value,
                embedding_blob,
                json.dumps(entity.metadata),
                entity.created_at.isoformat(),
            ),
        )
        self.conn.commit()
        return entity

    def get_entity(self, entity_id: UUID) -> Optional[Entity]:
        """Get an entity by ID."""
        cursor = self.conn.execute(
            "SELECT * FROM entities WHERE id = ?", (str(entity_id),)
        )
        row = cursor.fetchone()
        if row:
            return self._row_to_entity(row)
        return None

    def get_entity_by_name(self, name: str) -> Optional[Entity]:
        """Get an entity by name."""
        cursor = self.conn.execute(
            "SELECT * FROM entities WHERE name = ?", (name,)
        )
        row = cursor.fetchone()
        if row:
            return self._row_to_entity(row)
        return None

    def add_relationship(self, relationship: Relationship) -> Relationship:
        """Add a relationship to storage."""
        self.conn.execute(
            """
            INSERT INTO relationships (
                id, source_memory_id, target_memory_id, target_entity_id,
                relationship_type, metadata, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
        """,
            (
                str(relationship.id),
                str(relationship.source_memory_id) if relationship.source_memory_id else None,
                str(relationship.target_memory_id) if relationship.target_memory_id else None,
                str(relationship.target_entity_id) if relationship.target_entity_id else None,
                relationship.relationship_type.value,
                json.dumps(relationship.metadata),
                relationship.created_at.isoformat(),
            ),
        )
        self.conn.commit()
        return relationship

    def get_relationships(
        self,
        source_memory_id: Optional[UUID] = None,
        target_memory_id: Optional[UUID] = None,
        target_entity_id: Optional[UUID] = None,
        relationship_type: Optional[RelationshipType] = None,
    ) -> List[Relationship]:
        """Get relationships by filters."""
        query = "SELECT * FROM relationships WHERE 1=1"
        params = []

        if source_memory_id:
            query += " AND source_memory_id = ?"
            params.append(str(source_memory_id))
        if target_memory_id:
            query += " AND target_memory_id = ?"
            params.append(str(target_memory_id))
        if target_entity_id:
            query += " AND target_entity_id = ?"
            params.append(str(target_entity_id))
        if relationship_type:
            query += " AND relationship_type = ?"
            params.append(relationship_type.value)

        cursor = self.conn.execute(query, params)
        return [self._row_to_relationship(row) for row in cursor.fetchall()]

    def search_memories(
        self,
        query: Optional[str] = None,
        user_id: Optional[str] = None,
        agent_id: Optional[str] = None,
        project_id: Optional[str] = None,
        file_id: Optional[str] = None,
        limit: int = 10,
    ) -> List[Memory]:
        """Search memories with filters."""
        sql = "SELECT * FROM memories WHERE is_deleted = 0"
        params = []

        if query:
            # Use FTS for text search
            sql = """
                SELECT m.* FROM memories m
                INNER JOIN memories_fts fts ON m.id = fts.rowid
                WHERE memories_fts MATCH ? AND m.is_deleted = 0
            """
            params.append(query)

        if user_id:
            sql += " AND user_id = ?"
            params.append(user_id)
        if agent_id:
            sql += " AND agent_id = ?"
            params.append(agent_id)
        if project_id:
            sql += " AND project_id = ?"
            params.append(project_id)
        if file_id:
            sql += " AND file_id = ?"
            params.append(file_id)

        sql += " ORDER BY importance_score DESC, created_at DESC LIMIT ?"
        params.append(limit)

        cursor = self.conn.execute(sql, params)
        return [self._row_to_memory(row) for row in cursor.fetchall()]

    def _compute_hash(self, content: str) -> str:
        """Compute MD5 hash of content."""
        return hashlib.md5(content.encode()).hexdigest()

    def _row_to_memory(self, row: sqlite3.Row) -> Memory:
        """Convert database row to Memory model."""
        embedding = None
        if row["embedding"]:
            import numpy as np

            embedding = np.frombuffer(row["embedding"], dtype=np.float32).tolist()

        return Memory(
            id=UUID(row["id"]),
            content=row["content"],
            embedding=embedding,
            memory_type=MemoryType(row["memory_type"]),
            metadata=json.loads(row["metadata"]) if row["metadata"] else {},
            importance_score=row["importance_score"],
            access_count=row["access_count"],
            last_accessed=datetime.fromisoformat(row["last_accessed"]) if row["last_accessed"] else None,
            created_at=datetime.fromisoformat(row["created_at"]),
            hash=row["hash"],
            user_id=row["user_id"],
            agent_id=row["agent_id"],
            project_id=row["project_id"],
            file_id=row["file_id"],
            is_deleted=bool(row["is_deleted"]),
            deleted_at=datetime.fromisoformat(row["deleted_at"]) if row["deleted_at"] else None,
        )

    def _row_to_entity(self, row: sqlite3.Row) -> Entity:
        """Convert database row to Entity model."""
        embedding = None
        if row["embedding"]:
            import numpy as np

            embedding = np.frombuffer(row["embedding"], dtype=np.float32).tolist()

        return Entity(
            id=UUID(row["id"]),
            name=row["name"],
            entity_type=EntityType(row["entity_type"]),
            embedding=embedding,
            metadata=json.loads(row["metadata"]) if row["metadata"] else {},
            created_at=datetime.fromisoformat(row["created_at"]),
        )

    def _row_to_relationship(self, row: sqlite3.Row) -> Relationship:
        """Convert database row to Relationship model."""
        return Relationship(
            id=UUID(row["id"]),
            source_memory_id=UUID(row["source_memory_id"]) if row["source_memory_id"] else None,
            target_memory_id=UUID(row["target_memory_id"]) if row["target_memory_id"] else None,
            target_entity_id=UUID(row["target_entity_id"]) if row["target_entity_id"] else None,
            relationship_type=RelationshipType(row["relationship_type"]),
            metadata=json.loads(row["metadata"]) if row["metadata"] else {},
            created_at=datetime.fromisoformat(row["created_at"]),
        )

    def close(self) -> None:
        """Close database connection."""
        self.conn.close()
