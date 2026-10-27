#!/usr/bin/env bash
# Restore the original OTA/ADB files and remove fortress (revert the on-device
# protection). Reboots the station. Use with --yes in scripts.
set -euo pipefail

IP="${STATION_IP:-192.0.2.13}"
ADB="${ADB:-adb}"
S="$IP:5555"
Q=/system/vendor/quasar

if [[ "${1:-}" != "--yes" ]]; then
  read -r -p "Вернуть штатные OTA/ADB-файлы и перезагрузить $IP? [y/N] " answer
  [[ "$answer" == "y" || "$answer" == "Y" ]] || { echo "отменено"; exit 0; }
fi

"$ADB" connect "$S" >/dev/null 2>&1 || true
"$ADB" -s "$S" shell id 2>/dev/null | grep -q uid=0 || {
  echo "нет ADB-рута на $S" >&2
  exit 1
}

"$ADB" -s "$S" shell "mount -o remount,rw /"
"$ADB" -s "$S" shell "
  cd $Q
  for f in activate_adb.sh device_ota_update.sh ota_script.pub launch.json onboot.sh; do
    if [ -f \"\$f.orig\" ]; then cp -f \"\$f.orig\" \"\$f\"; echo \"restored \$f\"; else echo \"no \$f.orig\"; fi
  done
  chmod 755 activate_adb.sh device_ota_update.sh onboot.sh
  pkill -f rootkeeper.sh 2>/dev/null || true
  rm -f /data/quasar/rootkeeper.sh
  sync
  mount -o remount,ro /
"
"$ADB" -s "$S" shell reboot
echo "restored + reboot sent. After boot ADB may be off; use SSH or the fake-sync to re-enable."
