# Подключить Pulsar к Spotify, не сломав голос

**Да, это реализуемо — но не на самой колонке, а на неттопе.** Pulsar не получит Spotify-клиент: плагин `Spotify Connect` в Music Assistant 2.10.8 с движком Soloist запустит Connect-демон, который представит universal player `up77eb9a80` («Яндекс Станция Миди») как обычную Spotify-колонку, а звук уйдёт по уже работающему Sendspin-пути в Pulsar. Телефон сможет выбирать колонку в Spotify, переключать на неё и обратно, а метаданные, очередь и транспорт останутся в ведении Spotify; MA/HA получит title/artist/album/artwork/position и сможет проксировать pause/next; громкость в `player_only` останется за MA/Pulsar. Ограничение не в протоколе, а во владении одним активным источником: Connect-источник в MA эксклюзивен, поэтому голосовой `play_media` («включи любимое») вытеснит Spotify-сессию, а при одном аккаунте Connect и Spotify music provider делят единственный stream-slot. Пока Connect активен, пауза, следующий трек, громкость и TTS должны работать (это проверяется тестами T6/T7/T11); «продолжи» после паузы требует отдельной проверки, а «включи любимое» — паузы/стопа Connect перед запуском или второго Spotify-аккаунта. Рекомендуемая архитектура — **E1**: сохранить music provider для голоса и библиотеки, добавить Connect как «гостевой режим» телефона и соблюдать правило «один источник за раз»; **E2** со вторым аккаунтом нужен только для независимой одновременной Spotify-сессии на разных выходах. Всё обратимо: отключение плагина возвращает текущее поведение, Pulsar, HA и ESPHome не меняются.

Метки по тексту: `[подтверждено]` — локальная проверка или первичный источник; `[вывод из кода]` — чтение Music Assistant 2.10.8 / Pulsar v0.4.1; `[не проверено]` — эмпирики в этой лаборатории нет. Снимок состояния: 2026-10-26, MA 2.10.8, HA 2026.10.4, Pulsar v0.4.1. Данные сняты read-only; в HA/MA/Pulsar ничего не менялось.

## Короткий ответ: да, через мост на неттопе и с одним активным источником

**Если понимать вопрос буквально — «receiver внутри прошивки Pulsar» — ответ «нет».** Это вариант B: он требует ослабить egress-guard станции, поднять на armhf аудиосервер и решить ALSA/ducking-конфликты; ниже он разобран и отклонён. Если понимать вопрос как «колонка должна появиться в Spotify и играть с телефона» — ответ «да», и дальше речь именно об этом.

**Условия, без которых ответ превращается в «нет».** Нужен рабочий Soloist API key (он уже используется music-провайдером, но в проекте его plaintext-копии нет) ([STATE.md](file:///Users/developer/Desktop/yandex-station-lab/docs/STATE.md)); MA должен остаться на `network_mode: host`, иначе mDNS и произвольный порт демона не заработают ([docker-compose.yml](file:///Users/developer/Desktop/yandex-station-lab/projects/home-assistant/stack/docker-compose.yml)); телефон должен быть в той же L2-сети, что и неттоп, без guest VLAN и AP isolation; и должно быть принято продуктовое правило «Spotify-приложение и MA-очередь не играют одновременно на одной колонке». Последнее — не техническое ограничение MA, а свойство Spotify Connect: активным может быть только одно устройство, а MA не арбитрирует два источника за вас ([MA: Spotify Connect](https://www.music-assistant.io/plugins/spotify-connect/)).

**Рекомендуемая архитектура.** Connect-демон живёт внутри MA на неттопе; Pulsar остаётся Sendspin-эндпоинтом и не меняется. Целевой плеер — universal player `up77eb9a80`, а не Sendspin protocol player `fbp82cJ5eC3MlmMM8DHb_SWT6aYGNwXHPMZLum42iFc` и не ESPHome-сущность `media_player.iandeks_stantsiia_midi_media_player`: protocol-плееры вообще не попадают в список «Connected players», а ESPHome-сущность — это другой путь, который при transport-командах заставляет Pulsar выйти из Sendspin-группы ([config_entries.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/helpers/config_entries.py); [pulsar-coexistence.md](file:///Users/developer/Desktop/yandex-station-lab/research_notes/Spotify%20Connect%20на%20колонке%20Pulsar/pulsar-coexistence.md)).

| Вопрос | Короткий ответ |
|---|---|
| Колонка появится в Spotify? | Да, на той же L2; первое сопряжение — из Spotify-приложения. Вне домашней сети — не обещается. |
| Телефон → колонка и обратно? | Да, это штатный Connect-transfer; телефон остаётся пультом. |
| Метаданные и состояние в приложении? | Да, Spotify — источник истины; MA получает трек, позицию, play/pause, shuffle/repeat, но не зеркалит очередь. |
| Голос не сломается? | По коду пауза, следующий/предыдущий, громкость и TTS работают, но это надо подтвердить тестами; «продолжи» после паузы — проверить; «включи любимое» и любой `play_media` — вытеснение Connect. |
| Полностью бесконфликтно? | Нет: любой `play_media` — смена источника. Управляемо — политикой, скриптом или вторым аккаунтом. |
| Один аккаунт или два? | Для сценария «телефон → колонка, голос управляет паузой» достаточно одного; для независимой одновременной Spotify-сессии — два. |
| Откат? | Отключить плагин; Pulsar/HA/ESPHome не затрагиваются. |

Ожидаемый тракт:

```text
Spotify app (phone, same L2)
  → Spotify Connect (mDNS + cloud session) → Soloist daemon в MA-контейнере на неттопе
  → PCM 44.1 kHz / 32 bit → приватный PulseAudio pipe-sink → FIFO
  → MA streams controller (ffmpeg, -readrate) → MA Sendspin :8927
  → Pulsar v0.4.1 Sendspin :8928 → ALSA dmix → динамики

Голос: Pulsar wake word → HA Assist → intent scripts → MA universal player
       (pause/play/stop — на MA-сущность; громкость — на ESPHome-сущность)
```

Spotify WAN-трафик идёт с неттопа, а не со станции; egress-guard Pulsar, который режет публичный WAN и разрешает private LAN, при этой схеме не трогается ([STATE.md](file:///Users/developer/Desktop/yandex-station-lab/docs/STATE.md); [SECURITY.md](file:///Users/developer/Desktop/yandex-station-lab/docs/SECURITY.md)).

Здесь важно развести два смысла «не конфликтовать». Если требуется, чтобы **любая** голосовая команда работала одновременно с Spotify-сессией, ответ «нет»: `play_media` не может играть параллельно на том же плеере, а Spotify-аккаунт не даёт два активных потока. Если требуется **предсказуемый handoff** — Spotify забирает колонку, голос уступает или забирает обратно, TTS не рвёт сессию, — ответ «да», и именно это настраивается ниже.

## Почему колонка видна в Music Assistant, но отсутствует в Spotify

**В MA Pulsar существует как плеер, а не как Spotify-устройство.** Pulsar v0.4.1 — Sendspin-эндпоинт: он слушает `ws://<ip>:8928/sendspin`, анонсируется как `_sendspin._tcp`, принимает поток от MA и пишет PCM в ALSA `speakers` ([sendspin_service.h](https://github.com/Morvex885/pulsar/blob/v0.4.1/satellite/src/sendspin/sendspin_service.h); [sendspin_service.cpp](https://github.com/Morvex885/pulsar/blob/v0.4.1/satellite/src/sendspin/sendspin_service.cpp)). MA регистрирует два объекта: protocol-плеер Sendspin `fbp82…` и universal player `up77eb9a80` «Яндекс Станция Миди»; HA-сущность `media_player.iandeks_stantsiia_midi` имеет `unique_id = up77eb9a80`, то есть указывает именно на universal player ([pulsar-coexistence.md](file:///Users/developer/Desktop/yandex-station-lab/research_notes/Spotify%20Connect%20на%20колонке%20Pulsar/pulsar-coexistence.md)). В снимке лога MA видно, как universal player маршрутизирует звук в Sendspin: `Setting active output protocol on Яндекс Станция Миди to Sendspin` ([pulsar-coexistence.md](file:///Users/developer/Desktop/yandex-station-lab/research_notes/Spotify%20Connect%20на%20колонке%20Pulsar/pulsar-coexistence.md)). Для MA это полноценный output; для Spotify — нет. `[подтверждено]`

**Spotify видит устройства в двух независимых плоскостях.** Первая — локальная: mDNS-сервис `_spotify-connect._tcp`, `SRV` с IP/портом и `TXT CPath=/...`; телефон вызывает `http://<ip>:<port>/<path>?action=getInfo` и показывает устройство, только если получил валидный JSON ([Spotify: ZeroConf API](https://developer.spotify.com/documentation/commercial-hardware/implementation/guides/zeroconf)). Вторая — аккаунтная: после логина устройство появляется в реестре устройств Spotify и может быть видно даже при сбое mDNS, если тот же пользователь уже залогинен и в приложении, и на устройстве ([Spotify: Commercial Hardware FAQ](https://developer.spotify.com/documentation/commercial-hardware/implementation/faqs)). У Pulsar нет ни `_spotify-connect._tcp`, ни HTTP-обработчика `getInfo`, ни Spotify-сессии, ни аудиорендерера Spotify; он умеет только принимать Sendspin-поток от MA. Вдобавок egress-guard станции не пускает её в публичный WAN, так что даже гипотетический локальный receiver не смог бы авторизоваться и дотянуться до CDN без ослабления политики ([SECURITY.md](file:///Users/developer/Desktop/yandex-station-lab/docs/SECURITY.md)). Поэтому Pulsar отсутствует в обеих плоскостях Spotify — и это ожидаемо, а не баг. `[подтверждено]` для контракта Spotify; `[вывод из кода]` для возможностей Pulsar. Официальные источники Spotify при этом не полностью согласованы: consumer-support допускает видимость залогиненного устройства из другой сети, а MA docs для своего плагина требуют одну сеть; для стороннего MA-устройства принимаем консервативное правило same-L2.

**Текущий Spotify-путь — это «MA играет, Pulsar выводит», а не «Pulsar — Spotify-устройство».** Music-провайдер Spotify использует Web API для каталога и библиотеки, а Soloist — для звука; его capacity равна одному потоку аккаунта ([spotify/README.md](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify/README.md)). В режиме `--single-track` Soloist не анонсирует Connect-устройство и отключает remote control/transfer для своего контекста; при этом MA может показываться в Spotify как отдельный playback-девайс `Music Assistant Playback` — это устройство music-провайдера, а не колонка ([spotify/README.md](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify/README.md); [alternatives-risk.md](file:///Users/developer/Desktop/yandex-station-lab/research_notes/Spotify%20Connect%20на%20колонке%20Pulsar/alternatives-risk.md)). Поведение «показываться или нет» зависит от режима Soloist и в этой лаборатории не проверялось; в любом случае это не Pulsar. Важен вывод: даже когда MA играет Spotify, станция не становится Connect-целью. `[не проверено]` для деталей отображения; `[подтверждено]` для того, что это не колонка.

| Плоскость | Что требуется | Есть ли у Pulsar сегодня | Что добавляет Connect-плагин |
|---|---|---|---|
| L2-обнаружение | `_spotify-connect._tcp`, `SRV`, `TXT CPath`, HTTP `getInfo` | Нет; есть только `_sendspin._tcp` и `:8928` | mDNS-анонс и HTTP-ответ демона на неттопе |
| Аккаунтный реестр | Spotify-сессия, логин, стабильный `deviceID` | Нет; Pulsar не логинится в Spotify | Сессия Soloist, pairing из приложения, стабильный id |
| Аудиорендерер | Приём Spotify-аудио, декодирование, вывод | Нет; только Sendspin PCM → ALSA | Soloist декодирует в PCM, MA отправляет по Sendspin |
| Управление | Connect dealer/spclient, команды, состояние | Нет | Демон и MA-проксирование команд |
| Результат в приложении | Устройство в picker | Не видно | «Яндекс Станция Миди» в picker |

## Как `spotify_connect` превращает MA-плеер в Spotify-устройство

### Один демон на каждого выбранного плеера

Плагин — это `type: plugin` с `ProviderFeature.AUDIO_SOURCE`, один instance на всю MA. Для каждого выбранного плеера создаётся `_PlayerDaemon`, backend и синтетический `AudioSource` с `item_id = player_id`; `identity_key = "{instance_id}_{safe_player_id}"`, поэтому у каждого плеера свой data dir, свой mDNS-анонс и свой Connect-девайс ([provider.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/provider.py)). `AudioSource` создаётся с `exclusive=True`, `allow_external_trigger=True`, `can_initiate=True`; это и есть механизм «Spotify забирает колонку» ([provider.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/provider.py)). Плагин помечен как beta, но в 2.10.8 он уже является штатным провайдером ([manifest.json](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/manifest.json)). `[вывод из кода]`

### Soloist — официальный движок, но с эксплуатационной ценой

Soloist — официальный headless-клиент Spotify: Linux, Apple Silicon, Windows; рекламирует себя в локальной сети, логинится через Spotify Connect и играет через системный аудиосервер; на Linux сначала PipeWire, затем PulseAudio ([Spotify: Soloist Overview](https://developer.spotify.com/documentation/soloist/concepts/overview)). Для MA это Linux-only: на других ОС setup принудительно переключает на go-librespot ([soloist/runtime.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/soloist/runtime.py)). Сборки живут 90 дней с даты сборки и завершаются с кодом `10` после истечения; API key создаётся на `/dashboard/soloist` и требует Premium на момент создания, но слушать после этого может и Free-аккаунт ([Spotify: Soloist Authentication](https://developer.spotify.com/documentation/soloist/concepts/authentication); [Soloist blog](https://developer.spotify.com/blog/2026-08-13-introducing-spotify-soloist)). MA скачивает бинарник с серверов Spotify после согласия и обновляет его; MA прямо предупреждает, что условия Spotify не разрешают это явно, и использование идёт на ваш риск ([MA: Spotify Connect](https://www.music-assistant.io/plugins/spotify-connect/)). `[подтверждено]`

В лаборатории Soloist `1.3.10.13` уже установлен и проверен на music-провайдере через MA web player ([STATE.md](file:///Users/developer/Desktop/yandex-station-lab/docs/STATE.md); [HISTORY.md](file:///Users/developer/Desktop/yandex-station-lab/docs/HISTORY.md)). Это снимает риск «а запустится ли бинарник»: тот же runtime используется и Connect-плагином, хотя именно Connect-путь на этой машине ещё не запускался. `[подтверждено]` для установки; `[не проверено]` для Connect-пути. go-librespot остаётся fallback: он не требует ключа, но это reverse-engineered клиент, ориентированный на Premium-аккаунты, созданные до декабря 2024, и максимум 320 kbps Ogg Vorbis ([MA: Spotify Connect](https://www.music-assistant.io/plugins/spotify-connect/); [go-librespot](https://github.com/devgianlu/go-librespot)).

| Параметр | Soloist | go-librespot |
|---|---|---|
| Происхождение | Официальный Spotify | Community, reverse-engineered |
| Ключ/настройка | API key, consent, автоскачивание | Без ключа, но нужен бинарник |
| Аккаунты | Free (с рекламой) или Premium после создания ключа | Premium, обычно созданные до дек. 2024 |
| Качество | Lossless до 24/44.1 (фактически opaque) | Ogg Vorbis до 320 kbps |
| Аудиопоток | PulseAudio pipe-sink → FIFO | PCM в stdout |
| Очередь | Shuffle/repeat/queue control | Только transport |
| Риск | 90-дневные сборки, ToS-серая зона | Может сломаться при изменении протокола |

### Что именно анонсируется и как происходит pairing

В go-librespot-варианте MA пишет конфиг, по которому демон регистрирует `_spotify-connect._tcp` в `local.` с `TXT` `CPath=/`, `VERSION=1.0`, `Stack=SP`, тип устройства `speaker`, стабильный 40-hex `device_id = sha256(identity_key)[:40]` и имя из шаблона ([go-librespot zeroconf.go](https://github.com/devgianlu/go-librespot/blob/master/zeroconf/zeroconf.go); [go_librespot/backend.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/go_librespot/backend.py)). Soloist использует тот же протокол, но его точный TXT-набор и идентификатор в этой лаборатории не измерялись: identity и сессия Soloist хранятся в его data dir, а имя задаётся argv ([Spotify: Soloist Overview](https://developer.spotify.com/documentation/soloist/concepts/overview)). `[вывод из кода]` для go-librespot; `[не проверено]` для точного Soloist-анонса. MA **не логинится** за вас: Soloist-демон остаётся в состоянии «logged_in: false» и продолжает рекламироваться; любой Spotify-аккаунт может «забрать» устройство из своего приложения, и сессия сохранится в data dir демона ([soloist/README.md](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/soloist/README.md)). Пароли в MA не вводятся; удаление data dir сбрасывает identity и требует повторного pairing ([Spotify: Soloist Overview](https://developer.spotify.com/documentation/soloist/concepts/overview)).

Имя устройства задаётся шаблоном: `Player name | Music Assistant` (по умолчанию), `Player name` или `Music Assistant | Player name`; переименование плеера перезапускает демон, потому что имя вшивается при старте ([config_entries.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/helpers/config_entries.py); [soloist/backend.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/soloist/backend.py)). Для «нативного» вида выбирайте `Player name` — в picker будет «Яндекс Станция Миди»; для отличия от реальной Яндекс-колонки — оставьте суффикс `| Music Assistant`.

### Как идут звук, метаданные и команды

| Направление | Что происходит | Источник |
|---|---|---|
| Spotify → Soloist | Connect-сессия, декодирование в PCM 44.1/32 | [soloist/README.md](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/soloist/README.md) |
| Soloist → MA | Приватный PulseAudio `module-pipe-sink` → FIFO → ffmpeg `-readrate` | [soloist/backend.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/soloist/backend.py) |
| MA → Pulsar | Sendspin-поток, Pulsar декодирует FLAC/Opus/PCM в ALSA | [sendspin_service.cpp](https://github.com/Morvex885/pulsar/blob/v0.4.1/satellite/src/sendspin/sendspin_service.cpp) |
| Spotify → MA (метаданные) | title/artist/album/artwork/duration/position → `StreamMetadata` live-источника | [provider.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/provider.py) |
| MA/HA/Pulsar → Spotify | play/pause/next/previous/seek/shuffle/repeat; volume — по политике | [provider.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/provider.py) |
| Spotify → MA (очередь) | Не зеркалится: `QUEUE_CHANGED` игнорируется | [provider.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/provider.py) |

**Метаданные идут только в одну сторону — из Spotify в MA.** MA не отправляет Spotify название трека или обложку: Spotify и так авторитетен для каталога и очереди; MA лишь отображает текущий item и проксирует команды. Поэтому в HA-сущности будут title/artist/album/artwork/position, а в MA UI — один live-item, без списка «Up Next». Для телефона это означает, что приложение остаётся главным контроллером: play/pause/next/seek из HA меняют состояние через Spotify, и приложение обновляется, но позиция может отставать или опережать звук из-за буфера. Это не регрессия, а сознательное ограничение 2.10.8: «full queue-item mirroring comes later» ([provider.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/provider.py)). `[вывод из кода]`

**Громкость — отдельная политика.** В Spotify громкость привязана к устройству, а не к сессии: transfer её не переносит, и при подключении побеждает громкость плеера. Провайдер дополнительно игнорирует volume-события Spotify в первые 3 секунды после активации, чтобы избежать пинг-понга ([provider.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/provider.py)). В `player_only` (по умолчанию) демон фиксируется на 100%, MA/Pulsar владеет слышимой громкостью, а слайдер Spotify остаётся на 100% — так задумано, не баг. В `sync_spotify` слайдер приложения синхронизируется с плеером через кубическую компенсацию; при резком повышении возможен кратковременный full-scale артефакт, а на Sendspin-плеере с собственной DSP-громкостью поведение не проверено ([soloist/README.md](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/soloist/README.md); [MA: Spotify Connect](https://www.music-assistant.io/plugins/spotify-connect/)). `[вывод из кода]`, `[не проверено]` для Sendspin.

### Переключения и жизненный цикл

**Телефон → колонка.** При выборе устройства в Spotify демон получает активную сессию; MA видит `SESSION_ACTIVE`/`PLAYING`, ждёт 0,5 с (debounce), затем вызывает `play_media` live-источника на целевом плеере; текущая очередь останавливается, выбирается `AudioSource`, universal player переключает output protocol на Sendspin, и звук появляется с задержкой буфера ([provider.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/provider.py); [MA: Spotify Connect](https://www.music-assistant.io/plugins/spotify-connect/)). Прогресс в приложении может опережать звук — это документированное следствие буферизации. `[вывод из кода]`

**Колонка → телефон.** При выборе телефона или другого устройства MA получает `SESSION_INACTIVE`, очищает активного игрока и останавливает MA-плеер; очередь при этом не разрушается, но становится inactive и автоматически не возобновляется ([provider.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/provider.py); [player_queues/controller.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/controllers/player_queues/controller.py)). Телефон продолжает с логической позиции Spotify; буфер колонки не переносится. Чтобы вернуться к MA-очереди, нужно нажать play или выбрать queue source. `[вывод из кода]`

**Пауза — не transfer.** При `PAUSED` провайдер останавливает MA-плеер (Soloist pipe никогда не даёт EOF, поэтому `stream_ends_on_pause=False`), но сохраняет claim на устройство; play из приложения снова запускает `play_media`. Stop из MA, наоборот, освобождает устройство в Spotify ([soloist/README.md](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/soloist/README.md); [MA: Spotify Connect](https://www.music-assistant.io/plugins/spotify-connect/)). Пауза дольше 10 минут может потребовать переподключения по данным Spotify; MA дополнительно считает внешний источник «закончившимся» после 60 секунд паузы (`EXTERNAL_PAUSE_IDLE_TIMEOUT = 60`) — это строже, чем у Spotify ([Spotify Connect support](https://support.spotify.com/us/article/spotify-connect/); [constants.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/constants.py)). `[подтверждено]` для 10 минут; `[вывод из кода]` для 60 секунд.

**Возврат управления из MA.** Если устройство ещё session-active, но на паузе, `resume()`; если playback уже увели на телефон, MA вызывает `backend.play(last_context_uri, skip_to_uri=last_track_uri)` и делает устройство активным снова. Трек при этом начинается с начала: resume-at-position вызова нет, а Soloist не поддерживает `skip_to_uri` ([provider.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/provider.py); [soloist/backend.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/soloist/backend.py)). Для подкастов и аудиокниг это реальное ограничение: «продолжить с места» работает только в направлении телефон → колонка, где позицию восстанавливает сам Spotify. `[вывод из кода]`

## Конфликты: борьба за один активный источник, а не за протокол

У конфликтов три слоя, и их полезно не смешивать. **Аккаунтный** — один Spotify-аккаунт имеет один активный stream-slot; **источниковый** — у MA-плеера ровно один активный source, и Connect-источник эксклюзивен; **физический** — у Pulsar одни динамики и один ALSA-выход. Второй аккаунт снимает только первый слой; второй и третий остаются.

### Один аккаунт — один поток

Если телефон залогинен в тот же Spotify-аккаунт, что и music-провайдер MA, Connect-сессия и Spotify-очередь MA делят единственный stream-slot. Это ограничение Spotify, а не MA: активным может быть только одно устройство, и запуск второго останавливает первое ([Spotify Community KB](https://community.spotify.com/forum/en/kb/articles/834-premium-family-duo-only-one-person-can-listen-at-a-time); [spotify/README.md](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify/README.md)). MA 2.10.8 не арбитрирует это: в коде `spotify_connect` нет ни проверок stream-limit, ни связи с music-провайдером, кроме общего runtime Soloist ([provider.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/provider.py)). Кто «победит» при одновременном старте, зависит от тайминга: Connect-сессия может умереть от `SESSION_INACTIVE`, а может успеть занять слот раньше music-провайдера. Это не детерминированный handoff, а гонка. `[подтверждено]` для правила одного потока; `[не проверено]` для конкретного исхода в этой системе.

Если MA играет **не Spotify** — Яндекс Музыку, радио, локальные файлы — аккаунтного конфликта нет: music-провайдер не занимает Spotify-слот. Остаётся только источниковый конфликт на Pulsar. Это важное различие: «включи радио» и «включи любимое» ведут себя по-разному.

Если аккаунты **разные**, stream-slot независимы: телефон может играть Spotify на колонке, а MA — Spotify другого аккаунта на другом выходе. Но на одной Pulsar одновременно играть всё равно нельзя: `AudioSource` эксклюзивен, и физический выход один. Второй аккаунт — чистое решение для одновременности **на разных выходах/комнатах**, не для одновременности на одной колонке.

### Эксклюзивный источник и очередь

Connect-источник — не элемент очереди. При его выборе MA останавливает текущую очередь, но не очищает её: «live source is not queue content… the queue keeps its own items» ([player_queues/controller.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/controllers/player_queues/controller.py)). Любой другой `play_media` на этом плеере заменяет source и вызывает `on_source_unselected → backend.deactivate()`, чтобы Spotify отпустил устройство ([provider.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/provider.py)). Отсюда практическое следствие: после конца Connect-сессии MA-очередь не возобновляется сама; в снимке лога уже есть `Resume queue requested but queue Яндекс Станция Миди is empty`, то есть голосовое «продолжи» может уйти в пустую очередь, если source уже отпущен ([pulsar-coexistence.md](file:///Users/developer/Desktop/yandex-station-lab/research_notes/Spotify%20Connect%20на%20колонке%20Pulsar/pulsar-coexistence.md)). `[вывод из кода]`, `[подтверждено]` для строки лога.

### Голосовые команды: что работает, что вытесняет

| Команда | Путь | Поведение при активном Connect | Риск |
|---|---|---|---|
| «громче/тише» | ESPHome `media_player.iandeks_stantsiia_midi_media_player` → ALSA | Работает; в `player_only` Spotify-слайдер остаётся 100% | Нет |
| «пауза» | MA universal → `cmd_pause` → Connect provider | Ставит Spotify на паузу; MA-плеер останавливается, claim сохраняется | Средний: resume может уйти в пустую очередь |
| «продолжи» | MA universal → `cmd_play` | Если source ещё active — resume в Spotify; если уже отпущен — пустая очередь/ошибка | **Проверить** |
| «стоп/выключи музыку» | MA universal → `cmd_stop` | Останавливает плеер и освобождает Connect-устройство; Spotify теряет active device | Ожидаемо, но устройство надо выбрать заново |
| «стоп/хватит» локально на Pulsar | Sendspin controller → `music_pause()` | Фактически pause, не stop; сессия сохраняется | Низкий |
| «дальше/назад» локально на Pulsar | Sendspin NEXT/PREV → MA `cmd_next_track` | Должно проксироваться в Spotify, пока source active | Средний: если стрим остановлен — «no Sendspin queue to skip in» |
| «включи любимое» | `browse_media` + `music_assistant.play_media` (`enqueue: replace`) | Запускает MA-очередь и вытесняет Connect; сейчас блокировано 429 | **Главный конфликт** |
| TTS/announce | HA/Pulsar ANNOUNCE | Duck Sendspin до 0,15, затем restore; сессия не должна рваться | Низкий, но не проверено |

Два правила следуют из кода и текущих скриптов. **Не отправляйте transport на ESPHome-сущность** `media_player.iandeks_stantsiia_midi_media_player` во время Sendspin: Pulsar вызывает `yield_to_local()` и выходит из Sendspin-группы, то есть это обрыв потока, а не пауза Spotify ([controller.cpp](https://github.com/Morvex885/pulsar/blob/v0.4.1/satellite/src/controller.cpp)). Текущие intent-скрипты так и не делают: pause/play/stop идут на MA-сущность, громкость — на ESPHome ([intent_scripts.yaml](file:///Users/developer/Desktop/yandex-station-lab/projects/home-assistant/ha-voice/intent_scripts.yaml)). И **не рассчитывайте, что «продолжи» после паузы всегда сработает**: после `PAUSED` провайдер останавливает MA-плеер, а очередь может быть пустой; это UX-ограничение, которое надо либо принять, либо лечить отдельным скриптом.

### 429, TTS и очередь

**429 бьёт по Web API, а не по Connect.** В логах на момент снимка: `Not retrying _get_data, the server asked to wait 66521 seconds`, `Search on provider Spotify failed: Not retrying, asked to wait 66521 seconds`; это ~18 ч 28 мин, от 05:03:16 +05 до ~23:31:57 +05 ([status-20261026-0510-ma.txt](file:///Users/developer/Desktop/yandex-station-lab/logs/status-20261026-0510-ma.txt)). Connect-плагин не использует Web API: ему не нужны client id/secret, настроенный Spotify music provider, каталог или библиотека; playback и метаданные идут через Connect-протокол и Soloist, в коде нет обращений к `api.spotify.com/v1` ([spotify_connect/README.md](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/README.md); [provider.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/provider.py)). Поэтому Connect должен играть даже при активном 429 — но он **не лечит** 429 и не наполняет Liked Songs: «включи любимое» по-прежнему идёт в music-провайдер и останется сломанным до восстановления Web API. Если бы 429 был ограничением аккаунта, а не rate limit, Connect тоже мог бы пострадать; текущие логи показывают именно `Retry-After`, но эмпирической проверки Connect во время 429 нет. `[подтверждено]` для логов; `[вывод из кода]` для отсутствия Web API; `[не проверено]` для Connect во время 429.

**TTS/announce не должен рвать сессию.** MA не освобождает live source для `MediaType.ANNOUNCEMENT`, а Pulsar duck’ит музыку до `kDuckGain = 0.15` на время диалога, HA announcement, таймера или earcon ([players/controller.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/controllers/players/controller.py); [controller.cpp](https://github.com/Morvex885/pulsar/blob/v0.4.1/satellite/src/controller.cpp)). По коду сессия Connect должна пережить announce; на практике это надо проверить отдельным тестом, потому что TTS-путь зависит от того, идёт ли announce через MA ANNOUNCE, а не через ESPHome media playback. `[вывод из кода]`, `[не проверено]`.

**Очередь после Connect.** MA-очередь сохраняется, но не resume’ится; после `SESSION_INACTIVE` или stop она остаётся inactive. Это не потеря данных, а смена active source. Если пользователь ждёт «само вернётся», он удивится; это надо либо задокументировать, либо добавить автоматизацию «после конца Connect включить очередь», но автоматизация создаст новый конфликт с телефоном.

### Гонка, которую надо проверить

Самый неприятный сценарий: при старте MA-очереди (например, «включи любимое») MA останавливает Connect-источник, тот вызывает `deactivate()`, демон получает `SESSION_INACTIVE`, и провайдер планирует `cmd_stop` активного плеера. Если это событие обработается уже после старта новой очереди, оно может остановить **новый** playback. В коде есть защиты от stale-session (stop отменяется, если приходит `PLAYING`), но именно этот race на одном аккаунте и одном плеере не проверен ([provider.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/provider.py); [pulsar-coexistence.md](file:///Users/developer/Desktop/yandex-station-lab/research_notes/Spotify%20Connect%20на%20колонке%20Pulsar/pulsar-coexistence.md)). Это главная причина не включать Connect «навсегда» до теста T13. `[вывод из кода]`, `[не проверено]`.

| Конфликт | Механизм | Когда проявляется | Серьёзность | Что делать |
|---|---|---|---|---|
| Один аккаунт — один stream | Spotify останавливает первый поток при старте второго | Connect + Spotify music provider на одном аккаунте | Высокая | Один источник за раз или второй аккаунт |
| Эксклюзивный source | `AudioSource exclusive=True`; любой `play_media` заменяет source | Любая голосовая команда `play_media` | Высокая | Пауза/стоп Connect перед `play_media` |
| Connect vs очередь | Очередь сохраняется, но inactive; auto-resume нет | После transfer на телефон/stop | Средняя | Документировать; возвращать вручную |
| Pause/resume | Soloist pause останавливает MA-плеер, claim сохраняется | «Пауза», затем «продолжи» | Средняя | Тест T6/T7; при необходимости скрипт |
| Stop | `cmd_stop` освобождает устройство | «Стоп/выключи музыку» | Средняя | Ожидаемо; выбрать устройство заново |
| Локальный «стоп» | Pulsar вызывает `music_pause()`, не stop | «Стоп/хватит» на колонке | Низкая | Ничего, это мягче |
| Локальный next/prev | Sendspin NEXT → MA `cmd_next_track` → external source | «Дальше/назад» при Connect | Средняя | Тест T7 |
| «Включи любимое» | `play_media` replace вытесняет Connect; плюс 429; плюс race | Голосовой запуск плейлиста | **Высокая** | Арбитражный скрипт или Connect off |
| 429 | Web API rate limit; Connect его не использует | Library/search/favorites | Средняя | Ждать/личный client id; Connect не поможет |
| TTS | Announce exception + duck 0.15 | Wake word/TTS при Connect | Низкая | Тест T11 |
| Volume fight | `sync_spotify` + HA пишет ESPHome volume | Два владельца громкости | Средняя | `player_only`; один владелец |
| ESPHome transport | `yield_to_local()` / выход из Sendspin | Pause/stop на ESPHome-сущности | Высокая | Никогда не отправлять transport туда |
| Ghost device | Смена data dir/имени → новая identity | Переустановка/переименование | Низкая | Стабильное имя; «Log out of this device» |

## План внедрения: один плагин, восемь шагов, полный откат

### Pre-flight и backup

| Шаг | Что сделать | Зачем | Команда/место |
|---|---|---|---|
| 0.1 | Зафиксировать baseline: провайдеры MA, `up77eb9a80`, `fbp82…`, active output protocol, очередь, 429 Retry-After, IP/порты | Чтобы откат сравнивался с исходным состоянием | MA UI, `settings.json`, `ss -lntp`, `ip -br addr` |
| 0.2 | Проверить, что телефон в `192.0.2.0/24`, не в guest VLAN; на iPhone включён Local Network для Spotify | Без mDNS устройство не появится | Телефон/роутер |
| 0.3 | Найти существующий Soloist API key; не создавать новый вслепую `[не проверено]` | Ключ нужен setup’у; неизвестно, не инвалидирует ли новый ключ старый | Spotify developer dashboard / личное хранилище |
| 0.4 | Понять, тот же ли Spotify-аккаунт на телефоне, что у music-провайдера | Это определяет, будет ли stream-конфликт | Spotify app → профиль; MA → провайдер |
| 0.5 | Сделать backup: `cd /home/labuser/nettop && ./ctl.sh backup` | Штатный backup останавливает стек; это единственное «тяжёлое» действие | [stack/README.md](file:///Users/developer/Desktop/yandex-station-lab/projects/home-assistant/stack/README.md) |
| 0.6 | Решить по версии MA: 2.10.8 или откат на 2.10.7 | В 2.10.8 открыт regression по Spotify playback | [MA #6860](https://github.com/music-assistant/support/issues/6860) |
| 0.7 | Определить BIND TO: активный интерфейс в одной L2 с телефоном. На снимке это `wlp4s0 192.0.2.25`, Ethernet `.26` был down; если поднимут `.26` — перепроверить mDNS | На multi-homed хосте демон может анонсироваться не на тот интерфейс | MA → Settings → Streams → BIND TO |

mDNS link-local: VLAN без reflector и AP isolation ломают обнаружение даже при доступности по IP, поэтому «пинг работает» здесь ничего не доказывает.

Про 2.10.8 важно не перепутать: issue #6860 воспроизведён на **music-провайдере** (`music_assistant.spotify`), а не на Connect-плагине; пострадавшие используют Soloist и Sendspin/AirPlay/Chrome, откат на 2.10.7 помогает. Влияет ли regression на `spotify_connect`, не установлено: это другой код и другой путь сессии. Поэтому версия — это **gate**, а не автоматический запрет: сначала 60-минутный soak именно Connect-пути, и только потом решение.

### Настройки плагина

| Параметр | Значение для этой лаборатории | Почему |
|---|---|---|
| Plugin | `Spotify Connect`, один instance | Добавляется один раз на всю MA, не на плеер |
| Engine | **Spotify Soloist** | Уже работает 1.3.10.13; Linux; lossless; поддерживает аккаунты после 2024 |
| Connected players | только `Яндекс Станция Миди` (`up77eb9a80`; это HA-сущность `media_player.iandeks_stantsiia_midi`) | Universal player маршрутизирует в Sendspin; protocol `fbp82…` в списке не появится |
| Advertised name | `Player name` → «Яндекс Станция Миди» | Семейное узнавание; суффикс `Music Assistant` — если нужно отличить от реальной колонки |
| Volume mode | `player_only` | Громкостью владеют MA/Pulsar; нет volume-fight; слайдер Spotify 100% так задумано |
| Audio quality | `lossless` | Потолок; фактический Sendspin-поток всё равно 16-bit |
| Crossfade | `0` | Live-источник; не усложнять переходы |
| Loudness normalization | `on` | Spotify применяет −14 LUFS; иначе MA может нормализовать повторно |
| Group | не выбирать | Один девайс; группа + участник создают дубли и конфликты |

### Порядок действий

| Шаг | Действие | Ожидаемый результат | Если нет |
|---|---|---|---|
| 1 | MA → Settings → Plugins → Add plugin → Spotify Connect | Плагин появился, setup открылся | Проверить версию MA; плагин есть в 2.10.8 |
| 2 | Выбрать Soloist, принять consent, вставить существующий API key (если setup его принимает) | `Loaded plugin provider Spotify Connect`, демон стартовал | Не создавать новый ключ без понимания последствий; остановиться |
| 3 | В settings выбрать `up77eb9a80`, имя, `player_only`, `lossless`, crossfade 0, normalization on | В логе нет `FATAL_ERROR`; демон рекламируется | Проверить, что выбран именно universal player |
| 4 | Открыть Spotify на телефоне (same Wi-Fi), найти «Яндекс Станция Миди» | Устройство видно в picker | 5–10 мин, перезапуск app, 5353/AP isolation, BIND TO |
| 5 | Выбрать устройство, включить один трек | Звук; MA log: `Spotify Connect session active`, `Starting Spotify Connect playback … on player up77eb9a80`, `Setting active output protocol … to Sendspin` | Stop, disable plugin, смотреть лог |
| 6 | Сразу stop из MA | Плеер idle; устройство отпущено; Spotify больше не считает колонку активной | `media_stop`; reload plugin |
| 7 | Прогнать матрицу тестов T2–T16 | Ключевые сценарии проходят; известные ограничения зафиксированы | Disable plugin; вернуться к D |
| 8 | Решить режим: E1 (один аккаунт + политика) или E2 (второй аккаунт) | Семья понимает правила; нет двойного звука | Если не понимает — D |

Переиспользование ключа — гипотеза, а не гарантия: setup Connect может попросить новый ключ, а официально не описано, инвалидирует ли новый ключ старый. Если setup не принимает существующий, остановитесь и сначала выясните это в Spotify dashboard, а не создавайте ключ «на удачу».

### Откат

Откат — это **отключение плагина**, а не удаление данных. Settings → Plugins → Spotify Connect → Disable: provider выгружается, демоны останавливаются, sources освобождаются, устройство исчезает из Spotify; каталог `data/spotify_connect` остаётся для повторного включения. Если source залип — `media_player.media_stop` на MA-сущности, reload plugin, в крайнем случае `./ctl.sh restart ma`. Если MA не стартует — восстановить `settings.json`/`data/music-assistant` из backup, не удаляя `/data/spotify_connect` до подтверждения, что он не нужен. Pulsar, HA, ESPHome и сеть не менялись, поэтому их откат не требуется ([pulsar-coexistence.md](file:///Users/developer/Desktop/yandex-station-lab/research_notes/Spotify%20Connect%20на%20колонке%20Pulsar/pulsar-coexistence.md)).

## Матрица тестов: восемнадцать проверок до постоянного включения

| # | Сценарий | Действие | Pass | Где смотреть | Если fail |
|---|---|---|---|---|---|
| T0 | Baseline | Снять провайдеров, player id, queue, 429, IP/порты | `up77eb9a80`, Sendspin, 429 ~23:32, очередь пуста | MA UI/log, `ss`, `ip` | — |
| T1 | Plugin load | Добавить Connect, Soloist, key | `Loaded plugin provider Spotify Connect`; нет fatal | MA log | Disable plugin |
| T2 | Discovery | Открыть Spotify device picker | «Яндекс Станция Миди» виден | Spotify app | L2/5353/AP isolation; BIND TO; disable |
| T3 | Phone → speaker | Выбрать устройство, play трек | Звук; MA state playing; metadata | MA log/HA | Stop, disable |
| T4 | Metadata | Сменить трек, проверить title/artist/art | Обновление за несколько секунд; artwork | HA entity | Не блокер; зафиксировать |
| T5 | App transport | Pause/play/next/prev/seek из Spotify | Действует; звук следует | Spotify + HA | Disable |
| T6 | HA transport | `media_pause/play/stop` на MA entity | Pause работает; resume работает или ограничение задокументировано; stop освобождает device | HA + Spotify | Disable |
| T7 | Local voice | «пауза», «продолжи», «дальше», «назад» | Pause/next/prev работают; resume — проверить | Pulsar log/HA | Зафиксировать; disable если ломает |
| T8 | Volume | Кнопки Pulsar, HA volume, Spotify slider | Громкость меняется; slider 100% в `player_only` | Pulsar/HA | Сменить volume mode |
| T9 | Speaker → phone | В Spotify выбрать телефон | MA idle; телефон играет; очередь цела | MA log/HA | Disable |
| T10 | Phone → speaker back | Снова выбрать устройство | Playback возвращается; metadata | MA log | Disable |
| T11 | TTS/announce | Wake word/TTS во время Connect | Duck до 0,15, затем restore; сессия жива | Pulsar/HA | Disable если рвёт |
| T12 | Queue coexistence | Запустить radio/MA queue при Connect | Connect останавливается/отпускается; queue играет; Connect не auto-resume | MA log | Disable |
| T13 | Favorites conflict | После восстановления 429: «включи любимое» при Connect | Connect уступает queue; нет остановки нового playback | MA log | **Главный risk**; pause Connect first или disable |
| T14 | 429 independence | При активном 429 играть Connect | Connect играет; search/library всё ещё 429 | MA log | Если Connect тоже 429 — account-level, стоп |
| T15 | Pause/reconnect | Пауза 60 с, затем 10 мин; play из app | Возврат без сюрпризов; при необходимости reselect | Spotify/MA | Зафиксировать как ограничение |
| T16 | MA restart | `./ctl.sh restart ma` в тестовом окне | Device возвращается; phone reselect; player не stuck | MA log/HA | Disable; restore backup |
| T17 | Soak | 60+ мин Connect → Pulsar | Нет dropouts/самопроизвольных stop | MA log/уши | Pin MA/проверить #6047; disable |
| T18 | Rollback | Disable plugin | Device исчез; player idle; music/voice работают | MA/HA | Restore backup |

**Критерии приёмки.** T3, T4, T5, T6, T9, T10, T11, T12 проходят; T7 либо проходит, либо оформлен как документированное ограничение; T13 проходит или имеет явную политику/скрипт «сначала пауза Connect»; 60 минут Connect без dropouts; после T18 состояние идентично T0, кроме появившегося `data/spotify_connect`; нет роста CPU/памяти, ведущего к деградации HA/MA. Если T13 непредсказуем, это не «мелкий баг», а блокер постоянного включения: именно он превращает голосовую команду в остановку нового playback.

## Матрица решений: пять архитектур и шесть вариантов

Оценки сравнительные, из research notes, а не измерения этой лаборатории; веса отражают приоритет «app target + switching/sync + voice coexistence» ([alternatives-risk.md](file:///Users/developer/Desktop/yandex-station-lab/research_notes/Spotify%20Connect%20на%20колонке%20Pulsar/alternatives-risk.md)).

| Критерий | Вес | A: MA Connect → Pulsar | B: receiver на станции | C: receiver на неттопе + bridge | D: только music provider | E1: hybrid, один аккаунт | E2: hybrid, два аккаунта |
|---|---:|---:|---:|---:|---:|---:|---:|
| App presence + network scope | 20% | 5 | 2 | 5 | 1 | 5 | 5 |
| Phone↔speaker + metadata/position/volume | 20% | 4 | 3 | 4 | 1 | 4 | 4 |
| Voice/Alice + HA coexistence | 20% | 3 | 1 | 2 | 5 | 3 | 3 |
| Conflict/account/queue safety | 15% | 3 | 2 | 2 | 5 | 2 | 4 |
| Hardware/network feasibility | 10% | 5 | 1 | 4 | 5 | 5 | 5 |
| Setup/maintenance/observability/rollback | 10% | 4 | 1 | 2 | 5 | 3 | 3 |
| Legal/ToS + 429 resilience | 5% | 3 | 2 | 3 | 3 | 3 | 3 |
| **Взвешенный итог** | **100%** | **3,90** | **1,80** | **3,25** | **3,30** | **3,65** | **3,95** |

**A — MA Connect → Pulsar.** Минимальный вариант, который даёт app presence и сохраняет MA/HA-интеграцию без изменений станции: WAN и аудиосервер остаются на неттопе, egress-guard Pulsar не трогается. Цена — voice/source-конфликты и зависимость от beta-плагина, MA 2.10.8 и Sendspin-стабильности. Это primary candidate, но с gate по 60-минутному soak и T13. Счёт A выше E1 за счёт простоты, но A — это Connect без music provider; для лаборатории, где нужны голосовой поиск и библиотека, целевой остаётся E1.

**B — native receiver на станции.** Практически несовместим с текущей архитектурой: armhf userspace (по STATE.md; в RESOURCES.md есть расхождение с aarch64 — перед любым build надо проверить `uname -m` и `file`), read-only rootfs, нет подтверждённого PipeWire/PulseAudio, egress-guard режет Spotify WAN, а Pulsar не имеет документированного внешнего аудиовхода. Даже если бинарник запустится, Pulsar не будет duck’ить Spotify во время TTS, а HA/MA не увидят metadata. Это не «сложнее», а структурно другой проект с ослаблением security-политики ([SECURITY.md](file:///Users/developer/Desktop/yandex-station-lab/docs/SECURITY.md); [Pulsar README](file:///Users/developer/Desktop/yandex-station-lab/projects/station/upstream/pulsar/README.md)).

**C — receiver на неттопе + bridge.** Технически возможно: Soloist/go-librespot на x86_64, затем мост в Sendspin/HTTP. Но вы теряете MA/HA metadata и voice-интеграцию, получаете второй сервис и drift; для этой лаборатории C доминируется A. Оставить как lab fallback, если плагин окажется непригоден.

**D — только music provider.** Безопасный baseline: голос работает нативно, 429 — главная боль, app presence нет. Это fallback, если app target не обязателен или если T13 нестабилен.

**E1 — hybrid, один аккаунт.** Рекомендуемый default, если app target обязателен: Connect как «гостевой режим» телефона, music provider — для голоса; один активный источник, явная пауза/стоп перед сменой. Не одновременное воспроизведение, но предсказуемое, если политика соблюдается.

**E2 — hybrid, два аккаунта.** Чистейшее решение для независимой одновременной Spotify-сессии на разных выходах: у каждого аккаунта свой stream-slot, библиотеки/лайки разделены. Нужны два аккаунта (для music provider — Premium; слушатель Connect может быть Free после создания ключа) и, по текущим данным, отдельный Soloist key/pairing на каждый аккаунт. На одной Pulsar одновременность всё равно невозможна; E2 снимает account-level конфликт, но не source-level. Плюс есть риск MA multi-account leak (#6402), поэтому E2 тоже требует теста ([MA #6402](https://github.com/music-assistant/support/issues/6402)).

## Риски и неизвестные: что подтверждено, что выведено, что не проверено

| ID | Риск | Вероятность | Влияние | Доказательство | Смягчение |
|---|---|---|---|---|---|
| R1 | Один аккаунт: Connect и music provider останавливают друг друга | Высокая | Высокое | [Spotify KB](https://community.spotify.com/forum/en/kb/articles/834-premium-family-duo-only-one-person-can-listen-at-a-time); [spotify/README.md](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify/README.md) | Один источник за раз; E2 |
| R2 | MA 2.10.8 Spotify mid-track stop | Высокая для music provider; для Connect неизвестна | Высокое | [MA #6860](https://github.com/music-assistant/support/issues/6860) | Pin 2.10.7 или 60-мин soak Connect |
| R3 | Connect → Sendspin dropouts/60 s sync loss (на ESPHome Sendspin; Pulsar не проверен) | Средняя | Высокое | [MA #6047](https://github.com/music-assistant/support/issues/6047) | 60-мин soak; buffer/BIND TO; fallback D |
| R4 | «Включи любимое» может остановить новый playback (race) | Высокая | Среднее | Код `SESSION_INACTIVE` path; не проверено | Пауза/стоп Connect + wait idle; скрипт |
| R5 | mDNS не видит устройство | Средняя | Высокое | [MA docs](https://www.music-assistant.io/plugins/spotify-connect/) | Flat L2, BIND TO, Local Network, без guest VLAN |
| R6 | Soloist 90-day expiry | Средняя | Высокое | [Soloist Overview](https://developer.spotify.com/documentation/soloist/concepts/overview) | MA auto-update; календарь; monitor |
| R7 | ToS: MA скачивает/запускает Soloist | Средняя | Среднее | [MA docs](https://www.music-assistant.io/plugins/spotify-connect/) | Личное использование; без распространения; принять риск |
| R8 | Существующий API key недоступен | Высокая для setup | Блокер | [pulsar-coexistence.md](file:///Users/developer/Desktop/yandex-station-lab/research_notes/Spotify%20Connect%20на%20колонке%20Pulsar/pulsar-coexistence.md) | Найти ключ; не создавать новый вслепую |
| R9 | Universal player + Connect официально не описаны | Средняя | Среднее | Код; docs говорят «any MA player» | T3; если fail — вернуться к D |
| R10 | `sync_spotify` на Sendspin не проверен | Средняя | Низкое | [soloist/README.md](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/soloist/README.md) | `player_only`; тест отдельно |
| R11 | Multi-account leak (#6402) | Средняя | Среднее | [MA #6402](https://github.com/music-assistant/support/issues/6402) | Тест E2; отдельные провайдеры |
| R12 | Ghost device после rename/reset | Низкая | Среднее | [Spotify FAQ](https://developer.spotify.com/documentation/commercial-hardware/implementation/faqs) | Стабильное имя; «Log out of this device» |
| R13 | Egress-guard станции для B | Очень высокая | Высокое | [SECURITY.md](file:///Users/developer/Desktop/yandex-station-lab/docs/SECURITY.md) | Не открывать WAN; A/C вместо B |

Ниже — построчная проверка ключевых утверждений: что снято с машины, что выведено из кода и что остаётся гипотезой до тестов.

| Утверждение | Статус | Источник |
|---|---|---|
| MA 2.10.8, HA 2026.10.4, `network_mode: host`, Soloist `1.3.10.13`, 429 с `Retry-After` ~18,5 ч | `[подтверждено]` | [STATE.md](file:///Users/developer/Desktop/yandex-station-lab/docs/STATE.md); [status-20261026-0510-ma.txt](file:///Users/developer/Desktop/yandex-station-lab/logs/status-20261026-0510-ma.txt) |
| universal `up77eb9a80` ↔ HA `media_player.iandeks_stantsiia_midi`; protocol `fbp82…`; `/data/spotify_connect` отсутствует | `[подтверждено]` | [pulsar-coexistence.md](file:///Users/developer/Desktop/yandex-station-lab/research_notes/Spotify%20Connect%20на%20колонке%20Pulsar/pulsar-coexistence.md) |
| Pulsar v0.4.1: Sendspin `:8928`, ALSA, FLAC/Opus/PCM 16-bit 44.1/48 kHz, egress private-LAN-only | `[подтверждено]` | [sendspin_service.cpp](https://github.com/Morvex885/pulsar/blob/v0.4.1/satellite/src/sendspin/sendspin_service.cpp); [SECURITY.md](file:///Users/developer/Desktop/yandex-station-lab/docs/SECURITY.md) |
| Per-player daemon, `AudioSource exclusive=True`, сохранение очереди, `SESSION_INACTIVE → stop`, `QUEUE_CHANGED` игнорируется, политика громкости | `[вывод из кода]` | [provider.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/provider.py); [soloist/README.md](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/soloist/README.md) |
| mDNS/ZeroConf-контракт, аккаунтная плоскость, pairing через приложение, cloud-fallback | `[подтверждено]` | [Spotify ZeroConf](https://developer.spotify.com/documentation/commercial-hardware/implementation/guides/zeroconf); [Spotify FAQ](https://developer.spotify.com/documentation/commercial-hardware/implementation/faqs) |
| Soloist: 90-дневные сборки, PipeWire/PulseAudio, API key/Premium, lossless до 24/44.1 | `[подтверждено]` | [Soloist Overview](https://developer.spotify.com/documentation/soloist/concepts/overview); [Soloist blog](https://developer.spotify.com/blog/2026-08-13-introducing-spotify-soloist) |
| Один активный stream-slot на Spotify-аккаунт | `[подтверждено]` | [Spotify KB](https://community.spotify.com/forum/en/kb/articles/834-premium-family-duo-only-one-person-can-listen-at-a-time); [spotify/README.md](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify/README.md) |
| Connect можно привязать к universal player и он уйдёт в Sendspin | `[не проверено]` | Код-путь; routing universal→Sendspin подтверждён логом, Connect attachment — нет |
| Голосовые pause/next проксируются в Spotify при активном Connect; громкость — локально или по политике | `[вывод из кода]`, `[не проверено]` | [players/controller.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/controllers/players/controller.py) |
| TTS/announce не рвёт Connect-сессию | `[вывод из кода]`, `[не проверено]` | Announce exception + Pulsar duck 0.15 |
| 429 не влияет на Connect-playback | `[вывод из кода]`, `[не проверено]` | В Connect нет Web API; проверки во время 429 не было |
| «Включи любимое» может остановить новый playback (race) | `[вывод из кода]`, `[не проверено]` | `SESSION_INACTIVE` path; тест T13 |
| Реальный Connect playback на этой лаборатории | `[не проверено]` | Плагин не включён |
| Pulsar-специфичные metadata/latency и `sync_spotify` на Sendspin | `[не проверено]` | Нет замеров |
| Поведение после restart MA и в этой Wi-Fi-сети | `[не проверено]` | Тесты T16/T2 |
| Тот же ли Spotify-аккаунт на телефоне; место хранения API key | `[не проверено]` | Нужна проверка перед G0 |
| Влияние MA 2.10.8 regression на Connect | `[не проверено]` | #6860 — music provider; Connect не тестировался |

Это не мелкие оговорки: T3 и T13 — те тесты, которые отделяют «должно работать» от «работает».

## Итоговая рекомендация: гостевой режим, фазовые ворота, второй аккаунт по требованию

**Включать Connect, сохраняя music provider, можно — как E1, с явной политикой «один источник за раз».** Наиболее безопасная конфигурация для этой лаборатории: Soloist, `up77eb9a80`, имя `Player name`, `player_only`, `lossless`, crossfade 0, normalization on, один выбранный плеер. Connect — не постоянный «второй вход» колонки, а гостевой режим телефона: пока он активен, голосовые pause/next, локальная громкость и TTS по коду должны работать (T6/T7/T11), а «включи любимое» и любой `play_media` рассматриваются как смена источника, а не как параллельное воспроизведение.

| Фаза | Действия | Gate | Go | No-go |
|---|---|---|---|---|
| 0. Pre-flight (30 мин) | Baseline, ключ, сеть, аккаунт, backup, решение по 2.10.8 | **G0** | Ключ доступен; телефон в L2; backup есть; версия выбрана | Ключа нет и новый создавать рискованно; guest VLAN; нет backup |
| 1. Install (30 мин) | Плагин, Soloist, `up77eb9a80`, настройки | **G1** | Демон загрузился; устройство видно; один трек играет; stop отпускает; откат работает | Устройство не видно после сетевых проверок; universal player не работает |
| 2. Core validation (1–2 ч) | T3–T11, T15–T16, 60-мин soak | **G2** | Нет dropouts; metadata/transport/TTS проходят; voice pause/next задокументированы | Mid-track stops воспроизводятся на Connect; dropouts неприемлемы |
| 3. Conflict validation (1 ч + окно 429) | T12–T14; favorites после 429; при необходимости арбитражный скрипт | **G3** | Нет race, убивающего новый playback; либо политика «pause before favorites» принята | Непредсказуемый double-stop; семья не принимает правила |
| 4. Mode decision (1 неделя) | E1 vs E2 vs D; наблюдение за ghost/stuck | **G4** | Неделя без stuck/ghost; правила понятны | Постоянные сбои; E2 недоступен; D |
| 5. Hardening (ongoing) | Monitor Soloist expiry, MA pin, docs, backup | **G5** | Обновления не ломают; откат всегда доступен | Нет владельца обновлений |

**Как именно избежать известного конфликта.** Самое надёжное — не полагаться на арбитраж MA, которого нет, а сделать его в HA-скрипте: перед `music_assistant.play_media` для «включи любимое» вызвать `media_player.media_stop` на `media_player.iandeks_stantsiia_midi`, дождаться, пока состояние не станет `idle`/`off` (или выдержать 2–3 с), и только потом запускать очередь. Это снимает Connect-сессию и снижает вероятность того, что `SESSION_INACTIVE` остановит новый playback. Если и это нестабильно — не держать Connect включённым постоянно: включать плагин на время телефонного сценария и выключать перед голосовыми. Второй вариант — E2: отдельный Spotify-аккаунт для Connect, тогда music-провайдер и телефон не делят stream-slot, но правило «один источник на Pulsar» всё равно остаётся.

**Когда второй аккаунт — действительно чистое решение.** Если цель — чтобы телефон мог независимо играть Spotify в наушниках/другой комнате, пока MA играет Spotify на Pulsar, или чтобы два человека слушали разные Spotify-сессии, E2 обязателен: у каждого аккаунта свой stream-slot, и они не останавливают друг друга. Если цель — только «телефон переключает на колонку, голос ставит паузу», второй аккаунт не нужен: E1 с одним аккаунтом и политикой смены источника достаточен, а второй аккаунт добавит две библиотеки, два key/pairing и риск multi-account leak.

**Чего не делать.** Не запускать native receiver на станции (B): egress-guard, armhf/rootfs, отсутствие подтверждённого аудиосервера и ALSA/ducking-конфликты делают это проектом с ослаблением security-политики, а не «ещё одной настройкой». Не включать Connect «навсегда» до T13: цена ошибки — голосовая команда, которая останавливает музыку вместо того, чтобы её включить. Не отправлять pause/stop на ESPHome-сущность во время Sendspin: это выход из группы, а не пауза Spotify.

## Вывод

Главный сдвиг в понимании: задача не в том, чтобы «добавить Spotify Connect в колонку», а в том, чтобы признать, что Connect — это **вторая плоскость управления тем же единственным выходом**. Протокол решён: Soloist и MA уже умеют то, что нужно, а Pulsar не требует ни одной строки изменений. Всё, что остаётся, — продуктовое решение о владении источником: кто главный, когда телефон забирает колонку, что делает голосовая команда во время Spotify-сессии и кто отвечает за возврат к MA-очереди. Пока это правило не сформулировано, любой Connect будет «конфликтовать»; после формулировки конфликт становится предсказуемым handoff’ом.

Практический вывод для этой лаборатории: включать E1 как гостевой режим, с `player_only`, одним устройством и арбитражным скриптом для «включи любимое»; E2 — только если нужна независимая одновременная Spotify-сессия на разных выходах; D — если app presence не обязателен. 429 ортогонален: Connect обходит Web API и возвращает телефону playback, но не лечит библиотеку и не возвращает «любимое». Именно поэтому Connect стоит рассматривать не как замену текущему голосовому пути, а как дополнительный, обратимый режим, который живёт рядом с ним — и уступает ему, когда голос просит музыку.
