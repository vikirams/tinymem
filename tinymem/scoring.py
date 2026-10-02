"""Memory importance scoring and decay."""

import math
from datetime import datetime, timedelta
from typing import Optional

from tinymem.models import Memory


class ScoringManager:
    """Manage memory importance scoring and decay."""

    def __init__(
        self,
        decay_rate: float = 0.1,
        decay_interval_days: int = 30,
        min_importance: float = 0.1,
        access_boost: float = 0.05,
    ):
        """Initialize scoring manager.

        Args:
            decay_rate: Exponential decay rate (0-1)
            decay_interval_days: Days between decay applications
            min_importance: Minimum importance score
            access_boost: Boost amount per access
        """
        self.decay_rate = decay_rate
        self.decay_interval = timedelta(days=decay_interval_days)
        self.min_importance = min_importance
        self.access_boost = access_boost

    def calculate_initial_score(
        self,
        content: str,
        memory_type: str,
        metadata: Optional[dict] = None,
    ) -> float:
        """Calculate initial importance score for a memory.

        Args:
            content: Memory content
            memory_type: Type of memory
            metadata: Additional metadata

        Returns:
            Initial importance score (0-1)
        """
        # Base score based on memory type
        type_scores = {
            "fact": 0.5,
            "preference": 0.7,
            "decision": 0.8,
            "plan": 0.6,
            "relationship": 0.6,
            "code_context": 0.4,
            "architectural_decision": 0.9,
            "issue_solution": 0.8,
        }

        base_score = type_scores.get(memory_type, 0.5)

        # Content length factor (longer content might be more important)
        length_factor = min(len(content) / 1000, 0.2)

        # Metadata boosts
        metadata_boost = 0.0
        if metadata:
            if metadata.get("category") == "critical":
                metadata_boost += 0.2
            if metadata.get("priority") == "high":
                metadata_boost += 0.15

        # Calculate final score
        score = base_score + length_factor + metadata_boost
        return min(max(score, 0.0), 1.0)

    def apply_decay(self, memory: Memory, last_decay_date: Optional[datetime] = None) -> float:
        """Apply exponential decay to memory importance.

        Args:
            memory: Memory to decay
            last_decay_date: Last time decay was applied

        Returns:
            New importance score
        """
        if last_decay_date is None:
            last_decay_date = memory.created_at

        days_since_decay = (datetime.utcnow() - last_decay_date).days
        if days_since_decay < self.decay_interval.days:
            return memory.importance_score

        # Apply exponential decay
        decay_periods = days_since_decay / self.decay_interval.days
        new_score = memory.importance_score * math.exp(-self.decay_rate * decay_periods)

        # Ensure minimum importance
        return max(new_score, self.min_importance)

    def boost_on_access(self, memory: Memory) -> float:
        """Boost importance score when memory is accessed.

        Args:
            memory: Memory being accessed

        Returns:
            New importance score
        """
        new_score = memory.importance_score + self.access_boost

        # Decay effect of recent accesses (diminishing returns)
        time_since_last_access = (
            datetime.utcnow() - memory.last_accessed
            if memory.last_accessed
            else timedelta(days=0)
        )
        if time_since_last_access.days < 1:
            new_score -= self.access_boost * 0.5

        return min(new_score, 1.0)

    def should_forget(self, memory: Memory, threshold: float = 0.05) -> bool:
        """Determine if a memory should be forgotten (archived/deleted).

        Args:
            memory: Memory to evaluate
            threshold: Score threshold below which to forget

        Returns:
            True if memory should be forgotten
        """
        # Don't forget critical memories
        if memory.metadata.get("category") == "critical":
            return False

        # Check if score is below threshold
        if memory.importance_score < threshold:
            return True

        # Check if memory is very old and rarely accessed
        age_days = (datetime.utcnow() - memory.created_at).days
        if age_days > 365 and memory.access_count < 5:
            return True

        return False

    def calculate_retrieval_score(
        self,
        similarity: float,
        importance: float,
        recency_factor: float = 0.1,
    ) -> float:
        """Calculate combined retrieval score.

        Args:
            similarity: Semantic similarity score
            importance: Memory importance score
            recency_factor: Weight for recency

        Returns:
            Combined score
        """
        # Combine similarity and importance
        combined = (similarity * 0.7) + (importance * 0.3)

        # Add recency boost (can be calculated externally)
        combined += recency_factor

        return min(combined, 1.0)
