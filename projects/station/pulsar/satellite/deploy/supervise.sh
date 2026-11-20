#!/bin/sh
D=${PULSAR_DIR:-/data/pulsar}
LOG=${PULSAR_LOG:-/tmp/pulsar.log}
Q=${PULSAR_Q:-/system/vendor/quasar/q}
BIN=$D/pulsar
MAXLOG=1000000
LONG=${PULSAR_LONG:-300}
CRASHES=5
MINUP=5
BACKOFF_MIN=3
BACKOFF_MAX=30

uptime_s() { read -r u _ < /proc/uptime; echo "${u%%.*}"; }

rotate() {
  if [ -f "$LOG" ] && [ "$(wc -c < "$LOG")" -gt $MAXLOG ]; then
    cp "$LOG" "$LOG.1" && : > "$LOG"
  fi
}

say() { echo "=== supervise: $* $(date)" >> "$LOG"; }

give_back() {
  rm -f "$D/enabled"
  say "$1; disabled, starting the Yandex stack"
  pidof quasar_launcher2 >/dev/null 2>&1 || "$Q" start >/dev/null 2>&1
  exit 1
}

clear_dmix() {
  segs=$(ipcs -m 2>/dev/null) || return 0
  removed= busy=0
  for k in 400 401 8e5 8e6; do
    n=$(echo "$segs" | awk -v k="0x00000$k" '$1 == k { print $6; exit }')
    if [ "$n" = 0 ]; then
      ipcrm -M "0x$k" && removed="$removed shm 0x$k"
    elif [ -n "$n" ]; then
      busy=1
    fi
  done
  for k in 400 8e5; do
    if [ $busy = 0 ] && ipcs -s 2>/dev/null | awk -v k="0x00000$k" '$1 == k { f = 1 } END { exit !f }'; then
      ipcrm -S "0x$k" && removed="$removed sem 0x$k"
    fi
  done
  [ -z "$removed" ] || echo "=== alsa: removed stale dmix lock (${removed# }) $(date)" >> "$LOG"
}

( while sleep 60; do rotate; done ) &
ROT=$!
trap 'kill $ROT 2>/dev/null; exit 143' TERM INT HUP
trap 'kill $ROT 2>/dev/null' EXIT

fails=0 swapped=0 backoff=$BACKOFF_MIN
while [ -f "$D/enabled" ]; do
  rotate
  if [ ! -x "$BIN" ]; then
    say "$BIN missing or not executable"
    if [ "$swapped" = 0 ] && [ -x "$BIN.prev" ]; then
      cp -p "$BIN.prev" "$BIN.tmp" && mv "$BIN.tmp" "$BIN" && swapped=1 && fails=0 && say "restored .prev" && continue
    fi
    give_back "no usable binary"
  fi
  clear_dmix
  echo "=== start $(date)" >> "$LOG"
  t0=$(uptime_s)
  "$BIN" "$D/state.conf" >> "$LOG" 2>&1
  rc=$?
  up=$(($(uptime_s) - t0))
  echo "=== exit $rc after ${up}s $(date)" >> "$LOG"

  if [ $up -ge $LONG ]; then
    fails=0 swapped=0 backoff=$BACKOFF_MIN
    [ $rc -eq 0 ] && sleep 1 || sleep $backoff
    continue
  fi
  if [ $rc -eq 0 ] && [ $up -ge $MINUP ]; then
    sleep 1
    continue
  fi
  fails=$((fails + 1))
  if [ $fails -ge $CRASHES ]; then
    if [ "$swapped" = 0 ] && [ -f "$BIN.prev" ] && ! cmp -s "$BIN" "$BIN.prev"; then
      mv -f "$BIN" "$BIN.bad"
      if cp -p "$BIN.prev" "$BIN.tmp" && mv "$BIN.tmp" "$BIN"; then
        swapped=1 fails=0 backoff=$BACKOFF_MIN
        note=; [ -f "$D/api_key" ] && note="; api_key is set: the previous binary may not support API encryption, so Home Assistant (which holds the key) may be unable to connect - remove the key in HA or re-add the device"
        say "$CRASHES short crashes in a row, rolled back to .prev (bad binary kept as .bad)$note"
        continue
      fi
      mv -f "$BIN.bad" "$BIN"
    fi
    if [ "$swapped" = 1 ]; then give_back "$CRASHES short crashes in a row, also with the rolled-back .prev"
    else give_back "$CRASHES short crashes in a row, no different previous binary to fall back to"; fi
  fi
  sleep $backoff
  backoff=$((backoff * 2))
  [ $backoff -gt $BACKOFF_MAX ] && backoff=$BACKOFF_MAX
done
