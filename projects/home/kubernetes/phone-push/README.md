# phone-push (home deployment)

Один stateless Deployment в namespace `home-assistant`, ClusterIP `:8095`.
Наружу не публикуется: единственный вызывающий — `mcp-gateway`.

## Что нужно от владельца (один раз)

1. **Сервис-аккаунт FCM.** В GCP-проекте Firebase (тот же, что у
   `google-services.json`) создать сервис-аккаунт, например
   `phone-push-fcm`, и выдать ему роль с правом
   `cloudmessaging.messages.create`:

   - либо предустановленную **Firebase Cloud Messaging API Admin**
     (`roles/firebasecloudmessaging.admin`);
   - либо кастомную роль ровно с `cloudmessaging.messages.create`
     (плюс `cloudmessaging.messages.send`, если политика требует).

   Минимально достаточно `cloudmessaging.messages.create`; `firebase.admin`
   и `owner` не нужны.

2. **JSON-ключ** этого сервис-аккаунта положить в OpenBao:

   ```text
   path:  secret/k8s/home-assistant/phone-push
   field: service_account_json   # весь JSON одной строкой
   ```

   Значения не печатать и не класть в Git: ESO материализует Secret
   `phone-push` (`token` берётся из `scheduler-auth`, `service_account_json`
   — из этого пути). Запись — GET → merge → POST с `options.cas`
   (`docs/SECRETS.md`).

3. **Firebase Cloud Messaging API** включить в том же проекте (обычно уже
   включён, если Firebase используется).

4. Ничего не нужно добавлять в Remote Config телефона: приложение
   регистрирует FCM-токен на уже настроенном адресе scheduler'а и тем же
   bearer-токеном.

## Проверка после merge

```bash
mise run kubectl -- -n home-assistant get deploy phone-push
mise run kubectl -- -n home-assistant logs deploy/phone-push --tail=50
# реестр устройств (токены не печатать):
mise run kubectl -- -n home-assistant port-forward svc/phone-push 8095:8095
```

Звонок проверяется голосом: «Рикардо, где телефон?» — колонка отвечает
«Звоню на телефон», телефон звонит как будильник и останавливается кнопкой
«Нашёл» или через минуту. Отдельная проверка без телефона (в кластере,
токен — `scheduler-auth#token`):

```bash
curl -fsS -X POST "$PHONE_PUSH_URL/v1/push/find" \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"reason":"проверка"}'
# {"sent":1,"failed":0,"devices":1}
```

`sent: 0, devices: 0` — телефон ещё не зарегистрировался: открыть приложение
и экран «Уведомления и поиск телефона» → «Обновить регистрацию».

## Секреты и сеть

- `secret/k8s/home-assistant/scheduler-auth#token` → `phone-push#token`
  (общий токен приложения: его же предъявляет mcp-gateway и scheduler API).
- `secret/k8s/home-assistant/phone-push#service_account_json` →
  `phone-push#service_account_json` (FCM).
- NetworkPolicy: ingress только от `mcp-gateway` и kubelet; egress —
  scheduler `:8098`, CoreDNS `:53`, HTTPS `:443` (OAuth/FCM).
- Откат — revert pins-коммита `PHONE_PUSH_IMAGE` или `flux suspend`
  Kustomization `app-phone-push`; своего состояния у сервиса нет.
