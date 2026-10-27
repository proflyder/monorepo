import importlib.util
import io
import os
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, call, patch

spec = importlib.util.spec_from_file_location('health_exporter', Path(__file__).parent / 'chart/files/health_exporter.py')
exporter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(exporter)


class ExporterTests(unittest.TestCase):
    def test_ha_can_be_checked_on_separate_vm(self):
        request = SimpleNamespace(path='/metrics', wfile=io.BytesIO(),
                                  send_response=Mock(), send_header=Mock(), end_headers=Mock())
        with patch.dict(os.environ, {'ENDPOINT_HOST_HA': 'haos.home.arpa'}), patch.object(exporter.socket, 'create_connection') as connect:
            exporter.Handler.do_GET(request)
        self.assertIn(b'ricardo_endpoint_up{endpoint="ha"} 1', request.wfile.getvalue())
        self.assertIn(call(('haos.home.arpa', 8123), timeout=1), connect.call_args_list)
        self.assertIn(call(('127.0.0.1', 8095), timeout=1), connect.call_args_list)
