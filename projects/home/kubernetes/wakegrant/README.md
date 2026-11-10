# wakegrant (home deployment)

K3s deployment of [`solutions/wakegrant`](../../../../solutions/wakegrant) in
namespace `home-assistant`: one pod, SQLite on a `local-retain` PVC, a NodePort
on the LAN, and an ESO-owned Secret. The protocol contract is
[`packages/contracts/wakegrant/PROTOCOL.md`](../../../../packages/contracts/wakegrant/PROTOCOL.md).

```text
Deployment wakegrant   1 replica, Recreate (single SQLite writer)
PVC wakegrant-data     1Gi, storageClassName local-retain
Service wakegrant      NodePort ${WAKE_GRANT_NODE_PORT} → 8099
Secret wakegrant       ESO ← secret/k8s/home-assistant/wakegrant
```

NodePort `30130` is free in the repository: the scheduler uses `30098/30099`,
currency-bot `30100` and Grafana `30300`. The value is deployment-specific and
lives in OpenBao `secret/k8s/flux-system/app-settings` (key
`WAKE_GRANT_NODE_PORT` → `Secret/flux-app-settings`), never in Git.

## Secrets

OpenBao KV v2, path `secret/k8s/home-assistant/wakegrant`:

| key | content |
|---|---|
| `issuer_token` | random bearer token HA sends as the issuer (no `Bearer ` prefix) |
| `machines` | `{"komp":"<base64 of 32 random bytes>"}` or `komp=<base64>` lines |

In HA `secrets.yaml` the issuer token is stored **with** the `Bearer ` prefix
(`wakegrant_bearer`), and `wakegrant_endpoint` is
`http://<k3s-node>:30130`.

Generate the token and the machine key without printing them:

```bash
openssl rand -hex 32            # issuer_token
openssl rand -base64 32         # machine key for komp
```

Apply the ExternalSecret (ESO must already be installed):

```bash
mise run kubectl -- apply -f projects/home/kubernetes/wakegrant/manifests/external-secrets.yaml
mise run kubectl -- -n home-assistant wait --for=condition=Ready externalsecret/wakegrant --timeout=120s
```

## Deploy

Flux applies the unit (`Kustomization/app-wakegrant`) from `develop`, with the
image digest from `deploy/home-apps`. After a merge, `deploy-home`'s `pins`
job promotes the verified digest and commits it; no VPN, kubeconfig or
manual script is involved.

```bash
# what Flux is applying
mise run kubectl -- -n flux-system get kustomization app-wakegrant
mise run kubectl -- -n flux-system get configmap home-app-images -o yaml \
  | grep WAKEGRANT_IMAGE

# roll back to the previous digest: revert the pins commit
#   (or freeze first: flux suspend kustomization app-wakegrant)
git log --oneline origin/deploy/home-apps -- pins/home-app-images.yaml

# force a reconcile after a pins/manifest change
mise run kubectl -- -n flux-system annotate kustomization/app-wakegrant \
  reconcile.fluxcd.io/requestedAt="$(date +%s)" --overwrite
```

Do not delete the Kustomization: `prune: true` would remove the workload; the
PVC carries `kustomize.toolkit.fluxcd.io/prune: disabled` so the SQLite volume
survives.

## Verify

```bash
# from the LAN
curl -sS http://<k3s-node>:30130/healthz          # ok
mise run kubectl -- -n home-assistant logs deploy/wakegrant    # JSON, machine/outcome/grantId only
mise run kubectl -- -n home-assistant exec deploy/wakegrant -- /usr/local/bin/wakegrant -healthcheck
```

Issue a test grant (the token is read from a private file, never echoed):

```bash
curl -sS -X POST http://<k3s-node>:30130/v1/grants \
  -H "Authorization: Bearer $(<secrets/home/wakegrant.token)" \
  -H 'Content-Type: application/json' \
  -d '{"machine":"komp","ttlSeconds":60}'
```

Expected `201 {"grantId":"…","expiresAt":"…"}`. A 401 means the token does not
match; a 400 means the machine is missing from the `machines` Secret.

## Tests

```bash
python3 -m unittest discover -s projects/home/kubernetes/wakegrant -p 'test_*.py'
```

The deploy tests are hermetic: a stub `kubectl` fails at the first cluster
call, so only validation, rendering and manifest invariants are asserted. The
protocol itself is tested in Rust (`solutions/wakegrant`).
