#!/bin/sh
# Read-only, bounded network evidence. Logs may identify the LAN: keep private.
# Arm the rootkeeper hook separately for a controlled reboot; no radio resets.
ROOT=${DIAG_ROOT:-/data/quasar/network-diag}
PID=/var/run/station-network-diagnostic.pid
SELF=/data/quasar/station-network-diagnostic.sh
umask 077

bounded() {
  dest=$1
  shift
  "$@" > "$dest.tmp" 2>&1 &
  child=$!
  ( sleep 3; kill -TERM "$child" 2>/dev/null ) &
  guard=$!
  wait "$child"
  result=$?
  kill "$guard" 2>/dev/null || true
  wait "$guard" 2>/dev/null || true
  printf '\ncommand_exit=%s\n' "$result" >> "$dest.tmp"
  mv "$dest.tmp" "$dest"
}

case "${1:-start}" in
  start)
    if [ -f "$PID" ] && kill -0 "$(cat "$PID")" 2>/dev/null; then exit 0; fi
    nohup "$SELF" run </dev/null >/dev/null 2>&1 &
    echo $! > "$PID"
    ;;
  stop)
    [ ! -f "$PID" ] || kill "$(cat "$PID")" 2>/dev/null || true
    rm -f "$PID"
    ;;
  run)
    boot=$(cat /proc/sys/kernel/random/boot_id)
    DIR="$ROOT/$boot"
    mkdir -p "$DIR"
    chmod 700 "$ROOT" "$DIR"
    cp /proc/cmdline "$DIR/kernel-command-line.txt"
    cat /proc/modules > "$DIR/modules-at-start.txt"
    n=0
    while [ "$n" -lt "${DIAG_MAX_SAMPLES:-120}" ]; do
      bounded "$DIR/wpa.latest" /usr/sbin/wpa_cli -i wlan0 status
      {
        printf '\n--- sample=%s date=%s uptime=' "$n" "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
        cat /proc/uptime
        grep -E '^(wpa_state|freq|id|ip_address|command_exit)=' "$DIR/wpa.latest"
        ip addr show wlan0
        ip addr show p2p0
        [ ! -f /data/quasar/ble-gateway/status ] || cat /data/quasar/ble-gateway/status
        ip route
        ip neigh show dev wlan0
        ps | grep -E 'wpa_supplicant|dhcpcd|ble-gateway|dpp-provision|pulsar|rootkeeper' | grep -v grep
      } >> "$DIR/samples.log" 2>&1
      gateway=$(ip route | awk '$1 == "default" { print $3; exit }')
      if [ -n "$gateway" ]; then
        bounded "$DIR/gateway.latest" ping -c 1 -W 1 "$gateway"
        tail -n 4 "$DIR/gateway.latest" >> "$DIR/samples.log"
      fi
      dmesg | tail -n 1200 > "$DIR/kernel.latest"
      [ ! -f /tmp/messages ] || tail -n 1200 /tmp/messages > "$DIR/messages.latest"
      [ ! -f /tmp/messages.0 ] || tail -n 1200 /tmp/messages.0 > "$DIR/messages-previous.latest"
      n=$((n + 1))
      sleep "${DIAG_INTERVAL:-5}"
    done
    rm -f "$PID"
    ;;
  *) echo 'Usage: station-network-diagnostic.sh start|stop|run' >&2; exit 2 ;;
esac
