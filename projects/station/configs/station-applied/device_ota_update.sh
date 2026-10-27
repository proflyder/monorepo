#!/bin/sh
# FORTRESS-NO-OTA: прошивка по OTA запрещена (защита рута от сноса).
rm -f "$1" 2>/dev/null
echo "`date`: FORTRESS: OTA update blocked ($1)" >> /data/quasar/quasar_adb.log
exit 0
