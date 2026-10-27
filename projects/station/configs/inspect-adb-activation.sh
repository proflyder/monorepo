#!/usr/bin/env bash
# Collect read-only evidence showing how network ADB is activated.
set -euo pipefail

ADB="${ADB:-${HOME}/Library/Android/sdk/platform-tools/adb}"
SERIAL="${SERIAL:-192.0.2.13:5555}"
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${OUT:-${ROOT}/diagnostics/adb-activation}"

mkdir -p "${OUT}/binaries"

capture() {
  local name="$1"
  shift
  echo "== ${name} =="
  "${ADB}" -s "${SERIAL}" shell "$@" 2>&1 | tr -d '\r' | tee "${OUT}/${name}.txt"
}

"${ADB}" connect "${SERIAL}" >/dev/null
"${ADB}" -s "${SERIAL}" shell id

capture process-state '
  echo "--- processes ---"
  ps
  echo "--- adbd pid/status/cmdline ---"
  p="$(pidof adbd 2>/dev/null || true)"
  echo "pid=$p"
  if [ -n "$p" ]; then
    grep -E "^(Name|Pid|PPid|Uid|Gid):" "/proc/$p/status" 2>/dev/null
    readlink "/proc/$p/exe" 2>/dev/null
    tr "\0" " " < "/proc/$p/cmdline" 2>/dev/null
    echo
    tr "\0" "\n" < "/proc/$p/environ" 2>/dev/null | sort
  fi
  echo "--- listeners ---"
  netstat -lntp 2>/dev/null | grep -E "(^|[.:])5555[[:space:]]|adbd" || true
'

capture boot-config '
  echo "--- /etc/default/yandex-debugctl ---"
  cat /etc/default/yandex-debugctl 2>/dev/null || true
  echo "--- S95yandex-debugctl ---"
  cat /etc/init.d/S95yandex-debugctl 2>/dev/null || true
  echo "--- quasar onboot ---"
  cat /system/vendor/quasar/onboot.sh 2>/dev/null || true
'

capture debugctl '
  echo "--- executable ---"
  ls -l /usr/bin/yandex-debugctl 2>/dev/null || true
  echo "--- help ---"
  yandex-debugctl --help 2>&1 || true
  echo "--- status ---"
  yandex-debugctl status 2>&1 || true
  echo "--- no-argument output ---"
  yandex-debugctl 2>&1 || true
'

capture field-search '
  echo "--- exact configuration field strings ---"
  grep -R -a -l -E "remoteActivation|adbd[.]network|system_config" \
    /system/vendor/quasar /usr/bin /usr/lib 2>/dev/null || true
  echo "--- ADB/debugctl references in Quasar files ---"
  grep -R -a -n -E "yandex-debugctl|adb-network|adbd|remoteActivation" \
    /system/vendor/quasar 2>/dev/null | head -n 300 || true
'

capture logs '
  echo "--- logread ---"
  logread 2>/dev/null | grep -i -E "adbd|debugctl|remote.?activation|5555" | tail -n 300 || true
  echo "--- persistent text logs ---"
  grep -R -i -n -E "adbd|debugctl|remote.?activation|5555" \
    /var/log /data 2>/dev/null | tail -n 300 || true
'

"${ADB}" -s "${SERIAL}" pull /usr/bin/yandex-debugctl \
  "${OUT}/binaries/yandex-debugctl" >/dev/null 2>&1 || true
"${ADB}" -s "${SERIAL}" pull /system/vendor/quasar/onboot.sh \
  "${OUT}/onboot.sh" >/dev/null 2>&1 || true

if [[ -f "${OUT}/binaries/yandex-debugctl" ]]; then
  strings -a "${OUT}/binaries/yandex-debugctl" \
    | grep -i -E "adbd|network|remote|5555|start|stop|usb" \
    > "${OUT}/yandex-debugctl-strings.txt" || true
fi

echo "Collected read-only diagnostics in: ${OUT}"
