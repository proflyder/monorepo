#!/bin/sh
# Ephemeral /tmp-only session. Does not change enabled, state.conf, service scripts or binary.
set -eu
BASE=/tmp/ricardo-voice-recorder
ID=$1
case "$ID" in ''|*[!a-f0-9]*) exit 2;; esac
[ ${#ID} -eq 32 ] || exit 2
S=$BASE/$ID
mkdir /tmp/ricardo-voice-recorder.lock 2>/dev/null || exit 3
umask 077
mkdir "$S"
echo "$ID" > /tmp/ricardo-voice-recorder.lock/owner
SUP=$(pidof supervise.sh || true)
DAEMON=$(pidof pulsar || true)
WATCH=
HELPER=
daemon_alive() {
  # A paused parent cannot reap its terminated child. Z has already released
  # ALSA/file descriptors and is safe to leave until we resume that parent.
  [ -r /proc/$DAEMON/stat ] || return 1
  read -r _pid _name state _rest < /proc/$DAEMON/stat || return 1
  [ "$state" != Z ]
}
resume() {
  for p in $SUP; do
    case "$(tr '\000' ' ' < /proc/$p/cmdline 2>/dev/null || true)" in
      *'/data/pulsar/supervise.sh'*) kill -CONT "$p" 2>/dev/null || true;;
    esac
  done
}
stop_helper() {
  if [ -s "$S/helper.pid" ]; then
    p=$(cat "$S/helper.pid")
    if [ "$(readlink /proc/$p/exe 2>/dev/null || true)" = "$BASE/ricardo-voicestream" ]; then
      kill -KILL "$p" 2>/dev/null || true
    fi
  fi
}
cleanup() {
  trap '' HUP INT TERM
  stop_helper
  resume
  [ -z "$WATCH" ] || kill "$WATCH" 2>/dev/null || true
  echo restored > "$S/state"
  # Only remove our own lock; preserve all diagnostics until explicit cleanup.
  if [ "$(cat /tmp/ricardo-voice-recorder.lock/owner 2>/dev/null || true)" = "$ID" ]; then
    rm /tmp/ricardo-voice-recorder.lock/owner
    rmdir /tmp/ricardo-voice-recorder.lock
  fi
}
trap cleanup EXIT
trap 'exit 143' HUP INT TERM
[ -n "$SUP" ] && [ -n "$DAEMON" ] && [ -f /data/pulsar/enabled ] || exit 4
[ "$(readlink /proc/$DAEMON/exe 2>/dev/null)" = /data/pulsar/pulsar ] || exit 5
printf '%s\n' "$SUP" > "$S/supervisors"
echo 0 > "$S/heartbeat"
echo preparing > "$S/state"
# This independent watchdog survives a dead Mac, disconnected ADB or killed parent shell.
(
  trap '' HUP
  last=0
  stale=0
  count=0
  while [ "$count" -lt 300 ]; do
    sleep 2
    count=$((count + 1))
    now=$(cat "$S/heartbeat" 2>/dev/null || true)
    if [ "$now" = "$last" ]; then stale=$((stale + 1)); else stale=0; fi
    last=$now
    [ ! -f "$S/stop" ] && [ "$stale" -lt 6 ] || break
  done
  touch "$S/stop"
  stop_helper
  resume
  echo rescued > "$S/state"
) </dev/null >/dev/null 2>&1 &
WATCH=$!
for p in $SUP; do kill -STOP "$p"; done
kill -TERM "$DAEMON"
i=0
while daemon_alive && [ "$i" -lt 15 ]; do sleep 1; i=$((i + 1)); done
if daemon_alive; then exit 6; fi
[ ! -f "$S/stop" ] || exit 7
echo capturing > "$S/state"
"$BASE/ricardo-voicestream" 600 2>"$S/helper.log" &
HELPER=$!
echo "$HELPER" > "$S/helper.pid"
wait "$HELPER" || true
