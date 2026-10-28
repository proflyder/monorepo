"""Keep retired QR/DPP provisioning out of the deployed boot path."""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[3]
CONFIG = ROOT / 'projects/station/configs/station-applied'


class ProvisioningContract(unittest.TestCase):
    def test_boot_and_watchdog_keep_ble_without_respawning_dpp(self):
        keeper = (CONFIG / 'rootkeeper.sh').read_text()
        boot = (CONFIG / 'onboot.sh').read_text()
        self.assertIn('/data/quasar/ble-gateway/start.sh', keeper)
        self.assertIn('/data/quasar/rootkeeper.sh', boot)
        for text in [keeper, boot]:
            self.assertNotIn('dpp-provision', text)
            self.assertNotIn('dpp_listen', text)
        self.assertIn('/data/quasar/egress-guard.sh', keeper)
        self.assertIn('/system/vendor/quasar/activate_adb.sh', keeper)

    def test_retired_helpers_are_not_reintroduced(self):
        for name in ['station-dpp-provision.sh', 'station-dpp-debug.sh', 'station-dpp-event.sh']:
            self.assertFalse((ROOT / 'projects/station/scripts' / name).exists())
