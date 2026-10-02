#!/usr/bin/env python3
"""Deterministic vectors for integration tests; NOT a semantic embedding model."""
import json
import sys

request = json.load(sys.stdin)
text = request["text"]
if "espresso" in text or "morning drink" in text or text == "hot beverage":
    vector = [1.0, 0.0, 0.0]
elif "compiler" in text:
    vector = [0.0, 1.0, 0.0]
else:
    vector = [0.0, 0.0, 1.0]
json.dump({"embedding": vector}, sys.stdout)
