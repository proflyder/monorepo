import json
from pathlib import Path
import sqlite3
import tempfile
import unittest
from rebase_hermes_state import rebase


class RebaseTests(unittest.TestCase):
    def test_only_structured_profile_identity_changes_and_backup_survives(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "state.db"
            old, new = "/home/example/.hermes", "/opt/data"
            with sqlite3.connect(path) as db:
                db.execute("CREATE TABLE state_meta (key TEXT PRIMARY KEY,value TEXT)")
                db.execute("CREATE TABLE messages (text TEXT)")
                original = {"profile_id": old, "nested": [{"profile_id": old}], "text": old}
                db.execute("INSERT INTO state_meta VALUES (?,?)", ("binding", json.dumps(original)))
                db.execute("INSERT INTO state_meta VALUES (?,?)", ("unrelated", "plain metadata"))
                db.execute("INSERT INTO messages VALUES (?)", (old,))
            self.assertEqual(rebase(path, old, new), 1)
            with sqlite3.connect(path) as db:
                value = json.loads(db.execute("SELECT value FROM state_meta WHERE key='binding'").fetchone()[0])
                self.assertEqual(value["profile_id"], new)
                self.assertEqual(value["nested"][0]["profile_id"], new)
                self.assertEqual(value["text"], old)
                self.assertEqual(db.execute("SELECT text FROM messages").fetchone()[0], old)
            with sqlite3.connect(str(path) + ".before-profile-rebase") as db:
                self.assertEqual(json.loads(db.execute("SELECT value FROM state_meta WHERE key='binding'").fetchone()[0])["profile_id"], old)
            with self.assertRaises(FileExistsError):
                rebase(path, old, new)
