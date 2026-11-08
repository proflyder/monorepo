#!/usr/bin/env bash
# Regression test for deploy.sh control-mode handling with a mocked kubectl.
#
# Verifies:
#   * default rerun preserves the existing ha-control execute mode and replicas
#     (no apply of control.yaml, no set env, no scale);
#   * --control-catalog disables execution;
#   * --control-execute fails closed when the control queue has backlog and
#     only then enables execution;
#   * first install applies the control Deployment and Service.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
DIR="$ROOT/projects/home/kubernetes/scheduler"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# deploy.sh renders manifests and reads digests with the Rust `home-deploy` CLI
# (platform/deploy/resolve.sh). Build it once here unless the caller supplied one.
if [[ -z "${HOME_DEPLOY:-}" ]] && command -v cargo >/dev/null 2>&1; then
  ( cd "$ROOT" && cargo build -p home-deploy )
  HOME_DEPLOY="$ROOT/target/debug/home-deploy"
  export HOME_DEPLOY
fi
[[ -n "${HOME_DEPLOY:-}" && -x "${HOME_DEPLOY:-}" ]] || {
  echo "home-deploy is required: build //platform/deploy-rs:home-deploy or set HOME_DEPLOY" >&2
  exit 1
}

cat > "$TMP/env" <<'EOF'
HA_URL=http://192.0.2.28:8123
LAN_CIDR=192.0.2.0/24
NODE_LAN_IP=192.0.2.27
SCHEDULER_NODE_PORT=30098
SCHEDULER_PUBLIC_URL=http://192.0.2.27:30098
CONTROL_NODE_PORT=30099
EOF

# Pre-resolved digest-pinned images: the regression test must not build or
# push anything.
export SCHEDULER_IMAGE="ghcr.io/proflyder/scheduler@sha256:$(printf 'a%.0s' {1..64})"
export HA_BRIDGE_IMAGE="ghcr.io/proflyder/ha-bridge@sha256:$(printf 'b%.0s' {1..64})"
export HA_CONTROL_IMAGE="ghcr.io/proflyder/ha-control@sha256:$(printf 'c%.0s' {1..64})"
export HA_SCENARIO_IMAGE="ghcr.io/proflyder/ha-scenario@sha256:$(printf '4%.0s' {1..64})"
export PREFLIGHT_IMAGE="ghcr.io/proflyder/home-deploy@sha256:$(printf 'd%.0s' {1..64})"

cat > "$TMP/kubectl" <<'EOF'
#!/usr/bin/env bash
echo "$*" >> "$CALL_LOG"
args="$*"
case "$args" in
  *"get deployment ha-control -o jsonpath"*) echo "${CONTROL_REPLICAS:-1}"; exit 0 ;;
  *"get deployment scheduler -o json"*) printf '%s' '{"metadata":{"generation":1},"spec":{"template":{"spec":{"containers":[{"name":"scheduler","image":"old"}]}}}}'; exit 0 ;;
  *"get deployment ha-control -o json"*) printf '%s' '{"metadata":{"generation":1},"spec":{"template":{"spec":{"containers":[{"name":"ha-control","image":"old"}]}}}}'; exit 0 ;;
  *"get deployment ha-scenario -o json"*) printf '%s' '{"metadata":{"generation":1},"spec":{"template":{"spec":{"containers":[{"name":"ha-scenario","image":"old"}]}}}}'; exit 0 ;;
  *"get deployment ha-bridge -o json"*) printf '%s' '{"metadata":{"generation":1},"spec":{"template":{"spec":{"containers":[{"name":"ha-bridge","image":"old"}]}}}}'; exit 0 ;;
  *"get deployment ha-scenario"*) exit 0 ;;
  *"get deployment ha-control"*)
    # A missing Deployment answers like the real API server: state record only
    # treats an explicit NotFound as "absent" and fails closed on anything else.
    if [[ "${MOCK_CONTROL_EXISTS:-0}" != "0" ]]; then
      echo 'Error from server (NotFound): deployments.apps "ha-control" not found' >&2
      exit 1
    fi
    exit 0 ;;
  *"get deployment ha-bridge"*) exit 0 ;;
  *"get externalsecret"*) printf '%s' 'True'; exit 0 ;;
  *"get secret"*) exit 0 ;;
  *"exec deploy/scheduler"*status*)
    if [[ -n "${CONTROL_QUEUE_STATUS:-}" ]]; then printf '%s' "$CONTROL_QUEUE_STATUS"; else printf '%s' '{"queues":[]}'; fi
    exit 0 ;;
  *"exec deploy/ha-scenario"*healthcheck*|*"exec deploy/ha-control"*healthcheck*|*"exec deploy/scheduler"*healthcheck*) exit 0 ;;
  *) exit 0 ;;
esac
EOF
chmod +x "$TMP/kubectl"
export CALL_LOG="$TMP/calls.log"
export KUBECTL="$TMP/kubectl"
export PRIVATE_ENV="$TMP/env"
export SKIP_FIREWALL=1
export DEPLOY_STATE_DIR="$TMP/state"

run() {
  : > "$CALL_LOG"
  "$DIR/deploy.sh" --skip-build "$@" >"$TMP/out" 2>&1 || { echo "deploy.sh failed:"; cat "$TMP/out"; exit 1; }
}

fail() { echo "FAIL: $*"; echo "--- calls"; cat "$CALL_LOG"; exit 1; }

# 1. Default rerun preserves mode/replicas.
export MOCK_CONTROL_EXISTS=1
run
grep -q "apply -f .*control.yaml" "$CALL_LOG" || fail "first install did not apply control.yaml"
grep -q "apply -f .*control-service.yaml" "$CALL_LOG" || fail "first install did not apply control-service.yaml"
[[ -f "$DEPLOY_STATE_DIR/scheduler.applied" ]] || fail "scheduler apply was not recorded for rollback"
[[ -f "$DEPLOY_STATE_DIR/ha-bridge.applied" ]] || fail "ha-bridge apply was not recorded for rollback"
[[ -f "$DEPLOY_STATE_DIR/ha-scenario.applied" ]] || fail "ha-scenario apply was not recorded for rollback"
[[ -f "$DEPLOY_STATE_DIR/scheduler.before.json" ]] || fail "scheduler pre-deploy state was not recorded"
grep -q "create secret" "$CALL_LOG" && fail "deploy.sh wrote a Secret; values must come from OpenBao/ESO"

export MOCK_CONTROL_EXISTS=0
unset CONTROL_QUEUE_STATUS || true
run
grep -q "apply -f .*control.yaml" "$CALL_LOG" && fail "rerun applied control.yaml (would reset mode)"
[[ "$(grep -c 'replace -f' "$CALL_LOG")" -eq 4 ]] || fail "rerun did not converge scheduler, ha-control, ha-scenario and ha-bridge"
grep -q -- "--type strategic" "$CALL_LOG" && fail "rerun used a strategic patch (leaves drifted fields behind)"
grep -q "set env deployment/ha-control" "$CALL_LOG" && fail "rerun changed CONTROL_EXECUTE"
grep -q "scale deployment/ha-control" "$CALL_LOG" && fail "rerun changed control replicas"
grep -q "set env deployment/ha-scenario" "$CALL_LOG" && fail "rerun changed SCENARIO_EXECUTE"
grep -q "scale deployment/ha-scenario" "$CALL_LOG" && fail "rerun changed scenario replicas"

# 2. Explicit catalog mode.
export MOCK_CONTROL_EXISTS=0
run --control-catalog
grep -q "set env deployment/ha-control CONTROL_EXECUTE=false" "$CALL_LOG" || fail "catalog mode did not disable execution"

# 3. Execute with empty queue.
export CONTROL_QUEUE_STATUS='{"ready":true,"queues":[{"name":"home","pending":0,"leased":0},{"name":"tasks","pending":0,"leased":0}]}'
run --control-execute
grep -q "set env deployment/ha-control CONTROL_EXECUTE=true" "$CALL_LOG" || fail "execute mode did not enable execution"
grep -q "exec deploy/scheduler.*status" "$CALL_LOG" || fail "execute mode skipped the queue preflight"

# 4. Execute with backlog fails closed.
set +e
export CONTROL_QUEUE_STATUS='{"queues":[{"name":"control","pending":2,"leased":0}]}'
: > "$CALL_LOG"
"$DIR/deploy.sh" --skip-build --control-execute >"$TMP/out" 2>&1
rc=$?
set -e
[[ $rc -ne 0 ]] || fail "execute mode accepted a non-empty control queue"
grep -q "set env deployment/ha-control CONTROL_EXECUTE=true" "$CALL_LOG" && fail "execute mode enabled despite backlog"

# 5. Malformed or not-ready GetStatus must also fail closed.
for bad in \
  '{}' \
  '{"ready":false,"queues":[{"name":"home","pending":0,"leased":0}]}' \
  '{"ready":true,"queues":[]}' \
  '{"ready":true,"queues":"control"}' \
  '{"ready":true,"queues":[{"name":"control","pending":-1,"leased":0}]}'
do
  export CONTROL_QUEUE_STATUS="$bad"
  : > "$CALL_LOG"
  set +e
  "$DIR/deploy.sh" --skip-build --control-execute >"$TMP/out" 2>&1
  rc=$?
  set -e
  [[ $rc -ne 0 ]] || fail "execute mode accepted malformed status: $bad"
  grep -q "set env deployment/ha-control CONTROL_EXECUTE=true" "$CALL_LOG" && fail "enabled despite malformed status: $bad"
done

# 6. Prebuilt digests from deploy-home (DECISIONS 36) replace the local build.
#    The Bazel stub fails, so any build attempt is a hard failure.
mkdir -p "$TMP/bin"
cat > "$TMP/bin/bazel" <<'EOF'
#!/usr/bin/env bash
echo "$*" >> "$BAZEL_LOG"
exit 1
EOF
chmod +x "$TMP/bin/bazel"
export BAZEL_LOG="$TMP/bazel.log"
: > "$BAZEL_LOG"
unset SCHEDULER_IMAGE HA_BRIDGE_IMAGE HA_CONTROL_IMAGE HA_SCENARIO_IMAGE PREFLIGHT_IMAGE MOCK_CONTROL_EXISTS CONTROL_QUEUE_STATUS || true
export PREBUILT_DIGESTS="scheduler=sha256:$(printf 'd%.0s' {1..64}),ha-bridge=sha256:$(printf 'e%.0s' {1..64}),ha-control=sha256:$(printf 'f%.0s' {1..64}),ha-scenario=sha256:$(printf '2%.0s' {1..64}),home-deploy=sha256:$(printf '1%.0s' {1..64})"
: > "$CALL_LOG"
PATH="$TMP/bin:$PATH" "$DIR/deploy.sh" >"$TMP/out" 2>&1 || {
  cat "$TMP/out"
  fail "deploy.sh failed with prebuilt digests"
}
[[ ! -s "$BAZEL_LOG" ]] || fail "prebuilt digests did not skip the Bazel build"
grep -q "image scheduler: prebuilt ghcr.io/proflyder/scheduler@sha256:dddd" "$TMP/out" \
  || fail "scheduler did not use the prebuilt digest"
grep -q "image ha-bridge: prebuilt ghcr.io/proflyder/ha-bridge@sha256:eeee" "$TMP/out" \
  || fail "ha-bridge did not use the prebuilt digest"
grep -q "image ha-control: prebuilt ghcr.io/proflyder/ha-control@sha256:ffff" "$TMP/out" \
  || fail "ha-control did not use the prebuilt digest"
grep -q "image ha-scenario: prebuilt ghcr.io/proflyder/ha-scenario@sha256:2222" "$TMP/out" \
  || fail "ha-scenario did not use the prebuilt digest"
grep -q "image home-deploy: prebuilt ghcr.io/proflyder/home-deploy@sha256:1111" "$TMP/out" \
  || fail "home-deploy did not use the prebuilt digest"
grep -q "push" "$BAZEL_LOG" && fail "prebuilt images were pushed again"
unset PREBUILT_DIGESTS

# 7. DEPLOY_NO_BUILD=1 (deploy-home) never builds: every image, including the
#    sibling workloads, arrives as a promoted digest.
export DEPLOY_NO_BUILD=1
export PREBUILT_DIGESTS="scheduler=sha256:$(printf 'd%.0s' {1..64}),ha-bridge=sha256:$(printf 'e%.0s' {1..64}),ha-control=sha256:$(printf 'f%.0s' {1..64}),ha-scenario=sha256:$(printf '2%.0s' {1..64}),home-deploy=sha256:$(printf '1%.0s' {1..64})"
: > "$BAZEL_LOG"
: > "$CALL_LOG"
PATH="$TMP/bin:$PATH" "$DIR/deploy.sh" >"$TMP/out" 2>&1 || {
  cat "$TMP/out"
  fail "deploy.sh failed in DEPLOY_NO_BUILD mode"
}
[[ ! -s "$BAZEL_LOG" ]] || fail "DEPLOY_NO_BUILD=1 still built or ran Bazel"
grep -q "image ha-bridge: prebuilt ghcr.io/proflyder/ha-bridge@sha256:eeee" "$TMP/out" \
  || fail "ha-bridge did not use its promoted digest"

# 7b. deploy-home passes the unit's workloads as ONLY (every image name of the
#     ha-bridge unit, including ha-scenario); each must be a known group.
: > "$CALL_LOG"
ONLY="ha-bridge,ha-control,ha-scenario" PATH="$TMP/bin:$PATH" "$DIR/deploy.sh" >"$TMP/out" 2>&1 || {
  cat "$TMP/out"
  fail "deploy.sh rejected the ha-bridge unit workloads as ONLY"
}

# 8. A missing promoted digest fails closed instead of building.
: > "$CALL_LOG"
set +e
PREBUILT_DIGESTS="scheduler=sha256:$(printf 'd%.0s' {1..64}),ha-control=sha256:$(printf 'f%.0s' {1..64}),home-deploy=sha256:$(printf '1%.0s' {1..64})" \
  PATH="$TMP/bin:$PATH" "$DIR/deploy.sh" >"$TMP/out" 2>&1
rc=$?
set -e
[[ $rc -ne 0 ]] || fail "DEPLOY_NO_BUILD=1 with a missing image was accepted"
grep -q "no promoted digest for 'ha-bridge'" "$TMP/out" \
  || fail "missing promoted digest did not produce an actionable error"
grep -q "replace -f" "$CALL_LOG" && fail "a failed resolution still touched the cluster"
unset DEPLOY_NO_BUILD PREBUILT_DIGESTS

# 9. A malformed or duplicated value must fail closed, before any cluster call.
for bad in \
  "scheduler=sha256:short" \
  "scheduler=$(printf 'a%.0s' {1..64})" \
  "scheduler=sha256:$(printf 'a%.0s' {1..64}),scheduler=sha256:$(printf 'b%.0s' {1..64})"
do
  : > "$CALL_LOG"
  set +e
  PREBUILT_DIGESTS="$bad" PATH="$TMP/bin:$PATH" \
    "$DIR/deploy.sh" --skip-build >"$TMP/out" 2>&1
  rc=$?
  set -e
  [[ $rc -ne 0 ]] || fail "malformed prebuilt digest was accepted: $bad"
  grep -Eq "malformed prebuilt digest|duplicate prebuilt digest" "$TMP/out" \
    || fail "no clear error for: $bad"
  grep -q "prebuilt" "$CALL_LOG" && fail "malformed prebuilt digest reached the cluster: $bad"
done

echo "deploy control regression: ok"
