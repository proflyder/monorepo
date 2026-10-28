#!/usr/bin/env bash
# One-shot status of the station + lab.
set -u

IP="${STATION_IP:-192.0.2.13}"
ADB="${ADB:-adb}"
S="$IP:5555"
MITM="$HOME/Desktop/alice-mitm-bootstrap"
STACK="$HOME/Desktop/v3rt3p-stack"

echo "== date =="; date
echo "== mac ip =="; ipconfig getifaddr en0 2>/dev/null || true
echo "== listeners =="; lsof -nP -iUDP:53 -iTCP:80 -iTCP:443 -iTCP:31116 -sTCP:LISTEN 2>/dev/null | grep -v '^COMMAND' | head -6
echo "== dns/proxy =="; (cd "$MITM" && ./scripts/proxy-ctl.sh status 2>&1)
echo "== stack =="; (cd "$STACK" && docker compose ps --format 'table {{.Service}}\t{{.Status}}' 2>&1)
echo "== station ports =="
nc -z -G 2 "$IP" 22 2>/dev/null && echo "22 open" || echo "22 closed"
nc -z -G 2 "$IP" 5555 2>/dev/null && echo "5555 open" || echo "5555 closed"

"$ADB" connect "$S" >/dev/null 2>&1 || true
if "$ADB" -s "$S" shell id 2>/dev/null | grep -q uid=0; then
  "$ADB" -s "$S" shell '
    echo "== station ==";
    echo -n "version: "; cat /system/vendor/quasar/version_full; echo;
    echo -n "uptime: "; uptime;
    echo -n "adbd: "; pidof adbd;
    echo -n "keeper: "; ps | grep rootkeeper | grep -v grep | awk "{print \$1}";
    echo -n "dropbear: "; pidof dropbear;
    echo -n "updatesd: "; ps | grep updatesd | grep -v grep || echo none;
    echo "markers:"; ls -la /data/quasar/ 2>/dev/null | grep -i activate;
    echo -n "ota key: "; cmp -s /system/vendor/quasar/ota_script.pub /system/vendor/quasar/ota_script.pub.orig && echo ORIGINAL || echo REPLACED;
    echo -n "ota applier: "; head -2 /system/vendor/quasar/device_ota_update.sh | grep -q FORTRESS && echo BLOCKED || echo ORIGINAL;
    echo -n "adb wrapper: "; head -3 /system/vendor/quasar/activate_adb.sh | grep -q FORTRESS && echo FORTRESS || echo ORIGINAL;
    echo -n "launch updatesd: "; grep -c updatesd /system/vendor/quasar/launch.json;
    echo -n "egress guard: "; iptables -C OUTPUT -j EGRESS_GUARD 2>/dev/null && echo ON || echo OFF;
    echo -n "egress drop pkts: "; iptables -L EGRESS_GUARD -v -n 2>/dev/null | awk "/DROP/ {print \$1}";
    echo -n "egress ntp pkts: "; iptables -L EGRESS_GUARD -v -n 2>/dev/null | awk "/216.239.35.0/ {print \$1}";
    echo -n "root mount: "; mount | grep " / "
  '
else
  echo "ADB root: no"
fi
