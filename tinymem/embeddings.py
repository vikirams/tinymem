"""Embedding generation for TinyMem."""

from typing import List, Optional

from sentence_transformers import SentenceTransformer


class EmbeddingGenerator:
    """Generate embeddings using sentence-transformers."""

    def __init__(self, model_name: str = "all-MiniLM-L6-v2"):
        """Initialize embedding generator with model."""
        self.model = SentenceTransformer(model_name)
        self.model_name = model_name
        self.embedding_dim = self.model.get_sentence_embedding_dimension()

    def generate(self, text: str) -> List[float]:
        """Generate embedding for a single text."""
        embedding = self.model.encode(text, convert_to_numpy=True)
        return embedding.tolist()

    def generate_batch(self, texts: List[str]) -> List[List[float]]:
        """Generate embeddings for multiple texts."""
        embeddings = self.model.encode(texts, convert_to_numpy=True)
        return embeddings.tolist()

    def get_dimension(self) -> int:
        """Get embedding dimension."""
        return self.embedding_dim


# Singleton instance
_default_generator: Optional[EmbeddingGenerator] = None


def get_embedding_generator(model_name: str = "all-MiniLM-L6-v2") -> EmbeddingGenerator:
    """Get or create default embedding generator."""
    global _default_generator
    if _default_generator is None or _default_generator.model_name != model_name:
        _default_generator = EmbeddingGenerator(model_name)
    return _default_generator
