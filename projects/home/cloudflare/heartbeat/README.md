# home / Cloudflare heartbeat

Deployment wrapper for `solutions/heartbeat` — внешний dead man's switch дома
(Cloudflare Worker + KV, Cron Trigger `*/5 * * * *`). Сам Worker
project-agnostic; здесь только приватная конфигурация и деплой.

## Деплой

```bash
mise run admin:heartbeat
mise run admin:heartbeat -- --secrets   # один раз или при ротации секретов Worker
```

fnox (профиль `heartbeat`) материализует из OpenBao `secret/infra/cloudflare`
два env-файла (`workers/heartbeat_env`, `workers/cloudflare_workers_env`);
карта путей — `docs/SECRETS.md`. Ключи env: `CLOUDFLARE_ACCOUNT_ID`,
`CLOUDFLARE_API_TOKEN` (только Workers Scripts и KV аккаунта),
`KV_NAMESPACE_ID`, `PING_TOKEN`, `TELEGRAM_BOT_TOKEN`, `TELEGRAM_CHAT_ID`;
необязательные `WORKER_NAME`, `GRACE_SECONDS`, `REMIND_SECONDS`, `SITE_NAME`.

`wrangler.toml.example` — шаблон; `wrangler.generated.toml` рендерится из env и
удаляется после запуска. Wrangler закреплён точной версией в `deploy.sh`.
Секреты Worker передаются только через stdin.

Alertmanager пингует Worker: URL и тот же `PING_TOKEN` лежат в
`secret/k8s/monitoring/heartbeat` (`url`, `token`); после изменения —
`force-sync` ExternalSecret `alertmanager-telegram` в `monitoring`.

## Проверка

```bash
node --test solutions/heartbeat/src/*.test.js
bash -n projects/home/cloudflare/heartbeat/deploy.sh
```

После деплоя `GET /health` → `ok`, `POST /ping` без токена → `401`.
