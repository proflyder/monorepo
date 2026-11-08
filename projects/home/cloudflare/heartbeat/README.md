# home / Cloudflare heartbeat

Deployment wrapper for `solutions/heartbeat` — внешний dead-man switch дома.
Сам Worker project-agnostic; здесь только приватная конфигурация и деплой.

## Что нужно один раз

1. Создать Workers KV namespace и записать его `id`.
2. Создать Cloudflare API token с правами только на Workers Scripts и KV
   этого аккаунта.
3. Заполнить `secrets/home/heartbeat.env` (каталог уже в `.gitignore`):

   ```bash
   CLOUDFLARE_ACCOUNT_ID=<32 hex>
   CLOUDFLARE_API_TOKEN=<token>
   KV_NAMESPACE_ID=<32 hex>
   PING_TOKEN=<random token>
   TELEGRAM_BOT_TOKEN=<существующий инфра-бот>
   TELEGRAM_CHAT_ID=<чат владельца>
   # необязательные:
   # WORKER_NAME=home-heartbeat
   # GRACE_SECONDS=900
   # REMIND_SECONDS=21600
   # SITE_NAME=дом
   ```

4. Задеплоить Worker и затем один раз положить секреты:

   ```bash
   mise run admin:heartbeat
   mise run admin:heartbeat -- --secrets
   ```

5. Положить URL и тот же `PING_TOKEN` в OpenBao `secret/k8s/monitoring/heartbeat`
   (поля `url`, `token`) и применить Alertmanager:

   ```bash
   mise run admin:observability
   ```

`wrangler.toml.example` — только шаблон с плейсхолдерами. Реальный
`wrangler.generated.toml` рендерится из private env, содержит account/KV id и
удаляется после запуска. Wrangler закреплён точной версией в `deploy.sh`;
npm-зависимостей проекта нет.

URL Worker (`https://<worker>.<account-subdomain>.workers.dev/ping`) — не
секрет, но конкретный адрес хранится в private env/OpenBao, не в Git.
Cron Trigger `*/5 * * * *` уже в шаблоне.

## Проверка

```bash
node --test solutions/heartbeat/src/*.test.js
bash -n projects/home/cloudflare/heartbeat/deploy.sh
```

After deploy, `GET /health` returns `ok`; `POST /ping` without a token must be
`401`. Полный outage-тест (остановить Alertmanager/сеть и дождаться алерта) —
ручной, см. `docs/OPERATIONS.md`.
