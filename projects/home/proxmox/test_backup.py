import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("preinstall_backup", Path(__file__).with_name("backup-before-install.py"))
backup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(backup)


class BackupRestoreTests(unittest.TestCase):
    def test_copy_failure_restores_original_replica_counts(self):
        calls = []
        def kube(*args):
            calls.append(args)
            if args == ("get", "pv", "-o", "json"):
                return json.dumps({"items": []})
            if "get" in args:
                kind = args[args.index("get") + 1]
                if kind == "deploy":
                    return json.dumps({"items": [{"metadata": {"name": "application"}, "spec": {"replicas": 2}}]})
                if kind == "sts":
                    return json.dumps({"items": [{"metadata": {"name": "database"}, "spec": {"replicas": 1}}]})
                if kind == "ds":
                    return json.dumps({"spec": {"template": {"spec": {"nodeSelector": {"existing": "value"}}}}})
                if kind == "pods":
                    return json.dumps({"items": []})
            return ""
        with tempfile.TemporaryDirectory() as tmp, patch.object(backup, "kube", side_effect=kube), \
             patch.object(backup.os, "geteuid", return_value=0), \
             patch.object(backup.signal, "signal"), \
             patch("pwd.getpwuid", return_value=SimpleNamespace(pw_dir=tmp)), \
             patch.object(backup.subprocess, "run", side_effect=OSError("simulated archive failure")):
            with self.assertRaises(OSError):
                backup.main(Path(tmp) / "new-backup")
        self.assertIn(("-n", "home-assistant", "scale", "deploy/application", "--replicas=2"), calls)
        self.assertIn(("-n", "monitoring", "scale", "deploy/application", "--replicas=2"), calls)
        self.assertIn(("-n", "monitoring", "scale", "sts/database", "--replicas=1"), calls)
        last = calls[-1]
        self.assertEqual(last[:5], ("-n", "monitoring", "patch", "ds", "alloy"))
        self.assertEqual(json.loads(last[-1])[0]["value"], {"existing": "value"})


if __name__ == "__main__":
    unittest.main()
