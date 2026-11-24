# secrets-gateway (home deployment)

Owner-only API секретов приложения. Flux (`Kustomization/app-secrets-gateway`,
образ `SECRETS_GATEWAY_IMAGE` из pins). Один stateless Deployment в namespace
`home-assistant`, ClusterIP `:8096`. Наружу не публикуется: маршрут для
телефона и Android-экран — PR B. Вызывающие в кластере — Traefik (после
IngressRoute) и owner с узла через port-forward.

## Что нужно от владельца (один раз)

Предварительно: `mise run secrets:login` (operator token на рабочем месте) и
доступ `mise run kubectl -- ...` (admin kubeconfig + туннель).

1. **TOTP-секрет.** Идемпотентно создаёт `totp_secret` в
   `secret/k8s/home-assistant/secrets-gateway` и пишет otpauth URI в файл
   0600; URI и секрет не печатаются:

   ```bash
   mise run admin:secrets-totp-init
   # повторная ротация (старые коды перестают работать):
   mise run admin:secrets-totp-init -- --rotate
   ```

   Файл по умолчанию —
   `secrets/local/secrets-gateway/totp-otpauth.txt` (0600). Импортировать URI
   в приложение-аутентификатор; после проверки файл удалить. Ротация меняет
   секрет в OpenBao, поэтому после неё нужны force-sync ExternalSecret и
   рестарт пода (сервис читает TOTP при старте):

   ```bash
   mise run kubectl -- -n home-assistant rollout restart deploy/secrets-gateway
   mise run kubectl -- -n home-assistant rollout status deploy/secrets-gateway --timeout=180s
   ```

2. **Политика и Kubernetes-auth роль.** Применяются тем же bootstrap, что и
   остальные политики OpenBao (файл политики — `policy.hcl` в этом каталоге):

   ```bash
   mise run admin:openbao-bootstrap
   ```

   Создаются `policy: secrets-gateway` и
   `auth/kubernetes/role/secrets-gateway` (SA `secrets-gateway` в namespace
   `home-assistant`, TTL 20m/1h, policy `secrets-gateway`). Root/operator
   token в под не попадают.

3. **Синхронизировать ESO и дождаться пода.** ESO обновляет
   `ExternalSecret/secrets-gateway` раз в час; после init форсировать:

   ```bash
   mise run kubectl -- -n home-assistant annotate externalsecret secrets-gateway \
     force-sync="$(date +%s)" --overwrite
   mise run kubectl -- -n home-assistant wait --for=condition=Ready \
     externalsecret/secrets-gateway --timeout=120s
   mise run kubectl -- -n home-assistant get secret secrets-gateway \
     -o go-template='{{range $k, $v := .data}}{{$k}}{{"\n"}}{{end}}'
   ```

   В Secret должны быть `service_token` и `totp_secret`. Деплой — merge PR:
   `deploy-home` публикует digest и Flux применяет
   `app-secrets-gateway`.

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
```

`403 module_forbidden` — токен не owner; `502 openbao_forbidden` — роль/политика
не применены или pod не получил SA token.

## Секреты и сеть

- `secret/k8s/home-assistant/scheduler-auth#service_token` → `service_token`
  (snapshot доступа и bearer в phone-push).
- `secret/k8s/home-assistant/secrets-gateway#totp_secret` → `totp_secret`
  (step-up); путь закрыт для самого gateway политикой и кодом.
- NetworkPolicy: ingress только от Traefik и узла (port-forward); egress —
  OpenBao `:8200`, scheduler `:8098`, phone-push `:8095`, DNS `:53`. Интернета
  нет.
- Откат — revert pins-коммита `SECRETS_GATEWAY_IMAGE` или `flux suspend`
  Kustomization `app-secrets-gateway`; состояния у сервиса нет, TOTP-секрет
  остаётся в OpenBao.
