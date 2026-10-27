#!/usr/bin/env bash
# Installs the local CA into the Quasar firmware CA bundle on a station you own.
# Requires the ADB-over-network shell enabled by the fake sync config.
#
#   ./scripts/install-ca.sh certs/ca.crt
#
# Tested path: /system/vendor/quasar/certs/ca-certificates.crt
# The root filesystem is UBIFS and mounted read-only: remount rw, append the
# CA, sync, remount ro, then reboot.
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
env_station="$(sed -n 's/^STATION_IP=//p' "${ROOT}/.env" 2>/dev/null | tail -1)"
CA_PATH="${1:-${ROOT}/certs/ca.crt}"
ADB_TARGET="${ADB_TARGET:-${env_station:-192.0.2.13}:5555}"
BUNDLE="/system/vendor/quasar/certs/ca-certificates.crt"
TMP="/bkupgrade/data/alice-mitm-ca.crt"
BACKUP="${BUNDLE}.alice-mitm.bak"
REMOUNTED_RW=false

cleanup() {
  if [[ "${REMOUNTED_RW}" == true ]]; then
    adb shell 'mount -o remount,ro /' >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

if [[ ! -f "${CA_PATH}" ]]; then
  echo "missing CA: ${CA_PATH}" >&2
  exit 1
fi

echo "CA:     ${CA_PATH}"
echo "target: ${ADB_TARGET}"
echo "bundle: ${BUNDLE}"

adb connect "${ADB_TARGET}"
adb wait-for-device

bundle_status="$(adb shell "if test -f '${BUNDLE}'; then echo PRESENT; else echo ABSENT; fi" | tr -d '\r')"
if [[ "${bundle_status}" != "PRESENT" ]]; then
  echo "CA bundle not found: ${BUNDLE}" >&2
  echo "This script targets Quasar firmware; install the CA manually on other builds." >&2
  exit 1
fi

# A long base64 line from the CA is sufficient as an exact, stable marker in
# the concatenated PEM bundle and avoids appending the same CA repeatedly.
CA_MARKER="$(awk '/BEGIN CERTIFICATE/{getline; print; exit}' "${CA_PATH}")"
ca_status="$(adb shell "if grep -Fq '${CA_MARKER}' '${BUNDLE}'; then echo PRESENT; else echo ABSENT; fi" | tr -d '\r')"
if [[ "${ca_status}" == "PRESENT" ]]; then
  echo "CA is already present in ${BUNDLE}; nothing to do."
  exit 0
fi

adb shell 'mount -o remount,rw /'
REMOUNTED_RW=true
adb shell "test -f '${BACKUP}' || cp '${BUNDLE}' '${BACKUP}'"
adb push "${CA_PATH}" "${TMP}"
adb shell "cat '${TMP}' >> '${BUNDLE}' && sync"

ca_status="$(adb shell "if grep -Fq '${CA_MARKER}' '${BUNDLE}'; then echo PRESENT; else echo ABSENT; fi" | tr -d '\r')"
if [[ "${ca_status}" != "PRESENT" ]]; then
  echo "CA verification failed after append; restoring the original bundle." >&2
  adb shell "cp '${BACKUP}' '${BUNDLE}' && sync"
  exit 1
fi

adb shell 'mount -o remount,ro /'
REMOUNTED_RW=false
adb reboot

echo "CA appended and reboot sent. After boot, verify with:"
echo "  adb shell \"grep -c 'BEGIN CERTIFICATE' '${BUNDLE}'\""
echo "Backup: ${BACKUP}"
