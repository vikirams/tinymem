"""Context-aware memory management."""

from datetime import datetime, timedelta
from typing import Any, Dict, List, Optional
from uuid import UUID

from tinymem.models import Memory, MemoryType, Relationship, RelationshipType
from tinymem.storage import Storage


class ContextManager:
    """Manage context-aware memory operations."""

    def __init__(self, storage: Storage, session_duration_minutes: int = 30):
        """Initialize context manager.

        Args:
            storage: Storage instance
            session_duration_minutes: Duration for session memory
        """
        self.storage = storage
        self.session_duration = timedelta(minutes=session_duration_minutes)
        self.session_memories: Dict[str, List[UUID]] = {}

    def lookup_context(
        self,
        content: str,
        user_id: Optional[str] = None,
        project_id: Optional[str] = None,
        limit: int = 5,
    ) -> List[Memory]:
        """Lookup related existing memories to avoid duplicates.

        Args:
            content: Content to find context for
            user_id: Optional user ID filter
            project_id: Optional project ID filter
            limit: Maximum number of related memories

        Returns:
            List of related memories
        """
        # Simple keyword-based context lookup
        # In production, this would use semantic similarity
        keywords = self._extract_keywords(content)

        related_memories = []
        for keyword in keywords:
            memories = self.storage.search_memories(
                query=keyword,
                user_id=user_id,
                project_id=project_id,
                limit=limit,
            )
            related_memories.extend(memories)

        # Deduplicate by ID
        seen_ids = set()
        unique_memories = []
        for memory in related_memories:
            if memory.id not in seen_ids:
                seen_ids.add(memory.id)
                unique_memories.append(memory)

        return unique_memories[:limit]

    def add_to_session(
        self,
        memory_id: UUID,
        session_id: str,
    ) -> None:
        """Add memory to session context.

        Args:
            memory_id: Memory ID to add
            session_id: Session identifier
        """
        if session_id not in self.session_memories:
            self.session_memories[session_id] = []

        self.session_memories[session_id].append(memory_id)

    def get_session_context(
        self,
        session_id: str,
    ) -> List[Memory]:
        """Get all memories in a session.

        Args:
            session_id: Session identifier

        Returns:
            List of memories in the session
        """
        if session_id not in self.session_memories:
            return []

        memories = []
        for memory_id in self.session_memories[session_id]:
            memory = self.storage.get_memory(memory_id)
            if memory and not memory.is_deleted:
                memories.append(memory)

        return memories

    def clear_session(self, session_id: str) -> None:
        """Clear session memory.

        Args:
            session_id: Session identifier
        """
        if session_id in self.session_memories:
            del self.session_memories[session_id]

    def consolidate_memories(
        self,
        memory_ids: List[UUID],
        user_id: Optional[str] = None,
    ) -> Optional[UUID]:
        """Consolidate multiple related memories into one.

        Args:
            memory_ids: List of memory IDs to consolidate
            user_id: Optional user ID for the consolidated memory

        Returns:
            ID of consolidated memory, or None if consolidation not possible
        """
        if len(memory_ids) < 2:
            return None

        # Get all memories
        memories = []
        for memory_id in memory_ids:
            memory = self.storage.get_memory(memory_id)
            if memory and not memory.is_deleted:
                memories.append(memory)

        if len(memories) < 2:
            return None

        # Combine content
        combined_content = "\n\n".join([m.content for m in memories])

        # Combine metadata
        combined_metadata = {}
        for memory in memories:
            combined_metadata.update(memory.metadata)

        # Create consolidated memory
        from tinymem.models import Memory
        from uuid import uuid4

        consolidated = Memory(
            id=uuid4(),
            content=combined_content,
            memory_type=MemoryType.FACT,
            metadata=combined_metadata,
            user_id=user_id,
            importance_score=max(m.importance_score for m in memories),
        )

        # Store consolidated memory
        stored = self.storage.add_memory(consolidated)

        # Create relationships from original memories to consolidated
        for memory in memories:
            relationship = Relationship(
                source_memory_id=memory.id,
                target_memory_id=stored.id,
                relationship_type=RelationshipType.EXTENDS,
                metadata={"consolidated": True},
            )
            self.storage.add_relationship(relationship)

        return stored.id

    def detect_updates(
        self,
        new_memory: Memory,
        existing_memories: List[Memory],
        similarity_threshold: float = 0.8,
    ) -> List[UUID]:
        """Detect if new memory updates existing memories.

        Args:
            new_memory: New memory to check
            existing_memories: Existing memories to compare against
            similarity_threshold: Similarity threshold for detecting updates

        Returns:
            List of memory IDs that are updated by the new memory
        """
        # Simple detection based on content similarity
        # In production, this would use semantic similarity and LLM-based detection
        updated_ids = []

        for existing in existing_memories:
            # Check for common update patterns
            if self._is_update(new_memory.content, existing.content):
                updated_ids.append(existing.id)

        return updated_ids

    def create_update_relationship(
        self,
        new_memory_id: UUID,
        old_memory_id: UUID,
    ) -> None:
        """Create an update relationship between memories.

        Args:
            new_memory_id: ID of the new memory
            old_memory_id: ID of the old memory being updated
        """
        relationship = Relationship(
            source_memory_id=new_memory_id,
            target_memory_id=old_memory_id,
            relationship_type=RelationshipType.UPDATES,
        )
        self.storage.add_relationship(relationship)

    def _extract_keywords(self, content: str) -> List[str]:
        """Extract keywords from content for context lookup."""
        import re

        # Simple keyword extraction
        words = re.findall(r'\b\w{4,}\b', content.lower())
        # Remove common stop words
        stop_words = {
            "this", "that", "with", "from", "have", "been", "were", "they",
            "their", "what", "when", "where", "which", "will", "would", "could",
            "should", "about", "into", "through", "during", "before", "after",
        }
        keywords = [w for w in words if w not in stop_words]
        return list(set(keywords))[:10]  # Return top 10 unique keywords

    def _is_update(self, new_content: str, old_content: str) -> bool:
        """Check if new content is an update of old content."""
        # Simple heuristic: check for update patterns
        update_patterns = [
            "now",
            "changed",
            "updated",
            "switched",
            "instead of",
            "previously",
            "before",
        ]

        new_lower = new_content.lower()
        for pattern in update_patterns:
            if pattern in new_lower:
                return True

        return False
