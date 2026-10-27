#!/bin/sh
DIR=/data/quasar/ble-gateway
PID=/var/run/ble-gateway.pid
if [ -f "$PID" ]; then
    kill "$(cat "$PID")" 2>/dev/null || true
    rm -f "$PID"
fi
# Kill all gateway processes by exact comm name (never matches this shell).
for p in $(pidof ble-gateway 2>/dev/null); do
    kill "$p" 2>/dev/null || true
done
sleep 1
for p in $(pidof ble-gateway 2>/dev/null); do
    kill -9 "$p" 2>/dev/null || true
done
exit 0
