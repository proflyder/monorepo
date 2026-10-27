#!/bin/sh
# Run as root: sudo ./verify.sh
set -u

echo "=== host ==="
hostname
. /etc/os-release
echo "$PRETTY_NAME"
uname -r
echo "time: $(date '+%F %T %Z')"

echo
echo "=== addresses ==="
ip -br addr
echo
echo "=== routes ==="
ip route

echo
echo "=== secure boot / MOK ==="
mokutil --sb-state 2>&1 || true
mokutil --test-key /var/lib/shim-signed/mok/MOK.der 2>&1 || true

echo
echo "=== dkms / module ==="
dkms status 2>&1 || true
lsmod | grep -E '8821|rtw88' || true

echo
echo "=== wifi ==="
wpa_cli -i wlp4s0 status 2>&1 | grep -E 'wpa_state|ssid|freq|ip_address|bssid' || true
echo "h2c=$(dmesg | grep -c 'failed to send h2c' || true)"
echo "ubsan=$(dmesg | grep -c UBSAN || true)"

echo
echo "=== wait-online ==="
systemctl is-active systemd-networkd-wait-online.service 2>&1 || true
systemctl is-system-running 2>&1 || true
systemctl --failed --no-legend 2>&1 | head -10

echo
echo "=== HA / MA / STT / TTS ==="
docker ps --format '{{.Names}} {{.Image}} {{.Status}}' 2>&1 || true
curl -sSL -o /dev/null -w 'http_8123=%{http_code}\n' http://127.0.0.1:8123/ 2>&1 || true
curl -sSL -o /dev/null -w 'http_8095=%{http_code}\n' http://127.0.0.1:8095/ 2>&1 || true
docker inspect -f '{{.Name}} {{.State.Health.Status}}' wyoming-whisper wyoming-piper 2>&1 || true
ss -lntp 2>/dev/null | grep -E ':(8123|8095|8097|8927|10200|10300)\b' || true

echo
echo "=== disk ==="
df -h /
