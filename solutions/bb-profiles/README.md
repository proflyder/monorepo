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
- `null` (команда `all`) означает «без ограничения». Так устроены все профили
  без явной записи, поэтому включение плагина ничего не блокирует.
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
CI-юнит `bb-profiles` (`platform/ci/units.yml`) запускает те же
`tests/*.test.js` через `node --test`.

## Раскатка на сервер bb

Плагин установлен как локальный path-плагин. bb хранит состояние по id
(`<bb-data-dir>/plugins/profiles/`) отдельно от исходников, поэтому обновление
содержимого той же папки и `bb plugin reload` сохраняют профили, привязки и
allowlist. `bb plugin install path:<new dir>` для того же id тоже переносит
источник с сохранением настроек; `bb plugin remove` — нет.

1. Собрать и проверить новую версию на рабочей машине:

   ```bash
   cd solutions/bb-profiles
   npm install
   npm test
   bb plugin build
   ```

2. Сделать резервную копию текущей папки плагина на сервере (только исходники,
   не `<bb-data-dir>`):

   ```bash
   cp -a "$HOME/bb-plugins/bb-plugin-profiles" \
         "$HOME/bb-plugins/bb-plugin-profiles.backup"
   ```

3. Обновить тот же путь — предпочтительный вариант, id и kv не меняются:

   ```bash
   # скопировать новые исходники и dist/ в существующую папку
   rsync -a --delete --exclude node_modules --exclude .git \
     solutions/bb-profiles/ "$HOME/bb-plugins/bb-plugin-profiles/"
   cd "$HOME/bb-plugins/bb-plugin-profiles"
   npm install
   bb plugin build
   bb plugin reload profiles
   ```

   Либо установить из нового каталога — bb перенесёт path-источник и сохранит
   настройки:

   ```bash
   bb plugin install path:"$HOME/bb-plugins/bb-plugin-profiles-new"
   ```

4. Проверить:

   ```bash
   bb plugin list
   bb plugin logs profiles -n 50
   bb profiles list
   bb profiles show
   ```

5. Откат: вернуть содержимое из `bb-plugin-profiles.backup`, выполнить
   `bb plugin reload profiles`. Состояние в `<bb-data-dir>/plugins/profiles/`
   при этом не затрагивается.

Не менять `bb.name`, имя пакета и id; не выполнять `bb plugin remove` ради
обновления; не копировать `node_modules` и `dist/` в Git.
