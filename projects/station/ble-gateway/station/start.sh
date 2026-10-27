#!/bin/sh
# Start the local BLE Wi-Fi gateway. Safe no-op if already running.
DIR=/data/quasar/ble-gateway
BIN="$DIR/ble-gateway"
LOG="$DIR/ble-gateway.log"
PID=/var/run/ble-gateway.pid
OUT="$DIR/ble-gateway.out"

[ -x "$BIN" ] || exit 1
mkdir -p "$DIR"
chmod 700 "$DIR" 2>/dev/null || true

if [ -f "$PID" ] && kill -0 "$(cat "$PID")" 2>/dev/null; then
    exit 0
fi
if pidof ble-gateway >/dev/null 2>&1; then
    exit 0
fi

# keep one log backup, max ~1 MiB
if [ -f "$LOG" ] && [ "$(wc -c < "$LOG" 2>/dev/null || echo 0)" -gt 1048576 ]; then
    mv -f "$LOG" "$LOG.1"
fi

nohup "$BIN" --auto=true --log "$LOG" >>"$OUT" 2>&1 &
echo $! > "$PID"
exit 0
