#!/usr/bin/env bash
# Local control for the alice-mitm test setup (proxy + DNS).
#
#   ./scripts/proxy-ctl.sh start|stop|restart|status
#   ./scripts/proxy-ctl.sh stage1   # bootstrap (public CA chain, quasar only)
#   ./scripts/proxy-ctl.sh stage2   # local CA + uniproxy (after ADB + CA install)
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
BUN="${BUN:-${ROOT}/.tools/bun-darwin-aarch64/bun}"
LOGS="${ROOT}/logs"
PROXY_PID="${LOGS}/proxy.pid"
DNS_PID="${LOGS}/dns.pid"

alive() {
  local file="$1" pid
  [[ -f "${file}" ]] || return 1
  pid="$(cat "${file}" 2>/dev/null || true)"
  [[ -n "${pid}" ]] && kill -0 "${pid}" 2>/dev/null
}

spawn() {
  local name="$1" file="$2" script="$3" pid
  cd "${ROOT}"
  nohup "${BUN}" run "${script}" >> "${LOGS}/${name}.log" 2>&1 &
  pid=$!
  echo "${pid}" > "${file}"
  sleep 1
  if ! kill -0 "${pid}" 2>/dev/null; then
    echo "${name}: failed to start; see logs/${name}.log" >&2
    tail -20 "${LOGS}/${name}.log" >&2 || true
    return 1
  fi
  echo "${name}: started (pid ${pid})"
}

kill_pid() {
  local name="$1" file="$2" pid
  if ! alive "${file}"; then
    echo "${name}: not running"
    rm -f "${file}"
    return 0
  fi
  pid="$(cat "${file}")"
  kill "${pid}" 2>/dev/null || true
  for _ in $(seq 1 25); do
    kill -0 "${pid}" 2>/dev/null || break
    sleep 0.2
  done
  kill -9 "${pid}" 2>/dev/null || true
  rm -f "${file}"
  echo "${name}: stopped"
}

start() {
  if ! alive "${DNS_PID}"; then
    spawn dns "${DNS_PID}" src/dns.ts || return 1
  else
    echo "dns: already running (pid $(cat "${DNS_PID}"))"
  fi
  if ! alive "${PROXY_PID}"; then
    spawn proxy "${PROXY_PID}" src/index.ts || return 1
  else
    echo "proxy: already running (pid $(cat "${PROXY_PID}"))"
  fi
}

stop() {
  kill_pid proxy "${PROXY_PID}"
  kill_pid dns "${DNS_PID}"
}

dns_start() {
  if ! alive "${DNS_PID}"; then
    spawn dns "${DNS_PID}" src/dns.ts || return 1
  else
    echo "dns: already running (pid $(cat "${DNS_PID}"))"
  fi
}

dns_stop() {
  kill_pid dns "${DNS_PID}"
}

proxy_start() {
  if ! alive "${PROXY_PID}"; then
    spawn proxy "${PROXY_PID}" src/index.ts || return 1
  else
    echo "proxy: already running (pid $(cat "${PROXY_PID}"))"
  fi
}

proxy_stop() {
  kill_pid proxy "${PROXY_PID}"
}

status() {
  local ip
  ip="$(ipconfig getifaddr en0 2>/dev/null || true)"
  echo "lan ip: ${ip:-unknown}"
  if alive "${PROXY_PID}"; then
    echo "proxy:  running (pid $(cat "${PROXY_PID}"))"
  else
    echo "proxy:  not running"
  fi
  if alive "${DNS_PID}"; then
    echo "dns:    running (pid $(cat "${DNS_PID}"))"
  else
    echo "dns:    not running"
  fi
  curl -s --max-time 2 http://127.0.0.1/health || true
  echo
}

switch_stage() {
  local stage="$1" file="${ROOT}/.env.${1}"
  [[ -f "${file}" ]] || { echo "missing ${file}" >&2; exit 1; }
  cp "${file}" "${ROOT}/.env"
  echo "env -> ${file}"
  stop
  start
}

case "${1:-status}" in
  start) start ;;
  stop) stop ;;
  restart) stop; start ;;
  status) status ;;
  dns-start) dns_start ;;
  dns-stop) dns_stop ;;
  proxy-start) proxy_start ;;
  proxy-stop) proxy_stop ;;
  stage1) switch_stage stage1 ;;
  stage2) switch_stage stage2 ;;
  *)
    echo "usage: $0 start|stop|restart|status|dns-start|dns-stop|proxy-start|proxy-stop|stage1|stage2" >&2
    exit 2
    ;;
esac
