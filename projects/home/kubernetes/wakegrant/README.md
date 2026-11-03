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
lives in `secrets/home/wakegrant.env` (or OpenBao
`secret/ci/deploy-home` key `wakegrant_env`), never in Git.

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
kubectl apply -f projects/home/kubernetes/wakegrant/manifests/external-secrets.yaml
kubectl -n home-assistant wait --for=condition=Ready externalsecret/wakegrant --timeout=120s
```

## Deploy

CI deploys the unit after a merge to `develop` (see
`platform/ci/units.yml`, unit `wakegrant`). Manual flow:

```bash
cp projects/home/kubernetes/wakegrant/wakegrant.env.example secrets/home/wakegrant.env
# set WAKE_GRANT_NODE_PORT if 30130 is taken on this cluster

# workstation → import the digest-pinned image over SSH
NODE_SSH=<admin@node> projects/home/kubernetes/wakegrant/deploy.sh

# or build + push to GHCR and let the node pull
PUSH_IMAGES=1 projects/home/kubernetes/wakegrant/deploy.sh
```

`deploy.sh` builds the image hermetically with Bazel (no Docker daemon),
verifies the ESO Secret is Ready, renders the manifest into an ignored
directory, applies it and runs the in-container `-healthcheck` after rollout.

## Verify

```bash
# from the LAN
curl -sS http://<k3s-node>:30130/healthz          # ok
kubectl -n home-assistant logs deploy/wakegrant    # JSON, machine/outcome/grantId only
kubectl -n home-assistant exec deploy/wakegrant -- /usr/local/bin/wakegrant -healthcheck
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
protocol itself is tested in Go.
