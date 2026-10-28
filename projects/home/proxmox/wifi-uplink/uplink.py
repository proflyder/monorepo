#!/usr/bin/env python3
"""One L2 uplink at a time: Ethernet carrier preferred, tested WDS Wi-Fi fallback."""
import logging
import os
from pathlib import Path
import signal
import subprocess
import time

LOG = logging.getLogger('ricardo-uplink')


def choose(mode, wired_carrier):
    if mode not in ('auto', 'wifi', 'ethernet'):
        raise ValueError('Mode must be auto, wifi or ethernet')
    return 'ethernet' if mode == 'ethernet' or (mode == 'auto' and wired_carrier) else 'wifi'


def transitions(target, wired, wifi, members):
    """Detach the other physical uplink before attaching the requested one."""
    selected, other = (wired, wifi) if target == 'ethernet' else (wifi, wired)
    return ([('detach', other)] if other in members else []) + (
        [('attach', selected)] if selected not in members else [])


def run(*args):
    subprocess.run(args, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=20)


def active(unit):
    return subprocess.run(['systemctl', 'is-active', '--quiet', unit], check=False).returncode == 0


def carrier(interface):
    try:
        return (Path('/sys/class/net') / interface / 'carrier').read_text().strip() == '1'
    except OSError:
        return False


def tick(bridge, wired, wifi, mode):
    # A detached manual Ethernet interface starts DOWN after boot. Keep it up
    # so carrier detection can notice a cable being plugged in later.
    run('ip', 'link', 'set', wired, 'up')
    target = choose(mode, carrier(wired))
    members = {p.name for p in (Path('/sys/class/net') / bridge / 'brif').iterdir()}
    unit = f'wpa_supplicant@{wifi}.service'
    for action, interface in transitions(target, wired, wifi, members):
        if action == 'detach':
            run('ip', 'link', 'set', interface, 'nomaster')
        else:
            if interface == wifi:
                run('iw', 'dev', wifi, 'set', '4addr', 'on')
            run('ip', 'link', 'set', interface, 'up')
            run('ip', 'link', 'set', interface, 'master', bridge)
    if target == 'wifi':
        run('iw', 'dev', wifi, 'set', 'power_save', 'off')
        if not active(unit):
            run('systemctl', 'start', unit)
    elif active(unit):
        run('systemctl', 'stop', unit)
    return target


def main():
    logging.basicConfig(level=logging.INFO, format='%(message)s')
    bridge = os.environ.get('BRIDGE', 'vmbr0')
    wired = os.environ.get('WIRED', 'nic0')
    wifi = os.environ.get('WIFI', 'wlp4s0')
    default_mode = os.environ.get('MODE', 'auto')
    override = Path('/run/ricardo-uplink.mode')
    stopping = False

    def stop(*_):
        nonlocal stopping
        stopping = True
    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    previous = None
    while not stopping:
        try:
            mode = override.read_text().strip() if override.exists() else default_mode
            state = tick(bridge, wired, wifi, mode)
            if state != previous:
                LOG.info('Active bridge uplink: %s (policy: %s)', state, mode)
                previous = state
        except (OSError, ValueError, subprocess.SubprocessError) as exc:
            # Do not print command outputs: wireless diagnostics may contain SSIDs.
            LOG.warning('Uplink reconciliation failed: %s; retrying', type(exc).__name__)
        time.sleep(2)


if __name__ == '__main__':
    main()
