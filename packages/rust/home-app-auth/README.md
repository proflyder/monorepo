# home-app-auth

Общая проверка app-токенов с модулями: каталог модулей, `AccessSnapshot`
(sha256 токенов и их модули), `AccessCache` и `AccessClient` для сервисов,
которые читают snapshot у scheduler (`GET /v1/access/snapshot` service-токеном).

## Заметки к коду

- `AccessCache::new` — авторитетный кэш scheduler, без срока годности.
  `AccessCache::remote`/`default` — кэш потребителя: считается свежим
  `MAX_SNAPSHOT_AGE` (5 мин) после последнего успешного обновления, дальше
  все app-запросы отклоняются (fail-closed). 5 минут — 10 интервалов
  `REFRESH_INTERVAL` (30 с): переживает рестарт scheduler, но ограничивает
  жизнь отозванного токена.
- Каждое успешное обновление заменяет snapshot целиком, даже с меньшей
  ревизией: источник правды — scheduler (восстановление БД из бэкапа).
- `AccessClient` обновляет snapshot singleflight: параллельные запросы с
  неизвестным токеном ждут одно обновление; обновление по требованию не
  чаще раза в секунду, пока кэш свежий.
- `Modules::parse` строгий (admin API → `400`), `Modules::parse_lenient`
  игнорирует неизвестные id (snapshot и seed пишут `warn`).
- `AccessClient::fixed` — клиент без сети для тестов потребителей.
- `snapshot_url` срезает необязательный суффикс `/home.v1.SchedulerService`.
