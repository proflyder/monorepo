# bb-profiles

Плагин bb для изолированных профилей работы и личного: отдельные логины,
конфиги, память и история провайдеров на проект, плюс allowlist провайдеров и
машин.

Плагин не хранит секреты: профиль — это только пути и имена переменных
окружения. Провайдеры запускаются на машине треда; сервер bb может жить на
другом хосте.

## Что делает

- Профиль — набор env-переменных по провайдеру (`claude-code`, `codex`, `pi`,
  `acp-cursor`, `opencode`, `*`). `*` применяется ко всем провайдерам профиля.
- Проект привязан к одному профилю. Env применяется через
  `bb.providers.experimental_contributeEnv` со следующего сообщения.
- Allowlist провайдеров (`allowed`) и машин (`machines`) проверяется хуком
  `message.dispatch` до отправки сообщения провайдеру.
- `null` (команда `all`) означает «без ограничения». Профиль без записи в
  `allowed`/`machines` не ограничен. Исключение — свежая установка без ключа
  `allowed`: тогда действует `work → ["claude-code"]`.
- Профили по умолчанию (пока нет ключа `profiles`): `work` без env (обычные
  `~/.claude`, `~/.codex`) и `personal` с `CLAUDE_CONFIG_DIR` и
  `GH_CONFIG_DIR` в `~/.bb-profiles/personal/`. Профиль по умолчанию для
  непривязанных проектов — настройка `defaultProfile` (`work`).
- Боковая панель показывает только проекты активного профиля; страница
  **Profiles** назначает профили и создаёт проекты.

## Команды

```bash
bb profiles list
bb profiles show [project-id]
bb profiles create <name>
bb profiles use <project-id> <profile>
bb profiles env <profile> <provider|*> <NAME> <value>
bb profiles unset <profile> <provider|*> <NAME>
bb profiles providers <profile> <id,id,...|all>
bb profiles machines <profile> <hostId,...|all>
```

`bb profiles machines` проверяет hostId через `bb.sdk.hosts.list()`; `all`
снимает ограничение. Если список машин недоступен, обновление проходит с
предупреждением, что id не проверены.

### Чтение политики из других плагинов

RPC-контракт плагина (`profiles`):

- `allowedProviders({ profile })` → `string[] | null`
- `allowedMachines({ profile })` → `string[] | null`
- `profilePolicy({ profile })` → `{ providers: string[] | null, machines: string[] | null }`

`null` означает «все разрешены». Неизвестный профиль — отказ `not_found`.

## Хранилище

Состояние лежит в kv плагина `<bb-data-dir>/plugins/profiles/`:

| Ключ | Значение |
|---|---|
| `profiles` | профиль → провайдер → env-переменные |
| `assignments` | projectId → профиль |
| `allowed` | профиль → список провайдеров или `null` |
| `machines` | профиль → список hostId или `null` |
| `active` | активный профиль в интерфейсе |
| `lastThreads` | профиль → последний тред (навигация) |
| `hiddenManaged` | группы, которые плагин скрыл в списке тредов |

**Не менять id плагина (`profiles`), имя пакета (`bb-plugin-profiles`) и
формат этих ключей.** Живые данные должны пережить обновление. Команда
`bb plugin remove profiles` удаляет настройки и состояние — для обновления она
не нужна.

### profileEnv и раздельные сервер/провайдер

Новые профили создаются с путями относительно домашнего каталога машины
провайдера: `~/.bb-profiles/<profile>/...`. Абсолютные значения, сохранённые
раньше, не переписываются и работают как есть.

При запуске провайдера `~` раскрывается в домашний каталог целевой машины из
настройки `hostHomes`:

```bash
bb machines list                            # host id каждой машины
bb plugin config profiles set hostHomes '{"<hostId>":"/home/<user>"}'
```

Пустая настройка означает домашний каталог сервера bb — это верно только для
all-in-one установки. На раздельной установке (сервер на Linux, треды на Mac)
`hostHomes` нужно заполнить, иначе новые профили получат путь относительно
домашнего каталога сервера. Каталоги на самом сервере плагин создаёт только
для путей, лежащих в его собственном `~/.bb-profiles`; каталог на удалённой
машине создаёт провайдер или оператор.

### Выбор папки

`pickFolder` использует нативный диалог macOS (`osascript`). На не-macOS
сервере кнопка **Choose…** скрыта, путь вводится вручную; RPC возвращает
понятную ошибку вместо падения.

## Разработка и проверки

```bash
cd solutions/bb-profiles
npm install          # только для сборки и типов; node_modules не коммитить
npm test             # node --test tests/*.test.js, без внешних секретов
bb plugin build      # dist/ собирается локально и не коммитится
```

Тесты — чистые ES-модули из `lib/policy.js`, без bb SDK и npm-зависимостей.
CI-юнит `bb-profiles` (`platform/ci/units.mjs`) запускает те же
`tests/*.test.js` через `node --test`.

## Раскатка на сервер bb

Плагин установлен как локальный path-плагин. Состояние bb хранит по id
(`<bb-data-dir>/plugins/profiles/`) отдельно от исходников, поэтому
обновление той же папки сохраняет профили, привязки и allowlist;
`bb plugin install path:<new dir>` для того же id тоже их сохраняет,
`bb plugin remove` — нет.

```bash
cp -a "$HOME/bb-plugins/bb-plugin-profiles" "$HOME/bb-plugins/bb-plugin-profiles.backup"
rsync -a --delete --exclude node_modules --exclude .git \
  solutions/bb-profiles/ "$HOME/bb-plugins/bb-plugin-profiles/"
cd "$HOME/bb-plugins/bb-plugin-profiles" && npm install && bb plugin build
bb plugin reload profiles
bb plugin logs profiles -n 50 && bb profiles list
```

Откат: вернуть содержимое из `bb-plugin-profiles.backup` и выполнить
`bb plugin reload profiles`; состояние в `<bb-data-dir>` не затрагивается.
