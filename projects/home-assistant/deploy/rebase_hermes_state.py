#!/usr/bin/env python3
"""Rebase persisted Hermes profile references after relocating HERMES_HOME.

Stop the gateway before use. Changes only structured profile_id fields in
state_meta JSON; conversations, authentication, and plain text are untouched.
A SQLite backup is created before the transaction. Caller supplies private paths.
"""
import argparse
import json
from pathlib import Path
import sqlite3


def rebase(path: Path, old_home: str, new_home: str) -> int:
    backup = path.with_name(path.name + ".before-profile-rebase")
    if backup.exists():
        raise FileExistsError("Refusing to overwrite the pre-migration backup")
    with sqlite3.connect(path) as db, sqlite3.connect(backup) as saved:
        db.backup(saved)
        backup.chmod(0o600)
        assert db.execute("PRAGMA integrity_check").fetchone()[0] == "ok"

        def visit(value):
            changed = False
            if isinstance(value, dict):
                for key, child in value.items():
                    if key == "profile_id" and child == old_home:
                        value[key] = new_home
                        changed = True
                    else:
                        changed = visit(child) or changed
            elif isinstance(value, list):
                for child in value:
                    changed = visit(child) or changed
            return changed

        count = 0
        for key, raw in db.execute("SELECT key,value FROM state_meta").fetchall():
            try:
                value = json.loads(raw)
            except (ValueError, TypeError):
                continue
            if visit(value):
                db.execute("UPDATE state_meta SET value=? WHERE key=?",
                    (json.dumps(value, separators=(",", ":")), key))
                count += 1
        assert db.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
        return count


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--database", type=Path, required=True)
    parser.add_argument("--old-home", required=True)
    parser.add_argument("--new-home", required=True)
    args = parser.parse_args()
    print("Rebased metadata rows:", rebase(args.database, args.old_home, args.new_home))
