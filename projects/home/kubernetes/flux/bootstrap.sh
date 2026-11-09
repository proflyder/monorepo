#!/usr/bin/env bash
# Install the Flux toolkit and hand reconciliation over to Git.
#
# Run from the owner's workstation, never from CI:
#
#   mise run admin:flux-bootstrap
#
# Idempotent: the toolkit, the ExternalSecrets (deploy key + settings) and the
# GitRepository/root Kustomization are all applied with kubectl apply. The
# script never reads or prints secret values; it only waits for the ESO-owned
# Secrets to become Ready.
#
# Prerequisites:
#   * External Secrets Operator and ClusterSecretStore/openbao are installed;
#   * OpenBao holds `secret/k8s/flux-system/git-deploy-key` (identity,
#     known_hosts) and `secret/k8s/flux-system/settings` (NEXUS_HOST,
#     NEXUS_PUBLIC_HOST, NODE_LAN_IP, HOME_DOMAIN, LAN_CIDR, HAOS_ADDRESS,
#     PVE_NODE_TARGET);
#   * the matching public key is registered as a read-only deploy key on
#     github.com/proflyder/monorepo (see README).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
DIR="$ROOT/projects/home/kubernetes/flux"
KUBECTL=${KUBECTL:-kubectl}

if ! $KUBECTL get crd externalsecrets.external-secrets.io >/dev/null 2>&1; then
  echo "External Secrets Operator is not installed; bootstrap it first" >&2
  exit 1
fi
if ! $KUBECTL get clustersecretstore openbao >/dev/null 2>&1; then
  echo "ClusterSecretStore/openbao is missing; run the ESO deploy first" >&2
  exit 1
fi

echo "== toolkit (source, kustomize and helm controllers)"
$KUBECTL apply -k "$DIR"

echo "== waiting for controllers"
for deployment in source-controller kustomize-controller helm-controller; do
  $KUBECTL -n flux-system rollout status "deployment/$deployment" --timeout=300s
done

echo "== deploy key and settings from OpenBao"
$KUBECTL -n flux-system wait --for=condition=Ready \
  externalsecret/flux-git-deploy-key externalsecret/flux-settings --timeout=120s
for secret in flux-system flux-settings; do
  $KUBECTL -n flux-system get secret "$secret" >/dev/null
done

echo "== Git source and root Kustomization"
$KUBECTL apply -f "$DIR/sync.yaml"
$KUBECTL -n flux-system annotate kustomization/flux-system \
  reconcile.fluxcd.io/requestedAt="$(date +%s)" --overwrite >/dev/null

echo "Flux bootstrap complete."
echo "  source:        $KUBECTL -n flux-system get gitrepository flux-system"
echo "  root kustomization: $KUBECTL -n flux-system get kustomization flux-system"
echo "  Nexus:         $KUBECTL -n flux-system get kustomization nexus nexus-provision"
