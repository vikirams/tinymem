"""Core TinyMem memory system."""

import re
from datetime import datetime, timedelta
from typing import Any, Dict, List, Optional
from uuid import UUID, uuid4

import numpy as np

from tinymem.code import CodeMemoryManager
from tinymem.company import CompanyBrain
from tinymem.context import ContextManager
from tinymem.embeddings import get_embedding_generator
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


class TinyMem:
    """Main TinyMem memory system."""

    def __init__(
        self,
        db_path: str = "tinymem.db",
        embedding_model: str = "all-MiniLM-L6-v2",
        scoring_config: Optional[dict] = None,
        session_duration_minutes: int = 30,
    ):
        """Initialize TinyMem."""
        self.storage = Storage(db_path)
        self.embedding_generator = get_embedding_generator(embedding_model)
        self.embedding_dim = self.embedding_generator.get_dimension()
        self.scoring_manager = ScoringManager(**(scoring_config or {}))
        self.context_manager = ContextManager(self.storage, session_duration_minutes)
        self.code_manager = CodeMemoryManager(self.storage)
        self.company_brain = CompanyBrain(self.storage)

    def add(
        self,
        content: str,
        user_id: Optional[str] = None,
        agent_id: Optional[str] = None,
        project_id: Optional[str] = None,
        file_id: Optional[str] = None,
        memory_type: MemoryType = MemoryType.FACT,
        metadata: Optional[Dict[str, Any]] = None,
        infer: bool = True,
    ) -> UUID:
        """Add a memory to the system.

        Args:
            content: The content to remember
            user_id: Optional user ID for scoping
            agent_id: Optional agent ID for scoping
            project_id: Optional project ID for scoping
            file_id: Optional file ID for scoping
            memory_type: Type of memory
            metadata: Additional metadata
            infer: Whether to extract facts (LLM-based) or store raw

        Returns:
            The memory ID
        """
        # Context lookup to avoid duplicates
        if infer:
            context = self.context_manager.lookup_context(
                content, user_id=user_id, project_id=project_id
            )
        else:
            context = []

        # Generate embedding
        embedding = self.embedding_generator.generate(content)

        # Calculate initial importance score
        importance_score = self.scoring_manager.calculate_initial_score(
            content, memory_type.value, metadata
        )

        # Create memory
        memory = Memory(
            content=content,
            embedding=embedding,
            memory_type=memory_type,
            metadata=metadata or {},
            importance_score=importance_score,
            user_id=user_id,
            agent_id=agent_id,
            project_id=project_id,
            file_id=file_id,
        )

        # Store memory
        stored_memory = self.storage.add_memory(memory)

        # Detect and create update relationships
        if infer and context:
            updated_ids = self.context_manager.detect_updates(stored_memory, context)
            for old_id in updated_ids:
                self.context_manager.create_update_relationship(stored_memory.id, old_id)

        # Extract entities if infer is enabled
        if infer:
            self._extract_and_link_entities(stored_memory)

        return stored_memory.id

    def search(
        self,
        query: str,
        user_id: Optional[str] = None,
        agent_id: Optional[str] = None,
        project_id: Optional[str] = None,
        file_id: Optional[str] = None,
        limit: int = 5,
        min_score: float = 0.0,
    ) -> List[MemorySearchResult]:
        """Search for relevant memories.

        Args:
            query: Search query
            user_id: Optional user ID filter
            agent_id: Optional agent ID filter
            project_id: Optional project ID filter
            file_id: Optional file ID filter
            limit: Maximum number of results
            min_score: Minimum similarity score

        Returns:
            List of search results with scores
        """
        # Generate query embedding
        query_embedding = self.embedding_generator.generate(query)

        # Get candidate memories using text search
        candidates = self.storage.search_memories(
            query=query,
            user_id=user_id,
            agent_id=agent_id,
            project_id=project_id,
            file_id=file_id,
            limit=limit * 2,  # Get more candidates for re-ranking
        )

        # Calculate similarity scores
        results = []
        for memory in candidates:
            if memory.embedding:
                similarity = self._cosine_similarity(query_embedding, memory.embedding)
                # Use scoring manager for combined score
                combined_score = self.scoring_manager.calculate_retrieval_score(
                    similarity, memory.importance_score
                )
                if combined_score >= min_score:
                    results.append(
                        MemorySearchResult(
                            memory=memory,
                            score=combined_score,
                        )
                    )

        # Sort by score and limit
        results.sort(key=lambda x: x.score, reverse=True)
        results = results[:limit]

        # Update access statistics and boost importance
        for result in results:
            self.storage.update_memory_access(result.memory.id)
            # Boost importance on access
            new_score = self.scoring_manager.boost_on_access(result.memory)
            self.storage.update_memory_importance(result.memory.id, new_score)

        return results

    def get(self, memory_id: UUID) -> Optional[Memory]:
        """Get a memory by ID."""
        return self.storage.get_memory(memory_id)

    def delete(self, memory_id: UUID) -> None:
        """Soft delete a memory."""
        self.storage.soft_delete_memory(memory_id)

    def add_code_context(
        self,
        file_path: str,
        content: str,
        language: str,
        project_id: Optional[str] = None,
        metadata: Optional[Dict[str, Any]] = None,
    ) -> UUID:
        """Add code context to memory.

        Args:
            file_path: Path to the file
            content: File content
            language: Programming language
            project_id: Optional project ID
            metadata: Additional metadata

        Returns:
            The memory ID
        """
        return self.code_manager.index_file(
            file_path, content, language, project_id, metadata
        )

    def search_code(
        self,
        query: str,
        project_id: Optional[str] = None,
        file_path: Optional[str] = None,
        limit: int = 5,
    ) -> List[MemorySearchResult]:
        """Search code context.

        Args:
            query: Search query
            project_id: Optional project ID filter
            file_path: Optional file path filter
            limit: Maximum number of results

        Returns:
            List of search results
        """
        return self.search(
            query=query,
            project_id=project_id,
            file_id=file_path,
            limit=limit,
        )

    def get_file_relationships(self, file_path: str) -> List[Relationship]:
        """Get relationships for a file.

        Args:
            file_path: File path

        Returns:
            List of relationships
        """
        # Get memory for file
        memories = self.storage.search_memories(file_id=file_path, limit=1)
        if not memories:
            return []

        memory = memories[0]
        return self.storage.get_relationships(source_memory_id=memory.id)

    def get_file_dependencies(self, file_path: str) -> List[str]:
        """Get dependencies for a file."""
        return self.code_manager.get_file_dependencies(file_path)

    def get_file_usages(self, file_path: str) -> List[str]:
        """Get files that depend on this file."""
        return self.code_manager.get_file_usages(file_path)

    def search_code_entities(
        self,
        query: str,
        entity_type: Optional[EntityType] = None,
        project_id: Optional[str] = None,
        limit: int = 10,
    ) -> List[Entity]:
        """Search for code entities."""
        return self.code_manager.search_code_entities(query, entity_type, project_id, limit)

    def track_code_change(
        self,
        file_path: str,
        old_content: str,
        new_content: str,
        language: str,
        project_id: Optional[str] = None,
    ) -> UUID:
        """Track a code change."""
        return self.code_manager.track_code_change(
            file_path, old_content, new_content, language, project_id
        )

    def _extract_and_link_entities(self, memory: Memory) -> None:
        """Extract entities from memory and create relationships."""
        # Simple entity extraction (can be enhanced with NLP)
        entities = self._extract_entities_simple(memory.content)

        for entity_name, entity_type in entities:
            # Get or create entity
            entity = self.storage.get_entity_by_name(entity_name)
            if not entity:
                entity = Entity(
                    name=entity_name,
                    entity_type=entity_type,
                    embedding=self.embedding_generator.generate(entity_name),
                )
                entity = self.storage.add_entity(entity)

            # Create relationship
            relationship = Relationship(
                source_memory_id=memory.id,
                target_entity_id=entity.id,
                relationship_type=RelationshipType.REFERENCES,
            )
            self.storage.add_relationship(relationship)

    def _extract_entities_simple(self, text: str) -> List[tuple]:
        """Simple entity extraction (placeholder for NLP-based extraction)."""
        entities = []

        # Extract potential proper nouns (capitalized words)
        words = re.findall(r'\b[A-Z][a-z]+\b', text)
        for word in set(words):
            if len(word) > 2:  # Filter short words
                entities.append((word, EntityType.CONCEPT))

        return entities

    def _cosine_similarity(self, vec1: List[float], vec2: List[float]) -> float:
        """Calculate cosine similarity between two vectors."""
        a = np.array(vec1)
        b = np.array(vec2)
        return float(np.dot(a, b) / (np.linalg.norm(a) * np.linalg.norm(b)))

    def apply_decay_to_old_memories(self, days_threshold: int = 30) -> int:
        """Apply decay to memories older than threshold.

        Args:
            days_threshold: Age in days to apply decay

        Returns:
            Number of memories updated
        """
        threshold_date = datetime.utcnow() - timedelta(days=days_threshold)
        old_memories = self.storage.search_memories(limit=1000)  # Get recent memories

        updated_count = 0
        for memory in old_memories:
            if memory.created_at < threshold_date:
                new_score = self.scoring_manager.apply_decay(memory)
                if new_score != memory.importance_score:
                    self.storage.update_memory_importance(memory.id, new_score)
                    updated_count += 1

        return updated_count

    def cleanup_old_memories(self, threshold: float = 0.05) -> int:
        """Clean up (soft delete) memories that should be forgotten.

        Args:
            threshold: Importance score threshold

        Returns:
            Number of memories cleaned up
        """
        memories = self.storage.search_memories(limit=1000)
        cleaned_count = 0

        for memory in memories:
            if self.scoring_manager.should_forget(memory, threshold):
                self.storage.soft_delete_memory(memory.id)
                cleaned_count += 1

        return cleaned_count

    def add_to_session(self, memory_id: UUID, session_id: str) -> None:
        """Add memory to session context."""
        self.context_manager.add_to_session(memory_id, session_id)

    def get_session_context(self, session_id: str) -> List[Memory]:
        """Get all memories in a session."""
        return self.context_manager.get_session_context(session_id)

    def clear_session(self, session_id: str) -> None:
        """Clear session memory."""
        self.context_manager.clear_session(session_id)

    def consolidate_memories(self, memory_ids: List[UUID], user_id: Optional[str] = None) -> Optional[UUID]:
        """Consolidate multiple related memories into one."""
        return self.context_manager.consolidate_memories(memory_ids, user_id)

    # Company Brain methods
    def create_workspace(self, name: str, org_id: str, metadata: Optional[Dict[str, Any]] = None) -> UUID:
        """Create a new workspace."""
        return self.company_brain.create_workspace(name, org_id, metadata)

    def get_workspace(self, workspace_id: UUID):
        """Get a workspace by ID."""
        return self.company_brain.get_workspace(workspace_id)

    def get_workspaces_by_org(self, org_id: str) -> List:
        """Get all workspaces for an organization."""
        return self.company_brain.get_workspaces_by_org(org_id)

    def share_memory(self, memory_id: UUID, workspace_id: UUID, access_level: str = "read") -> None:
        """Share a memory with a workspace."""
        self.company_brain.share_memory(memory_id, workspace_id, access_level)

    def get_shared_memories(self, workspace_id: UUID, access_level: Optional[str] = None, limit: int = 50) -> List[Memory]:
        """Get memories shared with a workspace."""
        return self.company_brain.get_shared_memories(workspace_id, access_level, limit)

    def add_user_to_workspace(self, workspace_id: UUID, user_id: str, role: str = "member") -> None:
        """Add a user to a workspace."""
        self.company_brain.add_user_to_workspace(workspace_id, user_id, role)

    def get_workspace_users(self, workspace_id: UUID) -> Dict[str, Dict[str, Any]]:
        """Get users in a workspace."""
        return self.company_brain.get_workspace_users(workspace_id)

    def check_access(self, user_id: str, workspace_id: UUID, required_permission: str = "read") -> bool:
        """Check if a user has access to a workspace."""
        return self.company_brain.check_access(user_id, workspace_id, required_permission)

    def get_workspace_analytics(self, workspace_id: UUID) -> Dict[str, Any]:
        """Get analytics for a workspace."""
        return self.company_brain.get_workspace_analytics(workspace_id)

    def get_org_analytics(self, org_id: str) -> Dict[str, Any]:
        """Get analytics for an organization."""
        return self.company_brain.get_org_analytics(org_id)

    def close(self) -> None:
        """Close the storage connection."""
        self.storage.close()
