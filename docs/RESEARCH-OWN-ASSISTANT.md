# RESEARCH — свой ассистент вместо Алисы

**Дата:** 2026-10-25; обновление 2026-10-26  
**Станция:** Midi `cucumber`, `STATION_SERIAL`, прошивка `.160`  
**Статус:** исследование/архив решений. Pulsar `v0.4.1` уже активен на `.160`;
HA Assist, голос, кнопки и голосовые команды проверены. HA integration MA
добавлена 2026-10-26; Spotify API 429, Liked Songs ждёт sync. Текущие факты —
`STATE.md`; ниже — reasoning до/во время внедрения, а не текущий статус.

> Это не инструкция «сделай сейчас». Ниже сохранён разбор и план; часть
> пунктов уже выполнена. Текущие факты — `STATE.md`; задачи — `PENDING.md`;
> хронология — `HISTORY.md`.
>
> **2026-10-26:** локальные `v3rt3p-stack` и `alice-mitm-bootstrap`
> остановлены/удалены. Упоминания путей ниже — исторические; актуальные
> upstream-ссылки v3rt3p — в `RESOURCES.md`, архивы — в `archive/legacy/`.

## 1. Короткий вывод

1. **Сейчас Алиса зависает не из-за Яндекса.** Станция уже ходит в локальный
   `https-proxy` → `quasar-server`; запрос распознаётся. Падает следующий шаг:
   `processor` вызывает LLM, а `OPENAI_API_KEY` пустой. OpenRouter отвечает
   `401 Missing Authentication header`. `processor` ловит ошибку и **не отправляет
   ответ**, поэтому `maind` ждёт и держит анимацию «думает».
2. **Это чинится ключом LLM.** Нужен ключ OpenRouter (или другого
   OpenAI-совместимого провайдера). После этого текущий локальный контур сможет
   отвечать голосом.
3. **Текущий MITM-стек — это не «Алиса без Яндекса», а эмулятор части API.**
   Он хорошо покрывает текст/голос/громкость, но не музыку, HA, навыки, звонки,
   Алису Плюс и часть служебных запросов. Достраивать его можно, но это дорого и
   хрупко.
4. **Стратегически правильнее Pulsar + Home Assistant.** Pulsar заменяет
   `maind` своим демоном, использует родные VQE/споттеры и железо Midi, отдаёт
   колонку в HA как ESPHome-устройство, умеет Music Assistant/Sendspin, Zigbee,
   кольцо/часы/кнопки. Это лучший путь к «своим» Spotify и HA.
5. **Wake word.** Pulsar сейчас умеет только родные `Алиса`/`Яндекс`. В прошивке
   есть ещё модель `yasmina`, но её телефоны — английские/арабские
   (`Y AE1 S M IY1 N AH0`), это не русская «Ясмина». Проверить можно, но
   рассчитывать на неё как на готовую замену нельзя. Произвольное слово
   (`Джарвис`, `Компьютер`) требует отдельного детектора
   (openWakeWord/LiveKit/Porcupine) и патча Pulsar; это R&D, не «галочка в
   настройках».
6. **Spotify.** Нужен Premium. Через Music Assistant это работает, но есть
   развилка: librespot — только для старых аккаунтов; Spotify Soloist — для новых,
   но **не на Docker bridge**. Это влияет на то, где хостить HA/MA.
7. **MCP.** Процессор уже умеет MCP (Streamable HTTP), но переменная не была
   прокинута в compose. HA умеет и MCP-клиент (SSE), и MCP-сервер. MCP полезен для
   длинного хвоста команд, но простые команды и музыку лучше вести через родные
   интенты HA — иначе latency и цена растут.

## 2. Почему Алиса зависает: доказательства

### 2.1 Что происходит по логам

В момент обращения (пример `2026-10-25 00:59:04`):

`/data/quasar/daemons_logs/maind.log`:

```text
Start alice request. ID: 6ee5635b-...
onStartVinsRequest header = Vins.TextInput ...
```

Дальше — тишина: нет `Vins.Response`, нет TTS, нет закрытия диалога. Станция
продолжает ждать.

`docker logs v3rt3p-processor-1`:

```text
Session ... process: {"text":"..."}
Querying LLM
ERROR Failed on WebSocket message handling:
Error 401 Missing Authentication header
```

`docker exec v3rt3p-processor-1 env`:

```text
OPENAI_BASE_URL=https://openrouter.ai/api/v1
OPENAI_MODEL=nvidia/nemotron-3.5-lightning:free
OPENAI_API_KEY=EMPTY
```

`v3rt3p-stack/.env`:

```text
LLM_BASE_URL=...        # задан
LLM_MODEL=...           # задан
LLM_API_KEY=            # пустой
```

`https-proxy` при этом показывает `GET /uni.ws 101` — WebSocket до локального
сервера устанавливается. То есть DNS/egress/сертификаты не при чём: запрос уже
локальный.

### 2.2 Почему именно «зависает анимация»

`maind` — клиент диалога. Он ждёт от `uniproxy` поток директив/текста. Пока ответа
нет, он держит состояние «слушает/думает/отвечает». В `processor` ошибка LLM
попадает в общий `catch`, логируется и **не превращается в `partialResponse`**.
Сокет не закрывается, диалог не завершается. Отсюда бесконечная полоса.

Это не «Алиса пытается уйти в Яндекс и не может». Это локальный контур, который
не умеет корректно падать.

### 2.3 Что уже сделано

В `v3rt3p-stack/repos/processor/src/processor.ts` добавлен аварийный ответ:

```text
Извините, языковая модель сейчас недоступна.
```

Он отправляется как `partialResponse` с `finished: true`, после чего сокет
закрывается. Проверено синтетическим WebSocket-запросом к `processor`: ответ
пришёл, зависания нет. То есть при следующем обращении вместо вечной анимации
колонка должна сказать короткую ошибку.

Это **не** заменяет ключ LLM. Это страховка.

## 3. Что вообще представляет текущий стек

```text
Station maind
  │  uniproxy.alice.yandex.net:443 (DNS/hosts → Mac)
  ▼
Mac https-proxy :443
  │  TLS-терминация, маршрутизация
  ▼
quasar-server :31115
  │  WebSocket /uni.ws, директивы, sync, конфиг
  ├── GigaAM STT (quasistreaming-gigam)
  ├── processor :8080 → LLM (OpenAI-совместимый)
  ├── Supertonic TTS :9150
  └── Postgres
```

Что уже работает:

- установка WebSocket и приём аудио;
- STT (в логах видны распознанные фразы);
- текст/ответы при наличии LLM;
- громкость/часть директив;
- sync/конфиг, OTA-заглушка, телеметрия;
- BLE Wi-Fi gateway (отдельный сервис).

Чего нет или мало:

- музыка (Yandex Music/Spotify) — эмулятор не покрывает;
- HA/умный дом — нет моста;
- навыки, звонки, Алиса Плюс, покупки, будильники в полном объёме;
- часть `glagol`/`set_device_capability` отвечает `500`;
- устойчивость к ошибкам LLM (только что добавлена);
- custom wake word.

### 3.1 Что уже «проксируется к себе», а что нет

| Направление | Сейчас | Чего не хватает |
|---|---|---|
| `quasar.yandex.net`, sync/config | локальный `quasar-server` | мелочи `glagol`/capability |
| `uniproxy.alice.yandex.net` | локальный WebSocket | музыка/навыки/звонки |
| `passport`/`oauth` | локальные токены-заглушки | настоящий аккаунт не нужен |
| `music.yandex.*`, `strm`, CDN | hosts → Mac, но эмулятора нет | свой музыкальный backend |
| `glagol/family_device_list` | `500` | эмуляция family/IoT |
| telemetry (`clck`, `appmetrica`) | `204`/drop | — |
| OTA (`s3/cdn`) | `403`, updatesd вырезан | — |

То есть «проксировать» уже почти нечего: трафик **уже** локальный. Проблема не
в маршруте, а в том, что эмулятор не реализует музыку, HA и навыки. Поэтому
достраивать текущий стек — это писать эмулятор Яндекса; перейти на Pulsar/HA —
это заменить клиент, а не эмулировать сервер.

### 3.2 Вывод по стеку

Этот путь годится, чтобы **быстро вернуть голосового ассистента** и проверить
идеи. Он не годится как долгосрочная замена Яндекса: слишком много протокола
эмулируется вручную, и каждый новый сценарий — отдельная обратная разработка.

## 4. Три архитектуры

### A. Починить текущий эмулятор (минимум риска)

**Что делать:** ключ LLM + fallback + MCP/функции.

**Плюсы:**
- ничего не переустанавливается;
- сохраняются родной wake word, VQE, LED, кнопки, provisioning;
- быстрый результат;
- можно тестировать LLM/HA/Spotify по частям.

**Минусы:**
- музыка и Spotify на самой колонке не появятся «сами»;
- HA можно подключить как tool, но проигрывание на колонке всё равно требует
  директив `maind`;
- зависимость от Mac/DNS/TLS/прокси;
- custom wake word не решается;
- хрупкий протокол.

**Кому:** если сейчас важнее «чтобы Алиса просто отвечала», а переезд позже.

### B. Pulsar + Home Assistant (стратегически правильно)

**Что это:** отдельный демон на колонке вместо `maind`. Он использует родные
библиотеки прошивки для VQE/эхоподавления и споттеров, но говорит с HA по
ESPHome Native API. Колонка выглядит в HA как ESPHome-устройство: Assist
satellite, media player, кольцо, часы, кнопки, микрофонный mute, Zigbee.

**Плюсы:**
- локальный голосовой контур без Яндекса;
- HA Assist: свет, сценарии, скрипты, таймеры, объявления;
- Music Assistant/Sendspin: Spotify и локальная музыка;
- Zigbee-координатор Midi (экспериментально);
- нет MITM/TLS/DNS-костылей;
- обновления и откат бинарника;
- открытый код, сообщество.

**Минусы/риски:**
- **останавливает штатный стек Яндекса**;
- прошивка `.160` проверена 2026-10-26; Pulsar v0.4.1 работает;
- пропадают Yandex Music, Алиса, навыки, звонки, Алиса Плюс;
- голос TTS — уже не Алиса, а Piper/другой;
- нужен постоянно включённый HA;
- установщик пишет `/etc/init.d/S99zpulsar` (rootfs remount);
- BLE gateway и OTA/ADB fortress проверены после Pulsar;
- custom wake word — отдельная задача.

**Кому:** если цель — свой ассистент, HA и Spotify, а не «Алиса, но локально».

### Сравнение

| Критерий | A. Эмулятор | B. Pulsar + HA | C. Гибрид |
|---|---|---|---|
| Вернуть голос сейчас | быстро | средне | быстро |
| HA/свет/сцены | через MCP, ограниченно | нативно | поэтапно |
| Spotify на колонке | сложно/хрупко | через MA/Sendspin | после B |
| Родной wake word | да | да (alisa/yandex) | да |
| Custom wake word | нет | R&D | позже |
| Риск для станции | низкий | средний/высокий | низкий→средний |
| Зависимость от Mac | высокая | низкая (нужен HA) | средняя |
| Долгосрочно | тупик | основной путь | безопасный переход |

### C. Гибрид/поэтапно (рекомендуется)

1. Сейчас: ключ LLM + fallback → вернуть голос.
2. Параллельно: HA + Music Assistant + Spotify на отдельном железе/в VM.
3. Потом: Pulsar `--no-enable` (скопировать, не останавливать `maind`), проверить
   бинарник/ABI/железо; затем переключение с rollback.
4. Wake word: проверить `yasmina`; основной путь — pluggable microWakeWord
   («Рикардо») с fallback на native `alisa`.

Pulsar на `.160` проверен; этот гибридный план сохранён как история решения.

## 5. Pulsar: что важно знать до установки

### 5.0 Pulsar — это спутник, а не хаб

Pulsar **не делает колонку хостом/хабом**. Он заменяет `maind` на саму
колонку и превращает её в **satellite** для Home Assistant:

- HA — мозг: Assist pipeline, STT/TTS, LLM, Music Assistant, автоматизации;
- Pulsar — edge: микрофоны, VQE/эхоподавление, wake word, кольцо/часы,
  кнопки, локальные команды, воспроизведение, ESPHome API;
- для Zigbee Pulsar действительно может быть **координатором** (радио-хаб
  для ZHA/Zigbee2MQTT) — но это только Zigbee, не «хостинг всего».

Он не запускает Home Assistant, MQTT, Music Assistant, Spotify, LLM или MCP.
Если HA выключен, останутся локальные команды/подсветка, но полноценного
ассистента не будет. Запускать HA на самой колонке не стоит: мало RAM/CPU,
rootfs/OTA-риски, нет нормального контейнерного окружения.

Источник: локальный `projects/station/upstream/pulsar/README.md`, релиз `v0.4.1` (2026-10-24).
Локальная копия в `projects/station/upstream/pulsar` — снимок; перед установкой брать актуальный
release, а не старый бинарник из репозитория.

- Поддержка: Midi `cucumber`, Lite 2 `plum` (Lite 2 — экспериментально).
- Требование: root ADB; HA 2025.x+; ESPHome integration; Assist pipeline.
- Установка кладёт демон в `/data/pulsar`, autostart в `/etc/init.d/S99zpulsar`,
  останавливает стек Яндекса и запускает демон.
- Откат: `/data/pulsar/ctl.sh disable` — возвращает штатный стек, в том числе
  после перезагрузки.
- Автоматический откат: 5 падений подряд → `.prev`; если и он падает → штатный
  стек.
- Обновления: HA entity «Firmware», GitHub Releases; проверка SHA256.
- Zigbee: координатор deCONZ :6638 или router; экспериментально.
- Ограничения: DNS/NTP прошивки всё ещё яндексовые; API без ключа открыт, пока
  HA не запишет ключ; голоса Алисы нет.

### 5.1 Совместимость с `.160` (✅ проверено)

Upstream README тестировал Midi `.142`; у нас `.160`. 2026-10-26 проверено
фактически: Pulsar `v0.4.1` (armhf userspace), железо, BLE, OTA/ADB, кнопки,
голос и rollback работают. Connect уже играет на Pulsar; остались Bluetooth-пара
и sync Liked Songs для MA-ветки.
Процедура установки и отката — `../projects/station/upstream/pulsar/README.md`,
`OPERATIONS.md`.

### 5.2 Provisioning после Pulsar

Это отдельный риск. Сейчас Wi-Fi provisioning — наш BLE gateway, который
зависит от BlueZ и пассивного чтения `event2`; `maind` при этом жив. Pulsar
останавливает `maind`, но BlueZ/`event2` могут остаться доступны. Нужно
проверить:

- работает ли `bluetoothd` и GATT после `maind` stop;
- не захватывает ли Pulsar `event2`;
- поднимает ли `rootkeeper.sh` gateway;
- что делать, если Wi-Fi потерян, а ADB по сети недоступен.

Если BLE gateway не выживет, перед переключением нужен независимый fallback:
сохранённый `wpa_supplicant` профиль + USB/UART или AP. Это обязательный
decision gate, а не «потом».

## 6. Home Assistant: как собрать правильно

### 6.1 Где хостить

| Вариант | Плюсы | Минусы |
|---|---|---|
| HA OS на mini-PC/HA Green/Pi 5 | add-ons, mDNS, стабильно, всегда включено | отдельное железо |
| HA OS в VM на Mac (UTM/Colima) | использует Mac, add-ons | Mac спит/перезагружается; сеть VM |
| HA Container в Docker на Mac | знакомый Docker | нет add-ons, mDNS/ESPHome хуже, MA/Whisper/Piper отдельно |

**Рекомендация:** отдельный always-on mini-PC с HA OS. Mac M3 Pro 36 ГБ годится
для LLM/разработки, но не как единственный хост ассистента, если он уходит в сон.
Если Mac всегда включён — HA OS в VM, не в Docker.

### 6.2 Компоненты

- **HA OS** + ESPHome integration.
- **Whisper** (faster-whisper) или GigaAM-обёртка; русский.
- **Piper** — русский голос; быстрый, локальный.
- **Music Assistant** — Spotify/локальная музыка, Sendspin.
- **LLM conversation agent** — OpenRouter/OpenAI/Google/Anthropic или локальный
  Ollama/llama.cpp.
- **MCP** — опционально, для Spotify/веб/памяти/нестандартных действий.

### 6.3 Пайплайн

```text
Pulsar (Midi) → ESPHome API → HA Assist
  wake word: native (alisa/yandex; yasmina — кандидат)
  STT: Whisper/GigaAM
  conversation: native intents + LLM fallback
  TTS: Piper
  media: Music Assistant / Sendspin
```

Важно: **не всё отдавать LLM**. Простые команды («свет», «громче», «таймер»,
«играй») должны идти через родные интенты HA. LLM — для свободной речи и
нестандартных связок. Иначе латентность и цена растут, а надёжность падает.

## 6a. Вариант: HA на удалённом Linux-сервере

Короткий ответ: **технически можно, но для голосового спутника это не лучший
первый выбор.** HA можно поднять на сервере и связать с домашней сетью через
site-to-site VPN, но тогда HA теряет локальный L2-контекст: mDNS/SSDP не
проходят через VPN, discovery устройств не работает, а голосовой аудиопоток
идёт через интернет. Для Pulsar/ESPHome это решаемо (добавить по IP), для
Music Assistant/Spotify Connect — плохо (им нужен локальный L2).

### 6a.1 Рекомендуемая топология

```text
Дом 192.0.2.0/24
  Pulsar (Midi) 192.0.2.13
  Music Assistant + Spotify Soloist (локально)
  HAOS (локально, mini-PC/VM)  <-- мозг и Assist
        │
        │ WireGuard site-to-site (или Tailscale)
        ▼
Сервер (статический public IP)
  LLM (Ollama/vLLM), Whisper/GigaAM, Piper/Supertonic
  MCP, backups, monitoring
```

Почему так:

- HA остаётся рядом с устройствами: mDNS/ESPHome/Zigbee/MA работают нормально;
- сервер используется по назначению — тяжёлые LLM/STT/TTS/MCP;
- интернет упал — свет/сценарии/локальная музыка продолжают работать;
- Spotify Soloist и Sendspin остаются в локальной сети;
- статические IP с обеих сторон делают WireGuard простым и стабильным.

### 6a.2 Если всё-таки HA на сервере

Можно:

- **HAOS VM** на сервере (если есть KVM/виртуализация) — add-ons, Supervisor;
- **HA Container** в Docker — без add-ons; MA/Whisper/Piper отдельными
  контейнерами; на сервере с `network_mode: host`.

Схема:

```text
Home LAN 192.0.2.0/24
  router/WG peer 10.10.0.2
      │ WireGuard
Server 10.10.0.1
  HA 10.10.0.1:8123 / 192.0.2.13:6053 (Pulsar)
  LLM/STT/TTS/MCP
```

Что нужно:

- WireGuard на сервере и на домашнем роутере/мини-ПК (subnet router);
- `AllowedIPs` в обе стороны: сервер → `192.0.2.0/24`, дом → `10.10.0.0/24`;
- статический IP у Pulsar/ESPHome (или DHCP reservation);
- HA ESPHome integration: добавить Pulsar **вручную по IP:6053**, mDNS не
  пройдёт; в ESPHome dashboard включить ping-статус;
- HA Music Assistant integration: MA оставить локально, HA ходит к нему по
  VPN; Spotify Soloist — локально;
- HA UI — только через VPN/private, не публиковать 8123 в интернет;
- firewall: WireGuard UDP + SSH с admin-IP; HA только на `wg0`;
- MFA, backups, TLS/reverse proxy если нужен браузер.

Что сломается/усложнится:

- автоматическое обнаружение ESPHome/Pulsar, Chromecast, AirPlay, Sonos;
- mDNS/SSDP между подсетями (нужен reflector, но он капризный);
- latency голоса: аудио до сервера и TTS обратно; при удалённом сервере
  будет заметно;
- зависимость от интернета/VPN: нет сети — нет ассистента;
- Spotify/MA: Soloist и Sendspin требуют L2; удалённый MA — плохая идея;
- Zigbee: Pulsar-координатор локально, HA через VPN к :6638 — работает, но
  зависит от VPN; лучше local ZHA/Z2M.

### 6a.3 Что выбрать

- **Хочешь надёжного ассистента и Spotify** → HAOS локально, сервер — LLM/
  STT/TTS/MCP.
- **Хочешь использовать пустой сервер как «мозг»** → HA на сервере + WG,
  но MA/Spotify/Zigbee оставить локально; принять интернет-зависимость.
- **Хочешь минимум железа** → HAOS VM на сервере, локальный только
  Pulsar + MA; но это самый хрупкий вариант для голоса.

### 6a.4 Что нужно уточнить

1. Сервер в датацентре или дома? Если дома — HA лучше поставить локально.
2. Публичный статический IP у дома — без CGNAT? Роутер умеет WireGuard?
3. Сервер: KVM/`/dev/kvm`, RAM/CPU/диск, Docker?
4. Можно ли открыть только WireGuard UDP на сервере и дома?
5. Готов ли HA UI быть доступным только через VPN?

## 7. LLM: что выбрать

### 7.1 Немедленно

Нужен ключ. Текущий base URL — OpenRouter. Минимально:

```env
LLM_BASE_URL=https://openrouter.ai/api/v1
LLM_API_KEY=<ключ>
LLM_MODEL=nvidia/nemotron-3.5-lightning:free
```

Затем:

```bash
cd ~/Desktop/v3rt3p-stack
docker compose up -d processor
docker compose logs -f processor
```

Проверка: «Алиса, сколько времени?» — должен быть ответ, а не зависание.

Если free-модель капризничает (JSON/лимиты), взять быструю платную. По каталогу
OpenRouter на 2026-10-25 разумные кандидаты:

- `google/gemini-2.5-flash` — быстрый, хороший русский, tool calling;
- `deepseek/deepseek-v4.1-flash` — очень дешёвый, 1M контекст, tools;
- `qwen/qwen3.5-flash` — дешёвый, быстрый;
- `openai/gpt-4.1-mini` / `gpt-5-mini` — предсказуемый tool calling;
- `anthropic/claude-haiku-4.5` — хороший русский/инструкции, дороже.

Цены смотреть в OpenRouter; для голосовых запросов расходы мизерные. Главное —
латентность и стабильность JSON.

### 7.2 Локальная модель

M3 Pro 36 ГБ может запускать Qwen3 8B/14B/30B-A3B через Ollama/llama.cpp.
Плюс: приватность, нет ключа. Минус: Mac должен быть всегда включён; модель
конкурирует с HA/Whisper/TTS; latency на 14B может быть 2–5 с. Для голоса лучше
облачная быстрая модель или небольшая локальная 7–8B.

### 7.3 Формат текущего processor

Он просит LLM вернуть **строгий JSON** (`function_calls`, `text`,
`continue_dialog`, `can_cache`). Это не native tool calling, а парсинг текста.
Следствия:

- слабые модели ломают JSON;
- нужен хороший instruction-following;
- при ошибке нужен fallback (уже добавлен);
- лучше добавить `response_format`/JSON schema и таймаут.

Для Pulsar/HA этот слой не нужен: там native tool calling HA.

## 8. MCP: где он реально нужен

### 8.1 Текущий processor

В коде уже есть `RemoteMCPServer` (Streamable HTTP). Формат:
`PROCESSOR_MCP_SERVER_URLS=http://host:port/mcp#name`. Переменная теперь
прокинута в compose (по умолчанию пустая).

Это позволяет без переписывания processor подключить MCP-серверы:

- HA (управление сущностями/скриптами);
- Spotify (поиск/воспроизведение);
- веб-поиск/память;
- свои сервисы.

Но: MCP-инструмент возвращает текст, а не директиву `maind`. Для действий на
самой колонке (играть музыку) это не заменит `music_play`; для света/сцен/запросов
— вполне.

### 8.2 Home Assistant

HA 2025.2+ умеет MCP-клиент (SSE) и MCP-сервер (Streamable HTTP). Официальный
MCP-сервер HA отдаёт Assist API; клиент может использовать tools в conversation
agent. Ограничения: prompts/resources/sampling не поддерживаются, только tools.

Практично:

- для HA-команд — **родной Assist API**, не MCP;
- для Spotify/веба/памяти — отдельный MCP-сервер;
- не давать MCP права на lock/alarm без подтверждения;
- только LAN, токен, аудит.

### 8.3 Если хочешь хостить MCP

Можно поднять один MCP-сервер на Mac/HA-хосте:

- `ha-mcp` (неофициальный, long-lived token) — быстрый старт;
- официальный HA MCP Server — OAuth, но processor'у нужен заголовок/auth;
- собственный MCP на FastMCP/Node с инструментами `spotify_search`,
  `spotify_play`, `ha_call_service`, `web_search`.

Для начала достаточно одного инструмента `get_time`/`ha_toggle`, чтобы проверить
цепочку. Не начинай с 30 инструментов: LLM будет путаться.

## 9. Spotify

### 9.1 Через Music Assistant (рекомендуется для Pulsar)

Pulsar — Sendspin-плеер; MA видит его как плеер. Spotify добавляется как
music provider. Требуется **Spotify Premium** (Duo/Family тоже подходят, Free —
нет).

Два движка:

| Движок | Когда | Ограничения |
|---|---|---|
| **librespot** | аккаунт создан до ~дек. 2024 | новые аккаунты часто не играют (`Key Error`) |
| **Spotify Soloist** | любой Premium | официальный; **не работает на Docker bridge**; один стрим на аккаунт; старт медленнее |

Если MA в Docker bridge (как сейчас v3rt3p), Soloist не вариант. Нужен HA OS
add-on, host network или отдельный Linux-хост. Это ключевая причина не хостить
MA в текущем Mac Docker без переделки сети.

**Для нового аккаунта Spotify:** librespot, скорее всего, не подойдёт
(`Key Error`); нужен **Soloist**. Значит, MA лучше ставить как **app в HAOS**
(официально поддерживаемый путь, host network) или на Linux-хост с host
network, а не в текущий Docker bridge. Даже Docker с `network_mode: host` может
отдавать в mDNS докер-бридж для Soloist; HAOS app — самый безопасный вариант.

### 9.2 Голосом

HA 2025.6+ умеет **Search and Play** intent: «включи Rumours», «включи альбом
Rumours на кухне». MA-плееры работают из коробки, без LLM и blueprint. Для
русского надо проверить фразы и имена; при необходимости — custom sentences.

Для сложного («включи альбом X исполнителя Y, shuffle») есть MA voice blueprints
или LLM-tool. Это уже длинный хвост.

### 9.3 Альтернативы

- **HA Spotify integration** (Web API) + Spotify Connect-устройство. Станция сама
  по себе не Spotify Connect; нужен мост (MA/Spotify Connect plugin).
- **SpotifyPlus/HACS** — больше возможностей, но сложнее.
- **Spotiya** — Spotify Connect для Station Mini 2, не для Midi; не считать
  готовым решением.
- **SpotiYA/airplay** — отдельные проекты, требуют проверки на Midi.

## 10. Wake word

> **Актуальный план (2026-10-26):**
> [`RESEARCH-RICARDO-WAKE-WORD.md`](RESEARCH-RICARDO-WAKE-WORD.md).
> Ниже — раннее исследование; recommendation уточнён: MVP server-side на
> Pulsar v0.4.1, on-device после gate, v0.5.0 Rust — отдельная миграция.

### 10.1 Что есть сейчас

В прошивке Midi:

```text
/system/vendor/quasar/spotter_models/activation/
  alisa/    words: алиса
  yandex/   words: яндекс
  yasmina/  words: yasmina
```

Pulsar жёстко перечисляет только `alisa` и `yandex`:

```cpp
cfg.wake_words = {{"alisa", "Алиса", {"ru"}}, {"yandex", "Яндекс", {"ru"}}};
if (st_.wake_word != "alisa" && st_.wake_word != "yandex") st_.wake_word = "alisa";
```

Но модель `yasmina` уже лежит на устройстве и использует тот же формат
(`acoustic_model.nnet`, `words.txt`, `flags.txt`). При этом её
`tts-timings-info` содержит английские/арабские телефоны:

```text
phones: Y AE1 S M IY1 N AH0
words:  Yasmina / yasmina / ياسمينة
```

То есть это модель Yango/Yasmina для английского/арабского произношения, а не
русская «Ясмина». Это значит:

- **эксперимент возможен:** добавить `yasmina` в список и разрешить
  `model_dir("yasmina")`; проверить, ловит ли она русское «Ясмина»;
- ожидания низкие: акустика обучена на другое произношение;
- никакого обучения не нужно, откат — вернуть конфиг;
- если сработает — это всё ещё яндексовская модель, не своё слово.

Это не готовый «быстрый win», а дешёвая проверка. Для «Джарвиса» всё равно нужен
свой детектор.

### 10.2 Произвольное слово

Нужен свой детектор. Варианты на 2026:

1. **microWakeWord (on-device, лучший русский результат).** Trainer
   `interkelstar/microwakeword-trainer` обучен на Piper-голосах (Dmitri, Denis,
   Irina, Ruslan), модель ~50–300 КБ, TFLite; в примере для «Джарвиса» —
   94% recall на 50 реальных срабатываниях и 1/140 FP после FP-mining.
   Плюсы: приватность (аудио не уходит), latency, работает без HA-сети.
   Минус: надо встроить streaming frontend/модель в Pulsar C++ (TFLite уже есть
   в прошивке) и собрать toolchain; это R&D, но реалистичный.
2. **openWakeWord на HA (server-side).** HA-подход: satellite постоянно
   стримит 16 кГц аудио, HA/openWakeWord ищет слово. OpenWakeWord теперь
   поддерживает русский (openwakeword.com: 20+ языков, русские Piper-голоса).
   Плюсы: **самое простое изменение слова** — положить `.tflite` в
   `/share/openwakeword`, выбрать в HA; Pulsar не пересобирать.
   Минусы: постоянный аудиопоток на HA (privacy/трафик/CPU), HA должен быть
   всегда включён; Pulsar надо научить стримить аудио до wake word.
   По качеству публичная RU-модель «Джарвис» на openWakeWord даёт ~43% recall
   на чистой речи и ~34% с шумом; microWakeWord в тестах того же автора —
   ~94% на 50 реальных срабатываниях. Поэтому on-device microWakeWord
   предпочтительнее по качеству, server-side openWakeWord — по удобству смены.
3. **Porcupine** — официальные языки: EN/FR/DE/IT/JA/KO/ZH/PT/ES; русского
   в актуальном Model API нет (старый RU SDK — отдельная история). Не основной
   путь для «Рикардо».
4. **Отдельный always-on mic** (ESP32-S3/HA Voice PE) — не использует микрофоны
   Midi, но проще. Не то, что хочется.
5. **Кнопка** — без wake word, но не hands-free.

### 10.3 «Рикардо» и требование «легко менять слово»

«Рикардо» — хороший кандидат: 3 слога, чёткая структура `ри-кар-до`, редко
встречается в бытовой речи. Но он может встречаться в музыке/ТВ/испанских
именах; нужны negative-фразы (`рик`, `кардо`, `рикар`, `рикардо` в музыке) и
проверка FP.

Ключевая правда: **нельзя просто вписать новое слово в UI и получить модель**.
Любое слово — это обученная модель. «Легко менять» означает:

- один раз обучить модель (Piper TTS + negatives, ~час на GPU/CPU);
- положить файл в каталог моделей;
- выбрать его в HA/`ctl.sh` — без пересборки Pulsar.

Предлагаемый дизайн «pluggable wake word»:

```text
/data/pulsar/wakewords/
  ricardo.tflite
  ricardo.json          # имя, язык, порог, версия
  jarvis.tflite
  jarvis.json
```

- `state.conf`: `wake_word=custom:ricardo` (или `alisa`/`yandex`);
- HA select «Wake word»: `alisa`, `yandex`, `ricardo`, `jarvis`;
- `ctl.sh wakeword set ricardo` — переключение без пересборки;
- при старте Pulsar читает manifest, выбирает модель; при ошибке — fallback
  на native `alisa`;
- замена слова = заменить файл/добавить новый + выбрать; код не меняется.

Если приоритет — **менять слово как можно проще**, выбирай server-side
openWakeWord: модель живет в HA, Pulsar только стримит аудио. Если приоритет —
**приватность и latency**, выбирай on-device microWakeWord. Я бы начал с
on-device: Midi и так имеет VQE/микрофоны, а «Рикардо» — редкое слово.

### 10.4 Рекомендация по wake word

- `yasmina` — только дешёвая проверка, не основной путь.
- Для «Рикардо» — microWakeWord on-device + pluggable-каталог моделей.
- Держать native `alisa`/`yandex` как fallback; при ошибке модели не оставаться
  без wake word.
- Не пытаться «переименовать» `alisa` заменой `words.txt`: акустическая модель
  обучена на звучание «алиса», строка не меняет модель.

## 11. Безопасность и эксплуатация

- **API `:31116` без auth** — P0; закрыть bind `127.0.0.1` или auth. Пока не
  закрыто, любой в LAN может отправлять директивы.
- **BLE token** — текущий 8-значный токен слабый; локально, но лучше ротировать.
- **Pulsar ESPHome API** — без ключа открыт; HA должен записать ключ сразу.
- **HA/MCP/Spotify** — только LAN, токены в secrets, не в docs.
- **OTA/ADB fortress** — Pulsar installer пишет `/etc`; проверить, что OTA
  остаётся no-op, `rootkeeper` не конфликтует, ADB/SSH живы.
- **Firmware update** — не делать; `.160` и так новее тестовой; обновление может
  снести autostart Pulsar.
- **Factory reset** — не делать; сносит `/data`, root/SSH/OTA/guard.
- **Приватность** — облачный LLM видит текст; локальный LLM — нет. Для Spotify
  MA видит аккаунт; для HA — состояния устройств.

## 12. Пошаговый план

### Этап 0 — сегодня, без риска

1. Дать ключ LLM; положить в `v3rt3p-stack/.env` (chmod 600).
2. `docker compose up -d processor`; проверить голосом время/погоду.
3. Убедиться, что при отключённом ключе говорит ошибку, а не висит (fallback).
4. Зафиксировать статус в `logs/`.

### Этап 1 — HA/Spotify, без станции

1. Выбрать хост HA (рекомендуется отдельный mini-PC/HA OS).
2. Поднять HA, Whisper/Piper, Music Assistant.
3. Подключить Spotify (учётка/Premium/движок).
4. Проверить Assist в браузере/телефоне: свет, таймер, музыка.
5. Проверить MCP (один инструмент) — опционально.

### Этап 2 — Pulsar, с откатом (✅ выполнен)

1. Backup `/data`, `secrets`, текущих скриптов — сделано.
2. `pulsar-install --no-enable`; ABI/железо проверены.
3. BLE gateway/`event2`/BlueZ после `maind` проверены.
4. Wake word → HA → TTS → media проверено.
5. Rollback: `ctl.sh disable` проверен.

### Этап 3 — wake word

1. `yasmina` quick test (EN/AR, ожидания низкие).
2. Собрать microWakeWord «Рикардо» на Piper RU; проверить recall/FP.
3. Добавить в Pulsar pluggable-каталог `/data/pulsar/wakewords/` и выбор в HA.
4. Тесты: тишина, музыка, ТВ, несколько голосов, ложные срабатывания; fallback
   на native `alisa`.

### Этап 4 — полировка

- native intents + LLM fallback;
- Spotify voice (Search & Play/blueprint);
- HA dashboard/announcements;
- security (API, MCP, HA tokens);
- docs/backup.

## 13. Что нужно решить тебе

1. **Ключ LLM:** OpenRouter или прямой провайдер? Какой бюджет/приватность?
2. **HA:** отдельное железо или Mac/VM? Mac всегда включён?
3. **Spotify:** Premium есть? Аккаунт старый/новый (librespot vs Soloist)?
4. **Pulsar:** разрешаешь `--no-enable` тест и затем переключение?
5. **Wake word:** устроит `Ясмина` или нужно своё слово? Если своё — готов ли на R&D?
6. **MCP:** какие сервисы в первую очередь (HA, Spotify, веб, память)?

## 14. Источники

Локальные:

- `projects/station/upstream/pulsar/README.md` — возможности, установка, ограничения, проверенные прошивки.
- `projects/station/upstream/pulsar/AGENTS.md` — архитектура, VQE/spotter, железо.
- `v3rt3p-stack/repos/processor/src/processor.ts` — LLM/JSON/fallback.
- `v3rt3p-stack/repos/processor/src/llm/function/mcp-remote.ts` — MCP Streamable HTTP.
- `v3rt3p-stack/docker-compose.yml` — LLM/TTS/STT/MCP env.
- `docs/ARCHITECTURE.md`, `docs/STATE.md`, `docs/OPERATIONS.md`.

Внешние:

- Pulsar releases: https://github.com/Morvex885/pulsar/releases
- HA MCP integration: https://www.home-assistant.io/integrations/mcp/
- HA MCP Server: https://www.home-assistant.io/integrations/mcp_server
- HA Assist sentences: https://www.home-assistant.io/voice_control/builtin_sentences/
- HA wake words: https://www.home-assistant.io/voice_control/about_wake_word
- HA custom wake word: https://www.home-assistant.io/voice_control/create_wake_word
- Music Assistant voice: https://www.music-assistant.io/integration/voice
- Music Assistant Spotify: https://www.music-assistant.io/music-providers/spotify
- HA Spotify: https://www.home-assistant.io/integrations/spotify
- openWakeWord: https://github.com/dscripka/openWakeWord
- LiveKit WakeWord: https://livekit.com/blog/livekit-wakeword
- Picovoice wake word guide: https://picovoice.ai/blog/complete-guide-to-wake-word
