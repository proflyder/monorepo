K3S_VERSION=v1.35.9+k3s1
HAOS_VERSION=18.4
PVE_VERSION=9.2.24
MATTER_ADDON_VERSION=9.2.0
MUSIC_ASSISTANT_ADDON_VERSION=2.10.8
# Hermes runtime images are built and pushed by the `hermes` CI unit
# (`projects/home-assistant/deploy/images/build-and-push.sh`) from the upstream
# revision and Dockerfile in `projects/home-assistant/deploy/images/hermes/`
# (see `revision` there); `voice-proxy`, `health-exporter` and `home-deploy` are
# Bazel/rules_oci images (distroless). deploy-home then calls
# `home-deploy hermes rollout`, which sets the pushed digests on the `runtime` release,
# runs the voice smoke checks and rolls back on failure. The values below
# record the last reviewed pins; the live release is the source of truth.
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
# Nexus Repository OSS artifact cache. 3.76.1 is the last release before the
# Community Edition usage limits (3.77.0, 2025-02-04); the tag and its
# manifest-list digest are pinned together so a moved tag cannot slip in.
NEXUS_IMAGE_VERSION=3.76.1
NEXUS_IMAGE_DIGEST=sha256:390b8ec7e712c680ed8b449b766695a7c5ebbf41e489317618e63ca199f2cc09
# One-shot provisioning Job (curl + sh only; no Python or JDK needed).
NEXUS_PROVISION_IMAGE_VERSION=8.23.0
NEXUS_PROVISION_IMAGE_DIGEST=sha256:3fb7e13e5d179a845c97d78204f3f1064d1907037f26a3ebf91710571151fd93
# Vaultwarden (Bitwarden-compatible password manager): upstream image by digest.
VAULTWARDEN_IMAGE_VERSION=1.37.5
VAULTWARDEN_IMAGE_DIGEST=sha256:4fe17f415f90087377d37c755e0ea3cf189dcc6794e49cdd2a9adbb4937d9e1f
# External Secrets Operator: chart and image pinned; the image is referenced by
# digest in values/external-secrets.yaml so a moving tag cannot slip in.
EXTERNAL_SECRETS_CHART_VERSION=2.14.0
EXTERNAL_SECRETS_IMAGE_VERSION=v2.14.0
EXTERNAL_SECRETS_IMAGE_DIGEST=sha256:10608eb53e636e744903d5f3657a4d9c44e28d9d9c181b73e07bc7dd2e6884dc
# Cloudflare Tunnel sidecar for the currency-bot webhook. Pinned by tag and
# manifest-list digest; renovate-style bumps are a reviewed change here.
CLOUDFLARED_IMAGE=cloudflare/cloudflared:2026.10.7@sha256:4955524aeaa379e154023d5a027c250f7b35041a2e7e950cf13f144bc9b9c5ba
