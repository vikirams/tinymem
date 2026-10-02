"""Smoke the built wheel in isolated Python; run after pip wheel."""
import ast
from pathlib import Path
import sys
import zipfile

wheel = Path(sys.argv[1]).resolve()
with zipfile.ZipFile(wheel) as archive:
    for name in archive.namelist():
        if name.endswith(".py"):
            ast.parse(archive.read(name).decode("utf-8"), feature_version=(3, 10))
    assert "tinymem_native/py.typed" in archive.namelist()
    metadata_name = next(name for name in archive.namelist() if name.endswith(".dist-info/METADATA"))
    metadata = archive.read(metadata_name).decode()
    assert "Requires-Python: >=3.10" in metadata
    assert "Requires-Dist:" not in metadata
sys.path.insert(0, str(wheel))
from tinymem_native import Client, FileSourceInput, FileSourceReference, Freshness

assert FileSourceInput and FileSourceReference and Freshness

assert "tinymem" not in sys.modules
fixture = Path(__file__).with_name("fake_server.py").resolve()
fixture.chmod(0o755)
client = Client(binary=fixture, user_id="packaging")
assert client.call_tool("echo", {"content": "wheel"})["arguments"] == {"content": "wheel"}
print("Wheel import, dependency metadata, Python 3.10 syntax, and public API: PASS")
