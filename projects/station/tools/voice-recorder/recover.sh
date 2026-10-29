#!/bin/sh
# Explicit recovery for this temporary tool only. Never edits service configuration.
set -eu
BASE=/tmp/ricardo-voice-recorder
LOCK=/tmp/ricardo-voice-recorder.lock
if [ ! -d "$LOCK" ]; then
  pidof pulsar >/dev/null && echo 'Pulsar is running; no recording lock.'
  exit
fi
ID=$(cat "$LOCK/owner")
case "$ID" in ''|*[!a-f0-9]*) exit 2;; esac
[ ${#ID} -eq 32 ] || exit 2
S=$BASE/$ID
touch "$S/stop"
sleep 3
if [ -s "$S/helper.pid" ]; then
  p=$(cat "$S/helper.pid")
  if [ "$(readlink /proc/$p/exe 2>/dev/null || true)" = "$BASE/ricardo-voicestream" ]; then
    kill -KILL "$p" 2>/dev/null || true
  fi
fi
for p in $(cat "$S/supervisors"); do
  case "$(tr '\000' ' ' < /proc/$p/cmdline 2>/dev/null || true)" in
    *'/data/pulsar/supervise.sh'*) kill -CONT "$p" 2>/dev/null || true;;
  esac
done
i=0
while ! pidof pulsar >/dev/null && [ "$i" -lt 15 ]; do sleep 1; i=$((i + 1)); done
pidof pulsar >/dev/null || { echo 'Original service did not restart; inspect ctl.sh status.' >&2; exit 3; }
if [ "$(cat "$LOCK/owner" 2>/dev/null || true)" = "$ID" ]; then
  rm "$LOCK/owner"
  rmdir "$LOCK"
fi
echo 'Pulsar restored. No settings changed.'
