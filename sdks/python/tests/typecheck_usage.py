"""Static consumer checks for the exported SDK; no subprocesses on import."""
from typing import Literal

from tinymem_native import AssembledContext, Client, ContextSource, EmbeddingIndexReport, FileSourceInput, Freshness, MemoryEvent, MemoryRecord, SearchMode


def file_memory(client: Client) -> Freshness | None:
    reference: FileSourceInput = {"kind": "file", "role": "dependency", "path": "src/clock.rs"}
    saved = client.add_memory("Use UTC", source_refs=[reference])
    if saved is None:
        return None
    digest: str = saved["source_refs"][0]["content_hash"]
    retry: FileSourceInput = {**reference, "expected_hash": digest}
    client.add_memory("Use UTC", source_refs=[retry])
    return client.memory_history(saved["id"])[0]["freshness"]


def history_contents(client: Client, memory_id: str) -> list[str]:
    events: list[MemoryEvent] = client.memory_history(memory_id)
    contents: list[str] = []
    for event in events:
        kind: Literal["added", "updated", "deleted"] = event["event"]
        version: int = event["version"]
        at_ms: int = event["at_ms"]
        if kind == "deleted" and version > 0 and at_ms > 0:
            contents.append(event["memory"]["content"])
    return contents


def semantic_search(client: Client) -> list[MemoryRecord]:
    report: EmbeddingIndexReport = client.index_memories(limit=1)
    mode: SearchMode = "hybrid"
    if report["remaining"]:
        return []
    return client.search_memories("coding preferences", mode=mode, min_similarity=0.3)


def prompt_context(client: Client) -> str:
    sources: list[ContextSource] = ["memory"]
    context: AssembledContext = client.assemble_context(
        "auth", 2000, sources=sources, exclude_paths=["generated/**"]
    )
    for entry in context["entries"]:
        for source in entry["provenance"]:
            if source["type"] == "memory":
                memory_id: str = source["id"]
                assert memory_id
            else:
                path: str = source["path"]
                assert path
    return context["prompt"]
