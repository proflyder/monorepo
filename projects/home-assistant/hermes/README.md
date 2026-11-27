# Hermes instances

Each Hermes agent runs as a separate instance with its own process, data,
Telegram bot and secrets. Instances never share a `HERMES_HOME`.

| Instance | Directory | Deployment | Namespace | Data | Secrets |
|---|---|---|---|---|---|
| personal | [`personal/`](personal/README.md) | `HelmRelease/runtime` (`projects/home-assistant/deploy/chart`), Deployment `hermes`, `hostNetwork` | `home-assistant` | local PV `runtime-hermes` | `secret/k8s/home-assistant/hermes-*` via `ClusterSecretStore` |
| work | [`work/`](work/README.md) | `HelmRelease/hermes-work` ([`chart/`](chart)), Deployment `hermes-work` | `hermes-work` | PVC `hermes-work-data` (`local-retain`) | `secret/k8s/hermes-work/*` via namespaced `SecretStore` |

- [`chart/`](chart) — Helm chart `hermes`; every name derives from the
  `instance` value (`hermes-<instance>`). It renders the Deployment, the PVC,
  the ClusterIP Service, the config ConfigMap, a default-deny NetworkPolicy
  with an explicit egress allowlist and the post-upgrade API smoke.
- [`flux/base/`](flux/base) — Kustomize base for one instance: namespace
  `hermes-${HERMES_INSTANCE}`, `SecretStore`, the Telegram and API
  `ExternalSecret`s and the `HelmRelease`. The Flux Kustomization of the
  overlay sets `HERMES_INSTANCE` through `postBuild.substitute`.
- `work/` — overlay: chart values (`values.yaml`, including the Hermes
  `config.yaml`) and the model secret.

## Code notes

- Owner-only access is enforced at render time (`hermes.ownerOnly` in
  `chart/templates/_helpers.tpl`): `unauthorized_dm_behavior` must be `ignore`
  globally and for Telegram, `TELEGRAM_ALLOWED_USERS` must come from a secret,
  and `GATEWAY_ALLOW_ALL_USERS`, `GATEWAY_ALLOWED_USERS` and the Telegram
  group/guest variables are rejected. A violating values file fails `helm
  template`, so Flux never applies it.
- The `seed-config` init container copies the ConfigMap into
  `/opt/data/config.yaml` on every start: Git is the source of the config, and
  edits made inside the Pod do not survive a restart.
- `files/drain.py` is the preStop drain: it asks the gateway to stop taking
  new turns and waits up to 60 s for running agents.
- Pod Security `restricted`: the gateway runs as UID 10000 with all
  capabilities dropped. The root filesystem stays writable because the
  upstream image is not verified with a read-only root yet.

## Migration of the personal instance (stage 4)

The personal instance stays on `HelmRelease/runtime` until stage 4. Live
names are not renamed in this stage because each rename loses state or breaks
consumers:

| Live name | Why it stays | Stage 4 step |
|---|---|---|
| namespace `home-assistant`, Deployment `hermes` | voice-proxy and the chat relay share the Pod; the health exporter, smoke and mcp-gateway address it by name | install `HelmRelease/hermes-personal` with `instance: personal`, `hostNetwork: true` and the voice-proxy sidecar values, then drop Hermes from `runtime` |
| local PV `runtime-hermes` (`/srv/k3s/volumes/runtime-hermes`) | it holds sessions, memory and `auth.json`; a new PVC would start empty | static PV `hermes-personal-data` with `volumeName` on the same host path and `Retain`; no data copy |
| `secret/k8s/home-assistant/hermes-{environment,config,chat-relay}` | Alertmanager, mcp-gateway and ha-config-sync read the same entries | copy values to `secret/k8s/hermes-personal/*`, switch every consumer, then remove the old entries |
| `ClusterSecretStore openbao` for the personal instance | the namespace hosts other services | namespaced `SecretStore` with role `hermes-personal-eso` (add `personal` to `ISOLATED_HERMES_INSTANCES`) |
