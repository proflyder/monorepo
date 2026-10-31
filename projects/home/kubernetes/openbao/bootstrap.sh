#!/usr/bin/env bash
# Initialise a fresh OpenBao server and apply the repository configuration.
#
# With the `static` seal configured, `operator init` returns recovery keys (no
# unseal keys) and the server unseals itself. The response is written to
# secrets/openbao/init.json (mode 600) exactly once, because recovery keys can
# generate a new root token and therefore must be backed up offline.
#
# After configure.py succeeds the initial root token is revoked and removed
# from init.json. Re-running on an initialised server needs an admin token in
# BAO_TOKEN (for example a root token generated from recovery keys) and does
# not rotate anything.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/projects/home/kubernetes/versions.sh"

: "${KUBECONFIG:?Set KUBECONFIG to a private admin kubeconfig}"
umask 077
SECRETS_DIR="$ROOT/secrets/openbao"
INIT_FILE="$SECRETS_DIR/init.json"
install -d -m 700 "$SECRETS_DIR"

PRIVATE_ENV="${INGRESS_PRIVATE_ENV:-$ROOT/secrets/home/ingress.env}"
if [[ -f "$PRIVATE_ENV" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "$PRIVATE_ENV"
  set +a
fi
: "${HOME_DOMAIN:?Set HOME_DOMAIN (private ingress suffix)}"
NODE_IP="${NODE_IP:-192.0.2.27}"
ADDR="https://vault.$HOME_DOMAIN"
POD=openbao-0
# The human login name is a private value: never hard-code it in the repo.
HUMAN_USER="${OPENBAO_USER:-$(cat "$SECRETS_DIR/username" 2>/dev/null || true)}"
if [[ -z "$HUMAN_USER" ]]; then
  echo "set OPENBAO_USER or write the login name to $SECRETS_DIR/username" >&2
  exit 1
fi
NS=openbao

status_json() {
  kubectl -n "$NS" exec "$POD" -- /bin/sh -ec \
    'BAO_ADDR=http://127.0.0.1:8200 bao status -format=json' 2>/dev/null || true
}

status=$(status_json)
initialized=$(printf '%s' "$status" | python3 -c 'import json,sys; d=sys.stdin.read().strip(); print(json.loads(d).get("initialized") if d else False)' 2>/dev/null || echo False)

if [[ "$initialized" != "True" ]]; then
  if [[ -s "$INIT_FILE" ]]; then
    echo "init.json exists but the server reports initialized=false; refusing to overwrite keys" >&2
    exit 1
  fi
  echo "initialising OpenBao (auto-unseal, 5 recovery shares / threshold 3)"
  tmp="$SECRETS_DIR/.init.json.tmp"
  kubectl -n "$NS" exec "$POD" -- /bin/sh -ec \
    'BAO_ADDR=http://127.0.0.1:8200 bao operator init -format=json \
       -recovery-shares=5 -recovery-threshold=3' > "$tmp"
  python3 - "$tmp" "$INIT_FILE" <<'PY'
import json
import os
import sys

source, target = sys.argv[1], sys.argv[2]
data = json.load(open(source, encoding="utf-8"))
if not data.get("root_token") or not (
        data.get("recovery_keys") or data.get("recovery_keys_b64")):
    sys.exit("operator init response lacks a root token or recovery keys")
data["initialized_at"] = __import__("datetime").datetime.now(
    __import__("datetime").timezone.utc).isoformat()
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle, indent=2)
os.chmod(target, 0o600)
PY
  rm -f "$tmp"
  echo "recovery keys and root token written to $INIT_FILE (chmod 600)"
else
  echo "OpenBao is already initialised"
fi

if [[ "$(status_json)" == *'"sealed": true'* ]]; then
  echo "server is sealed; restarting the pod so the static seal unseals it"
  kubectl -n "$NS" delete pod "$POD" --wait
  kubectl -n "$NS" wait --for=jsonpath='{.status.phase}'=Running pod/"$POD" --timeout=180s
  sleep 5
fi

TOKEN="${BAO_TOKEN:-}"
if [[ -z "$TOKEN" && -s "$INIT_FILE" ]]; then
  TOKEN=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("root_token") or "")' "$INIT_FILE")
fi
if [[ -z "$TOKEN" ]]; then
  echo "no admin token available: set BAO_TOKEN (generate a root token from recovery keys)" >&2
  exit 1
fi
ROOT_TOKEN="$TOKEN"

OPENBAO_NODE_IP="$NODE_IP" BAO_TOKEN="$TOKEN" \
  python3 "$ROOT/projects/home/kubernetes/openbao/configure.py" \
  --addr "$ADDR" --resolve-ip "$NODE_IP" --user "$HUMAN_USER" ${1+"$@"}

# Revoke the initial root token and drop it from disk. Recovery keys stay.
if [[ -s "$INIT_FILE" ]]; then
  root=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("root_token") or "")' "$INIT_FILE")
  if [[ -n "$root" && "$root" == "$ROOT_TOKEN" ]]; then
    curl -sS --max-time 20 --resolve "vault.$HOME_DOMAIN:443:$NODE_IP" \
      -X POST -H "X-Vault-Token: $root" \
      -o /dev/null -w '' "$ADDR/v1/auth/token/revoke-self" || true
    python3 - "$INIT_FILE" <<'PY'
import json
import sys

path = sys.argv[1]
data = json.load(open(path, encoding="utf-8"))
data.pop("root_token", None)
data["root_token_revoked"] = True
with open(path, "w", encoding="utf-8") as handle:
    json.dump(data, handle, indent=2)
PY
    chmod 600 "$INIT_FILE"
    echo "initial root token revoked and removed from init.json"
  fi
fi

echo
echo "human login: https://vault.$HOME_DOMAIN/ (user $HUMAN_USER)"
echo "password:    $SECRETS_DIR/human.pass (change it in the UI after login)"
echo "kubernetes probe: ./projects/home/kubernetes/openbao/verify.sh --kubernetes"
