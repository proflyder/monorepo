#!/bin/sh
# FORTRESS-KEEPER v5: markers + network adbd; start adbd explicitly because
# yandex-debugctl says "already configured" and does not spawn it again.
M=/data/quasar/activate_adb_quasmodrom
N=/data/quasar/activate_network_adb
listening() { awk '$2 ~ /:15B3$/ && $4=="0A" {f=1} END{exit !f}' /proc/net/tcp 2>/dev/null; }
while :; do
  if ! ps | grep -q '[b]le-gateway/ble-gateway'; then
    [ -x /data/quasar/ble-gateway/start.sh ] && /data/quasar/ble-gateway/start.sh
  fi
  /data/quasar/yandex-block.sh >/dev/null 2>&1
  /data/quasar/egress-guard.sh >/dev/null 2>&1
  [ -f "$M" ] || touch "$M"
  [ -f "$N" ] || touch "$N"
  if ! listening; then
    /system/vendor/quasar/activate_adb.sh --quasmodrom activate --network >/dev/null 2>&1
    start-stop-daemon -S -b -x /usr/bin/adbd 2>/dev/null
  fi
  sleep 60
done
