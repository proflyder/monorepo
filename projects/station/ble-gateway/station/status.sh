#!/bin/sh
DIR=/data/quasar/ble-gateway
PID=/var/run/ble-gateway.pid
echo "== process =="
if [ -f "$PID" ]; then
    echo "pid $(cat "$PID")"
    ps | grep "$(cat "$PID")" | grep -v grep || true
else
    pidof ble-gateway 2>/dev/null || echo "not running"
fi
echo "== adapter =="
hciconfig hci0 2>/dev/null | sed -n '1,4p'
echo "== last log =="
tail -20 "$DIR/ble-gateway.log" 2>/dev/null || true
echo "== status =="
cat "$DIR/status" 2>/dev/null || true
