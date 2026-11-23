# phone-push (home deployment)

FCM-доставка на телефон владельца (поиск телефона, уведомления). Flux
(`Kustomization/app-phone-push`, образ `PHONE_PUSH_IMAGE` из pins). Один
stateless Deployment в namespace `home-assistant`, ClusterIP `:8095`.
Наружу не публикуется: вызывающие — `mcp-gateway` (поиск, уведомления) и
scheduler (тихие sync-пуши об изменении сущностей, `POST /v1/push/sync`).

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

Звонок проверяется голосом: «<wake word>, где телефон?» — колонка отвечает
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

## Sync-пуши

`POST /v1/push/sync` с телом `{"entity","op","id","originDeviceId"}` шлёт
data-only FCM без уведомления: `{type:"sync", entity, op, id}`. `entity` —
`alarms|reminders|timers|scenarios|notifications`, `op` — `upsert|delete`;
содержимое
сущности в пуш не попадает, приложение забирает его само. Необязательный
`targetDeviceId` адресует пуш ровно одному телефону (так scheduler обновляет
историю её владельцу); если `targetDeviceId` совпадает с `originDeviceId`,
пуш не отправляется. Устройство из
`originDeviceId` (заголовок `Home-Device-Id` исходного запроса к scheduler)
пуш не получает. Scheduler вызывает endpoint после своих мутаций и тиков
движка, если задан `SCHEDULER_PUSH_URL`, с тем же токеном `scheduler-auth`.
История уведомлений пишется в scheduler после каждой доставки (egress
`phone-push → scheduler:8098` уже разрешён политикой), поэтому отдельного
хранилища у сервиса нет.

## Секреты и сеть

- `secret/k8s/home-assistant/scheduler-auth#token` → `phone-push#token`
  (общий токен приложения: его же предъявляет mcp-gateway и scheduler API).
- `secret/k8s/home-assistant/phone-push#service_account_json` →
  `phone-push#service_account_json` (FCM).
- NetworkPolicy: ingress только от `mcp-gateway`, scheduler и kubelet; egress —
  scheduler `:8098`, CoreDNS `:53`, HTTPS `:443` (OAuth/FCM).
- Откат — revert pins-коммита `PHONE_PUSH_IMAGE` или `flux suspend`
  Kustomization `app-phone-push`; своего состояния у сервиса нет.
