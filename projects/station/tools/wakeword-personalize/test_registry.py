import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("registry", Path(__file__).with_name("registry.py"))
registry = importlib.util.module_from_spec(spec)
spec.loader.exec_module(registry)


class RegistryTests(unittest.TestCase):
    def test_snapshot_is_independent_and_cannot_be_overwritten(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            source = root / "source"
            source.mkdir()
            (source / "model.tflite").write_bytes(b"fixture-model")
            (source / "weights.h5").write_bytes(b"fixture-weights")
            saved = registry.snapshot(root / "registry", "v1", source)
            self.assertEqual(registry.verify(saved), 2)
            (source / "model.tflite").write_bytes(b"new contents")
            self.assertEqual((saved / "model.tflite").read_bytes(), b"fixture-model")
            with self.assertRaises(FileExistsError): registry.snapshot(root / "registry", "v1", source)
            (saved / "weights.h5").write_bytes(b"corruption")
            with self.assertRaises(ValueError): registry.verify(saved)

    def test_rejects_recursive_source_and_symlinks(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            with self.assertRaises(ValueError): registry.snapshot(root / "registry", "v1", root)
            source = root / "source"
            source.mkdir()
            (root / "outside").write_bytes(b"do not copy")
            (source / "link").symlink_to(root / "outside")
            with self.assertRaises(ValueError): registry.snapshot(root / "registry", "v1", source)
            self.assertFalse((root / "registry/v1").exists())


if __name__ == "__main__":
    unittest.main()
