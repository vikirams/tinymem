"""Local, model-free memory. Every agent process opens the same database file."""

from __future__ import annotations

import os
import re
import sqlite3
import sys
import time
import uuid
from datetime import datetime, timedelta, timezone
from pathlib import Path

KINDS = ("fact", "preference", "decision", "plan")
MAX_CONTENT = 8_000
RECOMMENDED_CONTENT = 500
MAX_PROFILE = 4_000
MAX_QUERY = 200
MAX_TERMS = 10
MAX_NAMESPACE = 64
MAX_ACTIVE_PER_NAMESPACE = 2_000
NAMESPACE_RE = re.compile(r"^[A-Za-z0-9_:-]{1,64}$")

_UNSET: object = object()


class StoreError(ValueError):
    pass


def default_db() -> Path:
    override = os.environ.get("TINYMEM_DB")
    if override:
        return Path(override).expanduser().resolve()
    return Path.home() / ".tinymem" / "tinymem.db"


def default_namespace() -> str:
    raw = os.environ.get("TINYMEM_NAMESPACE", "default")
    try:
        return _namespace(raw)
    except StoreError:
        if "TINYMEM_NAMESPACE" in os.environ:
            print(
                f"tinymem: ignoring invalid TINYMEM_NAMESPACE={raw!r}, using 'default'",
                file=sys.stderr,
            )
        return "default"


class Store:
    def __init__(self, path: Path) -> None:
        self.path = path
        path.parent.mkdir(parents=True, exist_ok=True)
        last: sqlite3.Error | None = None
        for _ in range(20):
            try:
                self._conn = sqlite3.connect(path, timeout=5)
                self._conn.row_factory = sqlite3.Row
                self._conn.execute("PRAGMA journal_mode=WAL")
                self._conn.execute("PRAGMA busy_timeout=5000")
                self._ensure_schema()
                return
            except sqlite3.OperationalError as exc:
                if "locked" not in str(exc).lower():
                    raise
                last = exc
                try:
                    self._conn.close()
                except Exception:
                    pass
                time.sleep(0.05)
        assert last is not None
        raise last

    def close(self) -> None:
        self._conn.close()

    def _ensure_schema(self) -> None:
        self._conn.execute(
            """
            CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                kind TEXT NOT NULL,
                namespace TEXT NOT NULL DEFAULT 'default',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                valid_until TEXT,
                deleted_at TEXT,
                norm TEXT NOT NULL DEFAULT ''
            )
            """
        )
        cols = {row[1] for row in self._conn.execute("PRAGMA table_info(memories)").fetchall()}
        if "namespace" not in cols:
            self._conn.execute(
                "ALTER TABLE memories ADD COLUMN namespace TEXT NOT NULL DEFAULT 'default'"
            )
        if "updated_at" not in cols:
            self._conn.execute("ALTER TABLE memories ADD COLUMN updated_at TEXT")
        if "valid_until" not in cols:
            self._conn.execute("ALTER TABLE memories ADD COLUMN valid_until TEXT")
        if "norm" not in cols:
            self._conn.execute("ALTER TABLE memories ADD COLUMN norm TEXT NOT NULL DEFAULT ''")
        self._conn.execute(
            "UPDATE memories SET updated_at = created_at WHERE updated_at IS NULL"
        )
        self._conn.execute(
            "UPDATE memories SET namespace = 'default' WHERE namespace IS NULL OR namespace = ''"
        )
        # Backfill normalized text for fuzzy dedup on old rows.
        for row in self._conn.execute(
            "SELECT id, content FROM memories WHERE norm IS NULL OR norm = ''"
        ).fetchall():
            self._conn.execute(
                "UPDATE memories SET norm = ? WHERE id = ?",
                (_normalize(row["content"]), row["id"]),
            )
        self._conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memories_active "
            "ON memories(namespace, deleted_at, updated_at DESC)"
        )
        self._conn.execute("CREATE INDEX IF NOT EXISTS idx_memories_kind ON memories(kind)")
        self._conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memories_norm "
            "ON memories(namespace, kind, norm)"
        )
        self._conn.execute(
            """
            CREATE TABLE IF NOT EXISTS profiles (
                namespace TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            """
        )
        self._conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts "
            "USING fts5(id UNINDEXED, content, tokenize='porter')"
        )
        self._conn.commit()
        self._sync_fts_if_needed()

    def _sync_fts_if_needed(self) -> None:
        try:
            self._conn.execute("BEGIN IMMEDIATE")
            try:
                active = self._conn.execute(
                    "SELECT COUNT(*) FROM memories WHERE deleted_at IS NULL"
                ).fetchone()[0]
                fts_count = self._conn.execute("SELECT COUNT(*) FROM memories_fts").fetchone()[0]
                if fts_count != active:
                    self._conn.execute("DELETE FROM memories_fts")
                    self._conn.execute(
                        "INSERT INTO memories_fts(id, content) "
                        "SELECT id, content FROM memories WHERE deleted_at IS NULL"
                    )
                self._conn.commit()
            except Exception:
                try:
                    self._conn.rollback()
                except sqlite3.Error:
                    pass
                raise
        except sqlite3.Error:
            try:
                self._conn.rollback()
            except sqlite3.Error:
                pass
            raise

    def add(
        self,
        content: str,
        kind: str = "fact",
        namespace: str = "default",
        valid_until: str | None = None,
    ) -> dict:
        text = _content(content)
        label = _kind(kind)
        space = _namespace(namespace)
        expiry = _valid_until(valid_until)
        norm = _normalize(text)
        existing = self._conn.execute(
            """
            SELECT id, content, kind, namespace, created_at, updated_at, valid_until
            FROM memories
            WHERE deleted_at IS NULL AND content = ? AND kind = ? AND namespace = ?
            AND ((valid_until IS NULL AND ? IS NULL) OR valid_until IS ?)
            """,
            (text, label, space, expiry, expiry),
        ).fetchone()
        if existing is not None:
            return {**_row(existing), "created": False}
        fuzzy = self._conn.execute(
            """
            SELECT id, content, kind, namespace, created_at, updated_at, valid_until
            FROM memories
            WHERE deleted_at IS NULL AND namespace = ? AND kind = ? AND norm = ?
            LIMIT 1
            """,
            (space, label, norm),
        ).fetchone()
        if fuzzy is not None:
            return {**_row(fuzzy), "created": False}
        count = self._conn.execute(
            "SELECT COUNT(*) FROM memories WHERE deleted_at IS NULL AND namespace = ?",
            (space,),
        ).fetchone()[0]
        if count >= MAX_ACTIVE_PER_NAMESPACE:
            raise StoreError(
                f"namespace '{space}' quota exceeded ({MAX_ACTIVE_PER_NAMESPACE} active);"
                " update, delete, or purge old memories"
            )
        now = _now()
        record = {
            "id": uuid.uuid4().hex,
            "content": text,
            "kind": label,
            "namespace": space,
            "created_at": now,
            "updated_at": now,
            "valid_until": expiry,
        }
        self._conn.execute(
            "INSERT INTO memories (id, content, kind, namespace, created_at, updated_at, valid_until, norm)"
            " VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            (
                record["id"],
                record["content"],
                record["kind"],
                record["namespace"],
                record["created_at"],
                record["updated_at"],
                record["valid_until"],
                norm,
            ),
        )
        self._conn.execute(
            "INSERT INTO memories_fts(id, content) VALUES (?, ?)",
            (record["id"], record["content"]),
        )
        self._conn.commit()
        return {**record, "created": True}

    def update(
        self,
        memory_id: str,
        content: str | None = None,
        kind: str | None = None,
        valid_until: str | None | object = _UNSET,
        namespace: str | None = None,
    ) -> dict:
        identity = _id(memory_id)
        space: str | None = _namespace(namespace) if namespace is not None else None
        with self._conn:
            row = self._conn.execute(
                """
                SELECT id, content, kind, namespace, created_at, updated_at, valid_until, norm
                FROM memories WHERE id = ? AND deleted_at IS NULL
                """,
                (identity,),
            ).fetchone()
            if row is None:
                raise StoreError("Memory not found")
            if space is not None and row["namespace"] != space:
                raise StoreError(
                    f"refusing cross-namespace update: id is in '{row['namespace']}',"
                    f" session is '{space}'"
                )
            new_content = row["content"] if content is None else _content(content)
            new_kind = row["kind"] if kind is None else _kind(kind)
            new_expiry = row["valid_until"] if valid_until is _UNSET else _valid_until(valid_until)
            if new_content == row["content"] and new_kind == row["kind"] and new_expiry == row["valid_until"]:
                return {**_row(row), "updated": False}
            new_norm = _normalize(new_content)
            clash = self._conn.execute(
                """
                SELECT id FROM memories
                WHERE deleted_at IS NULL AND namespace = ? AND kind = ?
                AND norm = ? AND id != ?
                LIMIT 1
                """,
                (row["namespace"], new_kind, new_norm, identity),
            ).fetchone()
            if clash is not None:
                raise StoreError(
                    "update would duplicate existing memory"
                    f" {clash['id']}; update or delete that one instead"
                )
            now = _now()
            self._conn.execute(
                "UPDATE memories SET content = ?, kind = ?, valid_until = ?, updated_at = ?, norm = ?"
                " WHERE id = ?",
                (new_content, new_kind, new_expiry, now, new_norm, identity),
            )
            if new_content != row["content"]:
                self._conn.execute(
                    "UPDATE memories_fts SET content = ? WHERE id = ?",
                    (new_content, identity),
                )
        updated = self._conn.execute(
            """
            SELECT id, content, kind, namespace, created_at, updated_at, valid_until
            FROM memories WHERE id = ?
            """,
            (identity,),
        ).fetchone()
        return {**_row(updated), "updated": True}

    def search(
        self,
        query: str,
        limit: int = 5,
        namespace: str | None = None,
        kind: str | None = None,
    ) -> dict:
        tokens = _tokens(query)
        if isinstance(limit, bool) or not isinstance(limit, int):
            raise StoreError("limit must be an integer")
        bounded = min(max(limit, 1), 20)
        space = _namespace(namespace) if namespace is not None else None
        label = _kind(kind) if kind is not None else None
        now = _now()
        fts_query = " AND ".join(f'"{token}"*' for token in tokens)
        try:
            sql = (
                "SELECT m.id, m.content, m.kind, m.namespace, m.created_at, m.updated_at,"
                " m.valid_until, bm25(memories_fts) AS rank"
                " FROM memories_fts JOIN memories m ON m.id = memories_fts.id"
                " WHERE memories_fts MATCH ? AND m.deleted_at IS NULL"
                " AND (m.valid_until IS NULL OR m.valid_until > ?)"
            )
            args: list[object] = [fts_query, now]
            if space is not None:
                sql += " AND m.namespace = ?"
                args.append(space)
            if label is not None:
                sql += " AND m.kind = ?"
                args.append(label)
            sql += " ORDER BY rank, m.updated_at DESC LIMIT 200"
            rows = self._conn.execute(sql, args).fetchall()
        except sqlite3.OperationalError:
            rows = self._like_fallback(tokens[0])
            rows = [
                row
                for row in rows
                if all(token in row["content"].lower() for token in tokens)
                and (space is None or row["namespace"] == space)
                and (label is None or row["kind"] == label)
                and (row["valid_until"] is None or row["valid_until"] > now)
            ]
            rows.sort(key=lambda r: r["updated_at"], reverse=True)
            rows = rows[:200]
        seen: dict[str, dict] = {}
        for row in rows:
            if row["id"] not in seen:
                seen[row["id"]] = _row(row)
        memories = list(seen.values())[:bounded]
        result: dict = {"memories": memories, "query": tokens}
        if space is not None:
            result["namespace"] = space
        if label is not None:
            result["kind"] = label
        return result

    def _like_fallback(self, first_token: str) -> list[sqlite3.Row]:
        like = "%" + first_token.replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_") + "%"
        return self._conn.execute(
            """
            SELECT id, content, kind, namespace, created_at, updated_at, valid_until
            FROM memories
            WHERE deleted_at IS NULL AND lower(content) LIKE ? ESCAPE '\\'
            ORDER BY updated_at DESC
            LIMIT 200
            """,
            (like,),
        ).fetchall()

    def delete(self, memory_id: str, namespace: str | None = None) -> dict:
        identity = _id(memory_id)
        space: str | None = _namespace(namespace) if namespace is not None else None
        now = _now()
        with self._conn:
            row = self._conn.execute(
                "SELECT namespace FROM memories WHERE id = ? AND deleted_at IS NULL",
                (identity,),
            ).fetchone()
            if row is None:
                raise StoreError("Memory not found")
            if space is not None and row["namespace"] != space:
                raise StoreError(
                    f"refusing cross-namespace delete: id is in '{row['namespace']}',"
                    f" session is '{space}'"
                )
            if space is None:
                cursor = self._conn.execute(
                    """
                    UPDATE memories SET deleted_at = ?
                    WHERE id = ? AND deleted_at IS NULL
                    """,
                    (now, identity),
                )
            else:
                cursor = self._conn.execute(
                    """
                    UPDATE memories SET deleted_at = ?
                    WHERE id = ? AND deleted_at IS NULL AND namespace = ?
                    """,
                    (now, identity, space),
                )
            if cursor.rowcount != 1:
                raise StoreError("Memory not found")
            self._conn.execute("DELETE FROM memories_fts WHERE id = ?", (identity,))
        return {"id": identity, "deleted": True}

    def get_profile(self, namespace: str) -> dict:
        space = _namespace(namespace)
        row = self._conn.execute(
            "SELECT namespace, content, updated_at FROM profiles WHERE namespace = ?",
            (space,),
        ).fetchone()
        if row is None:
            return {"namespace": space, "profile": "", "updated_at": None}
        return {"namespace": row["namespace"], "profile": row["content"], "updated_at": row["updated_at"]}

    def set_profile(self, namespace: str, content: str) -> dict:
        space = _namespace(namespace)
        if not isinstance(content, str):
            raise StoreError("profile must be a string")
        text = content.strip()
        if len(text) > MAX_PROFILE:
            raise StoreError(f"profile exceeds {MAX_PROFILE} characters")
        if not text:
            self._conn.execute("DELETE FROM profiles WHERE namespace = ?", (space,))
            self._conn.commit()
            return {"namespace": space, "profile": "", "updated_at": None, "cleared": True}
        now = _now()
        self._conn.execute(
            "INSERT INTO profiles (namespace, content, updated_at) VALUES (?, ?, ?)"
            " ON CONFLICT(namespace) DO UPDATE SET content = excluded.content,"
            " updated_at = excluded.updated_at",
            (space, text, now),
        )
        self._conn.commit()
        return {"namespace": space, "profile": text, "updated_at": now, "cleared": False}

    def stats(self) -> dict:
        now = _now()
        try:
            db_bytes = self.path.stat().st_size
        except OSError:
            db_bytes = 0
        active = self._conn.execute(
            "SELECT COUNT(*) FROM memories WHERE deleted_at IS NULL"
        ).fetchone()[0]
        deleted = self._conn.execute(
            "SELECT COUNT(*) FROM memories WHERE deleted_at IS NOT NULL"
        ).fetchone()[0]
        expired = self._conn.execute(
            "SELECT COUNT(*) FROM memories WHERE deleted_at IS NULL"
            " AND valid_until IS NOT NULL AND valid_until <= ?",
            (now,),
        ).fetchone()[0]
        by_namespace = [
            {"namespace": row[0], "active": row[1]}
            for row in self._conn.execute(
                "SELECT namespace, COUNT(*) FROM memories WHERE deleted_at IS NULL"
                " GROUP BY namespace ORDER BY COUNT(*) DESC"
            ).fetchall()
        ]
        by_kind = [
            {"kind": row[0], "active": row[1]}
            for row in self._conn.execute(
                "SELECT kind, COUNT(*) FROM memories WHERE deleted_at IS NULL"
                " GROUP BY kind ORDER BY COUNT(*) DESC"
            ).fetchall()
        ]
        return {
            "db_path": str(self.path),
            "db_bytes": db_bytes,
            "active": active,
            "deleted": deleted,
            "expired": expired,
            "by_namespace": by_namespace,
            "by_kind": by_kind,
        }

    def purge(self, grace_days: int = 30) -> dict:
        if isinstance(grace_days, bool) or not isinstance(grace_days, int):
            raise StoreError("grace_days must be an integer")
        if not 1 <= grace_days <= 365:
            raise StoreError("grace_days must be between 1 and 365")
        cutoff = (datetime.now(timezone.utc) - timedelta(days=grace_days)).strftime(
            "%Y-%m-%dT%H:%M:%SZ"
        )
        self._conn.execute(
            "DELETE FROM memories_fts WHERE id IN ("
            "SELECT id FROM memories WHERE"
            " (deleted_at IS NOT NULL AND deleted_at < ?)"
            " OR (valid_until IS NOT NULL AND valid_until < ?)"
            ")",
            (cutoff, cutoff),
        )
        cursor = self._conn.execute(
            "DELETE FROM memories WHERE"
            " (deleted_at IS NOT NULL AND deleted_at < ?)"
            " OR (valid_until IS NOT NULL AND valid_until < ?)",
            (cutoff, cutoff),
        )
        purged = cursor.rowcount if cursor.rowcount is not None and cursor.rowcount >= 0 else 0
        self._conn.commit()
        try:
            self._conn.execute("VACUUM")
            self._conn.commit()
            vacuumed = True
        except sqlite3.Error:
            vacuumed = False
        return {"purged": purged, "cutoff": cutoff, "vacuumed": vacuumed}


def _content(value: str) -> str:
    if not isinstance(value, str):
        raise StoreError("content must be a string")
    text = value.strip()
    if not text:
        raise StoreError("content is empty")
    if len(text) > MAX_CONTENT:
        raise StoreError(f"content exceeds {MAX_CONTENT} characters")
    return text


def _normalize(value: str) -> str:
    return " ".join(re.findall(r"[a-z0-9]+", value.lower()))


def _kind(value: str) -> str:
    if value not in KINDS:
        raise StoreError(f"kind must be one of: {', '.join(KINDS)}")
    return value


def _namespace(value: str) -> str:
    if not isinstance(value, str):
        raise StoreError("namespace must be a string")
    text = value.strip()
    if not NAMESPACE_RE.fullmatch(text):
        raise StoreError("namespace must match ^[A-Za-z0-9_:-]{1,64}$")
    return text


def _valid_until(value: str | None) -> str | None:
    if value is None:
        return None
    if not isinstance(value, str):
        raise StoreError("valid_until must be an ISO date string or null")
    text = value.strip()
    if not text:
        raise StoreError("valid_until is empty")
    try:
        if re.fullmatch(r"\d{4}-\d{2}-\d{2}", text):
            parsed = datetime.strptime(text, "%Y-%m-%d").replace(tzinfo=timezone.utc)
            parsed = parsed.replace(hour=23, minute=59, second=59)
        else:
            parsed = datetime.fromisoformat(text.replace("Z", "+00:00"))
            if parsed.tzinfo is None:
                parsed = parsed.replace(tzinfo=timezone.utc)
            else:
                parsed = parsed.astimezone(timezone.utc)
    except ValueError:
        raise StoreError("valid_until must be YYYY-MM-DD or ISO-8601") from None
    return parsed.strftime("%Y-%m-%dT%H:%M:%SZ")


def _id(value: str) -> str:
    if not isinstance(value, str) or not re.fullmatch(r"[0-9a-f]{32}", value):
        raise StoreError("id must be a 32-character hex memory id")
    return value


def _tokens(query: str) -> list[str]:
    if not isinstance(query, str):
        raise StoreError("query must be a string")
    if len(query) > MAX_QUERY:
        raise StoreError(f"query exceeds {MAX_QUERY} characters")
    tokens = re.findall(r"[A-Za-z0-9_]+", query.lower())
    if not tokens:
        raise StoreError("query needs at least one letter or number")
    if len(tokens) > MAX_TERMS:
        raise StoreError(f"query accepts at most {MAX_TERMS} terms")
    return tokens


def _row(row: sqlite3.Row) -> dict:
    return {
        "id": row["id"],
        "content": row["content"],
        "kind": row["kind"],
        "namespace": row["namespace"] if "namespace" in row.keys() else "default",
        "created_at": row["created_at"],
        "updated_at": row["updated_at"] if "updated_at" in row.keys() else row["created_at"],
        "valid_until": row["valid_until"] if "valid_until" in row.keys() else None,
    }


def _now() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
