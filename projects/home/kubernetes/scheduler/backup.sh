#!/usr/bin/env bash
# Consistent backup of the scheduler SQLite database and the bridge ledger.
#
# SQLite in WAL mode cannot be copied safely while the writer is running. This
# script:
#   1. records the real replica counts of both deployments;
#   2. installs the cleanup trap BEFORE scaling anything;
#   3. scales both writers to 0 and fails closed if a pod remains;
#   4. copies the volumes with a short-lived helper pod;
#   5. restores the recorded replica counts (not a hard-coded 1).
#
# The default destination is private and git-ignored. Never back up into the
# repository working tree.
#
# Usage: ./backup.sh [output-dir]
set -euo pipefail

NS=home-assistant
KUBECTL=${KUBECTL:-kubectl}
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
OUT=${1:-"$ROOT/secrets/local/backups/scheduler/$(date +%Y%m%d-%H%M%S)"}
HELPER=scheduler-backup-helper
DEPS=(scheduler ha-bridge)

SCHEDULER_REPLICAS=""
BRIDGE_REPLICAS=""
SCALED=false
HELPER_CREATED=false

cleanup() {
  local code=$?
  set +e
  if $HELPER_CREATED; then
    $KUBECTL -n "$NS" delete pod "$HELPER" --ignore-not-found >/dev/null 2>&1
  fi
  if $SCALED; then
    $KUBECTL -n "$NS" scale deployment/scheduler --replicas="${SCHEDULER_REPLICAS:-1}" >/dev/null 2>&1
    $KUBECTL -n "$NS" scale deployment/ha-bridge --replicas="${BRIDGE_REPLICAS:-0}" >/dev/null 2>&1
  fi
  exit $code
}
trap cleanup EXIT INT TERM

SCHEDULER_REPLICAS=$($KUBECTL -n "$NS" get deployment/scheduler -o jsonpath='{.spec.replicas}')
BRIDGE_REPLICAS=$($KUBECTL -n "$NS" get deployment/ha-bridge -o jsonpath='{.spec.replicas}')
echo "== current replicas: scheduler=$SCHEDULER_REPLICAS bridge=$BRIDGE_REPLICAS"

mkdir -p "$OUT"
chmod 700 "$OUT"

echo "== quiescing writers"
SCALED=true
$KUBECTL -n "$NS" scale deployment/scheduler --replicas=0
$KUBECTL -n "$NS" scale deployment/ha-bridge --replicas=0

# Fail closed: never copy WAL files while a writer still exists.
for dep in "${DEPS[@]}"; do
  if ! $KUBECTL -n "$NS" wait --for=delete pod -l "app.kubernetes.io/name=$dep" --timeout=120s; then
    echo "writer $dep did not stop; refusing to copy SQLite" >&2
    exit 1
  fi
  if [[ -n "$($KUBECTL -n "$NS" get pods -l "app.kubernetes.io/name=$dep" -o name)" ]]; then
    echo "writer $dep still has pods; refusing to copy SQLite" >&2
    exit 1
  fi
done

echo "== mounting volumes read-only"
cat <<EOF | $KUBECTL apply -f -
apiVersion: v1
kind: Pod
metadata:
  name: $HELPER
  namespace: $NS
spec:
  restartPolicy: Never
  securityContext:
    runAsNonRoot: true
    runAsUser: 65534
    runAsGroup: 65534
    fsGroup: 65534
  containers:
    - name: copy
      image: busybox@sha256:fd7dc98638c8e305f4dc34e979f1c0fdfdcaeb0fbf8fcff77ae834b6da3d7e6e
      command: ["sh", "-c", "sleep 600"]
      securityContext:
        allowPrivilegeEscalation: false
        readOnlyRootFilesystem: true
        capabilities:
          drop: ["ALL"]
      volumeMounts:
        - {name: scheduler-data, mountPath: /scheduler-data, readOnly: true}
        - {name: bridge-data, mountPath: /bridge-data, readOnly: true}
  volumes:
    - name: scheduler-data
      persistentVolumeClaim: {claimName: scheduler-data}
    - name: bridge-data
      persistentVolumeClaim: {claimName: ha-bridge-data}
EOF
HELPER_CREATED=true
$KUBECTL -n "$NS" wait --for=condition=Ready "pod/$HELPER" --timeout=120s

echo "== copying state"
$KUBECTL -n "$NS" cp "$HELPER:/scheduler-data" "$OUT/scheduler-data"
$KUBECTL -n "$NS" cp "$HELPER:/bridge-data" "$OUT/bridge-data"
# The redirect creates SHA256SUMS before find runs: exclude it explicitly.
( cd "$OUT" && find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 shasum -a 256 > SHA256SUMS )
cat "$OUT/SHA256SUMS"

echo "== restoring writers"
$KUBECTL -n "$NS" delete pod "$HELPER" --ignore-not-found >/dev/null
HELPER_CREATED=false
$KUBECTL -n "$NS" scale deployment/scheduler --replicas="$SCHEDULER_REPLICAS"
$KUBECTL -n "$NS" scale deployment/ha-bridge --replicas="$BRIDGE_REPLICAS"
SCALED=false
$KUBECTL -n "$NS" rollout status deployment/scheduler --timeout=180s

echo "backup written to $OUT"
