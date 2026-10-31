K3S_VERSION=v1.35.9+k3s1
HAOS_VERSION=18.4
PVE_VERSION=9.2.24
MATTER_ADDON_VERSION=9.2.0
MUSIC_ASSISTANT_ADDON_VERSION=2.10.8
# Hermes runtime images are built and pushed by the `hermes` CI unit
# (`projects/home-assistant/deploy/images/build-and-push.sh`). The chart values
# are private, so a successful build does not roll the live pod by itself:
# update the digest in the private runtime values and run apply-runtime.sh only
# after the voice circuit was checked against the new digest.
VOICE_PROXY_IMAGE=ghcr.io/proflyder/voice-proxy@sha256:238e0a0f3afe5f152e3d8b1de7a35452c016d68368e3bac24c5fde882869fca9
HERMES_IMAGE=ghcr.io/proflyder/hermes@sha256:09a542f3ae846026e466c0f0b4acf81522da12cf9cbb1bec22125ae068c9b7e6
PROMETHEUS_CHART_VERSION=92.14.0
LOKI_CHART_VERSION=20.0.2
ALLOY_CHART_VERSION=1.14.3
TRAEFIK_CHART_VERSION=42.2.0
CERT_MANAGER_CHART_VERSION=v1.21.3
HERMES_REVISION=bf0203a794a07aac3ff764972ab9087baa4e4f4e
