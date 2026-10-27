#!/bin/sh
# FORTRESS-DPP-DEBUG v1
# Snapshot wpa/driver state for a DPP attempt.
OUT=/data/quasar/dpp-debug.log
{
  date
  echo "=== force ==="
  ls -l /data/quasar/dpp-force-listen 2>/dev/null || echo none
  echo "=== status ==="
  wpa_cli -i wlan0 status 2>&1
  echo "=== bootstrap ==="
  wpa_cli -i wlan0 dpp_bootstrap_info 1 2>&1
  echo "=== iw ==="
  iw dev wlan0 info 2>&1
  echo "=== dmesg tail ==="
  dmesg 2>/dev/null | tail -80
} > "$OUT" 2>&1
chmod 600 "$OUT"
