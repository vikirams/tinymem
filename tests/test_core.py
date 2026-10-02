"""Basic tests for TinyMem core functionality."""

import pytest
from tinymem import TinyMem, MemoryType
from tinymem.models import Memory


@pytest.fixture
def tinymem():
    """Create a TinyMem instance for testing."""
    mem = TinyMem(db_path=":memory:")  # Use in-memory database
    yield mem
    mem.close()


def test_add_memory(tinymem):
    """Test adding a memory."""
    memory_id = tinymem.add(
        content="Test memory content",
        user_id="user1",
        memory_type=MemoryType.FACT,
    )

    assert memory_id is not None

    # Verify memory was stored
    memory = tinymem.get(memory_id)
    assert memory is not None
    assert memory.content == "Test memory content"
    assert memory.user_id == "user1"
    assert memory.memory_type == MemoryType.FACT


def test_search_memories(tinymem):
    """Test searching for memories."""
    # Add some memories
    tinymem.add("User lives in San Francisco", user_id="user1")
    tinymem.add("User prefers Python", user_id="user1")
    tinymem.add("User likes coffee", user_id="user1")

    # Search
    results = tinymem.search("location", user_id="user1", limit=5)

    assert len(results) > 0
    assert any("San Francisco" in result.memory.content for result in results)


def test_delete_memory(tinymem):
    """Test deleting a memory."""
    memory_id = tinymem.add("Test memory", user_id="user1")

    # Verify it exists
    memory = tinymem.get(memory_id)
    assert memory is not None

    # Delete it
    tinymem.delete(memory_id)

    # Verify it's soft deleted
    memory = tinymem.get(memory_id)
    assert memory is not None
    assert memory.is_deleted is True


def test_code_context(tinymem):
    """Test adding code context."""
    code = """
def authenticate(username, password):
    return True

class AuthManager:
    def __init__(self):
        pass
"""

    memory_id = tinymem.add_code_context(
        file_path="test.py",
        content=code,
        language="python",
        project_id="proj1",
    )

    assert memory_id is not None

    # Search for code
    results = tinymem.search_code("authenticate", project_id="proj1")
    assert len(results) > 0


def test_deduplication(tinymem):
    """Test memory deduplication."""
    # Add the same content twice
    id1 = tinymem.add("Test content", user_id="user1")
    id2 = tinymem.add("Test content", user_id="user1")

    # Should return the same memory ID due to deduplication
    assert id1 == id2


def test_scoping(tinymem):
    """Test memory scoping."""
    # Add memories for different users
    tinymem.add("User 1 memory", user_id="user1")
    tinymem.add("User 2 memory", user_id="user2")

    # Search for user1 should only return user1's memories
    results = tinymem.search("memory", user_id="user1")
    assert all(result.memory.user_id == "user1" for result in results)


def test_importance_scoring(tinymem):
    """Test importance scoring."""
    # Add different types of memories
    fact_id = tinymem.add("Simple fact", memory_type=MemoryType.FACT)
    decision_id = tinymem.add("Important decision", memory_type=MemoryType.DECISION)

    fact = tinymem.get(fact_id)
    decision = tinymem.get(decision_id)

    # Decision should have higher importance than fact
    assert decision.importance_score > fact.importance_score


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
