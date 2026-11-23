# OPERATIONS — ежедневные команды

Факты — `STATE.md`; K3s — `KUBERNETES-OPERATIONS.md`; сеть — `NETWORK.md`;
секреты и доступы — `SECRETS.md`, `WORKSTATION.md`. Подробные команды
модуля — в его README (таблица ниже); здесь только общие точки входа и то,
чего нет в README.

## Единая точка входа: `mise tasks`

`mise tasks` перечисляет локальные задачи. Деплоимые юниты и процедура отката
живут в `platform/ci/units.mjs` (`deploy` и `rollback`), список —
`mise run units`.

```bash
# пересчитать и запинить digest'ы (gh workflow run deploy-home-manual.yml
# --ref develop -f units=…); применяет Flux, в кластер из CI ничего не катится
mise run deploy scheduler --dry-run
mise run deploy scheduler,ha-bridge --yes --watch
mise run deploy all --yes

mise run deploy:status scheduler          # последние запуски deploy-home
mise run deploy:status scheduler --live   # + read-only kubectl get deploy

mise run rollback scheduler --dry-run     # план из units.mjs
mise run rollback scheduler --yes         # выполнить документированную команду
```

`with-kube-tunnel.sh` отказывается работать без явного `KUBECONFIG` и
проверяет, что его сервер — `https://127.0.0.1:16443`; поэтому голый
`kubectl` и `~/.kube/config` не используются. `--live` и `rollback --yes` берут admin kubeconfig из OpenBao через fnox
(профиль `kube`) и сами поднимают туннель к API. `rollback` для `forward-only`
завершается ненулевым кодом, для `pulsar` только печатает команду для
станции.

Бэкапы — реестр `platform/ops/backups.mjs` (раздел «Backup»):

```bash
mise run backup:list
mise run backup:status [<id>] [--dry-run]
mise run backup:now <id> --dry-run|--yes
mise run backup:verify [<id>] --dry-run|--yes
```

`backup:now` выполняет только документированную команду цели; цель только
через UI (HA backup) печатает `manual`. Restore не автоматизирован: вывод
содержит `restore_doc`.

App-токены (модули доступа) и service-token:

```bash
# 1. до деплоя: создать service_token, если его ещё нет (идемпотентно, молча)
mise run admin:service-token-init

# 2. после деплоя: bootstrap owner; токен owner пишется в файл 0600
mise run admin:app-token-bootstrap
# путь к owner-токену печатается; stdout сам токен не содержит
```

Bootstrap генерирует новый `service_token`, сохраняет старый как
`service_token_previous`, делает force-sync и wait **всех** ESO-потребителей
(`scheduler-auth`, `phone-push`, `mcp-gateway-scheduler`, `intercom`),
перезапускает scheduler и сервисы, проверяет фактический `service_token` в Kubernetes Secrets и в поде
scheduler (`-service-token-sha256`), затем удаляет `service_token_previous` и снова
перезапускает scheduler. Если ожидаемый deployment не найден или Secret
содержит старый токен, задача падает до финализации. Legacy `token` никогда
не становится service-token и после bootstrap остаётся только app-токеном
«Мама» (все модули, кроме `admin`).

Preflight до ротации читает k8s Secrets/ConfigMaps в `home-assistant`, кроме
производных ESO-секретов (`scheduler-auth`, `phone-push`,
`mcp-gateway-scheduler`, `intercom`), и останавливается, если находит
legacy-значение. Аварийный обход — `--skip-preflight`.

Внешние потребители, которые bootstrap не обновляет:

- HA `secrets.yaml` → `scheduler_bearer` (`rest_command.scheduler_rpc`);
- Hermes/voice-proxy private values → `HERMES_SCHEDULER_TOKEN[_FILE]` из
  `scheduler-auth#service_token` (`runtime-values`/private Helm values);
- локальные admin-файлы: `secrets/home/scheduler.token` (service-token),
  `secrets/home/app.token` (legacy app-token для verify/ingress) и
  `fnox`-профиль `ingress`;
- seed сценариев, `scheduler/deploy.sh` и разовые `SCHEDULER_TOKEN_FILE`
  на рабочей машине;
- ручные e2e/отладочные окружения.

Скрипты с секретами — только `mise run admin:*` (`mise tasks | grep admin:`),
kubectl/helm — `mise run kubectl|helm -- …`. Разовая команда с kubeconfig:

```bash
mise run secrets:exec -- -P kube exec -- \
  platform/workstation/with-kube-tunnel.sh kubectl get pods -A
```

## Где команды модулей

| Тема | Документ |
|---|---|
| VPN (ocserv, клиентские сертификаты, WireGuard, проброс портов) | `projects/home/proxmox/vpn/README.md` |
| BB server (CT 103), плагины, backup базы | `projects/home/proxmox/bb/README.md` |
| OpenBao | `projects/home/kubernetes/openbao/README.md` |
| External Secrets Operator | `projects/home/kubernetes/external-secrets/README.md` |
| Flux, pins, первая установка | `projects/home/kubernetes/flux/README.md` |
| HTTPS ingress, Cloudflare DNS | `projects/home/kubernetes/ingress/README.md` |
| Vaultwarden | `projects/home/kubernetes/vaultwarden/README.md` |
| ha-config-sync (доставка ha-voice) | `projects/home/kubernetes/ha-config-sync/README.md` |
| MCP-шлюз, голосовой маршрут, мост bb | `projects/home/kubernetes/mcp-gateway/README.md` |
| Планировщик, ha-bridge, ha-scenario, сид сценариев | `projects/home/kubernetes/scheduler/README.md` |
| Intercom | `projects/home/kubernetes/intercom/README.md` |
| wakegrant | `projects/home/kubernetes/wakegrant/README.md` |
| Currency bot | `projects/home/kubernetes/currency-bot/README.md` |
| Hermes / runtime | `projects/home-assistant/deploy/README.md`, `KUBERNETES-OPERATIONS.md` |
| Голос, фразы, профиль | `projects/home-assistant/ha-voice/README.md` |
| Комп (Windows-ПК) | `projects/home/windows-pc/README.md` |
| Модуль входа (logon provider) | `solutions/windows-logon-provider/README.md` |
| Android, App Distribution, ручной релиз | `projects/mobile/android/README.md` |
| BLE gateway | `projects/station/ble-gateway/README.md` |
| Wake-события и модель | `projects/station/tools/wakeword-personalize/README.md`, `WAKE-WORD.md` |
| Offsite backup | `projects/home/backup/offsite/README.md` |
| Тесты | `TESTING.md` |

## Доступ

```bash
adb connect 192.0.2.13:5555
ssh -i ~/.ssh/home-lab/station_ed25519 root@192.0.2.13   # пароль — fallback
mise run station:audit -- --ip 192.0.2.13                # read-only сверка

ssh -F ~/.ssh/home-lab/config pve|k3s|haos               # после secrets:pull
```

HA `http://192.0.2.28:8123`, MA `:8095`, Grafana LAN
`http://192.0.2.27:30300/`. HTTPS (LAN/VPN):
`https://{ha,grafana,alarms,control,currency,intercom,vault,passwords,nexus}.home.example.com/`.
Реальные адреса — `secrets/NETWORK.md`. Не пробрасывать на роутере
`8123/8095/8099/6053/6638` и `80/443`.

CT 102 (VPN): команды, которые перезапускают ocserv/nftables/ssh/сеть, —
только отсоединённо (`pct exec 102 -- systemd-run --unit=<name> <команда>`,
затем `journalctl -u <name>`): синхронный запуск рвёт связь владельцу,
сидящему через ocserv `full`.

## Проверка

```bash
mise run test                # hermetic; стенд не трогает
mise run test:e2e            # live HA/BLE read-only; нужны HA_URL, HA_TOKEN
STATION_E2E_MUTATE=1 mise run test:e2e
STATION_E2E_AUDIO=1 mise run test:e2e
```

```bash
mise run kubectl -- get kustomizations -A
mise run kubectl -- -n flux-system get configmap home-app-images -o yaml   # pins
mise run kubectl -- -n flux-system annotate kustomization/<name> \
  reconcile.fluxcd.io/requestedAt="$(date +%s)" --overwrite
mise run admin:ingress-verify
```

Откат приложения — не `kubectl rollout undo`: `flux suspend` Kustomization,
revert pins-коммита в `deploy/home-apps`, `resume` (`mise run rollback <unit>`
печатает план). `app-ha-executors` — один Kustomization на scheduler,
`ha-bridge`, `ha-control`, `ha-scenario`; режимы и реплики — в Git, учения —
через `flux suspend` + `deploy.sh` (README scheduler). `app-hermes` —
`HelmRelease/runtime` с `remediation.strategy: rollback`; аварийный путь —
`projects/home-assistant/deploy/rollout.sh`.

Не запускать второй writer для HA/MA/SQLite: один Deployment — одна replica,
Recreate; Helm rollback state не откатывает.

## Внешний heartbeat (dead-man switch)

`Watchdog` Alertmanager пингует Cloudflare Worker `solutions/heartbeat`
(контракт — его README). Приватные значения —
`secrets/home/heartbeat.env` (`CLOUDFLARE_ACCOUNT_ID`, `CLOUDFLARE_API_TOKEN`,
`KV_NAMESPACE_ID`, `PING_TOKEN`, `TELEGRAM_BOT_TOKEN`, `TELEGRAM_CHAT_ID`);
Alertmanager читает URL и токен из OpenBao `secret/k8s/monitoring/heartbeat`.

```bash
node --test solutions/heartbeat/src/*.test.js
mise run admin:heartbeat                 # деплой Worker
mise run admin:heartbeat -- --secrets    # + секреты Worker
mise run kubectl -- -n monitoring annotate externalsecret alertmanager-telegram \
  force-sync="$(date +%s)" --overwrite
```

Ротация `PING_TOKEN`: env → `admin:heartbeat -- --secrets` → запись в
OpenBao. Смена Telegram-бота — то же плюс
`secret/k8s/home-assistant/hermes-environment`, чтобы Alertmanager и Hermes
остались на одном боте. Outage-тест — silence только на `Watchdog`:

```bash
mise run kubectl -- -n monitoring exec alertmanager-monitoring-alertmanager-0 -c alertmanager -- \
  amtool silence add alertname=Watchdog --duration=25m \
  --comment="heartbeat outage test" --alertmanager.url=http://127.0.0.1:9093
```

Через `GRACE_SECONDS` + cron-тик приходит «дом молчит»; после истечения
silence первый ping даёт recovery.

## Станция / Pulsar

```bash
adb -s 192.0.2.13:5555 shell '/data/pulsar/ctl.sh status'
adb -s 192.0.2.13:5555 shell '/data/pulsar/ctl.sh log 100'
adb -s 192.0.2.13:5555 shell '/data/pulsar/ctl.sh restart'
adb -s 192.0.2.13:5555 shell '/data/pulsar/ctl.sh disable'  # вернуть maind
adb -s 192.0.2.13:5555 shell '/data/pulsar/ctl.sh enable'
adb -s 192.0.2.13:5555 shell '/data/pulsar/pulsar --version'
```

Откат на `pulsar.prev` — только на станции, не через CI (updater предлагает
лишь более новые версии); атомарно, не поверх работающего файла:

```bash
adb -s 192.0.2.13:5555 shell 'cd /data/pulsar && cp -p pulsar.prev pulsar.rollback && mv pulsar.rollback pulsar && /data/pulsar/ctl.sh reload'
```

`mise run rollback pulsar` печатает эту команду, станцию не трогает.

Локальный endpointing (экспериментальный) включается только переменной
процесса: `PULSAR_LOCAL_ENDPOINTING=1 /data/pulsar/ctl.sh reload`; reload без
переменной выключает. Не boot-persistent; `deploy.py` модели перезапускает
Pulsar без неё.

При возврате на stock (`ctl.sh disable` или crash-loop) снова откроется
`quasar-server :31116` без auth — перед возвратом закрыть порт правилом INPUT.
StationCancel без речи — HA-конфиг, не бинарник.

Модель wake word меняется отдельно от бинарника; текущая и профили отката —
`STATE.md`. Откат на профиль (подставить LAN-адрес HA):

```bash
mise run secrets:exec -- -P station exec -- \
  python3 projects/station/tools/wakeword-personalize/deploy.py \
  --ha-url http://192.0.2.28:8123 \
  --profile projects/station/configs/wakeword-v27.json
```

После рестарта Pulsar MA переподключается сам (~3 с). Если
`media_player.iandeks_stantsiia_midi` остаётся `unavailable` — один рестарт
add-on MA (`hassio.addon_restart`, `addon: d5369777_music_assistant`), без
рестарта HA.

```bash
projects/station/scripts/station-status.sh
projects/station/scripts/restore-ota-fortress.sh   # осторожно, откат
```

### Релиз и деплой Pulsar

Полная процедура — `projects/station/pulsar/RICARDO.md`, «Релизы и деплой»;
версия — решение 60. Кратко:

1. Обычный PR, squash после зелёного `ci-ok`; `VERSION` руками не правится.
   PR публикует `ghcr.io/proflyder/pulsar-firmware:tree-<hash>` с
   provenance; merge продвигает этот digest на `<version>` и `latest`.
2. Approve в Environment `Station` → `deploy-station` (VPN группа `ci`,
   сертификат `ci-pulsar`, OIDC-роль OpenBao `deploy-station`, секрет
   `secret/ci/deploy-station`) → HA `update_entity` + `update.install`.
   Job ждёт `installed_version == version` и 90 с стабильности.
3. Повтор уже опубликованного `latest` — `mise run deploy pulsar --yes`.
4. Сломанный релиз — «Пересобрать и задеплоить сервисы» с галочкой `pulsar`.

Sync upstream — патчем, см. `AGENTS.md` («Вложенные upstream-репозитории»).

## Egress / карантин станции

```bash
adb -s 192.0.2.13:5555 shell 'iptables -S EGRESS_GUARD; ip6tables -S EGRESS_GUARD6'
adb -s 192.0.2.13:5555 shell /data/quasar/egress-guard.sh
adb -s 192.0.2.13:5555 shell /data/quasar/egress-guard-off.sh
adb -s 192.0.2.13:5555 shell /data/quasar/yandex-block.sh
```

Guard разрешает LAN/DHCP/NTP и при `ALLOW_GITHUB=1` (по умолчанию) `tcp/443`
к диапазонам GitHub из `GITHUB_NETS` в скрипте (для updater'а); остальной
WAN и Яндекс — DROP. Снятие guard не снимает Яндекс-блок/OTA.

## Артефакты GHCR и CI-релизы

Сборка один раз на PR, после merge только promotion (решение 36); механика,
PR-head fallback и карта воркфлоу — `DECISIONS.md` и `TESTING.md` (CI).
Профилактика уехавшей базы — `mise run pr:merge <N>` (`GIT.md`).

Честная пересборка: Actions → «Пересобрать и задеплоить сервисы»
(`deploy-services.yml`) на `develop`. Галочки групп — `platform/ci/rebuild.mjs`,
поле `units` — точные id. `replace_artifacts` позволяет новым native-байтам
заменить опубликованный `tree-<hash>` и version-тег.

Что приехало: summary job'а `pins <unit>` в прогоне `deploy-home`, configmap
`home-app-images`, для Hermes — `mise run helm -- -n home-assistant history runtime`.

Воспроизводимость Hermes (две сборки без кэша → один digest):

```bash
projects/home-assistant/deploy/images/hermes/verify-reproducible.sh
```

Обновление Hermes — bump `projects/home-assistant/deploy/images/hermes/revision`.

Уборка — `_cleanup.yml` (джобы `ghcr` и `fad`), триггеры `cleanup.yml`
(ежедневно) и `cleanup-manual.yml` (`dry_run` по умолчанию `true`). Политика —
в коде скриптов; локальный dry-run на фикстурах:

```bash
node platform/ci/ghcr_cleanup.mjs \
  --fixture platform/ci/testdata/ghcr-versions.json --now 2026-11-04T00:00:00Z
node platform/ci/fad_cleanup.mjs \
  --fixture platform/ci/testdata/fad-releases.json --now 2026-11-04T12:00:00Z
```

`ghcr` без права удаления требует секрет `GHCR_CLEANUP_TOKEN`; `fad` — роль
Firebase App Distribution Admin у сервис-аккаунта и OIDC-роль OpenBao
`android-cleanup` (`configure.py`, применяется `mise run admin:openbao-bootstrap`).

## Reboot

```bash
ssh -F ~/.ssh/home-lab/config pve 'qm reboot 100'   # HAOS
ssh -F ~/.ssh/home-lab/config pve 'qm reboot 101'   # K3s
```

После reboot проверить HA, K3s pods, Grafana, HTTPS ingress, Sendspin и
Pulsar. Аварийный доступ к PVE — LAN/Wi-Fi.

## Backup

Все цели — `mise run backup:list` (реестр `platform/ops/backups.mjs`).
Proxmox: ночной vzdump `nightly` (04:00), storage `backup`.

```bash
ssh -F ~/.ssh/home-lab/config pve 'pvesh get /cluster/backup; pvesm list backup | tail'
ssh -F ~/.ssh/home-lab/config pve 'vzdump 101 --storage backup --mode snapshot --compress zstd'
ssh -F ~/.ssh/home-lab/config pve 'qmrestore /mnt/pve/backup/dump/<archive> <new-vmid> --storage local-lvm'
```

Проверочный restore — в новый VMID без сети (`--unique`, отключить `net0`),
не поверх живой VM. HA full backup — HA UI/Supervisor (automatic encrypted
backup 03:30, 3 копии). K3s — `KUBERNETES-OPERATIONS.md`, `REBUILD.md`. Не
удалять: `/srv/k3s/volumes/*`, `/srv/k3s/recovery`, `secrets/`, `backups/`.

### Offsite backup (Google Drive + restic)

K3s-узел, `offsite-backup.timer` 04:45 → restic `rclone:gdrive:home-offsite-v2`
(scope `drive.file`: rclone видит только файлы своего OAuth client).
Устройство, лимиты, retention и restore —
`projects/home/backup/offsite/README.md`.

```bash
mise run admin:offsite-install
mise run admin:pve-backup-install
mise run backup:status offsite-restic
mise run backup:verify offsite-restic --yes
```

Приватные входы — OpenBao `secret/infra/backup` (`offsite/*`, fnox-профиль
`offsite`); файлы `home/offsite-rclone.conf` и
`home/offsite-gdrive-client.env` — через `mise run secrets:file`
(каталог — `SECRETS.md`). Новый
OAuth-токен: `rclone authorize drive <base64 JSON {"client_id","client_secret","scope":"drive.file"}>`,
собрать `[gdrive]` в `offsite-rclone.conf`, переустановить. Смена client =
новый репозиторий (drive.file не видит файлы другого client).
