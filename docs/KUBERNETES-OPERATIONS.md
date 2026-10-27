# Kubernetes: эксплуатация неттопа и восстановление

Внедрение: 2026-10-27. С 2026-10-28 это baseline до переустановки: Proxmox
уже установлен, восстановление VM/runtime идёт; текущие факты — STATE.md.
Исходное исследование —
[PLAN-KUBERNETES-MIGRATION.md](PLAN-KUBERNETES-MIGRATION.md).
Адреса и пользователь в публичных примерах обезличены.

## Состав и владение

- Один K3s `v1.35.9+k3s1`, node `lab-host`, Ubuntu сохранена.
- Namespace `home-assistant`: восемь Deployments, девять контейнеров:
  HA/MA/Matter/GigaAM/Whisper/Piper, Hermes gateway+proxy и health exporter.
- Namespace `monitoring`: Prometheus/Alertmanager/Grafana, Operator,
  kube-state-metrics/node-exporter, Loki и Alloy.
- Все release versions — `projects/home/kubernetes/versions.sh`.
- Приложения сохраняют исходные versions/digests и hostNetwork/loopback ports.
  HA privileged/D-Bus сохранены как в предыдущем рабочем Compose.
- Hermes: исходный revision сохранён, собственный OCI image с API extras,
  tini и SQLite 3.53.4. Gateway+proxy в одном hostNetwork Pod.
- SSH, Wi-Fi/netplan, chrony, Tailscale и K3s остаются host services.
  Это намеренная инфраструктура восстановления, не незавершённая миграция.

## Данные и секреты

`/srv/k3s` — ext4 LV `ubuntu-vg/k3s-data`, 150 GiB; около 224 GiB VG оставлено
нераспределёнными. `/srv/k3s/runtime` — K3s data-dir, собственный containerd,
cluster datastore и local-path provisioner. `/srv/k3s/volumes/hermes` — новый
writable Hermes home, в Pod `/opt/data`. `/srv/k3s/recovery` — закрытые backups.

HA/MA/Matter и модели первого переноса используют прежние каталоги в
`/home/labuser/nettop/homeassistant/config` и `nettop/data` через static local
PV/PVC. **Эти каталоги активны, их нельзя удалить как «остатки Docker».**
Все application PV и observability StorageClass имеют reclaimPolicy Retain.
Размер local PV не является файловой квотой; контролировать реальные FS.

Секреты Hermes — `Secret/hermes-environment`; refreshable auth — в writable
Hermes home. Grafana credentials — `Secret/grafana-admin`. K3s secrets encryption
включено, kubeconfig имеет режим 600. Значения не хранятся в tracked values.
Реальная LAN-конфигурация/пути — только в private values.

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

Grafana: существующий HTTPS Tailscale-адрес HA + **`/grafana/`**.
Главный `/` по-прежнему ведёт в HA. Login `admin`, password — в приватном файле
выше; в чат/документацию значение не копировать. Loki/Prometheus не опубликованы.

С хоста:

```bash
~/nettop/ctl.sh status
~/nettop/ctl.sh logs ha
~/nettop/ctl.sh logs hermes gateway
~/nettop/ctl.sh restart piper
~/nettop/ctl.sh doctor
sudo k3s kubectl get pods -A
sudo k3s kubectl get pvc -A
sudo k3s kubectl top nodes
```

`~/nettop/ctl.sh` теперь Kubernetes wrapper из `projects/home/kubernetes/ctl.sh`.
Старые Docker `up/down/pull/update/backup` через него не поддерживаются.
`~/.local/bin/hermes` теперь выполняет CLI в gateway container; можно проверять
`hermes mcp test homeassistant` и `hermes mcp test music-assistant`.
Не выполнять native `hermes gateway install/start/update` на хосте: lifecycle
принадлежит Kubernetes, обновление — через image/chart.

С Mac для kubectl/Helm нужен SSH tunnel, пока kubeconfig указывает на localhost:

```bash
ssh -N -L 127.0.0.1:16443:127.0.0.1:6443 labuser@192.0.2.25
export KUBECONFIG=/private/path/kubeconfig
kubectl get pods -A
```

Для диагностики внутри SSH tunnel или на хосте можно сделать port-forward:

```bash
kubectl -n monitoring port-forward svc/monitoring-grafana 13000:80
kubectl -n monitoring port-forward svc/monitoring-prometheus 19090:9090
kubectl -n monitoring port-forward svc/loki 13100:3100
```

Port-forward привязан к конкретному Pod и может потребовать перезапуска после
rollout/reboot. Он не является постоянным способом публикации сервиса.

Tailscale `/grafana` proxy направлен на Service ClusterIP Grafana с **`/grafana`**
в upstream path; без этого префикса login попадал бы в redirect loop.
При удалении/создании самого Service или восстановлении кластера обновить IP:

```bash
GRAFANA_SERVICE_IP=$(sudo k3s kubectl -n monitoring get svc monitoring-grafana -o jsonpath='{.spec.clusterIP}')
sudo tailscale serve --bg --yes --https=443 --set-path=/grafana \
  "http://$GRAFANA_SERVICE_IP:80/grafana"
```

Не выполнять `tailscale serve reset`: он удалит и HA route. Private Grafana
`server.root_url` должен совпадать с настоящим Tailscale URL и `/grafana/`.

## Сеть и автозапуск

Node IP сейчас закреплён за Wi-Fi `wlp4s0`; Ethernet был DOWN при переносе.
При переходе на Ethernet согласованно обновить private K3s node-ip/flannel-iface,
host firewall и Matter PRIMARY_INTERFACE, затем отдельно проверить discovery.
Автоматический failover между NIC не обещается.

`ricardo-k3s-firewall.service` ограничивает INPUT на 6443/10250/9100/9116 и
8472/udp; localhost, pod CIDR и сам node разрешены. С Mac проверено, что
управляющие/metrics TCP-порты недоступны напрямую в LAN. SSH/Tailscale и прежние
HA/MA/voice endpoints сохранены. Не включать UFW поверх этих правил без проверки.

Swap `/swap.img` выключен, его fstab entry закомментирован с сохранённым оригиналом.
K3s резервирует по 1 GiB system/kube памяти; CRI log rotation 10Mi × 3.
LV монтируется через UUID в fstab. Старые native Hermes user units masked.

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
