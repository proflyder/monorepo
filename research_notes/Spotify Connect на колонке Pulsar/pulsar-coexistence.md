# Spotify Connect на колонке Pulsar: сосуществование с текущим Spotify/Soloist-путём

**Дата снимка:** 2026-10-26, ~07:05 +05. **Режим:** только чтение. Не менялись HA/MA/Pulsar, не выполнялись restart/sync/playback, секреты не печатались.

**Обозначения источников:**
- **[L]** — локальная read-only проверка на `labuser@192.0.2.25` (SSH/`docker exec`: `ls`, `grep`, `jq`, `ps`, `df`, `ip`), 2026-10-26.
- **[C]** — код Music Assistant 2.10.8 / Pulsar v0.4.1, прочитанный локально.
- **[D]** — локальные документы проекта или официальная документация.

**Короткий вывод:** архитектурно `spotify_connect` можно включить рядом с music-провайдером `spotify`. Он должен выбрать **не** сырой Sendspin-протокол-плеер, а MA-universal-плеер `up77eb9a80` («Яндекс Станция Миди»), который уже отдаёт звук в Sendspin → Pulsar. Оба провайдера могут быть загружены одновременно, но **одновременно играть на одной колонке не могут**; при одном и том же Spotify-аккаунте они вдобавок делят единственный stream-slot аккаунта. Текущий Web API 429 не должен влиять на Connect, потому что Connect не использует Web API.

---

## 1. Может ли MA Spotify Connect выбрать существующий Pulsar/Sendspin-плеер как выход?

### Takeaway
Да, но целью должен быть **MA universal player** `up77eb9a80` («Яндекс Станция Миди»), а не Sendspin protocol player `fbp82cJ5eC3MlmMM8DHb_SWT6aYGNwXHPMZLum42iFc`. Connect вешает на выбранный MA-плеер live `AudioSource`; universal player маршрутизирует воспроизведение в свой активный output protocol — Sendspin.

### Cited Findings
- Плагин делает Spotify Connect-устройство из «any Music Assistant player (or sync group)»; на каждого выбранного плеера — свой daemon, свой zeroconf и свой `AudioSource` (`item_id` = player id). — [MA 2.10.8 `spotify_connect/README.md`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/README.md)
- Список «Connected players» фильтруется по `PlayerType.PLAYER`, `STEREO_PAIR`, `GROUP`; `PROTOCOL`-плееры в него не попадают. — [MA 2.10.8 `helpers/config_entries.py`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/helpers/config_entries.py)
- В текущей MA зарегистрированы: universal player `up77eb9a80` («Яндекс Станция Миди», provider `universal_player`, type `player`) и Sendspin protocol player `fbp82…` (type `protocol`), связанный с `up77eb9a80`. — **[L]** `settings.json`, `musicassistant.log`; [MA log line: “Player (type player) registered: up77eb9a80/Яндекс Станция Миди”]
- HA-сущность `media_player.iandeks_stantsiia_midi` имеет `unique_id = up77eb9a80`; это именно universal player, а не ESPHome-сущность `media_player.iandeks_stantsiia_midi_media_player` (`unique_id = 02:00:00:00:00:02/0/media_player/Media Player`). — **[L]** `homeassistant/config/.storage/core.entity_registry`
- Universal Player «does NOT have PLAY_MEDIA capability»; воспроизведение уходит в выбранный output protocol. — [MA 2.10.8 `universal_player/README.md`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/universal_player/README.md)
- Live `AudioSource` — это не `PLAY_MEDIA`, а отдельный source-flow: `_handle_select_source` вызывает `_start_audio_source`, а `_handle_play_media` для `AUDIO_SOURCE` выбирает output protocol и вызывает `play_media` уже у protocol-плеера. — [MA 2.10.8 `controllers/players/controller.py`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/controllers/players/controller.py)
- В логе MA уже есть подтверждение маршрутизации universal → Sendspin: `Setting active output protocol on Яндекс Станция Миди to Sendspin`. — **[L]** `musicassistant.log`, 2026-10-26 05:04:20.
- Официальная документация: «Any MA player can be exposed as a Spotify Connect device, including groups»; при этом «Home Assistant players are supported but not recommended — native Music Assistant players work best». — [Music Assistant: Spotify Connect](https://music-assistant.io/plugins/spotify-connect/)

### Inferences
- В UI плагина нужно выбирать **«Яндекс Станция Миди» (`up77eb9a80`)**, а не строку Sendspin/protocol. Сырой `fbp82…` вообще не будет предложен.
- Это **не** тот «HA player», который документация не рекомендует: universal player — MA-native, а Sendspin — нативный MA player provider. Но связка «universal player + Connect» официально не описана, поэтому её надо проверить, а не считать гарантированной.
- ESPHome-сущность `media_player.iandeks_stantsiia_midi_media_player` **не нужно** добавлять в MA/Connect: это другой путь (HA media relay), и его transport-команды заставляют Pulsar выйти из Sendspin-группы (см. §6).

### Gaps
- Нет эмпирического подтверждения, что плагин 2.10.8 корректно ведёт live `AudioSource` именно на universal player в этой конфигурации; официально проверенный путь — «native MA player», а universal player — обёртка.
- Неизвестно, как HA-интеграция MA отобразит `source_list`/`active_source` для universal player после выбора Connect-источника; это надо снять в тесте.

---

## 2. Точный аудиотракт: Spotify app → Connect → MA → Sendspin → Pulsar

### Takeaway
Звук идёт **не через колонку в интернет** и не через PulseAudio на Pulsar. Soloist-демон на неттопе принимает Spotify Connect-сессию, декодирует в PCM и отдаёт в PulseAudio pipe-sink → FIFO; MA-стример читает FIFO через ffmpeg, перекодирует под Sendspin и отправляет на Pulsar, который пишет PCM прямо в ALSA.

### Cited Findings
- Soloist backend: `Spotify app → Connect protocol → soloist daemon → PulseAudio module-pipe-sink → FIFO (s32le/44.1 kHz/2ch) → MA streams controller (NAMED_PIPE, ffmpeg) → MA player`. — [MA 2.10.8 `spotify_connect/soloist/README.md`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/soloist/README.md)
- Connect-провайдер отдаёт `StreamDetails` с `stream_type=NAMED_PIPE`, `decoded_audio_format` и `audio_format`; для Soloist это PCM, который читает ffmpeg. — [MA 2.10.8 `spotify_connect/provider.py`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/provider.py)
- Официальная документация: сигнальный путь показывает декодированный PCM «44.1 kHz/32-bit with Soloist», а не исходное качество Spotify. — [Music Assistant: Spotify Connect](https://music-assistant.io/plugins/spotify-connect/)
- В MA-контейнере есть `pulseaudio 17.0` и `ffmpeg 7.1.2`; MA работает с `network_mode: host`. Music-провайдер уже проверен на playback через MA web player (Soloist → тот же `helpers/pulse_capture.py`), то есть PA-capture инфраструктура в этом контейнере рабочая. — **[L]** `docker exec`; [projects/home-assistant/stack/README.md](file:///Users/developer/Desktop/yandex-station-lab/projects/home-assistant/stack/README.md); [docs/HISTORY.md](file:///Users/developer/Desktop/yandex-station-lab/docs/HISTORY.md)
- MA Sendspin server слушает `:8927`; Pulsar Sendspin — server-initiated endpoint: библиотека слушает `ws://<ip>:8928/sendspin`, устройство анонсируется как `_sendspin._tcp`, MA подключается к нему. — [Pulsar v0.4.1 `sendspin_service.h`](https://github.com/Morvex885/pulsar/blob/v0.4.1/satellite/src/sendspin/sendspin_service.h); **[L]** `ss -lntp` (8927 на неттопе), [docs/STATE.md](file:///Users/developer/Desktop/yandex-station-lab/docs/STATE.md)
- Pulsar принимает FLAC/Opus/PCM, 2 канала, 16 bit, 44.1 или 48 kHz (FLAC — первым в списке предпочтений); декодированный PCM пишется в ALSA `speakers` (dmix, вместе с GStreamer-каналами). — [Pulsar v0.4.1 `sendspin_service.cpp`](https://github.com/Morvex885/pulsar/blob/v0.4.1/satellite/src/sendspin/sendspin_service.cpp); [Pulsar v0.4.1 `sendspin_service.h`](https://github.com/Morvex885/pulsar/blob/v0.4.1/satellite/src/sendspin/sendspin_service.h)
- На станции egress-guard режет публичный WAN, но разрешает private LAN; сам Pulsar не ходит в Spotify. — [docs/STATE.md](file:///Users/developer/Desktop/yandex-station-lab/docs/STATE.md); [docs/ARCHITECTURE.md](file:///Users/developer/Desktop/yandex-station-lab/docs/ARCHITECTURE.md)

### Inferences
Ожидаемая цепочка для этой лаборатории:

```text
Spotify app (phone, same L2)
  → Spotify Connect session to soloist daemon on lab-host (x86_64, MA container)
  → soloist decodes to PCM 44.1 kHz/32-bit into private PulseAudio pipe-sink FIFO
  → MA streams controller: ffmpeg reads FIFO (-readrate), converts to Sendspin format
  → MA Sendspin server 192.0.2.25:8927
  → Pulsar Sendspin endpoint :8928 (MA initiates; station is a client/endpoint)
  → Pulsar decodes FLAC/Opus/PCM → ALSA "speakers" dmix → динамики
```

- Spotify CDN/audio traffic идёт **с неттопа**, а не со станции. Egress-guard Pulsar не мешает Connect; ему нужен только LAN до MA.
- Практический потолок качества — **16 bit / 44.1 или 48 kHz** на Sendspin. Spotify может отдать lossless до 24/44.1 (фактическое качество opaque), но Pulsar примет только 16-bit FLAC/Opus/PCM. Это не bit-perfect lossless до колонки.
- Pulsar не является «сервером с PulseAudio»: у него нет PA-сервера, только ALSA/GStreamer. PA 17.0 живёт внутри MA-контейнера на неттопе; это и есть причина, почему armhf-станция не ограничивает Connect.

### Gaps
- Точный выбранный Sendspin-кодек (FLAC vs Opus) и фактический sample rate не измерены. По коду Pulsar предпочитает FLAC 48 kHz/16 bit, поэтому ожидается FLAC 48k/16; но это вывод, не замер.
- Не измерены end-to-end задержка и поведение буфера Sendspin при live-источнике; официально MA предупреждает о «short delay» и о том, что прогресс в Spotify-приложении может опережать звук. — [Music Assistant: Spotify Connect](https://music-assistant.io/plugins/spotify-connect/)

---

## 3. Можно ли держать `spotify` (music provider) и `spotify_connect` (plugin) одновременно? Какие ровно конфликты?

### Takeaway
Да, оба провайдера могут быть включены одновременно: это разные типы провайдеров (`music` и `plugin`), у них разные instance id, разные Soloist data dir и разные роли. Конфликты возникают не при загрузке, а при **воспроизведении**: один active source на плеер, один stream-slot на Spotify-аккаунт (если аккаунт тот же), и один физический выход. MA-очередь при этом не разрушается, а становится неактивной.

### Cited Findings
- Connect-плагину **не нужны** Spotify Web API credentials и **не нужен** настроенный Spotify music provider. — [MA 2.10.8 `spotify_connect/README.md`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/README.md)
- Плагин — `type: plugin`, `domain: spotify_connect`, `multi_instance: false`, stage `beta`. — [MA 2.10.8 `spotify_connect/manifest.json`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/manifest.json)
- Music-провайдер Spotify — `type: music` (в текущей MA instance `spotify--NK972n5q`), с Soloist backend. — **[L]** `settings.json`; [MA 2.10.8 `spotify/README.md`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify/README.md)
- Soloist music-провайдер: source capacity **1** — «the account's single stream»; один `--single-track` daemon на трек. — [MA 2.10.8 `spotify/README.md`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify/README.md)
- Connect: один daemon на каждого выбранного плеера, свой credentials/cache dir по ключу `{instance_id}_{safe_player_id}`, свой zeroconf и свой `AudioSource`; все daemons делят managed Soloist binary и API key. — [MA 2.10.8 `spotify_connect/README.md`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/README.md)
- Фактические пути: music-провайдер использует `/data/spotify/spotify--NK972n5q/soloist-data`; Connect будет использовать `/data/spotify_connect/spotify_connect_up77eb9a80/soloist-data` (identity key = `{instance_id}_{safe_player_id}`). Каталога `/data/spotify_connect` сейчас **нет** — Connect ещё не включали. — **[L]** `find /data/...`; [MA 2.10.8 `spotify_connect/soloist/backend.py`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/soloist/backend.py)
- `AudioSource` создаётся с `exclusive=True`, `allow_external_trigger=True`, `can_initiate=True`; при выборе на другом плеере провайдер останавливает предыдущий. — [MA 2.10.8 `spotify_connect/provider.py`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/provider.py)
- Live-источник не является содержимым очереди: «A live source is not queue content: it plays on the player while the queue keeps its own items. Selecting it is the real operation». — [MA 2.10.8 `controllers/player_queues/controller.py`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/controllers/player_queues/controller.py)
- `_start_audio_source`: «The player's queue is left exactly as it is: it simply stops being the active source, so it is still there to resume when the source ends». — [MA 2.10.8 `controllers/players/controller.py`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/controllers/players/controller.py)
- Connect **не** зеркалит очередь Spotify в MA: события `QUEUE_CHANGED` явно игнорируются («queue snapshots are not consumed yet»). — [MA 2.10.8 `spotify_connect/provider.py`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/provider.py)
- Connect-демон не логинится со стороны MA: «MA never signs the daemon in — any Spotify account may claim it from its app». Аккаунт выбирает телефон в Spotify app. — [MA 2.10.8 `spotify_connect/soloist/README.md`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/soloist/README.md)
- Advertised name задаётся шаблоном: `Player name | Music Assistant` (default), `Player name`, `Music Assistant | Player name`; переименование плеера перезапускает daemon. — [Music Assistant: Spotify Connect](https://music-assistant.io/plugins/spotify-connect/); [MA 2.10.8 `helpers/config_entries.py`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/helpers/config_entries.py)

### Inferences

**Конфликтная матрица:**

| Измерение | Поведение при сосуществовании | Конфликт? |
|---|---|---|
| Загрузка провайдеров | `spotify` (music) + `spotify_connect` (plugin) грузятся независимо; Connect не требует music provider | Нет |
| Один stream-slot аккаунта | Soloist music-провайдер имеет capacity 1; Connect-демон играет в тот же аккаунт, если телефон залогинен в тот же Spotify-аккаунт | **Да, если аккаунт тот же**: одновременно два потока невозможны |
| Разные аккаунты | Connect-демон может быть заявлен другим Spotify-аккаунтом; у каждого аккаунта свой stream-slot | На уровне аккаунта — нет; на уровне колонки — всё равно один источник |
| Очередь MA | Connect-источник не очередь; очередь остаётся, но inactive; после конца источника сама не resume | Нет потери очереди; есть UX-пауза |
| Active source | Один active source на плеер; Connect exclusive; при выборе Connect текущее queue-воспроизведение останавливается | **Да, взаимоисключение** |
| Transport | Пока Connect active, play/pause/seek/next/prev/shuffle/repeat идут в Spotify; иначе — в MA queue | Нет, если команды идут на MA-плеер |
| Volume | По умолчанию `player_only`: Spotify-слайдер фиксируется на 100%, громкость принадлежит MA/Pulsar; `sync_spotify` синхронизирует | Не конфликт, но ожидания пользователя надо выбрать заранее |
| Soloist data dir | Разные каталоги; один engine не блокирует другой | Нет |
| Soloist binary | Общий managed binary (`/data/soloist/soloist`, 1.3.10.13); обновление/90-day expiry общее | Не конфликт, но общая точка maintenance |
| API key | Music-провайдер хранит свой ключ; Connect setup просит ключ отдельно; значения не печатались | Не конфликт; **не создавать новый ключ вслепую** (см. §9) |
| Device name | Один Connect-девайс на выбранный плеер; music-провайдер в `--single-track` не анонсирует Connect-девайс | Нет; stale-записи в Spotify app возможны |
| Сеть/mDNS | Один mDNS-анонс на universal player; MA host network | Нет, если телефон в той же L2 |

Ключевой вывод: **оба провайдера могут быть включены, но не могут одновременно играть на Pulsar**. Если телефон и music-провайдер используют **один** Spotify-аккаунт, добавьте к этому единственный stream-slot аккаунта. Если **разные** — stream-slot независимы, но MA-плеер всё равно один.

Отдельный риск (inferred, требует теста): при старте queue-воспроизведения (например, «включи любимое») MA сначала останавливает Connect-источник. Это вызывает `deactivate()` Connect-сессии; событие `SESSION_INACTIVE` у Connect-провайдера планирует `cmd_stop` активного плеера. Если это событие обработается уже после старта новой очереди, оно может остановить **новый** playback. В коде есть защиты от stale-session, но именно этот race на одном и том же аккаунте надо проверить отдельным тестом (§10, T13).

### Gaps
- Не проверено эмпирически, что именно произойдёт при одном аккаунте: Connect-сессия уступит stream music-провайдеру, music-провайдер уступит Connect, или обе стороны будут циклически останавливать друг друга.
- Неизвестно, какой аккаунт используется в Spotify app на телефоне и совпадает ли он с аккаунтом music-провайдера.
- Неизвестно, где хранится существующий Soloist API key вне MA (в проекте `secrets/` его нет); без него setup Connect не завершить, а создание нового ключа — внешний риск для music-провайдера.

---

## 4. Поведение переключений: телефон → колонка, колонка → телефон

### Takeaway
Переключение на колонку выглядит как штатный Spotify Connect transfer: приложение выбирает MA-устройство, Connect-демон становится active, MA получает `PLAYING`/`SESSION_ACTIVE` и через debounce запускает live `AudioSource` на universal player. Переключение обратно на телефон останавливает MA-плеер и освобождает устройство; очередь MA остаётся, но не возобновляется автоматически.

### Cited Findings
- Внешний триггер: `PLAYING` → debounced `play_media` на target player (явно выбранный или собственный плеер daemon). — [MA 2.10.8 `spotify_connect/README.md`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/README.md); [MA 2.10.8 `provider.py`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/provider.py)
- Debounce `PLAY_MEDIA_DEBOUNCE_S = 0.5 s`; ожидание старта `PLAYBACK_START_TIMEOUT_S = 3.0 s`. — [MA 2.10.8 `spotify_connect/provider.py`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/provider.py)
- При выборе нового source MA сначала останавливает текущее воспроизведение, ждёт `IDLE` (до 5 s), затем стартует live source; очередь сохраняется. — [MA 2.10.8 `controllers/players/controller.py`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/controllers/players/controller.py)
- `SESSION_INACTIVE` (телефон забрал playback / deselected device) → `_clear_active_player` + stop активного MA-плеера. — [MA 2.10.8 `provider.py`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/provider.py)
- Официально: «Stopping playback in Music Assistant releases the device in the Spotify app; moving playback to another device in the Spotify app stops the MA player». — [Music Assistant: Spotify Connect](https://music-assistant.io/plugins/spotify-connect/)
- Soloist pipe-sink не даёт EOF на pause; поэтому при `PAUSED` провайдер сам останавливает MA-плеер (bounded stop), а claim сохраняется для resume. — [MA 2.10.8 `spotify_connect/soloist/README.md`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/soloist/README.md)

### Inferences
**Телефон → колонка (transfer):**
1. В Spotify app выбирается «Яндекс Станция Миди» (имя зависит от шаблона).
2. Connect-демон получает active session; провайдер ждёт 0.5 s и вызывает `play_media` live-источника.
3. MA останавливает текущую очередь (если играла), выбирает `AudioSource`, выбирает output protocol Sendspin, запускает поток.
4. Звук появляется с задержкой ~ debounce + буфер; прогресс в приложении может опережать звук.
5. MA-очередь остаётся в памяти, но inactive.

**Колонка → телефон (transfer):**
1. В Spotify app выбирается телефон/другое устройство.
2. Connect-демон получает `SESSION_INACTIVE`; MA останавливает плеер, освобождает source; телефон продолжает playback.
3. MA-очередь не resume автоматически; чтобы вернуться к ней, нужно нажать play/выбрать queue source.

**Пауза vs transfer:**
- Пауза из приложения (`PAUSED`) не освобождает устройство: провайдер останавливает MA-плеер, но держит claim; play из приложения снова запускает `play_media`.
- Transfer на телефон (`SESSION_INACTIVE`) освобождает устройство.
- Stop из MA освобождает устройство (app больше не считает колонку активной).

### Gaps
- Точная задержка переключения и величина расхождения прогресса не измерены.
- Поведение Spotify app при повторном выборе устройства после паузы/stop (нужно ли заново выбирать из списка) не проверено.
- Если телефон и сервер в разных подсетях/VLAN или включён AP isolation, mDNS не увидит устройство; это сетевой тест, не код.

---

## 5. Метаданные, состояние, транспорт: что синхронизируется и как

### Takeaway
Connect отдаёт в MA live-метаданные (title/artist/album/artwork/position), состояние playback и volume; MA обновляет очередь-элемент live-источника и HA-сущность. Обратно в Spotify уходят transport/seek/next/prev/shuffle/repeat (и volume в `sync_spotify`). Очередь Spotify в MA **не** отображается: MA видит только текущий трек, а не список следующих.

### Cited Findings
- Live metadata: title/artist/album/artwork/position пушатся в `StreamDetails.stream_metadata` активного queue item — тот же канал, что ICY radio metadata. — [MA 2.10.8 `spotify_connect/README.md`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/README.md)
- `POSITION` обновляет `elapsed_time`; `OPTIONS_CHANGED` — shuffle/repeat; `VOLUME` — volume; `QUEUE_CHANGED` игнорируется. — [MA 2.10.8 `provider.py`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/provider.py)
- `_forward_to_external_source` проксирует PLAY/PAUSE/SEEK/NEXT/PREVIOUS в активный AudioSource; `_audio_source_owner` поднимается от protocol-плеера к parent (universal). — [MA 2.10.8 `controllers/players/controller.py`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/controllers/players/controller.py)
- Sendspin controller role на Pulsar отправляет `PLAY/PAUSE/STOP/NEXT/PREVIOUS` в MA; MA вызывает `cmd_play/cmd_pause/cmd_stop/cmd_next_track/cmd_previous_track`. — [Pulsar v0.4.1 `sendspin_service.h`](https://github.com/Morvex885/pulsar/blob/v0.4.1/satellite/src/sendspin/sendspin_service.h); [MA 2.10.8 `providers/sendspin/player.py`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/sendspin/player.py)
- Pulsar `on_metadata` только логирует title/artist; отдельного показа текста трека на часах нет. LED/clock используют visualizer-данные Sendspin. — [Pulsar v0.4.1 `sendspin_service.cpp`](https://github.com/Morvex885/pulsar/blob/v0.4.1/satellite/src/sendspin/sendspin_service.cpp); [Pulsar v0.4.1 README](https://github.com/Morvex885/pulsar/blob/v0.4.1/README.md)
- Volume modes: `player_only` (default; daemon pinned 100%, MA player owns audible volume) или `sync_spotify` (app slider ↔ player volume, с кубической компенсацией). — [MA 2.10.8 `spotify_connect/soloist/README.md`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/soloist/README.md); [Music Assistant: Spotify Connect](https://music-assistant.io/plugins/spotify-connect/)

### Inferences
- В HA `media_player.iandeks_stantsiia_midi` будут title/artist/album/artwork/position от Connect; это тот же entity, что используется голосом.
- На Pulsar метаданные не отображаются текстом на часах; работают LED-визуализация (и, в v0.4.1, опциональная music-визуализация на часах, если включена). Это не регрессия — так и сейчас с MA/Sendspin.
- Next/previous из Pulsar (локальный voice) и из HA будут работать, пока Connect active: Sendspin controller → MA `cmd_next_track` → `_forward_to_external_source` → Spotify.
- MA UI не покажет очередь Spotify; только текущий item. Планировать «следующий трек» через MA-очередь нельзя, пока играет Connect.
- С `player_only` слайдер в Spotify app будет стоять на 100%; это by design, не баг. Если нужен контроль из приложения — `sync_spotify`, но с документированным риском brief full-scale artifact при резком повышении громкости.

### Gaps
- Точность position/artwork и задержка metadata не измерены.
- Не проверено, как HA-интеграция отобразит `media_position`/`media_duration` для live `AudioSource` (может быть 0/неизвестно).
- Поведение pause→stop→resume: после `PAUSED` провайдер останавливает MA-плеер; не проверено, останется ли source выбранным для голосового «продолжи» (см. §6).

---

## 6. Поведение голосовых команд в этой архитектуре

### Takeaway
Голосовой контур (wake word → Whisper → HA intent → Piper) не меняется. Пока Connect active, транспортные команды, идущие на MA-плеер, проксируются в Spotify; громкость идёт в ESPHome-плеер (локальная громкость Pulsar). TTS/announce duck’ит Sendspin, не разрывая Connect-сессию. «Включи любимое» — единственная команда, которая явно конфликтует: она запускает MA-очередь и вытесняет Connect.

### Cited Findings
- Текущие intent-скрипты: «пауза/продолжи/стоп» → `media_player.media_pause/play/stop` на `media_player.iandeks_stantsiia_midi`; «громче/тише» → `media_player.volume_set` на ESPHome `media_player.iandeks_stantsiia_midi_media_player`; «включи любимое» → `browse_media` + `music_assistant.play_media` (`enqueue: replace`). — [projects/home-assistant/ha-voice/intent_scripts.yaml](file:///Users/developer/Desktop/yandex-station-lab/projects/home-assistant/ha-voice/intent_scripts.yaml)
- `_handle_cmd_pause/play` сначала ищут активный AudioSource (с подъёмом protocol → parent) и проксируют в plugin; `_handle_cmd_stop` останавливает плеер, что приводит к release Connect-сессии. — [MA 2.10.8 `controllers/players/controller.py`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/controllers/players/controller.py)
- Pulsar локальные команды: «стоп/хватит» при музыке вызывают `music_pause()` (то есть pause, не stop); «пауза» → `music_pause`; «продолжи» → `music_resume`; «дальше/назад» → Sendspin NEXT/PREV. — [Pulsar v0.4.1 `controller.cpp`](https://github.com/Morvex885/pulsar/blob/v0.4.1/satellite/src/controller.cpp)
- Pulsar duck’ит музыку (включая Sendspin) до `kDuckGain = 0.15` на время диалога, HA announcement, timer ring или closing earcon; TTS идёт в ANNOUNCE-канал. — [Pulsar v0.4.1 `controller.cpp`](https://github.com/Morvex885/pulsar/blob/v0.4.1/satellite/src/controller.cpp)
- MA `_handle_play_media` не освобождает live source для `MediaType.ANNOUNCEMENT` — announce не должен рвать Connect-сессию. — [MA 2.10.8 `controllers/players/controller.py`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/controllers/players/controller.py)
- Если HA отправляет pause/stop **на ESPHome-плеер** во время Sendspin, Pulsar вызывает `yield_to_local()` / выходит из Sendspin-группы. — [Pulsar v0.4.1 `controller.cpp`](https://github.com/Morvex885/pulsar/blob/v0.4.1/satellite/src/controller.cpp)
- «Включи любимое» зависит от Web API/Liked Songs; при 429 плейлист пуст, команда отвечает «Любимый плейлист пока не синхронизирован…». — [docs/STATE.md](file:///Users/developer/Desktop/yandex-station-lab/docs/STATE.md); [logs/status-20261026-0510-ma.txt](file:///Users/developer/Desktop/yandex-station-lab/logs/status-20261026-0510-ma.txt)
- Текущая MA-очередь «Яндекс Станция Миди» пуста: `players/cmd/play: Resume queue requested but queue Яндекс Станция Миди is empty`. — **[L]** `musicassistant.log`, 05:09:01.

### Inferences

**Команда за командой:**

| Команда | Куда идёт | Поведение при Connect active | Риск |
|---|---|---|---|
| «громче/тише» | ESPHome media_player → Pulsar ALSA volume | Работает; в `player_only` громкость локальная, не Spotify | Нет |
| «пауза» (HA) | MA universal → `cmd_pause` → Connect provider | Ставит Spotify на паузу; MA-плеер останавливается после pause | Может понадобиться resume из app/MA |
| «продолжи» (HA) | MA universal → `cmd_play` | Если source ещё active — resume в Spotify; если source уже отпущен — может уйти в пустую очередь/ошибку | **Проверить** |
| «стоп/выключи музыку» (HA) | MA universal → `cmd_stop` | Останавливает плеер и освобождает Connect-устройство; Spotify app теряет active device | Ожидаемо, но надо пере-выбрать устройство |
| «стоп/хватит» (Pulsar local) | Sendspin controller → PAUSE | Фактически pause, не stop; Connect-сессия сохраняется | Меньше рискованно |
| «дальше/назад» (Pulsar local) | Sendspin controller → NEXT/PREV | Работает, пока Sendspin streaming/Connect active | Если стрим уже остановлен — «no Sendspin queue to skip in» |
| «включи любимое» | MA `browse_media` + `play_media` | Запускает MA-очередь, вытесняет Connect; сейчас блокировано 429 | **Главный конфликт**; тест после 429 |
| TTS/announce | HA/Pulsar ANNOUNCE | Duck Sendspin до 15%, затем restore; Connect-сессия не рвётся | Нет, но проверить на практике |

Дополнительно: **не отправляйте transport на ESPHome-сущность** (`media_player.iandeks_stantsiia_midi_media_player`) во время Connect. По коду Pulsar это `yield_to_local()` / выход из Sendspin-группы, то есть обрыв потока, а не пауза Spotify. Текущие скрипты так не делают — это важно сохранить.

### Gaps
- Не проверено, что голосовое «продолжи» после pause реально resume’ит Connect, а не пытается resume пустую MA-очередь (лог уже показывает пустую очередь).
- Не проверено, что HA announce + Pulsar duck не вызывает `SESSION_INACTIVE`; по коду не должен, но это эмпирика.
- Не проверено поведение «включи любимое» после восстановления 429 при активном Connect: возможен race остановки нового playback (см. §3).

---

## 7. Влияет ли текущий Spotify 429 на Connect или только на library/voice?

### Takeaway
Только на Web API-путь music-провайдера: синхронизацию библиотеки, поиск, browse, содержимое Liked Songs и, следовательно, голосовую команду «включи любимое». Connect не использует Spotify Web API; его playback и метаданные идут через Connect-протокол и Soloist, поэтому 429 не должен мешать Connect. Важно: Connect — **не голосовой источник**; `music_assistant.play_media` по-прежнему идёт в music-провайдер, а не в Connect.

### Cited Findings
- 429 в логах: `Not retrying _get_data, the server asked to wait 66521 seconds`; `Search on provider Spotify failed: Not retrying, asked to wait 66521 seconds`; фоновые `Sync Spotify artists/albums/tracks/playlists failed` с ~66494 s. — **[L]** [logs/status-20261026-0510-ma.txt](file:///Users/developer/Desktop/yandex-station-lab/logs/status-20261026-0510-ma.txt)
- 66521 s ≈ 18 h 28 m 41 s; от 05:03:16 +05 это ~23:31:57 +05. Более поздний лог 05:05:55 даёт тот же target (~23:31:57). На 07:04 +05 новых 429-записей не было; ожидаемое окно — до ~23:32 +05. — **[L]** лог; расчёт по `Retry-After`.
- Music-провайдер Spotify использует Web API для каталога/библиотеки и Soloist для аудио; source capacity 1. — [MA 2.10.8 `spotify/README.md`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify/README.md)
- Connect: «Music Assistant needs **no** Spotify Web API credentials and **no** configured Spotify music provider for this». — [MA 2.10.8 `spotify_connect/README.md`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/README.md)
- Connect-код не содержит обращений к `api.spotify.com/v1`; он работает через локальный WebSocket Soloist и Connect-протокол. — [MA 2.10.8 `spotify_connect/provider.py`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/provider.py)
- Официальная документация Connect не упоминает Web API; качество/метаданные идут от Connect-сессии. — [Music Assistant: Spotify Connect](https://music-assistant.io/plugins/spotify-connect/)

### Inferences
- При пустой MA-библиотеке Connect всё равно сможет играть трек, выбранный в Spotify app: контекст и аудио приходят из Connect, а не из MA-каталога.
- Connect **не** лечит 429 и не наполняет Liked Songs. «Включи любимое» останется сломанным до восстановления Web API.
- Если 429 — это лишь rate limit клиента MA, а не блокировка аккаунта, Connect playback не затронут. Если бы Spotify наложил account-level restriction, это была бы другая история; текущие логи показывают именно `Retry-After` rate limit.

### Gaps
- Нет эмпирической проверки Connect playback во время активного 429 (не включали плагин).
- Неизвестно, не сбросит ли 429 свой таймер/не продлится ли; ориентир ~23:32 +05 может измениться.
- Неизвестно, тот же ли Spotify-аккаунт использует телефон; если другой, независимость от 429 ещё выше, но это надо подтвердить.

---

## 8. Ограничения из-за Pulsar armhf / Sendspin endpoint (а не сервера с PulseAudio)

### Takeaway
Pulsar — Sendspin endpoint: он слушает `:8928`, принимает поток от MA, декодирует FLAC/Opus/PCM и пишет в ALSA. Никакого PulseAudio-сервера на станции нет и не требуется. Весь Connect/Soloist/PulseAudio/ffmpeg работает на неттопе x86_64. Armhf влияет только на потолок формата (16 bit, 44.1/48 kHz) и CPU декодирования, но не на возможность Connect.

### Cited Findings
- Pulsar Sendspin: server-initiated mode; библиотека слушает `ws://<ip>:8928/sendspin`; устройство анонсируется как `_sendspin._tcp`; декодированный PCM идёт в ALSA `speakers` (dmix, вместе с GStreamer). — [Pulsar v0.4.1 `sendspin_service.h`](https://github.com/Morvex885/pulsar/blob/v0.4.1/satellite/src/sendspin/sendspin_service.h)
- Pulsar поддерживает FLAC/Opus/PCM, 2 ch, 16 bit, 44.1/48 kHz; FLAC стоит первым. — [Pulsar v0.4.1 `sendspin_service.cpp`](https://github.com/Morvex885/pulsar/blob/v0.4.1/satellite/src/sendspin/sendspin_service.cpp)
- Pulsar README: динамики — HTTP-потоки через GStreamer прошивки, громкость через ALSA; штатный стек Яндекса заменён на Pulsar. — [Pulsar v0.4.1 README](https://github.com/Morvex885/pulsar/blob/v0.4.1/README.md); [docs/STATE.md](file:///Users/developer/Desktop/yandex-station-lab/docs/STATE.md)
- MA-контейнер: `network_mode: host`, внутри `pulseaudio 17.0`, `ffmpeg 7.1.2`; это обязательное условие для Soloist/mDNS/стриминга. — [projects/home-assistant/stack/README.md](file:///Users/developer/Desktop/yandex-station-lab/projects/home-assistant/stack/README.md); **[L]** `docker exec`
- Egress-guard станции: private LAN разрешён, публичный WAN — DROP; Pulsar не должен ходить в Spotify. — [docs/STATE.md](file:///Users/developer/Desktop/yandex-station-lab/docs/STATE.md)
- Официально Connect требует, чтобы телефон/планшет/компьютер были в той же сети, что MA-сервер; mDNS/zeroconf. — [Music Assistant: Spotify Connect](https://music-assistant.io/plugins/spotify-connect/)

### Inferences
- `armhf` — не блокер: Connect-демон и PulseAudio не запускаются на станции. Станция остаётся таким же Sendspin-эндпоинтом, как сейчас.
- Потолок качества: Spotify lossless 24/44.1 → MA → Sendspin 16/44.1 или 16/48. Это нормально для Midi, но не «lossless до колонки».
- Сетевые требования: телефон и неттоп в одной L2; сейчас на неттопе поднят только `wlp4s0 192.0.2.25/24` (Ethernet down), MA слушает 8927/8095/8097 на host network; Pulsar — 192.0.2.13. — **[L]** `ip -br addr`, `ss -lntp`
- Если позже включить Ethernet `.26`, mDNS может анонсировать устройство на двух интерфейсах; при проблемах с обнаружением официально предлагается `BIND TO` в Streams Settings. — [Music Assistant: Spotify Connect](https://music-assistant.io/plugins/spotify-connect/)

### Gaps
- Не проверено, видит ли Spotify app устройство через текущий Wi-Fi/роутер (AP isolation, multicast, iOS Local Network permission).
- Не измерены CPU/RAM второго Soloist-демона; запас есть (12 GiB available, MA ~385 MiB), но это не замер Connect.
- Не проверено, как поведёт себя mDNS при появлении второго интерфейса `.26`.

---

## 9. Минимальный обратимый план внедрения

### Takeaway
Минимальное изменение — **добавить один plugin** `Spotify Connect` в MA, выбрать backend Soloist, переиспользовать существующий Soloist API key, выбрать universal player `up77eb9a80` и оставить `player_only` volume. HA, Pulsar, music-провайдер и сеть не трогать. Откат — выключить plugin; данные Connect остаются, music/voice не затрагиваются.

### Cited Findings
**Шаг 0. Pre-flight (read-only, 10 мин).**
- Зафиксировать: список провайдеров MA, `media_player.iandeks_stantsiia_midi` → `up77eb9a80`, active output protocol, текущий 429 Retry-After, пустоту очереди, `ip -br addr`, `ss -lntp` (8927/8095/8097/5353). — **[L]**
- Проверить, что телефон в той же Wi-Fi-сети `192.0.2.0/24`, а не в гостевой/VLAN; что Spotify app имеет доступ к локальной сети (iOS Local Network).
- Убедиться, что есть **существующий Soloist API key** (тот, которым настроен music-провайдер). В `secrets/` проекта его нет; в MA он хранится в зашифрованном `setup_data` и не может быть прочитан как plaintext. — **[L]** `settings.json` (только факт наличия, без значений); [MA 2.10.8 `spotify/README.md`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify/README.md)
- **Не создавать новый API key вслепую**: неизвестно, инвалидирует ли Spotify старый ключ; это может сломать music-провайдер. Если ключ утерян — сначала найти его в Spotify developer dashboard/личном хранилище, затем действовать.

**Шаг 1. Backup.**
- `cd /home/labuser/nettop && ./ctl.sh backup` — штатный backup останавливает стек на время tar; это единственное «тяжёлое» действие, планировать окно. — [projects/home-assistant/stack/README.md](file:///Users/developer/Desktop/yandex-station-lab/projects/home-assistant/stack/README.md)
- Альтернатива без остановки — сохранить только `data/music-assistant/settings.json`; SQLite-базы (`library.db`, `auth.db`) на живом MA лучше не копировать через `cp` — штатный backup надёжнее.

**Шаг 2. Добавить plugin.**
- MA UI → **Settings → Plugins → Add a plugin → Spotify Connect**. — [Music Assistant: Spotify Connect](https://music-assistant.io/plugins/spotify-connect/)
- Backend: **Spotify Soloist** (recommended; go-librespot не подходит для нового аккаунта после дек. 2024). — [Music Assistant: Spotify Connect](https://music-assistant.io/plugins/spotify-connect/); [projects/home-assistant/stack/README.md](file:///Users/developer/Desktop/yandex-station-lab/projects/home-assistant/stack/README.md)
- Для создания personal API key нужен Spotify **Premium** (один раз); после этого устройство может заявить и Free-аккаунт (с рекламой). — [Music Assistant: Spotify Connect](https://music-assistant.io/plugins/spotify-connect/)
- Принять ToS/consent; вставить существующий personal API key. Ключ не печатать.
- Ожидаемый лог: `Loaded plugin provider Spotify Connect`; Soloist binary уже установлен (`/data/soloist/soloist`, 1.3.10.13), повторной загрузки быть не должно, если build не истёк. — **[L]** `musicassistant.log`; [MA 2.10.8 `spotify_connect/soloist/README.md`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/soloist/README.md)

**Шаг 3. Настроить plugin options.**
- **Connected players:** `Яндекс Станция Миди` (`up77eb9a80`). Не выбирать Sendspin/protocol `fbp82…`; он не появится в списке.
- **Advertised device name:** `Player name` (короткое «Яндекс Станция Миди») или default `Player name | Music Assistant`; выбрать одно, чтобы не путать со stale-записями.
- **Volume:** `Player volume only` (default) — качество и привычное управление с Pulsar/HA. `sync_spotify` — только если нужен слайдер в приложении.
- **Audio quality:** lossless (потолок; фактический поток всё равно 16-bit на Sendspin).
- **Crossfade:** 0 (не усложнять live-источник).
- **Loudness normalization:** on (default) — Spotify применяет; иначе MA будет нормализовать повторно.

**Шаг 4. Проверить без playback.**
- В логах: daemon start, нет `FATAL_ERROR`, нет `Giving up on Spotify Connect device`.
- В Spotify app устройство появилось в списке (same Wi-Fi). Если нет — 5–10 мин, перезапуск app, проверить 5353/AP isolation; не менять MA без причины.

**Шаг 5. Один тестовый трек.**
- Телефон → устройство → play. Ожидаемо: MA log `Spotify Connect session active`, `Starting Spotify Connect playback ... on player up77eb9a80`, `Setting active output protocol ... to Sendspin`; звук; HA state `playing`; метаданные.
- Сразу stop и убедиться, что MA-плеер вернулся в idle, а music-провайдер/голос живы.

**Шаг 6. Полная матрица §10, затем acceptance.**

### Inferences
- Это изменение затрагивает только MA: `settings.json`, `data/spotify_connect/`, runtime. HA-конфиг, Pulsar, ESPHome, Sendspin, сеть, music-провайдер не меняются.
- Plugin можно выключить без удаления; device исчезнет, data dir останется для повторного включения.
- Если API key недоступен — **не начинать** setup; это единственный внешний блокер.

### Gaps
- Неизвестно, где именно лежит существующий Soloist API key вне MA; в проекте его нет.
- Неизвестно, требует ли Spotify повторного consent/ключа при повторном добавлении plugin после удаления; при простом disable/enable, вероятно, нет, но это не проверено.

---

## 10. Матрица тестов, acceptance, rollback и что выключать при конфликте

### Takeaway
Тестировать надо не «играет/не играет», а пять границ: (1) обнаружение устройства, (2) transfer в обе стороны, (3) coexistence с MA-очередью и голосом, (4) 429-независимость, (5) откат. Критерий успеха — после disable/stop всё возвращается к текущему поведению без restart Pulsar/HA.

### Cited Findings
| # | Сценарий | Действие | Ожидаемо | Где смотреть | Если fail |
|---|---|---|---|---|---|
| T0 | Baseline | Снять провайдеров, player id, queue, 429, IP/порты | `up77eb9a80`, Sendspin, 429 ~23:32, очередь пуста | MA UI/log, `ss`, `ip` | — |
| T1 | Plugin load | Добавить Connect, Soloist, key | `Loaded plugin provider Spotify Connect`; нет fatal | MA log | Disable plugin |
| T2 | Discovery | Открыть Spotify device picker | «Яндекс Станция Миди…» виден | Spotify app | Проверить L2/5353/AP isolation; disable |
| T3 | Phone→speaker | Выбрать устройство, play трек | Звук; MA state playing; metadata | MA log/HA | Stop, disable |
| T4 | Metadata | Сменить трек, проверить title/artist/art | Обновление в пределах нескольких секунд; artwork | HA entity | Не блокер; зафиксировать |
| T5 | App transport | Pause/play/next/prev/seek из Spotify | Действует; звук следует | Spotify + HA | Disable |
| T6 | HA transport | `media_pause/play/stop` на MA entity | Pause/resume; stop освобождает device | HA + Spotify | Disable |
| T7 | Local voice | «пауза», «продолжи», «дальше», «назад» | Pause/next/prev работают; resume — проверить | Pulsar log/HA | Зафиксировать; disable если ломает |
| T8 | Volume | Кнопки Pulsar, HA volume, Spotify slider | Громкость меняется; slider 100% в player_only | Pulsar/HA | Переключить volume mode |
| T9 | Speaker→phone | В Spotify выбрать телефон | MA idle; телефон играет; очередь цела | MA log/HA | Disable |
| T10 | Phone→speaker back | Снова выбрать устройство | Playback возвращается; metadata | MA log | Disable |
| T11 | TTS/announce | Wake word/TTS во время Connect | Duck до 15%, затем restore; сессия жива | Pulsar/HA | Disable если рвёт |
| T12 | Queue coexistence | Запустить radio/MA queue при Connect | Connect останавливается/отпускается; queue играет; Connect не auto-resume | MA log | Disable |
| T13 | Favorites conflict | После 429: «включи любимое» при Connect | Ожидаемо: Connect уступает queue; нет остановки нового playback | MA log | **Главный risk**; pause Connect first или disable |
| T14 | 429 independence | При активном 429 играть Connect | Connect играет; search/library всё ещё 429 | MA log | Если Connect тоже 429 — это account-level, стоп |
| T15 | MA restart | `./ctl.sh restart ma` (в тестовом окне) | Device возвращается; phone reselect; player не stuck | MA log/HA | Disable; restore backup |
| T16 | Rollback | Disable plugin | Device исчез; player idle; music/voice работают | MA/HA | Restore backup |

**Acceptance criteria:**
- 10–15 мин непрерывного Connect playback без dropouts/перезапусков.
- T3/T4/T5/T6/T9/T10/T11/T12 pass; T7/T13 — либо pass, либо документированный known limitation.
- После T16 состояние идентично T0 (кроме появившегося `data/spotify_connect`).
- Нет роста CPU/памяти, ведущего к деградации HA/MA; запас 12 GiB, но замерить.

**Rollback (минимальный):**
1. MA UI → Settings → Plugins → **Spotify Connect → Disable**. Это выгружает provider, останавливает daemons и освобождает sources.
2. Проверить: device исчез из Spotify; `media_player.iandeks_stantsiia_midi` idle; music-провайдер и голос работают.
3. Если source залип: `media_player.media_stop` на MA entity; reload plugin; в крайнем случае `./ctl.sh restart ma`.
4. Если MA не стартует после изменений: остановить MA, восстановить `settings.json`/`data/music-assistant` из backup, поднять. Не удалять `/data/spotify_connect` до подтверждения, что он не нужен.
5. Pulsar/HA не трогать: их конфигурация не менялась.

**Что выключать при конфликте:**
- Всегда **только Connect plugin**, не music-провайдер и не Sendspin/Universal Player.
- Если конфликтует только «включи любимое»: pause/stop Connect перед командой; если это неприемлемо — disable Connect.
- Если конфликтует TTS: не disable Sendspin; сначала проверить, что announce идёт через ANNOUNCE, а не ESPHome media playback.
- Если конфликтует volume: сменить `player_only` ↔ `sync_spotify`, не трогая плеер.
- Если устройство не видно: сеть/BIND TO, не настройки Pulsar.

### Inferences
- T13 — самый вероятный источник «странного» поведения: один аккаунт + два источника + stop-on-session-inactive. Если он нестабилен, рабочий компромисс — держать Connect для телефона, а «включи любимое» использовать только после ручной паузы Connect, либо развести аккаунты.
- T7 «продолжи» может не сработать, потому что после pause провайдер останавливает MA-плеер, а MA-очередь пуста (лог 05:09). Это UX-ограничение, не обязательно блокер.

### Gaps
- Нет фактических результатов тестов: плагин не включён, playback не запускался.
- Неизвестно, появится ли устройство в Spotify app с первого раза в этой Wi-Fi-сети.
- Неизвестно, переживёт ли Connect-сессия restart MA без повторного выбора устройства.

---

## 11. Что подтверждено локально, что выведено из кода, и оставшиеся пробелы

### Takeaway
Локально подтверждены: версии, сеть, player id, HA-маппинг, Soloist, 429, отсутствие Connect. Из кода/документации выведены: выбор universal player, аудиотракт, coexistence-модель, transfer, metadata/transport, voice proxy. Эмпирически не проверено главное — реальный Connect playback и race «favorites vs Connect» на одном аккаунте.

### Cited Findings

| Утверждение | Статус | Источник |
|---|---|---|
| MA 2.10.8, HA 2026.10.4, host network, PA 17.0, ffmpeg 7.1.2 | **Confirmed local** | [docs/STATE.md](file:///Users/developer/Desktop/yandex-station-lab/docs/STATE.md); [projects/home-assistant/stack/README.md](file:///Users/developer/Desktop/yandex-station-lab/projects/home-assistant/stack/README.md); **[L]** `docker exec` |
| Pulsar v0.4.1, Sendspin player `fbp82…`, universal `up77eb9a80`, HA entity maps to `up77eb9a80` | **Confirmed local** | **[L]** `settings.json`, `musicassistant.log`, HA entity registry; [docs/STATE.md](file:///Users/developer/Desktop/yandex-station-lab/docs/STATE.md) |
| Music provider Spotify configured with Soloist 1.3.10.13; playback verified via MA web player (not Pulsar); 429 library | **Confirmed local** | [docs/HISTORY.md](file:///Users/developer/Desktop/yandex-station-lab/docs/HISTORY.md); [logs/status-20261026-0510-ma.txt](file:///Users/developer/Desktop/yandex-station-lab/logs/status-20261026-0510-ma.txt); **[L]** log |
| `spotify_connect` provider **not** configured; no `/data/spotify_connect` | **Confirmed local** | **[L]** `settings.json`, `find /data` |
| Connect plugin present in MA 2.10.8; supports Soloist; no Web API | **Confirmed code** | [MA 2.10.8 `spotify_connect/README.md`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/README.md) |
| Picker excludes PROTOCOL, includes PLAYER/STEREO_PAIR/GROUP | **Confirmed code** | [MA 2.10.8 `helpers/config_entries.py`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/helpers/config_entries.py) |
| Live source preserves queue, stops current playback, exclusive | **Confirmed code** | [MA 2.10.8 `controllers/players/controller.py`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/controllers/players/controller.py); [MA 2.10.8 `controllers/player_queues/controller.py`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/controllers/player_queues/controller.py) |
| Same-account Soloist = one stream | **Confirmed docs** | [MA 2.10.8 `spotify/README.md`](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify/README.md) |
| Pulsar formats/ALSA; no PA; duck 0.15 | **Confirmed code** | [Pulsar v0.4.1 `sendspin_service.cpp`](https://github.com/Morvex885/pulsar/blob/v0.4.1/satellite/src/sendspin/sendspin_service.cpp); [Pulsar v0.4.1 `controller.cpp`](https://github.com/Morvex885/pulsar/blob/v0.4.1/satellite/src/controller.cpp) |
| Connect can attach to `up77eb9a80` and route to Sendspin | **Inferred** | Code path; universal→Sendspin routing itself confirmed by log, Connect attachment not tested |
| Both providers coexist at config level | **Inferred/strong** | Separate types/IDs; no code conflict; not tested |
| Phone→speaker / speaker→phone behavior | **Inferred** | Code/docs; not tested |
| Voice transport proxies to Spotify | **Inferred** | Code path; not tested |
| 429 does not affect Connect | **Inferred/strong** | No Web API in Connect; not tested during 429 |
| TTS does not break Connect | **Inferred** | Announcement exception + Pulsar duck; not tested |
| Race favorites vs Connect stops new playback | **Inferred risk** | Session-inactive stop path; not tested |
| Universal player + Connect explicitly supported | **Gap** | Official docs only say “any MA player”; universal player not called out |

### Inferences
- Наиболее вероятный рабочий сценарий: включить Connect, выбрать universal player, играть с телефона; голосовые pause/next/volume и TTS работают; favorites — единственная проблемная команда.
- Наиболее безопасный режим для текущей лаборатории: `player_only` volume, device name без двусмысленности, Connect выключен по умолчанию, включается на время телефонного сценария.
- Если приоритет — голосовые «включи любимое» и единый аккаунт, Connect лучше не включать постоянно; если приоритет — телефон, держать Connect и не запускать favorites во время Connect.

### Gaps
- **Нет эмпирики:** плагин не включён, playback не запускался; все выводы о поведении — из кода/доков.
- **API key:** местоположение существующего Soloist key вне MA неизвестно; в `secrets/` проекта его нет.
- **Account:** неизвестно, тот же ли Spotify-аккаунт на телефоне, что у music-провайдера.
- **Network:** не проверены AP isolation, multicast, iOS Local Network permission, поведение при появлении `.26`.
- **Universal player:** официально не подтверждено, что Connect + universal player — поддерживаемая комбинация; нужен T3.
- **Pause/resume:** не проверено, что «продолжи» после pause не упирается в пустую MA-очередь.
- **Race:** не проверено, останавливает ли `SESSION_INACTIVE` новый queue playback при favorites.
- **Качество/задержка:** не измерены фактический кодек, sample rate, end-to-end latency, расхождение прогресса.
- **ToS:** Soloist через MA — на свой риск; плагин Connect — beta. Это не технический блокер, но риск для аккаунта.

---

### Source index (для отчёта)
- Локальные документы: [docs/STATE.md](file:///Users/developer/Desktop/yandex-station-lab/docs/STATE.md), [docs/ARCHITECTURE.md](file:///Users/developer/Desktop/yandex-station-lab/docs/ARCHITECTURE.md), [docs/HISTORY.md](file:///Users/developer/Desktop/yandex-station-lab/docs/HISTORY.md), [projects/home-assistant/stack/README.md](file:///Users/developer/Desktop/yandex-station-lab/projects/home-assistant/stack/README.md), [projects/home-assistant/HA.md](file:///Users/developer/Desktop/yandex-station-lab/projects/home-assistant/HA.md), [projects/home-assistant/ha-voice/README.md](file:///Users/developer/Desktop/yandex-station-lab/projects/home-assistant/ha-voice/README.md), [projects/home-assistant/ha-voice/intent_scripts.yaml](file:///Users/developer/Desktop/yandex-station-lab/projects/home-assistant/ha-voice/intent_scripts.yaml), [logs/status-20261026-0510-ma.txt](file:///Users/developer/Desktop/yandex-station-lab/logs/status-20261026-0510-ma.txt).
- MA 2.10.8: [Connect README](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/README.md), [Connect provider.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/provider.py), [Connect setup_flow.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/setup_flow.py), [Soloist README](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify_connect/soloist/README.md), [Spotify music README](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/spotify/README.md), [config_entries.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/helpers/config_entries.py), [players controller.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/controllers/players/controller.py), [player_queues controller.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/controllers/player_queues/controller.py), [Universal Player README](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/universal_player/README.md), [Sendspin player.py](https://github.com/music-assistant/server/blob/2.10.8/music_assistant/providers/sendspin/player.py).
- Pulsar v0.4.1: [README](https://github.com/Morvex885/pulsar/blob/v0.4.1/README.md), [sendspin_service.h](https://github.com/Morvex885/pulsar/blob/v0.4.1/satellite/src/sendspin/sendspin_service.h), [sendspin_service.cpp](https://github.com/Morvex885/pulsar/blob/v0.4.1/satellite/src/sendspin/sendspin_service.cpp), [controller.cpp](https://github.com/Morvex885/pulsar/blob/v0.4.1/satellite/src/controller.cpp).
- Официальная документация: [Music Assistant: Spotify Connect](https://music-assistant.io/plugins/spotify-connect/).
