# Этап 0: замер задержки звука и UDP-достижимости

Процедура для владельца/primary перед реализацией `pulsar-call`. Ничего не
меняет в прошивке: только существующие probe-утилиты Pulsar, временные файлы
в `/tmp/pulsar-test` и `nc`. Живые действия — с согласия владельца; колонка
должна быть в LAN, громкость — умеренная.

Цель — получить три числа:

1. **output delay** (dmix → динамик) из `pulsar-playback::Stats` через
   `sendspin_probe`;
2. **capture latency / FIFO** из `frontend_probe`;
3. **UDP reachability** из VPN-клиента (Mac; телефон — тем же способом).

Без этих чисел нельзя выбирать между «звонок» и «рация».

## 0. Предусловия

- Мак в домашней LAN или в VPN-группе `lan`; колонка `192.0.2.13`.
- ADB: `adb -s 192.0.2.13:5555 shell true` отвечает.
- Нет активного звонка/будильника; музыка и Sendspin остановлены.
- `maind`/Яндекс-стек уже остановлены (Pulsar в режиме `pulsar`:
  `adb shell '/data/pulsar/ctl.sh status'` → `mode: pulsar`).

Все временные файлы — только в `/tmp/pulsar-test`; аудио никуда не
коммитить и не выкладывать.

## 1. Собрать probe-утилиты

Активная Midi — armhf. Из корня репозитория:

```bash
bazelisk build -c opt \
  --platforms=//platform/bazel/zig:armhf_linux \
  //projects/station/pulsar/satellite/crates/pulsar-probes:player_probe \
  //projects/station/pulsar/satellite/crates/pulsar-probes:frontend_probe \
  //projects/station/pulsar/satellite/crates/pulsar-probes:sendspin_probe

mkdir -p /tmp/pulsar-test
adb -s 192.0.2.13:5555 shell 'mkdir -p /data/pulsar/probe /tmp/pulsar-test'
for b in player_probe frontend_probe sendspin_probe; do
  adb -s 192.0.2.13:5555 push \
    "bazel-bin/projects/station/pulsar/satellite/crates/pulsar-probes/$b" \
    "/data/pulsar/probe/$b"
  adb -s 192.0.2.13:5555 shell "chmod 755 /data/pulsar/probe/$b"
done
```

## 2. Остановить демон (probe владеет устройством)

`ctl.sh disable` запустит стек Яндекса — это не нужно. Останавливаем только
supervisor и daemon, режим `enabled` сохраняется:

```bash
adb -s 192.0.2.13:5555 shell \
  'killall supervise.sh 2>/dev/null; killall pulsar 2>/dev/null; sleep 2; pidof pulsar || echo "pulsar stopped"'
```

После замеров вернуть:

```bash
adb -s 192.0.2.13:5555 shell '/data/pulsar/ctl.sh restart'
adb -s 192.0.2.13:5555 shell '/data/pulsar/ctl.sh status'
```

## 3. Output delay (dmix): `sendspin_probe` + `Stats`

`sendspin_probe` открывает тот же модельный output, что и звонок
(`pulsar-playback` → dmix `speakers`/`mini`), и раз в 10 с печатает строку
`pulsar-playback::stats`:

```
48000 Hz/2 ch: ... output delay 42.3 ms (min 38.1, max 51.0), xruns 0, duck gain 1.00
```

Это `snd_pcm_delay` устройства: буфер dmix + hardware. Именно эта величина
ограничивает downlink звонка.

На Mac (из `projects/station/pulsar`, зависимости — `tools/requirements.txt`):

```bash
python3 -m venv /tmp/pulsar-test/venv
/tmp/pulsar-test/venv/bin/pip install -r tools/requirements.txt
```

Терминал A — probe (слушает `18928`):

```bash
adb -s 192.0.2.13:5555 shell \
  '/data/pulsar/probe/sendspin_probe 18928 pulsar-test 40 /tmp/pulsar-test/sendspin'
```

Терминал B — сервер, который шлёт синус/FLAC (не громче 15 %):

```bash
/tmp/pulsar-test/venv/bin/python tools/sendspin_server.py 192.0.2.13 \
  --port 18928 --seconds 20 --volume 15
```

Записать 3–4 строки `output delay` и `xruns`. `xruns > 0` — отдельный риск.

Повторить 3 раза (холодный/прогретый буфер). Значение — медиана `last`, а
`max` показывает джиттер буфера.

## 4. Capture latency: `frontend_probe`

`frontend_probe` захватывает `hw:0,2` (8 каналов Midi) и раз в секунду
печатает:

```
t=  5s chunks  31 audio  31 music   0 (...) captured 1536 frames fifo 12 frames (8.0 ms) dropped 0
```

- `fifo N frames (X ms)` — сколько захвата ждёт обработки; это нижняя граница
  задержки capture → VQE.
- `chunks`/`audio` — по 512 сэмплов (32 мс) на VQE-чанк.
- `dropped` — потери при отставании обработки; должно быть `0`.

Запуск на 30 с (без воспроизведения):

```bash
adb -s 192.0.2.13:5555 shell \
  '/data/pulsar/probe/frontend_probe 30 --stream'
```

Записать максимальный FIFO и число dropped. `--record` для сырого WAV
(все каналы, S16) — по желанию, если нужен офлайн-разбор:

```bash
adb -s 192.0.2.13:5555 shell \
  '/data/pulsar/probe/frontend_probe 10 --record /tmp/pulsar-test/capture.wav 8'
adb -s 192.0.2.13:5555 pull /tmp/pulsar-test/capture.wav /tmp/pulsar-test/
```

## 5. Акустическая проверка (опционально, но полезно для AEC)

Один процесс владеет capture, второй может играть. `frontend_probe --record`
пишет все 8 каналов: 0–3 — микрофоны, 4–7 — аппаратный reference динамика.
Если сыграть клик через `player_probe`, в одном WAV видно и reference
(канал 4–7), и акустику (канал 0–3). Разница onset'ов = акустика + capture,
а положение reference относительно начала записи — грубая проверка output.

Клик (48 кГц, 16 бит, моно) сгенерировать на Mac:

```bash
python3 - <<'PY'
import math, struct, wave
rate, n = 48000, 2400
with wave.open('/tmp/pulsar-test/click.wav', 'wb') as w:
    w.setnchannels(1); w.setsampwidth(2); w.setframerate(rate)
    w.writeframes(b''.join(struct.pack('<h', int(12000*math.sin(2*math.pi*1000*i/rate)*(1-i/n))) for i in range(n)))
PY
adb -s 192.0.2.13:5555 push /tmp/pulsar-test/click.wav /tmp/pulsar-test/click.wav
```

`player_probe` требует music, announce и HTTP-base. Достаточно локального
клика для обоих и любого HTTP-сервера для `announce.flac`; фазы HTTP/HTTPS
после основного сценария можно проигнорировать.

```bash
# Mac: раздать click.wav как announce.flac на 8000
cd /tmp/pulsar-test && cp click.wav announce.flac && python3 -m http.server 8000
# Колонка, терминал A: запись 15 с
adb shell '/data/pulsar/probe/frontend_probe 15 --record /tmp/pulsar-test/ref.wav 15'
# Колонка, терминал B (через 1–2 с): клик в Music
adb shell '/data/pulsar/probe/player_probe /tmp/pulsar-test/click.wav /tmp/pulsar-test/click.wav http://192.0.2.26:8000'
```

`192.0.2.26` заменить на LAN-адрес Mac; `player_probe` сам ограничивает
громкость 0.2 и восстанавливает её. Разбор WAV — любым редактором; достаточно
найти первый пик на каналах 4–7 и 0–3.

## 6. UDP-достижимость колонки из VPN

Проверяем, что VPN-клиент может слать UDP **на колонку** и получать ответ.
WebRTC использует эфемерные порты; для теста берём 50000/50001.

### 6.1 Mac в VPN (`lan`), колонка слушает

На колонке:

```bash
adb -s 192.0.2.13:5555 shell \
  'nc -u -l -p 50000 > /tmp/pulsar-test/udp-in.txt & echo listener-started'
```

На Mac:

```bash
printf 'intercom-udp-hello' | nc -u -w 2 192.0.2.13 50000
adb -s 192.0.2.13:5555 shell 'cat /tmp/pulsar-test/udp-in.txt'
```

Ожидаем `intercom-udp-hello`. Если пусто — UDP до колонки не доходит
(проверить `occtl show users`, таблицу NAT на CT 102, INPUT-цепочку станции).

### 6.2 Обратное направление (колонка → VPN-клиент)

На Mac узнать адрес в VPN:

```bash
ifconfig | awk '/^utun/{iface=$1} /inet /{if ($2 ~ /^10\.77\./) print iface, $2}'
```

На Mac — слушатель:

```bash
nc -u -l 50001
```

На колонке — отправить на адрес Mac (подставить `10.77.0.x`):

```bash
adb -s 192.0.2.13:5555 shell \
  "printf 'intercom-udp-pong' | nc -u -w 2 10.77.0.x 50001"
```

Ожидаем `intercom-udp-pong` в терминале Mac. Проверить оба направления
важно: WebRTC ICE проверяет пары в обе стороны.

### 6.3 Телефон

На телефоне в VPN — тот же тест из Termux (`pkg install netcat-openbsd`):

```bash
printf 'phone-udp' | nc -u -w 2 192.0.2.13 50000
nc -u -l 50001   # затем отправить с колонки на VPN-адрес телефона
```

Если Termux нет — достаточно Mac-теста: путь VPN-клиент → CT 102 → LAN у
телефона тот же (группа `lan`). Отдельно проверить, что телефон получает
адрес `10.77.0.x` и видит `192.0.2.13` (`ping`).

### 6.4 Если UDP закрыт

Временное правило только для проверки (снять после):

```bash
adb -s 192.0.2.13:5555 shell \
  'iptables -I INPUT -p udp --dport 50000:50010 -j ACCEPT; iptables -L INPUT -n | head'
```

Не оставлять это правило в прошивке: `pulsar-call` должен открывать ровно
свой UDP-порт (или использовать ICE/STUN), а не диапазон.

## 7. Критерии и решение

| Измерение | «Звонок» | «Рация/PTT» | Стоп |
|---|---|---|---|
| output delay (`Stats`, медиана) | < 80 мс | 80–150 мс | > 150 мс |
| capture FIFO (`frontend_probe`, max) | < 64 мс, dropped 0 | 64–120 мс | > 120 мс или dropped > 0 |
| UDP Mac ↔ колонка | оба направления | — | любое направление не проходит |

Дополнительно: `xruns == 0`, отсутствие `output delay` > 200 мс на пиках.
Если UDP не проходит, coturn **не уменьшит задержку** — он нужен только для
reachability; сначала исправить NAT/allow-list, потом решать про TURN.

Результаты (числа, без секретов и без аудио) записать в PR/`docs/STATE.md`
при выкатке; эта процедура ничего не коммитит сама.

## 8. Убрать за собой

```bash
adb -s 192.0.2.13:5555 shell 'rm -rf /tmp/pulsar-test /data/pulsar/probe'
adb -s 192.0.2.13:5555 shell '/data/pulsar/ctl.sh restart'
adb -s 192.0.2.13:5555 shell '/data/pulsar/ctl.sh status'
```

Проверить, что HA видит колонку и голосовой pipeline отвечает (короткий
`conversation.process` — по skill `station-test`, только с согласия).

## 9. Результаты (замерено вживую)

Этап 0 выполнен на станции мейнтейнера (Миди, armhf, релиз форка):

| Измерение | Значение | Вывод |
|---|---|---|
| output delay dmix (`sendspin_probe`, sink Sendspin) | **124–160 мс** | выше 80 мс: чистый «звонок» с бюджетом < 80 мс на этом dmix недостижим |
| xruns за прогон | **0** | устройство стабильно, дополнительная подстройка буферов не нужна |
| чанк очищенного VQE-потока | **32 мс** (512 сэмплов 16 кГц) | репакетизация в Opus 20 мс — на стороне `pulsar-call` |
| UDP из VPN (10.77.0.0/24, masquerade) до колонки | **проходит** | TURN не нужен |

Следствия для реализации:

- TURN/STUN не включаем: телефон инициирует ICE, колонка отвечает как
  `controlled`-агент, кандидат телефона приходит peer-reflexive.
- Ограничение задержки downlink — dmix, а не сеть. Поэтому `pulsar-call`
  выводит голос собеседника отдельным каналом `Call` (`pulsar-playback`,
  период 5 мс, буфер 30 мс) и не использует ANNOUNCE-политику с prime 200 мс.
  Итоговая задержка «сеть + буфер» на станции — порядка 124–160 мс dmix плюс
  джиттер-буфер 40–100 мс; параметры `CallTuning`/`JitterBuffer` остаются
  настраиваемыми, живая приёмка — отдельным шагом с согласия владельца.
- Uplink: 32 мс чанк VQE + ресемплинг 16→48 кГц + Opus 20 мс ≈ 52–72 мс до
  отправки, сеть 1–5 мс внутри LAN/VPN.
