#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

COMPOSE=(docker compose)
TAIL="${TAIL:-200}"

die() { echo "ERROR: $*" >&2; exit 1; }

need() {
  command -v "$1" >/dev/null 2>&1 || die "command not found: $1"
}

service_name() {
  case "${1:-}" in
    ha|homeassistant) echo homeassistant ;;
    ma|music|music-assistant) echo music-assistant ;;
    matter|matter-server) echo matter-server ;;
    "") echo "" ;;
    *) echo "$1" ;;
  esac
}

http_code() {
  local code
  code="$(curl -fsSL -m 3 -o /dev/null -w '%{http_code}' "$1" 2>/dev/null)" || {
    echo "down"
    return
  }
  echo "${code:-down}"
}

port_open() {
  ss -lnt 2>/dev/null | grep -q ":$1 "
}

wait_ready() {
  local i ha ma matter
  for i in $(seq 1 30); do
    ha="$(http_code http://127.0.0.1:8123/)"
    ma="$(http_code http://127.0.0.1:8095/)"
    matter="$(port_open 5580 && echo open || echo down)"
    if [ "$ha" = "200" ] && [ "$ma" = "200" ] && port_open 10200 && port_open 10300 && [ "$matter" = "open" ]; then
      return 0
    fi
    sleep 2
  done
  echo "WARN: endpoints not ready after 60s (HA=$ha MA=$ma Matter=$matter)" >&2
  return 1
}

ensure_dirs() {
  mkdir -p homeassistant/config data/music-assistant data/whisper data/piper data/matter-server backups
  if [ ! -f .env ]; then
    cp .env.example .env
    chmod 600 .env
    echo "created .env from .env.example"
  fi
}

status() {
  echo "=== stack ==="
  "${COMPOSE[@]}" ps
  echo
  echo "=== endpoints ==="
  printf 'HA      http://127.0.0.1:8123 -> %s\n' "$(http_code http://127.0.0.1:8123/)"
  printf 'MA      http://127.0.0.1:8095 -> %s\n' "$(http_code http://127.0.0.1:8095/)"
  printf 'Matter  127.0.0.1:5580 -> %s\n' "$(port_open 5580 && echo open || echo down)"
  printf 'Whisper 127.0.0.1:10300 -> %s\n' "$(port_open 10300 && echo open || echo down)"
  printf 'Piper   127.0.0.1:10200 -> %s\n' "$(port_open 10200 && echo open || echo down)"
  echo
  echo "=== host ==="
  printf 'systemd: %s\n' "$(systemctl is-system-running 2>/dev/null || true)"
  printf 'wait-online: %s\n' "$(systemctl is-active systemd-networkd-wait-online.service 2>/dev/null || true)"
  echo "failed units:"
  systemctl --failed --no-legend 2>/dev/null | head -10 || true
}

up() {
  ensure_dirs
  "${COMPOSE[@]}" up -d --remove-orphans
  wait_ready || true
  status
}

down() {
  "${COMPOSE[@]}" down
}

stop() {
  local svc
  svc="$(service_name "${1:-}")"
  if [ -n "$svc" ]; then
    "${COMPOSE[@]}" stop "$svc"
  else
    "${COMPOSE[@]}" stop
  fi
}

restart() {
  local svc
  svc="$(service_name "${1:-}")"
  if [ -n "$svc" ]; then
    "${COMPOSE[@]}" restart "$svc"
  else
    "${COMPOSE[@]}" restart
  fi
  wait_ready || true
  status
}

logs() {
  local svc="${1:-}"
  if [ -n "$svc" ]; then
    svc="$(service_name "$svc")"
    shift
    "${COMPOSE[@]}" logs -f --tail="$TAIL" "$svc" "$@"
  else
    "${COMPOSE[@]}" logs -f --tail="$TAIL" "$@"
  fi
}

pull() {
  "${COMPOSE[@]}" pull
}

update() {
  ensure_dirs
  echo "NOTE: run './ctl.sh backup' first for a consistent snapshot."
  "${COMPOSE[@]}" pull
  "${COMPOSE[@]}" up -d --remove-orphans
  wait_ready || true
  status
}

backup() {
  ensure_dirs
  local ts out files
  ts="$(date '+%Y%m%d-%H%M%S')"
  out="backups/nettop-$ts.tar.gz"
  files=(docker-compose.yml .env.example homeassistant/config data/music-assistant data/matter-server data/whisper data/piper)
  [ -f .env ] && files+=(.env)

  echo "Stopping stack for a consistent backup..."
  "${COMPOSE[@]}" stop

  # HA/MA write as root; prefer passwordless sudo for a complete archive.
  local tar_cmd=(tar)
  if [ "$(id -u)" -ne 0 ] && sudo -n true 2>/dev/null; then
    tar_cmd=(sudo -n tar)
  fi

  if ! "${tar_cmd[@]}" -czf "$out" "${files[@]}"; then
    echo "Backup failed; starting stack again." >&2
    "${COMPOSE[@]}" start
    exit 1
  fi
  if [ "$(id -u)" -ne 0 ] && [ "${tar_cmd[0]}" = "sudo" ]; then
    sudo -n chown "$(id -u):$(id -g)" "$out" 2>/dev/null || true
  fi
  "${COMPOSE[@]}" start
  wait_ready || true
  echo "Backup written: $ROOT/$out"
}

shell() {
  local svc
  svc="$(service_name "${1:-}")"
  [ -n "$svc" ] || die "usage: ./ctl.sh shell <ha|ma|matter>"
  "${COMPOSE[@]}" exec "$svc" sh
}

config() {
  "${COMPOSE[@]}" config
}

doctor() {
  need docker
  need curl
  echo "=== docker ==="
  docker info --format '{{.ServerVersion}}' >/dev/null && echo "docker ok"
  "${COMPOSE[@]}" config -q && echo "compose config ok"
  echo
  echo "=== files ==="
  for f in docker-compose.yml homeassistant/config data/music-assistant data/matter-server data/whisper data/piper; do
    if [ -e "$f" ]; then echo "ok  $f"; else echo "MISSING  $f"; fi
  done
  echo
  echo "=== ports ==="
  ss -lntp 2>/dev/null | grep -E ':(8123|8095|8097|8927|10200|10300|5580)\b' || echo "no HA/MA/Matter/STT/TTS listeners yet"
  echo
  echo "=== network ==="
  ip -br addr
  ip route
  echo
  status
}

usage() {
  cat <<'EOF'
Usage: ./ctl.sh <command> [service]

Commands:
  up                 Create dirs/.env and start HA + Music Assistant + Matter
  down               Stop and remove containers (data stays)
  stop [ha|ma|matter] Stop all or one service
  restart [ha|ma|matter] Restart all or one service
  status             Containers, HTTP endpoints, system state
  logs [ha|ma|matter] Follow logs (all or one service)
  pull               Pull pinned images
  update             Pull images and recreate containers
  backup             Stop stack, tar compose + HA config + MA/Matter data, start
  shell <ha|ma|matter> Open a shell inside a container
  config             Render docker compose config
  doctor             Read-only diagnostics

Examples:
  ./ctl.sh up
  ./ctl.sh status
  ./ctl.sh logs ma
  ./ctl.sh logs matter
  ./ctl.sh restart ha
EOF
}

case "${1:-}" in
  up|start) up ;;
  down) down ;;
  stop) shift; stop "${1:-}" ;;
  restart) shift; restart "${1:-}" ;;
  status|ps) status ;;
  logs) shift; logs "${1:-}" ;;
  pull) pull ;;
  update) update ;;
  backup) backup ;;
  shell) shift; shell "${1:-}" ;;
  config) config ;;
  doctor) doctor ;;
  help|-h|--help|"") usage ;;
  *) usage; exit 2 ;;
esac
