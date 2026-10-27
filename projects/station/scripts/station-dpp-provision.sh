#!/bin/sh
# FORTRESS-DPP-PROVISION v1
# Keep a stable DPP bootstrap from /data/quasar/dpp-bootstrap.hex.
# Start DPP listen only when there is no default route (i.e. no known network).
# QR is generated once from the same DER key and stays valid across reboots.

KEY_FILE=/data/quasar/dpp-bootstrap.hex
WPA=/usr/sbin/wpa_cli
MAC=acbac0797774
CHAN=81/1
FREQ=2412

[ -s "$KEY_FILE" ] || exit 0
KEY=$(cat "$KEY_FILE" 2>/dev/null)
[ -n "$KEY" ] || exit 0

bootstrap() {
  "$WPA" -i wlan0 set dpp_config_processing 2 >/dev/null 2>&1
  "$WPA" -i wlan0 dpp_bootstrap_remove '*' >/dev/null 2>&1
  "$WPA" -i wlan0 dpp_bootstrap_gen type=qrcode mac="$MAC" chan="$CHAN" key="$KEY" >/dev/null 2>&1
}

bootstrap

while :; do
  if [ -f /data/quasar/dpp-force-listen ]; then
    # Manual test/recovery: listen even while a network is connected.
    "$WPA" -i wlan0 dpp_listen "$FREQ" >/dev/null 2>&1
  elif ip route show default 2>/dev/null | grep -q .; then
    "$WPA" -i wlan0 dpp_stop_listen >/dev/null 2>&1
  else
    "$WPA" -i wlan0 dpp_listen "$FREQ" >/dev/null 2>&1
  fi
  sleep 3
done
