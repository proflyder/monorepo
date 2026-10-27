# Configs — снимки и источники

Это **снимки/конфиги**, не живые сервисы. `services/` с битыми symlink’ами на
удалённые Desktop-проекты удалён 2026-10-27.

| Что | Где сейчас |
|---|---|
| Quasar bootstrap source | `archive/legacy/alice-mitm-bootstrap/` |
| v3rt3p compose/prompts/switch | `archive/legacy/v3rt3p-stack/` |
| v3rt3p source repos | `archive/legacy/v3rt3p-stack/repos/` (см. `UPSTREAM.md`) |
| Quasar env stage1/stage2 | `projects/station/configs/alice-mitm*.env`, `secrets/legacy-alice-mitm/` |
| DNS/proxy scripts | `archive/legacy/alice-mitm-bootstrap/scripts/`, `projects/station/configs/` |
| Root/OTA applied files | `projects/station/configs/station-applied/` |
| HTTPS proxy generate | `projects/station/configs/stack/https-proxy-generate.sh` |
| LLM/TTS env | `projects/station/configs/stack.env` (содержит `LLM_API_KEY`; не публиковать) |

Env физически хранятся в `secrets/local/projects/station/configs/` от корня
репозитория; прежние `*.env` — игнорируемые локальные ссылки, режим целей 600.

## Важно

- `projects/station/configs/stack.env` — **секрет** (`LLM_API_KEY`); хранить 600/в secrets.
- `projects/station/configs/*.crt` — публичные части; приватные ключи — `secrets/certs/`.
- v3rt3p `.env` — `secrets/legacy-v3rt3p/.env` (600).
- Не запускать старый стек без отдельной задачи; DNS на Mac — единая точка
  отказа. Recovery — `docs/REBUILD.md` §4.
