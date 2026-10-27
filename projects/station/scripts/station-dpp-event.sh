#!/bin/sh
# FORTRESS-DPP-EVENT v1
# wpa_cli -a action script: log DPP/CTRL events without credentials.
LOG=/data/quasar/dpp-events.log
printf '%s %s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$*" >> "$LOG"
