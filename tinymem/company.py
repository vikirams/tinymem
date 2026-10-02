"""Company brain features for multi-tenancy and sharing."""

from datetime import datetime
from typing import Any, Dict, List, Optional
from uuid import UUID, uuid4

from tinymem.models import Memory, Workspace
from tinymem.storage import Storage


class CompanyBrain:
    """Manage company-wide memory organization and sharing."""

    def __init__(self, storage: Storage):
        """Initialize company brain."""
        self.storage = storage

    def create_workspace(
        self,
        name: str,
        org_id: str,
        metadata: Optional[Dict[str, Any]] = None,
    ) -> UUID:
        """Create a new workspace.

        Args:
            name: Workspace name
            org_id: Organization ID
            metadata: Additional metadata

        Returns:
            Workspace ID
        """
        workspace = Workspace(
            name=name,
            org_id=org_id,
            metadata=metadata or {},
        )

        # Add workspace to storage
        self.storage.conn.execute(
            """
            INSERT INTO workspaces (id, name, org_id, metadata, created_at)
            VALUES (?, ?, ?, ?, ?)
        """,
            (
                str(workspace.id),
                workspace.name,
                workspace.org_id,
                str(workspace.metadata),
                workspace.created_at.isoformat(),
            ),
        )
        self.storage.conn.commit()

        return workspace.id

    def get_workspace(self, workspace_id: UUID) -> Optional[Workspace]:
        """Get a workspace by ID."""
        cursor = self.storage.conn.execute(
            "SELECT * FROM workspaces WHERE id = ?", (str(workspace_id),)
        )
        row = cursor.fetchone()
        if row:
            return self._row_to_workspace(row)
        return None

    def get_workspaces_by_org(self, org_id: str) -> List[Workspace]:
        """Get all workspaces for an organization."""
        cursor = self.storage.conn.execute(
            "SELECT * FROM workspaces WHERE org_id = ?", (org_id,)
        )
        return [self._row_to_workspace(row) for row in cursor.fetchall()]

    def share_memory(
        self,
        memory_id: UUID,
        workspace_id: UUID,
        access_level: str = "read",
    ) -> None:
        """Share a memory with a workspace.

        Args:
            memory_id: Memory ID to share
            workspace_id: Workspace ID to share with
            access_level: Access level (read, write, admin)
        """
        # Add workspace_id to memory metadata
        memory = self.storage.get_memory(memory_id)
        if memory:
            metadata = memory.metadata.copy()
            metadata["shared_with"] = metadata.get("shared_with", [])
            metadata["shared_with"].append(
                {
                    "workspace_id": str(workspace_id),
                    "access_level": access_level,
                    "shared_at": datetime.utcnow().isoformat(),
                }
            )

            # Update memory metadata (would need update method in storage)
            # For now, this is a placeholder

    def get_shared_memories(
        self,
        workspace_id: UUID,
        access_level: Optional[str] = None,
        limit: int = 50,
    ) -> List[Memory]:
        """Get memories shared with a workspace.

        Args:
            workspace_id: Workspace ID
            access_level: Optional access level filter
            limit: Maximum number of results

        Returns:
            List of shared memories
        """
        # Get all memories and filter by sharing metadata
        # In production, this would use a more efficient query
        all_memories = self.storage.search_memories(limit=1000)

        shared_memories = []
        for memory in all_memories:
            shared_with = memory.metadata.get("shared_with", [])
            for share in shared_with:
                if share.get("workspace_id") == str(workspace_id):
                    if access_level is None or share.get("access_level") == access_level:
                        shared_memories.append(memory)
                        break

        return shared_memories[:limit]

    def add_user_to_workspace(
        self,
        workspace_id: UUID,
        user_id: str,
        role: str = "member",
    ) -> None:
        """Add a user to a workspace.

        Args:
            workspace_id: Workspace ID
            user_id: User ID
            role: User role (admin, member, viewer)
        """
        # This would require a separate workspace_users table
        # For now, store in workspace metadata
        workspace = self.get_workspace(workspace_id)
        if workspace:
            metadata = workspace.metadata.copy()
            metadata["users"] = metadata.get("users", {})
            metadata["users"][user_id] = {
                "role": role,
                "added_at": datetime.utcnow().isoformat(),
            }

            # Update workspace metadata
            self.storage.conn.execute(
                """
                UPDATE workspaces SET metadata = ? WHERE id = ?
                """,
                (str(metadata), str(workspace_id)),
            )
            self.storage.conn.commit()

    def get_workspace_users(self, workspace_id: UUID) -> Dict[str, Dict[str, Any]]:
        """Get users in a workspace.

        Args:
            workspace_id: Workspace ID

        Returns:
            Dictionary of user_id -> user_info
        """
        workspace = self.get_workspace(workspace_id)
        if workspace:
            return workspace.metadata.get("users", {})
        return {}

    def check_access(
        self,
        user_id: str,
        workspace_id: UUID,
        required_permission: str = "read",
    ) -> bool:
        """Check if a user has access to a workspace.

        Args:
            user_id: User ID
            workspace_id: Workspace ID
            required_permission: Required permission (read, write, admin)

        Returns:
            True if user has access
        """
        users = self.get_workspace_users(workspace_id)
        user_info = users.get(user_id)

        if not user_info:
            return False

        role = user_info.get("role", "viewer")

        # Permission hierarchy
        permissions = {
            "admin": ["read", "write", "admin"],
            "member": ["read", "write"],
            "viewer": ["read"],
        }

        return required_permission in permissions.get(role, [])

    def get_workspace_analytics(
        self,
        workspace_id: UUID,
    ) -> Dict[str, Any]:
        """Get analytics for a workspace.

        Args:
            workspace_id: Workspace ID

        Returns:
            Analytics data
        """
        # Get all memories in workspace
        # For now, filter by project_id (assuming project_id maps to workspace)
        memories = self.storage.search_memories(
            project_id=str(workspace_id),
            limit=1000,
        )

        # Calculate statistics
        total_memories = len(memories)
        total_access_count = sum(m.access_count for m in memories)
        avg_importance = sum(m.importance_score for m in memories) / total_memories if total_memories > 0 else 0

        # Group by memory type
        memory_types = {}
        for memory in memories:
            memory_type = memory.memory_type.value
            memory_types[memory_type] = memory_types.get(memory_type, 0) + 1

        return {
            "total_memories": total_memories,
            "total_access_count": total_access_count,
            "average_importance": avg_importance,
            "memory_types": memory_types,
            "user_count": len(self.get_workspace_users(workspace_id)),
        }

    def get_org_analytics(
        self,
        org_id: str,
    ) -> Dict[str, Any]:
        """Get analytics for an organization.

        Args:
            org_id: Organization ID

        Returns:
            Organization analytics
        """
        workspaces = self.get_workspaces_by_org(org_id)

        total_memories = 0
        total_users = 0
        workspace_analytics = []

        for workspace in workspaces:
            analytics = self.get_workspace_analytics(workspace.id)
            total_memories += analytics["total_memories"]
            total_users += analytics["user_count"]
            workspace_analytics.append(
                {
                    "workspace_id": str(workspace.id),
                    "workspace_name": workspace.name,
                    **analytics,
                }
            )

        return {
            "org_id": org_id,
            "total_workspaces": len(workspaces),
            "total_memories": total_memories,
            "total_users": total_users,
            "workspaces": workspace_analytics,
        }

    def _row_to_workspace(self, row) -> Workspace:
        """Convert database row to Workspace model."""
        import json

        return Workspace(
            id=UUID(row["id"]),
            name=row["name"],
            org_id=row["org_id"],
            metadata=json.loads(row["metadata"]) if row["metadata"] else {},
            created_at=datetime.fromisoformat(row["created_at"]),
        )
