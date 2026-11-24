# home-app-auth

Общая проверка app-токенов с модулями: каталог модулей, `AccessSnapshot`
(sha256 токенов и их модули), `AccessCache` и `AccessClient` для сервисов,
которые читают snapshot у scheduler (`GET /v1/access/snapshot` service-токеном).

## Заметки к коду

- `AccessCache::new` — авторитетный кэш scheduler, без срока годности.
  `AccessCache::remote`/`default` — кэш потребителя: считается свежим
  `MAX_SNAPSHOT_AGE` (1 ч) после последнего успешного обновления, дальше
  все app-запросы отклоняются (fail-closed). Час переживает долгую раскатку,
  перенос пода или PVC без отказа звонков и управления; цена — отозванный
  токен живёт у потребителей до часа, если scheduler всё это время недоступен
  (при живом scheduler отзыв доходит за 30 с).
- Каждое успешное обновление заменяет snapshot целиком, даже с меньшей
  ревизией: источник правды — scheduler (восстановление БД из бэкапа).
- `AccessClient` обновляет snapshot singleflight: параллельные запросы с
  неизвестным токеном ждут одно обновление, пока кэш свежий; обновление по
  требованию не чаще раза в секунду. При протухшем кэше запрос не ждёт
  чужое обновление: если оно уже идёт, сразу `401`.
- `Modules::parse` строгий (admin API → `400`), `Modules::parse_lenient`
  игнорирует неизвестные id (snapshot и seed пишут `warn`).
- `secrets` и `admin` — owner-only модули: `Module::owner_only` и
  `Principal::allows` отказывают не-owner'у, даже если id оказался в его
  списке. `Modules::all_but_admin` (legacy «Мама») не содержит owner-only
  модулей; issuance проверяет scheduler.
- `AccessClient::fixed` — клиент без сети для тестов потребителей.
- `snapshot_url` срезает необязательный суффикс `/home.v1.SchedulerService`.
