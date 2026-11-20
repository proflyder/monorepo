#!/bin/sh
sleep 180
if [ ! -f /data/quasar/egress-guard.confirmed ]; then
  /data/quasar/egress-guard-off.sh
fi
exit 0
