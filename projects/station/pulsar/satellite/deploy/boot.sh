#!/bin/sh
D=/data/pulsar
[ -f "$D/enabled" ] || exit 0
[ -x "$D/pulsar" ] && [ -x "$D/supervise.sh" ] || exit 0
mkdir /tmp/pulsar-boot.lock 2>/dev/null || exit 0
pidof pulsar >/dev/null 2>&1 && exit 0
pidof supervise.sh >/dev/null 2>&1 && exit 0
if ! tr '\0' '\n' </proc/device-tree/compatible | grep -qxE 'yandex,(cucumber|plum|station-mini2|bergamot)'; then
  [ "$(tr -d '\0' </proc/device-tree/amlogic-dt-id 2>/dev/null)" = axg_s420_v03 ] &&
    grep -qE '"deviceType"[[:space:]]*:[[:space:]]*"yandexmini"' /system/vendor/quasar/quasar.cfg 2>/dev/null ||
    exit 0
fi

i=0
while [ $i -lt 30 ] && ! pidof quasar_launcher2 >/dev/null 2>&1; do
  sleep 1
  i=$((i + 1))
done
/system/vendor/quasar/q stop >/dev/null 2>&1

exec setsid "$D/supervise.sh" </dev/null >/dev/null 2>&1
