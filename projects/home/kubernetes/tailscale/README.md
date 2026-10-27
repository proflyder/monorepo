# Tailscale Operator ingress

Текущий внешний вход после Proxmox cutover. Operator `1.104.2` работает в
namespace `tailscale`; Helm release `home-access` (namespace `monitoring`)
публикует:

- `pl-home.<tailnet>.ts.net/` → HAOS `:8123` через EndpointSlice;
- `/grafana/` → `monitoring-grafana`.

`pl-home` должен разрешаться в operator proxy с `tag:k8s`, а не в старый
host `tailscale serve`.

## Приватные значения

`chart/values.yaml` содержит только пустые placeholders. Реальные
`hostname` и `haosAddress` передаются из закрытого values-файла; OAuth
client secret — только в Kubernetes Secret `operator-oauth`, не в Git.

## Установка/обновление

```bash
export KUBECONFIG=/private/path/kubeconfig
export TAILSCALE_OAUTH_FILE=/private/path/oauth.json
./deploy-operator.sh
helm upgrade --install home-access ./chart -n monitoring \
  -f /private/path/access-values.yaml
```

`smoke-service.yaml`/`smoke-ingress.yaml` — временные проверки; удалить их
после acceptance. `check-*.sh` и `verify-tailnet.sh` — read-only проверки.

## Проверка

```bash
TAILNET_HOST=pl-home.example-tailnet.ts.net \
  ./verify-tailnet.sh
```

Требования: DNS `pl-home` → operator proxy, тег `tag:k8s`, HTTPS 200 на
`/` и `/grafana/api/health`. Host Tailscale на Proxmox пока не включён;
аварийный доступ к host — LAN/Ethernet.
