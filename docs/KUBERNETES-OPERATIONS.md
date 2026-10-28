# Kubernetes: эксплуатация неттопа и восстановление

Внедрение: 2026-10-27. С 2026-10-28 runtime работает внутри Proxmox VM,
а не на bare-metal Ubuntu; текущие факты — STATE.md. Исходное исследование —
[PLAN-KUBERNETES-MIGRATION.md](PLAN-KUBERNETES-MIGRATION.md).
Адреса и пользователь в публичных примерах обезличены.

## Состав и владение

- Proxmox VE host `192.0.2.26`; VM `101 lab-k3s` — Ubuntu 24.04.5,
  K3s `v1.35.9+k3s1`, node `lab-host`, `192.0.2.27`.
- VM `100 haos` — HAOS `2026.10.4`, 4 GiB, HA Core/Supervisor, `192.0.2.28`;
  add-ons Matter Server `9.2.0` и Music Assistant `2.10.8`.
  HA больше не Deployment в K3s.
- VM `101 lab-k3s` — 8 GiB; namespaces `home-assistant`: GigaAM/Whisper/
  Piper, Hermes gateway+proxy и health exporter. MA/Matter Deployments,
  PV и data удалены из K3s; rollback-копии — на Mac. HAProxy `haos-proxy`
  удалён; HAOS доступен напрямую `.28:8123`.
- Namespace `monitoring`: Prometheus/Alertmanager/Grafana, Operator,
  kube-state-metrics/node-exporter, Loki и Alloy.
- Namespace `tailscale`: Operator и ingress proxy; `pl-home` → HAOS,
  `/grafana` → Grafana.
- Все release versions — `projects/home/kubernetes/versions.sh`.
- Приложения сохраняют исходные versions/digests и hostNetwork/loopback ports.
  HAOS использует отдельные Supervisor data; HA Core state больше не в
  `runtime-ha` PVC (PVC сохранён, но не является текущим writer).
- Hermes: revision `bf0203a794a07aac3ff764972ab9087baa4e4f4e`, собственный
  OCI image, tini, SQLite 3.53.4; gateway+proxy в одном hostNetwork Pod.
- SSH/netplan/chrony/Tailscale/K3s — в K3s VM; Proxmox host management —
  отдельный SSH/Ethernet. Host Tailscale на PVE сейчас не запущен.

## Данные и секреты

`/srv/k3s` — путь внутри VM `lab-k3s` на диске 200 GiB. `/srv/k3s/runtime` —
K3s data-dir, собственный containerd, cluster datastore и local-path
provisioner. `/srv/k3s/volumes/hermes` — writable Hermes home, в Pod
`/opt/data`. `/srv/k3s/recovery` — закрытые recovery-файлы внутри VM.

STT/TTS и Hermes используют Retain local PV/PVC. `runtime-music` и
`runtime-matter` удалены после переноса MA/Matter в HAOS; их tar-копии лежат
на Mac. `runtime-ha` PVC сохранён как архив прежнего HA Container. **Не удалять старые PV и каталоги
`/srv/k3s/volumes/*` без отдельной проверки.** Все application PV и
observability StorageClass имеют reclaimPolicy Retain.

Секреты Hermes — `Secret/hermes-environment`; refreshable auth — в writable
Hermes home. Grafana credentials — `Secret/grafana-admin`. K3s secrets encryption
включено, kubeconfig имеет режим 600. Значения не хранятся в tracked values.
Terraform state — root-only на PVE (`/var/lib/ricardo-terraform/`) и копия
в `secrets/`; не коммитить. Реальная LAN-конфигурация/пути — только в
private values.

Локальный Mac, вне Git:

```text
secrets/kubernetes-research/kubeconfig
secrets/kubernetes-research/runtime-values.yaml
secrets/kubernetes-research/prom-private.yaml
secrets/kubernetes-research/grafana-password
secrets/kubernetes-research/nettop-baseline.tar.gz
secrets/kubernetes-research/recovery/
```

В `recovery/` проверены SHA-256 трёх файлов: `cluster-and-auth.tar.gz`,
`custom-images.tar`, `pre-cutover.tar.gz`. Они содержат реальные секреты и
не предназначены для публикации. Копия на Mac защищает от потери неттопа,
но не является географически независимым/зашифрованным offsite backup.

## Доступ и обычные команды

Grafana: `https://pl-home.<tailnet>.ts.net/grafana/` через Tailscale
Operator и LAN NodePort `http://192.0.2.27:30300/grafana/`; `/` ведёт в
HAOS. Login `admin`, password — в приватном файле; в чат/документацию
значение не копировать. Loki/Prometheus не опубликованы.

Внутри K3s VM:

```bash
sudo k3s kubectl get pods -A
sudo k3s kubectl get pvc -A
sudo k3s kubectl top nodes
```

Старый `~/nettop/ctl.sh` и Docker `up/down/pull/update` больше не являются
рабочим lifecycle. Hermes CLI запускается в gateway container; lifecycle —
Helm/Kubernetes, обновление — через image/chart. Не выполнять native
`hermes gateway install/start/update` в VM.

С Mac для kubectl/Helm нужен SSH tunnel к K3s VM:

```bash
ssh -N -L 127.0.0.1:16443:127.0.0.1:6443 labuser@192.0.2.27
export KUBECONFIG=/private/path/kubeconfig
kubectl get pods -A
```

HAOS доступен напрямую `http://192.0.2.28:8123`; compatibility proxy удалён.
Hermes voice proxy доступен HAOS на `.27:8643`; nftables разрешает только
HAOS, K3s node и loopback. Установка правила —
[`voice-firewall/`](../projects/home/kubernetes/voice-firewall/).


Для диагностики внутри SSH tunnel или на хосте можно сделать port-forward:

```bash
kubectl -n monitoring port-forward svc/monitoring-grafana 13000:80
kubectl -n monitoring port-forward svc/monitoring-prometheus 19090:9090
kubectl -n monitoring port-forward svc/loki 13100:3100
```

Port-forward привязан к конкретному Pod и может потребовать перезапуска после
rollout/reboot. Он не является постоянным способом публикации сервиса.

Tailscale публикует не host Serve, а Kubernetes Ingress `home`:
`/` → `home-assistant-external` (EndpointSlice на HAOS `.28:8123`),
`/grafana` → `monitoring-grafana`. Helm release — `home-access` в namespace
`monitoring`; chart — `projects/home/kubernetes/tailscale/chart/`.
При смене HAOS IP обновить private `haosAddress` и применить chart; при
смене Grafana Service Helm/Kubernetes DNS обновит backend автоматически.
`pl-home` должен указывать на operator proxy с `tag:k8s`, а не на host Serve;
проверка — `projects/home/kubernetes/tailscale/check-*.sh` и
`curl` из tailnet.

## Сеть и автозапуск

Node IP `192.0.2.27` закреплён за виртуальным Ethernet `eth0` K3s VM.
Физический uplink `vmbr0` на Proxmox выбирается между Ethernet и Wi-Fi
4-address/WDS; внутри VM интерфейсы/IP не меняются. Wi-Fi остаётся на host,
PCI passthrough и NAT не используются. Инструкция:
[`wifi-uplink/`](../projects/home/proxmox/wifi-uplink/).
Не включать оба физических uplink одновременно в мост.
Matter теперь HAOS add-on и не использует K3s `PRIMARY_INTERFACE`; старые
K3s Matter values сохранены только как rollback. MA также HAOS add-on.
K3s VM — 8 GiB, HAOS VM — 4 GiB.

Исходники и установка: [`management-firewall/`](../projects/home/kubernetes/management-firewall/).

`ricardo-k3s-firewall.service` ограничивает INPUT на 6443/10250/9100/9116 и
8472/udp; localhost, pod CIDR и сам node разрешены. С Mac проверено, что
управляющие/metrics TCP-порты недоступны напрямую в LAN. HAOS/voice endpoints
HAOS — `.28:8123`; Hermes voice proxy — `.27:8643`, ограничен nftables
только HAOS/K3s/loopback. Matter/MA add-ons — в HAOS (`127.0.0.1:5580`,
`.28:8095`). Не включать UFW поверх этих правил без проверки.

Swap в K3s VM — `/swap.img` выключен; на Proxmox host swap 8 GiB. K3s
резервирует по 1 GiB system/kube памяти; CRI log rotation 10Mi × 3.
Диски VM — local-lvm; host root 64 GiB, local-lvm около 379 GiB.

## Обновление приложений и дашбордов

[Composition scripts](../projects/home/kubernetes/README.md),
[application chart](../projects/home-assistant/deploy/README.md),
[dashboard chart](../projects/home-assistant/observability/README.md).

Перед upgrade сделать актуальный backup и сравнить версии/image digests.
Не менять модели, API и storage schema одновременно с patch infrastructure.
Один Deployment = одна replica, Recreate: запрещены второй writer и параллельный
native/Docker запуск. Helm rollback не откатывает SQLite или auth/state.

Dashboard JSON редактируется в проекте и доставляется Helm ConfigMap sidecar’ом;
не требуется `kubectl cp`/ручной import. Datasource UIDs `prometheus`/`loki`
и dashboard UIDs стабильны. Custom metrics label `endpoint` не конфликтует
с автоматически добавляемым Prometheus label `service`.

Images Hermes/proxy пока локальные, `imagePullPolicy: Never`. При новом узле
или потере containerd сначала импортировать recovery OCI archive:

```bash
sudo k3s ctr images import --platform linux/amd64 /private/path/custom-images.tar
```

Новые images собирать из pinned source на builder с target `linux/amd64`,
проверять aiohttp/SQLite и реальный chat/SSE, затем импортировать или публиковать
в выбранный private registry. Публичный registry в этом внедрении не создавался.

## Backup и восстановление

Разделять cluster state, прикладные данные и собственные images.
Сохранить конфигурацию K3s, cluster SQLite snapshot, server token и cred/tls.
Без исходного server token восстановление datastore неработоспособно.
Прикладные PVC не входят в cluster DB. `etcd-snapshot` не используется: datastore SQLite.

Сделанные snapshots K3s/Hermes DB получены через SQLite backup API, проверен
`integrity_check`. Перед каждым application cutover была cold-копия его данных.
Pre-cutover baseline/cold архивы сохраняют прежние данные. Актуальный Hermes
state/auth добавлен отдельно в `cluster-and-auth.tar.gz` после переноса профиля.
Не заменять свежий auth старой копией: refresh tokens могли обновиться.

Для новых регулярных backups нужен отдельный защищённый процесс с уведомлением
о freshness и restore rehearsal. Сейчас сохранён migration recovery-комплект,
а не настроенный offsite backup SLA. Не удалять эти данные ради очистки Git.

При потере хоста:

1. Восстановить Ubuntu/network/DKMS/MOK из существующего rebuild runbook.
2. Создать/mount storage, установить **ту же** K3s version; остановить K3s перед
   восстановлением datastore. Не запускать приложения на пустых каталогах.
3. Восстановить SQLite cluster snapshot, token, конфигурацию и ключи в исходную
   структуру data-dir с закрытыми правами. Проверить node identity/IP и TLS SAN.
4. Восстановить данные application PV **до** их запуска. Для Hermes использовать
   актуальные auth/state, в которых profile references уже `/opt/data`.
5. Импортировать custom images, запустить K3s, проверить volumes/namespace/Secret.
6. Проверить endpoints и Tailscale routes, затем voice/music/alarm tests.

Полное восстановление на запасном неттопе в этом внедрении не выполнялось;
проверены архивы/checksums/SQLite integrity и перезапуск существующего хоста.

## Возврат к прежнему runtime

Recovery на неттопе: `/var/backups/k3s-migration` и `/srv/k3s/recovery`.
Нативные Compose/env/ctl и Hermes home сохранены отдельно от активных Kubernetes
данных. Возврат требует остановить соответствующие Pods и дождаться их удаления,
установить прежний runtime и только затем запускать старые services.

Для native Hermes прежний home имеет старые profile_id. Если переносится новая
история обратно, нужен обратный offline rebase на отдельной копии DB. Не удалять
разговоры и не подменять весь свежий state прежним ради обхода HTTP 409.
Исходная версия и миграционная копия DB сохранены.

Root `nettop/homeassistant/config` и `nettop/data` остаются текущими PV:
они не должны быть удалены при очистке старых deployment-файлов.

## Проверки и ограничения

Пройдены Helm lint/render contracts, unit test переноса profile metadata,
`make test`, HA API/local intents/schedule/alarm/stop/resume, MA HTTP audio →
станция, Puriso → GigaAM/Whisper, Hermes real chat + SSE/persona, HA/MA MCP,
Grafana dashboard provisioning, Prometheus endpoints, Loki/Alloy synthetic
redaction и закрытие LAN admin ports.

Audio test использовал малую громкость и восстановил source/volume; это
проверка Sendspin/MA и alarm state machine, не повторный тест Spotify account
search/remote Connect. Микрофонное распознавание wake word пользователем заново
не выполнялось: Pulsar на станции не менялся. Matter server/reconnect проверены;
новые Matter devices не спаривались. Внешний канал alerts и полная voice latency
instrumentation пока не настроены. Нет второго узла и распределённого storage.
