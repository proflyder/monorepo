import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('restore_volumes', Path(__file__).with_name('restore-volumes.py'))
restore = importlib.util.module_from_spec(spec)
spec.loader.exec_module(restore)


class RestoreSafetyTests(unittest.TestCase):
    def test_archive_link_cannot_escape_volume(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            archive = base / 'backup.tar.gz'
            with tarfile.open(archive, 'w:gz') as tar:
                metadata = tarfile.TarInfo('backup/volumes.json')
                payload = json.dumps([]).encode()
                metadata.size = len(payload)
                tar.addfile(metadata, io.BytesIO(payload))
                link = tarfile.TarInfo('backup/volumes/runtime-ha/escape')
                link.type = tarfile.SYMTYPE
                link.linkname = '../../../outside'
                tar.addfile(link)
            with patch.object(restore.os, 'geteuid', return_value=0):
                with self.assertRaisesRegex(ValueError, 'Unsafe volume link'):
                    restore.main(archive, base / 'volumes', base / 'pv.json')
            self.assertFalse((base / 'volumes').exists())

    def test_existing_data_is_never_overwritten(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            with patch.object(restore.os, 'geteuid', return_value=0):
                with self.assertRaisesRegex(SystemExit, 'Refusing to overwrite'):
                    restore.main(base / 'missing.tar', base, base / 'pv.json')
