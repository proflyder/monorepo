import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("personal_deploy", Path(__file__).with_name("deploy.py"))
deploy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(deploy)


class DeployProfileTests(unittest.TestCase):
    def test_pin_checked_and_manifest_renamed_without_modifying_archive(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            base = root / "secrets/model-registry/v2"
            base.mkdir(parents=True)
            model = b"fixture-personal-model"
            (base / "model.tflite").write_bytes(model)
            original = {"model": "personal.tflite", "micro": {"probability_cutoff": .9, "sliding_window_size": 10}}
            (base / "model.json").write_text(json.dumps(original))
            profile = {"model_path": "secrets/model-registry/v2/model.tflite", "manifest_path": "secrets/model-registry/v2/model.json",
                       "model_sha256": hashlib.sha256(model).hexdigest(), "probability_cutoff": .9, "sliding_window_size": 10}
            data, metadata = deploy.payload(profile, root)
            self.assertEqual(data, model)
            self.assertEqual(json.loads(metadata)["model"], "ru_ricardo_mww.tflite")
            self.assertEqual(json.loads((base / "model.json").read_text()), original)
            (base / "model.tflite").write_bytes(b"changed")
            with self.assertRaises(ValueError): deploy.payload(profile, root)

    def test_rejects_models_outside_registry(self):
        with self.assertRaises(ValueError):
            deploy.payload({"model_path": "public/model.tflite", "manifest_path": "public/model.json"}, Path("/tmp"))


if __name__ == "__main__":
    unittest.main()
