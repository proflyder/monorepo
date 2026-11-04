# platform

Общая платформенная обвязка: Bazel toolchains, CI-контракты, генераторы,
кросс-компиляция и правила для мобильных платформ.

- `platform/bazel/` — toolchains и общие Bazel-настройки.
- `platform/ci/` — карта юнитов (`units.mjs`), маршрутизация затронутых
  юнитов и `ci-ok` gate.
- `platform/quality/` — аудит репозитория, статические проверки, Gitleaks.
- `platform/deploy/` — `home-deploy` (Go): render/converge/rollout юнитов.
- `platform/android/` — Bazel-правила, lint и инструменты Android-сборки.
- `platform/observability/` — общие Helm values Prometheus/Loki/Alloy.

CI работает на GitHub-hosted runners, использует `.github/workflows/_*.yml`
и публикует обязательный `ci-ok`; деплой — `deploy-home.yml`. Подробности —
`docs/TESTING.md` и `docs/GIT.md`.
