K3S_VERSION=v1.35.9+k3s1
HAOS_VERSION=18.4
PVE_VERSION=9.2.24
MATTER_ADDON_VERSION=9.2.0
MUSIC_ASSISTANT_ADDON_VERSION=2.10.8
# Hermes runtime images are built and pushed by the `hermes` CI unit
# (`projects/home-assistant/deploy/images/build-and-push.sh`). deploy-home then
# calls `hermes_rollout.py`, which sets the pushed digests on the `runtime`
# release, runs the voice smoke checks and rolls back on failure. The values
# below record the last reviewed pins; the live release is the source of truth.
VOICE_PROXY_IMAGE=ghcr.io/proflyder/voice-proxy@sha256:238e0a0f3afe5f152e3d8b1de7a35452c016d68368e3bac24c5fde882869fca9
HERMES_IMAGE=ghcr.io/proflyder/hermes@sha256:09a542f3ae846026e466c0f0b4acf81522da12cf9cbb1bec22125ae068c9b7e6
PROMETHEUS_CHART_VERSION=92.14.0
LOKI_CHART_VERSION=20.0.2
ALLOY_CHART_VERSION=1.14.3
TRAEFIK_CHART_VERSION=42.2.0
CERT_MANAGER_CHART_VERSION=v1.21.3
# OpenBao: single-node raft + static auto-unseal, UI behind Traefik.
# Image is pinned by digest; chart is pinned by version.
OPENBAO_CHART_VERSION=0.30.6
OPENBAO_IMAGE_VERSION=2.7.2
OPENBAO_IMAGE_DIGEST=sha256:5046911075e137e154ce49ab051502f4ec7cf367d484ae4a4dce665d9b90561f
# External Secrets Operator: chart and image pinned; the image is referenced by
# digest in values/external-secrets.yaml so a moving tag cannot slip in.
EXTERNAL_SECRETS_CHART_VERSION=2.14.0
EXTERNAL_SECRETS_IMAGE_VERSION=v2.14.0
EXTERNAL_SECRETS_IMAGE_DIGEST=sha256:10608eb53e636e744903d5f3657a4d9c44e28d9d9c181b73e07bc7dd2e6884dc
HERMES_REVISION=bf0203a794a07aac3ff764972ab9087baa4e4f4e
