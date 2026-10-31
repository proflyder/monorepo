#!/usr/bin/env bash
# Acceptance checks for the home OpenBao deployment.
#
#   ./projects/home/kubernetes/openbao/verify.sh            # HTTPS, UI, login
#   ./projects/home/kubernetes/openbao/verify.sh --kubernetes  # + pod auth probe
#
# Reads HOME_DOMAIN/NODE_IP from secrets/home/ingress.env and the temporary
# human password from secrets/openbao/human.pass. Nothing secret is printed.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"

: "${KUBECONFIG:?Set KUBECONFIG to a private admin kubeconfig}"

RUN_KUBERNETES=0
for arg in "$@"; do
  case "$arg" in
    --kubernetes) RUN_KUBERNETES=1 ;;
    -h|--help) sed -n '2,9p' "$0"; exit 0 ;;
    *) echo "unknown argument: $arg" >&2; exit 2 ;;
  esac
done

PRIVATE_ENV="${INGRESS_PRIVATE_ENV:-$ROOT/secrets/home/ingress.env}"
if [[ -f "$PRIVATE_ENV" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "$PRIVATE_ENV"
  set +a
fi
: "${HOME_DOMAIN:?Set HOME_DOMAIN}"
NODE_IP="${NODE_IP:-192.0.2.27}"
HOST="vault.$HOME_DOMAIN"
SECRETS_DIR="$ROOT/secrets/openbao"
PASS_FILE="$SECRETS_DIR/human.pass"
HUMAN_USER="$(cat "$SECRETS_DIR/username" 2>/dev/null || true)"

fail=0
check() {
  local label=$1 expected=$2 actual=$3
  if [[ "$actual" == "$expected" ]]; then
    printf 'OK   %-40s %s\n' "$label" "$actual"
  else
    printf 'FAIL %-40s expected %s, got %s\n' "$label" "$expected" "$actual"
    fail=1
  fi
}
check_any() {
  local label=$1 expected=$2 actual=$3
  if [[ "|$expected|" == *"|$actual|"* ]]; then
    printf 'OK   %-40s %s\n' "$label" "$actual"
  else
    printf 'FAIL %-40s expected %s, got %s\n' "$label" "$expected" "$actual"
    fail=1
  fi
}
curl_host() {
  curl -sS --max-time 15 --resolve "$HOST:443:$NODE_IP" "$@"
}

resolved=$(dig +short A "$HOST" @harmony.ns.cloudflare.com | tail -1)
check "dns $HOST" "$NODE_IP" "${resolved:-none}"

ui=$(curl_host -o /dev/null -w '%{http_code}' "https://$HOST/ui/")
check_any "https /ui/" "200|301|302|307" "$ui"

redirect=$(curl -sS -o /dev/null -w '%{http_code}' --max-time 15 \
  --resolve "$HOST:80:$NODE_IP" "http://$HOST/" || true)
check_any "http redirect" "301|308" "$redirect"

issuer=$(openssl s_client -connect "$NODE_IP:443" -servername "$HOST" </dev/null 2>/dev/null \
  | openssl x509 -noout -issuer 2>/dev/null || true)
if [[ "$issuer" == *"Let's Encrypt"* && "$issuer" != *"STAGING"* ]]; then
  check "certificate issuer" "prod" "prod"
else
  check "certificate issuer" "prod" "$issuer"
fi

health=$(curl_host "https://$HOST/v1/sys/health")
check "health initialized" "True" \
  "$(printf '%s' "$health" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("initialized"))' 2>/dev/null || echo bad)"
check "health sealed" "False" \
  "$(printf '%s' "$health" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("sealed"))' 2>/dev/null || echo bad)"

# Human login through userpass, then prove the admin policy denies sealing.
if [[ -r "$PASS_FILE" && -n "$HUMAN_USER" ]]; then
  password=$(tr -d '\r\n' <"$PASS_FILE")
  login=$(printf '%s' "{\"password\":\"$password\"}" | curl_host -X POST -d @- \
    "https://$HOST/v1/auth/userpass/login/$HUMAN_USER" || true)
  token=$(printf '%s' "$login" | python3 -c 'import json,sys; print((json.load(sys.stdin).get("auth") or {}).get("client_token") or "")' 2>/dev/null || echo "")
  check "human userpass login" "ok" "$([[ -n "$token" ]] && echo ok || echo bad)"
  if [[ -n "$token" ]]; then
    caps=$(printf '%s' '{"paths":["sys/seal","sys/raw/foo","secret/data/ci/test"]}' \
      | curl_host -X POST -H "X-Vault-Token: $token" -d @- \
        "https://$HOST/v1/sys/capabilities-self")
    seal_caps=$(printf '%s' "$caps" | python3 -c 'import json,sys; d=json.load(sys.stdin).get("data") or {}; v=d.get("sys/seal") or []; print(",".join(v.get("capabilities", []) if isinstance(v, dict) else v))' 2>/dev/null || echo "")
    check "admin denied sys/seal" "deny" "$seal_caps"
    ci_caps=$(printf '%s' "$caps" | python3 -c 'import json,sys; d=json.load(sys.stdin).get("data") or {}; v=d.get("secret/data/ci/test") or []; print(",".join(v.get("capabilities", []) if isinstance(v, dict) else v))' 2>/dev/null || echo "")
    if [[ "$ci_caps" == *read* ]]; then check "admin reads secret/ci" "read" "read"; else check "admin reads secret/ci" "read" "$ci_caps"; fi
    curl_host -o /dev/null -X POST -H "X-Vault-Token: $token" \
      "https://$HOST/v1/auth/token/revoke-self" || true
  fi
else
  echo "SKIP human login (need $PASS_FILE and $SECRETS_DIR/username)"
fi

# Policy preconditions for the machine identities.
for policy in ci-deploy k8s-read; do
  body=$(curl_host "https://$HOST/v1/sys/policies/acl/$policy" || true)
  if [[ "$body" == *'"write"'* || "$body" == *'"update"'* ]]; then
    check "policy $policy write" "absent" "present"
  else
    check "policy $policy write" "absent" "absent"
  fi
done

# Kubernetes auth probe: real pod, real ServiceAccount, real TokenReview.
if [[ "$RUN_KUBERNETES" == 1 ]]; then
  source "$ROOT/projects/home/kubernetes/versions.sh"
  image="${OPENBAO_IMAGE_VERSION}@${OPENBAO_IMAGE_DIGEST}"
  kubectl -n openbao delete job openbao-kubernetes-auth --ignore-not-found >/dev/null
  kubectl delete namespace openbao-smoke --ignore-not-found --wait=true >/dev/null
  sed "s|__OPENBAO_IMAGE_TAG__|$image|g" \
    "$ROOT/projects/home/kubernetes/openbao/manifests/smoke-kubernetes.yaml" | kubectl apply -f -
  if kubectl -n openbao-smoke wait --for=condition=complete job/openbao-kubernetes-auth --timeout=180s; then
    kubectl -n openbao-smoke logs job/openbao-kubernetes-auth
    check "kubernetes auth probe" "ok" "ok"
  else
    kubectl -n openbao-smoke logs job/openbao-kubernetes-auth || true
    check "kubernetes auth probe" "ok" "failed"
  fi
fi

exit "$fail"
