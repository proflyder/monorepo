# secrets-gateway (home deployment)

Owner-only API секретов приложения. Flux (`Kustomization/app-secrets-gateway`,
образ `SECRETS_GATEWAY_IMAGE` из pins). Один Deployment в namespace
`home-assistant`, ClusterIP `:8096`; PVC `secrets-gateway-state` хранит отметки
об использованных step-up nonce (не дольше 60 секунд). Владелец открывает его
через Traefik по `secrets.<HOME_DOMAIN>` (IngressRoute `secrets-gateway` в
`../../ingress/routes/routes.yaml`, middleware `home-allowlist`); путь вне
LAN/VPN закрыт. Вызывающие в кластере — Traefik, mcp-gateway (health-check).

## Что нужно от владельца (один раз)

Предварительно: `mise run secrets:login` (operator token на рабочем месте) и
доступ `mise run kubectl -- ...` (admin kubeconfig + туннель).

0. **DNS-запись.** Одна A-запись `secrets.<HOME_DOMAIN>` → LAN-адрес узла,
   Cloudflare DNS only (не proxied). Wildcard-сертификат её покрывает, Flux
   запись не создаёт. Один раз, вручную (владелец, не CI):

   ```bash
   mise run secrets:file home/vpn/cloudflare.env
   python3 projects/home/kubernetes/cloudflare_dns.py \
     secrets/home/vpn/cloudflare.env secrets.<home-domain> <node-lan-ip>
   mise run secrets:clean
   ```

   Те же записи перечислены в `projects/home/kubernetes/README.md`,
   `docs/NETWORK.md` и `docs/STATE.md`; `verify.sh` проверяет DNS и HTTPS.

1. **Политика и Kubernetes-auth роль.** Применяются тем же bootstrap, что и
   остальные политики OpenBao (файл политики — `policy.hcl` в этом каталоге):

   ```bash
   mise run admin:openbao-bootstrap
   ```

   Создаются `policy: secrets-gateway` и
   `auth/kubernetes/role/secrets-gateway` (SA `secrets-gateway` в namespace
   `home-assistant`, TTL 20m/1h, audience `secrets-gateway`, policy
   `secrets-gateway`). Root/operator token в под не попадают.

2. **Merge PR.** `deploy-home` публикует digest и Flux применяет
   `app-secrets-gateway`. Если merge случился раньше шага 1, под остаётся
   `Pending` (нет Secret-тома), а Kustomization `app-secrets-gateway` —
   NotReady и ретраится каждую минуту; другие приложения не зависят от неё и
   не ломаются. Scheduler-часть обратно совместима: owner получает `secrets`
   неявно, миграция не нужна.

3. **Синхронизировать ESO и дождаться пода.** ESO обновляет
   `ExternalSecret/secrets-gateway` раз в час; после merge форсировать:

   ```bash
   mise run kubectl -- -n home-assistant annotate externalsecret secrets-gateway \
     force-sync="$(date +%s)" --overwrite
   mise run kubectl -- -n home-assistant wait --for=condition=Ready \
     externalsecret/secrets-gateway --timeout=120s
   mise run kubectl -- -n home-assistant get secret secrets-gateway \
     -o go-template='{{range $k, $v := .data}}{{$k}}{{"\n"}}{{end}}'
   ```

   В Secret должен быть только `service_token`. Проверить, что под
   `secrets-gateway` Ready: `readyz` требует свежий snapshot доступа и
   доступный OpenBao.

4. **Установить новый APK.** `/v1/device/*` существует только после шага 2,
   поэтому APK из CI (`android-distribute`) ставится после rollout. Старый APK
   после merge не умеет step-up.

5. **Привязать телефон.** Одноразовый код создаётся в OpenBao, в файл 0600
   пишется только код; stdout печатает путь файла, не код:

   ```bash
   mise run admin:secrets-device-enroll
   ```

   Файл по умолчанию —
   `secrets/local/secrets-gateway/device-enroll.txt` (0600), срок — 15 минут.
   Открыть на телефоне «Система → Секреты → Привязать телефон», ввести код;
   приложение создаёт EC P-256 ключ в Android Keystore (StrongBox при наличии)
   и отправляет публичный ключ, цепочку key attestation и код. Сервер сверяет
   хеш кода (constant-time, одноразово, с backoff), сохраняет публичный ключ и
   device id и присылает push. После проверки файл удалить.

   Повторная привязка: новый код и снова «Привязать телефон» — старый ключ
   перестаёт приниматься сразу после успешной привязки. Отзыв без телефона:

   ```bash
   mise run admin:secrets-device-enroll -- --revoke
   ```

   Отзыв удаляет ключ из OpenBao; сервис читает его на каждый запрос, поэтому
   новые step-up и reveal прекращаются без рестарта пода (уже открытые
   5-минутные сессии тоже перестают проходить). Отзыв оставляет в OpenBao
   маркер, который сервис замечает опросом (не чаще раза в 30 секунд) и
   присылает один push «Отзыв телефона». Только после успешной привязки и
   проверки reveal/update удалить legacy-путь
   `secret/k8s/home-assistant/secrets-gateway`.

6. **Удалить legacy-состояние.** Файл последнего TOTP-шага больше не нужен:

   ```bash
   mise run kubectl -- -n home-assistant exec deploy/secrets-gateway -- \
     rm -f /state/last-totp-step
   ```

## Проверка после merge

```bash
mise run kubectl -- -n home-assistant get deploy secrets-gateway
mise run kubectl -- -n home-assistant logs deploy/secrets-gateway --tail=50
mise run kubectl -- -n home-assistant port-forward svc/secrets-gateway 8096:8096
```

Локально (owner app token, файл 0600 `secrets/local/app-tokens/owner.token`):

```bash
TOKEN="$(cat secrets/local/app-tokens/owner.token)"
curl -fsS http://127.0.0.1:8096/v1/secrets -H "Authorization: Bearer $TOKEN"
curl -fsS http://127.0.0.1:8096/v1/device -H "Authorization: Bearer $TOKEN"
```

`403 module_forbidden` — токен не owner; `502 openbao_forbidden` — роль/политика
не применены или pod не получил SA token. `GET /v1/device` показывает,
привязан ли телефон.

## Секреты и сеть

- `secret/k8s/home-assistant/scheduler-auth#service_token` → `service_token`
  (snapshot доступа и bearer в phone-push).
- `secret/k8s/home-assistant/secrets-gateway-device` — публичный ключ,
  device id, время привязки, хеш аттестации и хеш одноразового кода. Путь
  закрыт для HTTP API только проверкой в коде: `deny.json` его не покрывает,
  потому что из этого файла генерируется `policy.hcl`, а поду нужен доступ.
  Читается подом через роль `secrets-gateway`; ESO его не синхронизирует.
- `secret/k8s/home-assistant/secrets-gateway` — legacy TOTP-секрет. После
  перехода на ключ устройств его можно удалить; путь остаётся в `deny.json`,
  пока он существует.
- ConfigMap `secrets-gateway-config` собирается kustomize из
  `manifests/catalog.json` и `manifests/deny.json`; тест сверяет каталог с
  `platform/workstation/catalog.json`, а `policy.hcl` генерируется из того же
  `deny.json`.
- SA-токен проецируется с audience `secrets-gateway` (не дефолтный);
  `automountServiceAccountToken: false`.
- NetworkPolicy: ingress от Traefik, mcp-gateway (health-check) и узла
  (port-forward); egress — OpenBao `:8200`, scheduler `:8098`, phone-push
  `:8095`, DNS `:53`. Интернета нет.
- Откат — revert pins-коммита `SECRETS_GATEWAY_IMAGE` или `flux suspend`
  Kustomization `app-secrets-gateway`; ключ устройства остаётся в OpenBao, PVC
  `secrets-gateway-state` (использованные nonce) сохраняется и не удаляется
  Flux.
