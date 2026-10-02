"""Coding agent specific features."""

import re
from typing import Any, Dict, List, Optional, Set
from uuid import UUID

from tinymem.models import (
    CodeContext,
    Entity,
    EntityType,
    Memory,
    MemoryType,
    Relationship,
    RelationshipType,
)
from tinymem.storage import Storage


class CodeMemoryManager:
    """Manage code-specific memory operations."""

    def __init__(self, storage: Storage):
        """Initialize code memory manager."""
        self.storage = storage

    def index_file(
        self,
        file_path: str,
        content: str,
        language: str,
        project_id: Optional[str] = None,
        metadata: Optional[Dict[str, Any]] = None,
    ) -> UUID:
        """Index a file and extract code entities.

        Args:
            file_path: Path to the file
            content: File content
            language: Programming language
            project_id: Optional project ID
            metadata: Additional metadata

        Returns:
            Memory ID for the file context
        """
        # Extract code entities
        code_context = self._extract_code_entities(content, language)

        # Create memory for file
        memory = Memory(
            content=f"Code file: {file_path}",
            memory_type=MemoryType.CODE_CONTEXT,
            metadata={
                **(metadata or {}),
                "file_path": file_path,
                "language": language,
                "functions": code_context.functions,
                "classes": code_context.classes,
                "imports": code_context.imports,
                "line_count": len(content.splitlines()),
            },
            project_id=project_id,
            file_id=file_path,
        )

        stored_memory = self.storage.add_memory(memory)

        # Create entities for code elements
        self._create_code_entities(
            file_path,
            language,
            code_context,
            stored_memory.id,
        )

        # Create import relationships
        self._create_import_relationships(
            file_path,
            code_context.imports,
            project_id,
            stored_memory.id,
        )

        return stored_memory.id

    def get_file_dependencies(self, file_path: str) -> List[str]:
        """Get dependencies for a file.

        Args:
            file_path: File path

        Returns:
            List of dependent file paths
        """
        # Get memory for file
        memories = self.storage.search_memories(file_id=file_path, limit=1)
        if not memories:
            return []

        memory = memories[0]

        # Get relationships
        relationships = self.storage.get_relationships(
            source_memory_id=memory.id,
            relationship_type=RelationshipType.IMPORTS,
        )

        dependencies = []
        for rel in relationships:
            if rel.target_memory_id:
                target_memory = self.storage.get_memory(rel.target_memory_id)
                if target_memory and target_memory.file_id:
                    dependencies.append(target_memory.file_id)

        return dependencies

    def get_file_usages(self, file_path: str) -> List[str]:
        """Get files that depend on this file.

        Args:
            file_path: File path

        Returns:
            List of file paths that depend on this file
        """
        # Get memory for file
        memories = self.storage.search_memories(file_id=file_path, limit=1)
        if not memories:
            return []

        memory = memories[0]

        # Get relationships where this file is the target
        relationships = self.storage.get_relationships(
            target_memory_id=memory.id,
            relationship_type=RelationshipType.IMPORTS,
        )

        usages = []
        for rel in relationships:
            if rel.source_memory_id:
                source_memory = self.storage.get_memory(rel.source_memory_id)
                if source_memory and source_memory.file_id:
                    usages.append(source_memory.file_id)

        return usages

    def search_code_entities(
        self,
        query: str,
        entity_type: Optional[EntityType] = None,
        project_id: Optional[str] = None,
        limit: int = 10,
    ) -> List[Entity]:
        """Search for code entities.

        Args:
            query: Search query
            entity_type: Optional entity type filter
            project_id: Optional project ID filter
            limit: Maximum number of results

        Returns:
            List of matching entities
        """
        # Get all entities (simple text search for now)
        entities = []

        # In production, this would use vector search on entities
        # For now, use simple name matching
        all_entities = []
        cursor = self.storage.conn.execute("SELECT * FROM entities")
        for row in cursor.fetchall():
            entity = self.storage._row_to_entity(row)
            all_entities.append(entity)

        # Filter by query
        query_lower = query.lower()
        for entity in all_entities:
            if query_lower in entity.name.lower():
                if entity_type is None or entity.entity_type == entity_type:
                    entities.append(entity)

        return entities[:limit]

    def get_function_definition(self, function_name: str, project_id: Optional[str] = None) -> Optional[Dict[str, Any]]:
        """Get definition of a function.

        Args:
            function_name: Function name
            project_id: Optional project ID filter

        Returns:
            Function definition with context
        """
        # Get entity for function
        entity = self.storage.get_entity_by_name(function_name)
        if not entity or entity.entity_type != EntityType.FUNCTION:
            return None

        # Get memories that reference this entity
        relationships = self.storage.get_relationships(
            target_entity_id=entity.id,
            relationship_type=RelationshipType.CONTAINS,
        )

        if not relationships:
            return None

        # Get the memory
        memory = self.storage.get_memory(relationships[0].source_memory_id)
        if not memory:
            return None

        return {
            "name": function_name,
            "file_path": memory.file_id,
            "project_id": memory.project_id,
            "metadata": entity.metadata,
        }

    def track_code_change(
        self,
        file_path: str,
        old_content: str,
        new_content: str,
        language: str,
        project_id: Optional[str] = None,
    ) -> UUID:
        """Track a code change and create relationships.

        Args:
            file_path: File path
            old_content: Old file content
            new_content: New file content
            language: Programming language
            project_id: Optional project ID

        Returns:
            Memory ID for the change
        """
        # Create memory for the change
        change_memory = Memory(
            content=f"Code change in {file_path}",
            memory_type=MemoryType.ISSUE_SOLUTION,
            metadata={
                "file_path": file_path,
                "language": language,
                "change_type": "modification",
            },
            project_id=project_id,
            file_id=file_path,
        )

        stored_change = self.storage.add_memory(change_memory)

        # Re-index the file with new content
        new_memory_id = self.index_file(file_path, new_content, language, project_id)

        # Create relationship from change to new file
        relationship = Relationship(
            source_memory_id=stored_change.id,
            target_memory_id=new_memory_id,
            relationship_type=RelationshipType.UPDATES,
        )
        self.storage.add_relationship(relationship)

        return stored_change.id

    def create_code_relationship(
        self,
        source_file: str,
        target_file: str,
        relationship_type: RelationshipType,
        metadata: Optional[Dict[str, Any]] = None,
    ) -> None:
        """Create a relationship between two files.

        Args:
            source_file: Source file path
            target_file: Target file path
            relationship_type: Type of relationship
            metadata: Additional metadata
        """
        # Get memories for both files
        source_memories = self.storage.search_memories(file_id=source_file, limit=1)
        target_memories = self.storage.search_memories(file_id=target_file, limit=1)

        if not source_memories or not target_memories:
            return

        relationship = Relationship(
            source_memory_id=source_memories[0].id,
            target_memory_id=target_memories[0].id,
            relationship_type=relationship_type,
            metadata=metadata or {},
        )
        self.storage.add_relationship(relationship)

    def _extract_code_entities(self, content: str, language: str) -> CodeContext:
        """Extract code entities based on language."""
        functions = []
        classes = []
        imports = []

        if language == "python":
            # Extract function definitions
            functions = re.findall(r'def\s+(\w+)\s*\(', content)
            # Extract class definitions
            classes = re.findall(r'class\s+(\w+)\s*[:\(]', content)
            # Extract imports
            imports = re.findall(r'import\s+(\w+)|from\s+(\w+)\s+import', content)
            imports = [item for sublist in imports for item in sublist if item]

        elif language in ["javascript", "typescript"]:
            # Extract function definitions
            functions = re.findall(r'function\s+(\w+)\s*\(|const\s+(\w+)\s*=\s*\(', content)
            functions = [item for sublist in functions for item in sublist if item]
            # Extract class definitions
            classes = re.findall(r'class\s+(\w+)\s*', content)
            # Extract imports
            imports = re.findall(r'import.*from\s+[\'"]([^\'"]+)[\'"]', content)

        elif language == "java":
            # Extract method definitions
            functions = re.findall(r'(?:public|private|protected)?\s*(?:static)?\s*\w+\s+(\w+)\s*\(', content)
            # Extract class definitions
            classes = re.findall(r'class\s+(\w+)\s*', content)
            # Extract imports
            imports = re.findall(r'import\s+([^;]+);', content)

        elif language == "go":
            # Extract function definitions
            functions = re.findall(r'func\s+(\w+)\s*\(', content)
            # Extract imports
            imports = re.findall(r'"([^"]+)"', content)

        return CodeContext(
            file_path="",
            language=language,
            content=content,
            functions=functions,
            classes=classes,
            imports=imports,
        )

    def _create_code_entities(
        self,
        file_path: str,
        language: str,
        code_context: CodeContext,
        memory_id: UUID,
    ) -> None:
        """Create entities for code elements."""
        # Create function entities
        for func in code_context.functions:
            entity = Entity(
                name=func,
                entity_type=EntityType.FUNCTION,
                metadata={"file_path": file_path, "language": language},
            )
            stored_entity = self.storage.add_entity(entity)

            # Create relationship
            relationship = Relationship(
                source_memory_id=memory_id,
                target_entity_id=stored_entity.id,
                relationship_type=RelationshipType.CONTAINS,
            )
            self.storage.add_relationship(relationship)

        # Create class entities
        for cls in code_context.classes:
            entity = Entity(
                name=cls,
                entity_type=EntityType.CLASS,
                metadata={"file_path": file_path, "language": language},
            )
            stored_entity = self.storage.add_entity(entity)

            # Create relationship
            relationship = Relationship(
                source_memory_id=memory_id,
                target_entity_id=stored_entity.id,
                relationship_type=RelationshipType.CONTAINS,
            )
            self.storage.add_relationship(relationship)

    def _create_import_relationships(
        self,
        file_path: str,
        imports: List[str],
        project_id: Optional[str],
        memory_id: UUID,
    ) -> None:
        """Create import relationships."""
        for import_path in imports:
            # Try to find if the imported file is indexed
            imported_files = self.storage.search_memories(
                query=import_path,
                project_id=project_id,
                limit=1,
            )

            if imported_files:
                relationship = Relationship(
                    source_memory_id=memory_id,
                    target_memory_id=imported_files[0].id,
                    relationship_type=RelationshipType.IMPORTS,
                    metadata={"import_path": import_path},
                )
                self.storage.add_relationship(relationship)
