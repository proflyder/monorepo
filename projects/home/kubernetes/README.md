# Домашний Kubernetes

Один K3s server/worker в Ubuntu VM на Proxmox: GigaAM/Whisper/Piper/Hermes,
scheduler и HA-исполнители, intercom, OpenBao/ESO, mcp-gateway, Nexus,
observability и HTTPS ingress. HA Core, Music Assistant и Matter Server живут в
HAOS VM, Pulsar и BLE gateway — на станции. Версии chart'ов и образов —
`versions.sh`; факты (адреса, ресурсы, версии add-ons) — `docs/STATE.md`, сеть —
`docs/NETWORK.md`, эксплуатация и восстановление —
`docs/KUBERNETES-OPERATIONS.md`.

## Кто что применяет

- **Flux** (`flux/`): платформа (ESO → cert-manager → Traefik → маршруты),
  observability, Vaultwarden, Nexus, pve-exporter и все юниты приложений
  (образы — из ветки pins). Меняется только merge'ем в `develop`; ручной
  `kubectl apply`/Helm над этими объектами Flux откатит.
- **Вручную**: `openbao/`, host-firewall (`management-firewall/`,
  `voice-firewall/`, `scheduler/nft/`), Cloudflare DNS и storage chart'а
  `runtime`.

| Каталог | Что |
|---|---|
| [flux/](flux/) | GitOps, pins, приватные значения Flux |
| [openbao/](openbao/) | секрет-сервер |
| [external-secrets/](external-secrets/) | ESO и `ClusterSecretStore/openbao` |
| [ingress/](ingress/) | Traefik + cert-manager, общие маршруты |
| `observability/` | kube-prometheus-stack, Loki, Alloy, правила; дашборды — `projects/home-assistant/observability/` |
| [vaultwarden/](vaultwarden/), [nexus/](nexus/) | менеджер паролей, кэш артефактов CI |
| [scheduler/](scheduler/), [mcp-gateway/](mcp-gateway/), [intercom/](intercom/), [phone-push/](phone-push/), [wakegrant/](wakegrant/), [currency-bot/](currency-bot/), [ha-config-sync/](ha-config-sync/), [pve-exporter/](pve-exporter/) | юниты приложений |
| [management-firewall/](management-firewall/), [voice-firewall/](voice-firewall/) | nftables на узле K3s |

Chart Hermes (`runtime`) — `projects/home-assistant/deploy/README.md`.

## Ручные инструменты

Storage chart'а `runtime` (static PV/PVC) Flux не применяет:

```bash
RUNTIME_PRIVATE_VALUES=/private/path/runtime-values.yaml \
HEALTH_EXPORTER_IMAGE=ghcr.io/proflyder/health-exporter@sha256:… \
KUBECONFIG=… ./projects/home/kubernetes/apply-runtime.sh
```

DNS-only A-запись для приватного имени (`ha`, `grafana`, `alarms`, `control`,
`currency`, `intercom`, `secrets`, `passwords`, `vault`, `nexus`):

```bash
mise run secrets:file home/vpn/cloudflare.env
python3 projects/home/kubernetes/cloudflare_dns.py \
  secrets/home/vpn/cloudflare.env <host>.<home-domain> <node-lan-ip>
mise run secrets:clean
```

`ctl.sh` — статус/логи на самом узле (`sudo k3s kubectl`).

## Ограничения

- `/srv/k3s` — локальный диск VM (data-dir `/srv/k3s/runtime`); PVC —
  `local-retain` или static local PV с Retain. Это не репликация.
- API/kubelet/node metrics закрыты от обычного LAN (`management-firewall/`);
  доступ — `mise run kubectl` (SSH-туннель) или VPN.
- Не выполнять параллельные Helm-операции над одним release.
