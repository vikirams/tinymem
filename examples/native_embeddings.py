#!/usr/bin/env python3
"""Optional offline embedding executable for native TinyMem.

Install sentence-transformers separately in a virtualenv and set this script's
shebang (or a small executable wrapper) to that virtualenv's Python. Pass an
absolute, pre-downloaded local model directory as --embedding-model. No downloads
are allowed here. The script loads a model once per invocation; it is a reference
adapter for small stores, not an optimized persistent inference server.
"""
import contextlib
import json
import sys


def main() -> None:
    request = json.load(sys.stdin)
    # Keep stdout exclusively JSON even if optional libraries print diagnostics.
    with contextlib.redirect_stdout(sys.stderr):
        from sentence_transformers import SentenceTransformer

        model = SentenceTransformer(request["model"], local_files_only=True)
        embedding = model.encode(request["text"], normalize_embeddings=True).tolist()
    json.dump({"embedding": embedding}, sys.stdout, allow_nan=False)


if __name__ == "__main__":
    main()
