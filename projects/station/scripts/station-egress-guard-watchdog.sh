#!/bin/sh
# FORTRESS-EGRESS-GUARD-WATCHDOG v1
# Safety net for first deployment: if not confirmed within 180s, remove guard.
sleep 180
if [ ! -f /data/quasar/egress-guard.confirmed ]; then
  /data/quasar/egress-guard-off.sh
fi
exit 0
