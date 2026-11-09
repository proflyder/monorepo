# Домашний Kubernetes

Рабочая композиция: один K3s server/worker в Ubuntu VM `lab-k3s` на Proxmox,
собственный containerd, GigaAM/Whisper/Piper/Hermes, observability и
HTTPS ingress. HA Core, **Music Assistant** и **Matter Server** живут
в HAOS VM как add-ons. Pulsar и
BLE gateway остаются на станции. Версии зафиксированы в `versions.sh`.

- [Эксплуатация и восстановление](../../../docs/KUBERNETES-OPERATIONS.md).
- [Текущее состояние](../../../docs/STATE.md).
- [Сеть и порты](../../../docs/NETWORK.md).
- [Chart приложений](../../home-assistant/deploy/README.md).
- [Дашборды и мониторинг](../../home-assistant/observability/README.md).
- [HTTPS ingress Traefik + cert-manager](ingress/).
- [Nexus — кэш артефактов для CI](nexus/).
- [Flux — pull-based GitOps для Nexus](flux/).
- [OpenBao](openbao/).
- [External Secrets Operator](external-secrets/).
- [Hermes voice-proxy firewall](voice-firewall/).
- Intercom signaling (WebRTC call) — `intercom/`, деплой merge'ем в `develop`.

`/srv/k3s` находится на диске VM 200 GiB. K3s data-dir:
`/srv/k3s/runtime`. Новые observability PVC используют `local-retain`;
существующие данные приложений — static local PV с Retain. Это локальное
хранение, не репликация. VM `lab-k3s` — 12 GiB, `192.0.2.27`;
HAOS — 4 GiB, `192.0.2.28`; host Proxmox — `192.0.2.26`.
MA/Matter add-ons: `core_matter_server 9.2.0`, `d5369777_music_assistant 2.10.8`.

## Обновление

Нужны Helm repositories `prometheus-community`, `grafana-community`, `grafana`,
приватный kubeconfig и закрытые values. Kubernetes Secret `grafana-admin`
создаётся External Secrets Operator из OpenBao; значения не передаются через
командную строку.

```bash
export PROMETHEUS_PRIVATE_VALUES=/private/path/prometheus.yaml
mise run admin:observability
export RUNTIME_PRIVATE_VALUES=/private/path/runtime-values.yaml
./projects/home/kubernetes/apply-runtime.sh
```

Не выполнять параллельные Helm операции над одним release; rollback — в
runbook (`docs/KUBERNETES-OPERATIONS.md`).

Администрирование — по SSH или через закрытый tunnel. API/kubelet/node metrics
закрыты от обычного LAN; Grafana доступна по `https://grafana.home.example.com/`,
в LAN по `http://192.0.2.27:30300/`. HTTPS-имена `*.home.example.com`
обслуживает Traefik (`hostNetwork`, `80/443`, IP allow-list) с
wildcard-сертификатом Let's Encrypt от cert-manager; см. [ingress/](ingress/).
Вход снаружи — собственный VPN (ocserv, CT `102`); CI ходит группой `ci`
только до K3s `tcp/{443,6443}`. Аварийный доступ к host — LAN/Wi-Fi или
Ethernet. Реальные URL и credentials — в secrets.
