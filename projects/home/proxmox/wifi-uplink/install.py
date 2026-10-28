#!/usr/bin/env python3
"""Install a previously tested 4-address Wi-Fi bridge uplink on Proxmox."""
import argparse
from datetime import datetime, timezone
import os
from pathlib import Path
import re
import shutil
import subprocess


def bridge_config(text, bridge, mac):
    pattern = re.compile(r'(^iface ' + re.escape(bridge) + r' inet static\n)(.*?)(?=^\S|\Z)', re.M | re.S)
    match = pattern.search(text)
    if match is None:
        raise ValueError('Expected an existing static management bridge stanza')
    body = match.group(2)
    for option, value in [('bridge-ports', 'none'), ('hwaddress', 'ether ' + mac),
                          ('bridge-mcsnoop', '0')]:
        line = '\t' + option + ' ' + value + '\n'
        key = re.compile(r'^\s*' + option + r'\s+[^\n]*\n', re.M)
        body = key.sub(lambda _: line, body) if key.search(body) else body.rstrip('\n') + '\n' + line
    return text[:match.start()] + match.group(1) + body.rstrip('\n') + '\n\n' + text[match.end():]


def main():
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile', type=Path, required=True, help='Private wpa_supplicant config')
    parser.add_argument('--bridge', default='vmbr0')
    parser.add_argument('--wired', default='nic0')
    parser.add_argument('--wifi', default='wlp4s0')
    args = parser.parse_args()
    if os.geteuid() != 0:
        parser.error('Run on the Proxmox host as root')
    for name in (args.bridge, args.wired, args.wifi):
        if not re.fullmatch(r'[a-zA-Z0-9_.-]{1,15}', name):
            parser.error('Invalid interface name')
        if not (Path('/sys/class/net') / name).exists():
            parser.error('Required interface does not exist')
    for command in ('iw', 'wpa_supplicant', 'ip', 'systemctl'):
        if not shutil.which(command):
            parser.error('Missing required executable: ' + command)
    if args.profile.stat().st_mode & 0o077:
        parser.error('Private Wi-Fi profile must not be group/world accessible')
    profile = args.profile.read_text()
    network = Path('/etc/network/interfaces')
    mac = (Path('/sys/class/net') / args.bridge / 'address').read_text().strip()
    config = bridge_config(network.read_text(), args.bridge, mac)
    backup = Path('/root/wifi-cutover') / datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')
    backup.mkdir(parents=True, mode=0o700)
    shutil.copy2(network, backup / 'interfaces.before')
    source = Path(__file__).resolve().parent
    files = {
        '/usr/local/sbin/ricardo-uplink': ((source / 'uplink.py').read_text(), 0o755),
        '/etc/systemd/system/ricardo-uplink.service': ((source / 'ricardo-uplink.service').read_text(), 0o644),
        '/etc/default/ricardo-uplink': (f'BRIDGE={args.bridge}\nWIRED={args.wired}\nWIFI={args.wifi}\nMODE=auto\n', 0o644),
        f'/etc/wpa_supplicant/wpa_supplicant-{args.wifi}.conf': (profile, 0o600),
        f'/etc/systemd/system/wpa_supplicant@{args.wifi}.service.d/bridge.conf': (
            '[Service]\nExecStart=\nExecStart=/usr/sbin/wpa_supplicant '
            f'-c/etc/wpa_supplicant/wpa_supplicant-{args.wifi}.conf -i{args.wifi} -b{args.bridge}\n', 0o644),
    }
    for name, (body, mode) in files.items():
        path = Path(name)
        if path.exists():
            saved = backup / path.relative_to('/')
            saved.parent.mkdir(parents=True, mode=0o700, exist_ok=True)
            shutil.copy2(path, saved)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(body)
        path.chmod(mode)
    network.write_text(config)
    subprocess.run(['ip', 'link', 'set', args.bridge, 'type', 'bridge', 'mcast_snooping', '0'], check=True)
    subprocess.run(['systemctl', 'daemon-reload'], check=True)
    subprocess.run(['systemctl', 'enable', 'ricardo-uplink'], check=True)
    subprocess.run(['systemctl', 'restart', 'ricardo-uplink'], check=True)
    print('Installed; private previous configuration:', backup)


if __name__ == '__main__':
    main()
