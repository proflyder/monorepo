import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('ha_network', Path(__file__).with_name('migrate-ha-network.py'))
network = importlib.util.module_from_spec(spec)
spec.loader.exec_module(network)


class NetworkMigrationTests(unittest.TestCase):
    def test_only_known_local_endpoints_change_and_auth_survives(self):
        source = {'data': {'entries': [
            {'domain': 'wyoming', 'data': {'host': '127.0.0.1', 'port': 10305}},
            {'domain': 'llama_cpp', 'data': {'base_url': 'http://localhost:8643/v1', 'api_key': 'test-only'}},
            {'domain': 'matter', 'data': {'url': 'ws://192.0.2.50:5580/ws'}},
            {'domain': 'unrelated', 'data': {'url': 'http://localhost:8643'}}
        ]}}
        result, count = network.migrate(source, '192.0.2.27')
        self.assertEqual(count, 2)
        self.assertEqual(result['data']['entries'][0]['data']['host'], '192.0.2.27')
        self.assertEqual(result['data']['entries'][1]['data'], {'base_url': 'http://192.0.2.27:8643/v1', 'api_key': 'test-only'})
        self.assertEqual(result['data']['entries'][2:], source['data']['entries'][2:])
        self.assertEqual(source['data']['entries'][0]['data']['host'], '127.0.0.1')
