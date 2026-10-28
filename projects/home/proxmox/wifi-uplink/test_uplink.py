import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch, MagicMock

spec = importlib.util.spec_from_file_location('uplink', Path(__file__).with_name('uplink.py'))
uplink = importlib.util.module_from_spec(spec)
spec.loader.exec_module(uplink)


class ExclusiveUplinkTests(unittest.TestCase):
    def test_auto_prefers_carrier_and_falls_back_without_it(self):
        self.assertEqual(uplink.choose('auto', True), 'ethernet')
        self.assertEqual(uplink.choose('auto', False), 'wifi')
        self.assertEqual(uplink.choose('wifi', True), 'wifi')
        with self.assertRaises(ValueError):
            uplink.choose('both', True)

    def test_transition_never_connects_two_uplinks_and_keeps_vm_ports(self):
        for target, start in [('wifi', {'nic0', 'tap100i0', 'tap101i0'}),
                              ('ethernet', {'wlp4s0', 'tap100i0'}),
                              ('wifi', {'nic0', 'wlp4s0', 'tap100i0'})]:
            members = set(start)
            guest_ports = members - {'nic0', 'wlp4s0'}
            for action, interface in uplink.transitions(target, 'nic0', 'wlp4s0', members):
                if action == 'detach':
                    members.remove(interface)
                else:
                    members.add(interface)
                self.assertLessEqual(len(members & {'nic0', 'wlp4s0'}), 1)
            self.assertTrue(guest_ports <= members)
            self.assertEqual(members & {'nic0', 'wlp4s0'},
                             {'nic0' if target == 'ethernet' else 'wlp4s0'})

    def test_stable_state_does_not_detach_guest_or_uplink(self):
        self.assertEqual(uplink.transitions('wifi', 'nic0', 'wlp4s0',
                                           {'wlp4s0', 'tap100i0'}), [])

spec_install = importlib.util.spec_from_file_location('install', Path(__file__).with_name('install.py'))
installer = importlib.util.module_from_spec(spec_install)
spec_install.loader.exec_module(installer)


class PersistentBridgeTests(unittest.TestCase):
    def test_static_address_and_other_stanzas_survive_and_reapply_is_stable(self):
        before = 'auto vmbr0\niface vmbr0 inet static\n address 192.0.2.26/24\n gateway 192.0.2.1\n bridge-ports nic0\n\niface nic1 inet manual\n'
        result = installer.bridge_config(before, 'vmbr0', '02:00:00:00:00:26')
        self.assertIn('address 192.0.2.26/24', result)
        self.assertIn('gateway 192.0.2.1', result)
        self.assertIn('iface nic1 inet manual', result)
        self.assertIn('bridge-ports none', result)
        self.assertIn('hwaddress ether 02:00:00:00:00:26', result)
        self.assertEqual(installer.bridge_config(result, 'vmbr0', '02:00:00:00:00:26'), result)

    def test_refuses_unrecognized_bridge_configuration(self):
        with self.assertRaises(ValueError):
            installer.bridge_config('iface vmbr0 inet dhcp\n', 'vmbr0', '02:00:00:00:00:26')


class LiveReconciliationTests(unittest.TestCase):
    def test_manual_ethernet_is_brought_up_before_carrier_check(self):
        calls = []
        root = MagicMock()
        root.__truediv__.return_value = root
        root.iterdir.return_value = [type('Port', (), {'name': 'wlp4s0'})()]
        def check_carrier(name):
            self.assertEqual(calls[0], ('ip', 'link', 'set', 'nic0', 'up'))
            return False
        with patch.object(uplink, 'Path', return_value=root), \
             patch.object(uplink, 'run', side_effect=lambda *a: calls.append(a)), \
             patch.object(uplink, 'carrier', side_effect=check_carrier), \
             patch.object(uplink, 'active', return_value=True):
            self.assertEqual(uplink.tick('vmbr0', 'nic0', 'wlp4s0', 'auto'), 'wifi')
        self.assertNotIn(('ip', 'link', 'set', 'nic0', 'master', 'vmbr0'), calls)
