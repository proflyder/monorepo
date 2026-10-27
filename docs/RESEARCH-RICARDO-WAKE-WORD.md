# Wake word «Рикардо» — исследование и план реализации

**Дата:** 2026-10-26  
**Статус:** исторический research + implementation plan от 2026-10-26.
Текущее состояние после обучения и интеграции — `STATE.md`; результаты —
`../artifacts/wakeword/ricardo-station/README.md`.  
**Область:** только wake word. TTS — `TTS-WAKE-WORD-RESEARCH.md`; STT — `STATE.md`.  
**Связанные документы:** `STATE.md`, `PENDING.md`, `OPERATIONS.md`,
`TTS-WAKE-WORD-RESEARCH.md` (раздел 4 — предыдущая версия плана).

> **Короткий вывод.** «Рикардо» нельзя получить переименованием в UI:
> Pulsar `v0.4.1` умеет только закрытые яндексовские модели `alisa` и
> `yandex`. Нужна отдельная обученная модель microWakeWord и патч Pulsar.
> Модель реальна: русский «Джарвис» на этом пайплайне дал 94% recall
> (47/50) и 1/140 FP после FP-mining.
>
> **On-device runtime уже доказан сторонним `jarvis.zip`:** на Midi
> работает microWakeWord 84 КБ, ~1% CPU в тишине, ~20% при звуке,
> 22–32 МБ RSS. Главная техническая деталь — Yandex `libtflite.so`
> требует `TfLiteCustomOpResolverCreate("FULL")`; обычный
> `TfLiteInterpreterCreate` падает. Поэтому главный риск теперь —
> **качество модели**, а не запуск.
>
> Рекомендуемый первый шаг — **server-side microWakeWord на `lab-host` +
> минимальный патч Pulsar `v0.4.1` C++** для быстрой проверки модели на
> живом микрофоне; затем **on-device** той же модели. Если важнее
> приватность/независимость от неттопа — можно сразу on-device, runtime
> уже проверен. Обновление до `v0.5.0` (Rust-порт) — отдельная миграция,
> не условие для «Рикардо».

---

## 1. Verdict

| Вопрос | Ответ |
|---|---|
| Можно поменять «Алиса» → «Яндекс» сейчас? | **Да**, select в HA, без перепрошивки. |
| Можно поставить «Рикардо» сейчас? | **Нет**, только после обучения модели и патча Pulsar. |
| Это реально? | **Да.** Есть рабочий open-source пайплайн, русские примеры и подходящее железо. |
| Главный риск? | Не интеграция, а **качество модели**: ложные срабатывания на музыке/ТВ и пропуски. |
| Рекомендуемый путь? | **MVP server-side** (быстро проверить модель) → **on-device** (приватность и независимость от неттопа). |
| Нужно ли обновлять Pulsar до v0.5.0? | **Нет** для MVP. v0.5.0 — полный Rust-порт; его миграция должна быть отдельной задачей. |
| Ориентир по срокам? | MVP 2–3 недели part-time; on-device ещё 1–2 недели; итого 4–7 недель с итерациями. |

---

## 2. Что проверено на живом контуре

### 2.1 Станция

Проверено по ADB 2026-10-26:

| Параметр | Значение |
|---|---|
| Модель | Yandex Station Midi `cucumber`, `STATION_SERIAL` |
| Прошивка | `0.369.1.46.4778262955.20261013.160` |
| Pulsar | `v0.4.1`, `/data/pulsar/pulsar`, armhf |
| Kernel / userland | Linux 5.4.180 aarch64; userspace armhf, glibc 2.30 |
| CPU | 4 ядра Amlogic, NEON/ASIMD |
| RAM | 974 МБ total; ~700 МБ free, ~286 МБ used |
| `/data` | 2.5 ГБ, свободно ~2.3 ГБ |
| Pulsar CPU | ~14% одного ядра в покое (с VQE/споттером) |
| Load average | ~1.76 на 4 ядра |
| Текущий wake word | `wake_word=alisa` в `/data/pulsar/state.conf` |
| HA select | `select.iandeks_stantsiia_midi_wake_word` = `Алиса`; опции `no_wake_word`, `Алиса`, `Яндекс` |

Вывод: запас по CPU/RAM/диску есть. Модель 50–100 КБ и инференс раз в
30 мс не должны заметно изменить нагрузку, но это нужно измерить, а не
предполагать.

### 2.2 Что уже есть в прошивке для on-device

Ключевой факт: на станции уже лежит **полный TensorFlow Lite runtime**,
который Pulsar и так загружает для яндексовских моделей:

```text
/system/vendor/quasar/libs/libtflite.so   ~3.3 МБ
```

В нём есть C API (`TfLiteModelCreateFromFile`, `TfLiteInterpreterInvoke`,
`TfLiteTensor*`) и нужные microWakeWord-операции:

- `CALL_ONCE`, `VAR_HANDLE`, `READ_VARIABLE`, `ASSIGN_VARIABLE`;
- `CONV_2D`, `DEPTHWISE_CONV_2D`, `FULLY_CONNECTED`, `RESHAPE`, `QUANTIZE`;
- XNNPACK/RUY delegates.

**Важная поправка, найденная в `jarvis.zip`:** стандартный
`TfLiteInterpreterCreate()` на этой сборке **падает** — у яндексовского
`libtflite.so` нет рабочего default resolver. Нужно использовать
яндексовский API:

```c
void *resolver = TfLiteCustomOpResolverCreate("FULL");
TfLiteInterpreter *it = TfLiteInterpreterCreateWithCustomResolver(
    model, resolver, options);
```

Без этого on-device-путь не заработает, хотя модель и ops в библиотеке есть.

То есть on-device путь **не требует тащить PyTorch или ONNX Runtime**.
Можно сделать тонкую обёртку через `dlopen`/`dlsym`, как уже сделано для
`libyandex_evil.so`. Это сильный аргумент, что on-device реалистичен.

### 2.3 Формат модели microWakeWord v2

Проверено на публичной модели `hey_jarvis.tflite`:

| Параметр | Значение |
|---|---|
| Размер | ~52 КБ |
| Вход | `[1, 3, 40]`, `int8` |
| Выход | `[1, 1]`, `uint8` (вероятность / 255) |
| Tensor arena | ~22.9 КБ (для TFLite Micro) |
| Feature frontend | 40-bin log-mel, окно 30 мс, шаг 10 мс |
| Frontend | TFLite Micro microfrontend + PCAN AGC + noise reduction |
| Инференс | каждые 3 feature-кадра = 30 мс |
| Сглаживание | среднее по `sliding_window_size` (обычно 5–10) |

Это не «тяжёлая» модель. Основная работа — точно повторить feature
frontend и streaming state, а не считать нейросеть.

---

## 3. Почему «Рикардо» нельзя включить настройкой

### 3.1 Pulsar v0.4.1

В `satellite/src/controller.cpp` жёстко:

```cpp
cfg.wake_words = {{"alisa", "Алиса", {"ru"}}, {"yandex", "Яндекс", {"ru"}}};
if (st_.wake_word != "alisa" && st_.wake_word != "yandex") st_.wake_word = "alisa";
...
std::string Controller::model_dir(const std::string &id) const {
  return kModels + id;  // /system/vendor/quasar/spotter_models/activation/<id>
}
```

`state.conf` принимает только `alisa`/`yandex`; HA select строится из
`cfg.wake_words`. Строка `words.txt` внутри модели — это лишь id фразы;
акустика обучена на звучание «Алиса»/«Яндекс». Замена строки не меняет
модель.

### 3.2 `yasmina` не подходит

В прошивке есть модель `yasmina`, но её телефоны — английские/арабские
(`Y AE1 S M IY1 N AH0`), это не русская «Ясмина». Как drop-in для
«Рикардо» она бесполезна.

### 3.3 v0.5.0 не решает проблему

Pulsar `v0.5.0` (2026-10-25) — это **полный порт демона с C++ на Rust**
(commit #28), а не «версия с кастомными wake word». В Rust-коде те же
`alisa`/`yandex` и тот же `MODELS`:

```rust
if st.wake_word != "alisa" && st.wake_word != "yandex" {
    st.wake_word = "alisa".into();
}
```

Плюс v0.5.0 — свежий порт, который нужно отдельно регрессировать на Midi
(голос, музыка, кнопки, HA, Sendspin). **Нельзя совмещать миграцию на
Rust с экспериментом над wake word**: если что-то сломается, будет
непонятно, что именно.

**Правило:** для MVP патчим `v0.4.1` C++, потому что именно он сейчас
работает на станции. v0.5.0 — отдельная задача после стабилизации.

---

## 4. Варианты и сравнение

| Вариант | Русский | Где работает | Смена слова | Качество / риск | Вердикт |
|---|---|---|---|---|---|
| Родной `alisa`/`yandex` | да | станция | нельзя | отлично, проверено | оставить fallback |
| `yasmina` | нет (EN/AR) | станция | нельзя | не подходит | исключить |
| **microWakeWord, server-side** | да, обучаем | `lab-host` | файл `.tflite` | 94% recall в RU-примере; зависит от LAN/неттопа | **MVP** |
| **microWakeWord, on-device** | да, обучаем | станция | файл `.tflite` | то же; приватность, нет LAN-аудио | **production** |
| openWakeWord, server-side | да, но качество ниже | `lab-host` | файл | RU «Джарвис» ~43% recall; хуже | fallback/не основной |
| HA openWakeWord app | да (обучаем) | HA | файл | рассчитан на Wyoming streaming, не на ESPHome satellite | не прямой путь |
| Porcupine | **нет RU** в актуальных языках | станция | платно | enterprise/licensing | исключить |
| Vosk/Whisper keyword | да | nettop | грамматика | тяжёлый, latency | не для wake |
| Отдельный ESP32-S3 / Voice PE | да | рядом | файл | не использует 4 микрофона Midi | только workaround |
| Кнопка | — | станция | — | 100%, но не hands-free | fallback |

### 4.1 Почему server-side первым

1. Не нужно собирать TFLite/frontend под armhf и отлаживать streaming state
   в C++.
2. Модель и пайплайн те же, что потом пойдут on-device; переобучать не
   нужно.
3. Быстрее получить реальные цифры recall/FP именно на голосе пользователя.
4. Pulsar уже умеет отдавать очищенный VQE-аудиопоток 16 кГц; патч
   минимальный.
5. Если модель окажется плохой, мы узнаем это до встраивания в демон.

### 4.2 Почему on-device — конечная цель

1. Аудио не покидает станцию до wake word.
2. Нет зависимости от неттопа и LAN.
3. Нет постоянного трафика ~32 КБ/с (около 2.7 ГБ/сутки).
4. Меньше задержка: нет TCP/Python-слоя.
5. Станция уже имеет `libtflite.so`, а модель — 50–100 КБ.

Server-side MVP — это **лабораторный этап**, а не окончательная
архитектура. В плане должен быть явный переключатель
`wake_backend=gateway|ondevice`, чтобы можно было вернуться.

---

## 5. Рекомендуемая архитектура

### 5.1 MVP: server-side gateway

```text
Pulsar v0.4.1 (станция)
  ALSA 4 mic + speaker ref
    → Yandex VQE (AEC/beamforming/NR)
    → VQE_OMNI_WITH_GAIN, 16 kHz mono s16, чанки 512 сэмплов (~32 мс)
    → TCP 192.0.2.26:10310, framed PCM + token
         │
         ▼
wakeword-gateway (lab-host, Python + pymicro-wakeword)
  microWakeWord model ru_ricardo_mww.tflite
  feature frontend → streaming CNN → probability
  WAKE <prob> по тому же TCP
         │
         ▼
Pulsar: on_wake("Рикардо", level) → pre-roll 0.5 с → HA Assist pipeline
```

Почему TCP: 32 КБ/с, LAN, порядок и reconnect важнее, чем экономия
заголовков. UDP с потерями сломает streaming-модель.

Приватность: аудио идёт только по локальной сети на свой неттоп, не в
интернет. Но это изменение модели угроз: раньше аудио не покидало
колонку до wake. Если это неприемлемо — сразу on-device, но дольше.

### 5.2 Production: on-device

```text
Pulsar v0.4.1 (станция)
  VQE_OMNI (16 kHz mono, echo-cancelled)
    → microfrontend (40-bin log-mel, 30/10 мс)
    → libtflite.so (dlopen) + ru_ricardo_mww.tflite
    → sliding window → on_wake("Рикардо")
    → существующий HA pipeline
```

Никакого TCP и Python на станции. CPU-бюджет: ориентир <5% одного ядра,
инференс <10 мс; это нужно подтвердить на устройстве.

---

## 6. Модель «Рикардо»: обучение

### 6.1 Инструмент

`interkelstar/microwakeword-trainer` (MIT) — конфигурируемый пайплайн
вокруг microWakeWord. Он делает:

1. синтез позитивов/негативов через Piper;
2. извлечение 40-bin features через `pymicro-features`;
3. обучение MixedNet/Inception;
4. экспорт uint8/int8 TFLite + JSON manifest;
5. оценку на WAV и FP-mining.

Требования: Linux, Python 3.9+, ~15 ГБ диска, GPU сильно рекомендуется.
На неттопе i7-12650H (16 потоков, 14 ГБ RAM, 62 ГБ free, без NVIDIA)
полный прогон 8–14 ч CPU. На арендованном GPU — 0.5–3 ч обучения,
полный пайплайн 6–12 ч; стоимость порядка единиц долларов за прогон
(проверять актуальный тариф).

### 6.2 Данные

| Класс | Объём | Источник |
|---|---|---|
| Позитивы | 50k клипов | Piper RU: `dmitri`, `denis`, `irina`, `ruslan` |
| Валидация | 10k | те же голоса, другие фразы/интонации |
| Реальные позитивы | 50–200 | пользователь/домочадцы, 16 кГц mono, 0.5–4 м, тихо/нормально/громко |
| Негативы | 2–3 ч | музыка, ТВ, подкасты, бытовая речь |
| HF-негативы | ACAV100M и др. | автоматически скачивает trainer |
| FP-mining | 100–300 клипов | реальные ложные срабатывания после первых дней |

Важно: реальные записи сначала использовать **для оценки**, а не
обязательно для обучения. Trainer предупреждает, что комнатный шум в
позитивах может ухудшить обобщение. Схема:

1. обучить на синтетике;
2. измерить recall на 50 held-out реальных записях;
3. если recall < 90% — добавить чистые реальные позитивы или
   fine-tune;
4. собрать реальные FP и добавить их в `user_negatives/` с dense
   windowing;
5. повторить `features → train → export`.

Именно FP-mining дал основной прирост в RU-примере: 208 реальных
FP-клипов снизили hard-case FP с 91/140 до 1/140.

### 6.3 Конфиг

Базовая конфигурация — `example_ru_jarvis.yaml`, переименованная в
`ricardo.yaml`:

```yaml
model_name: ru_ricardo
wake_word: "Рикардо"
author: station-lab
trained_languages: ["ru"]
version: 1

target_phrases:
  - Рикардо
  - Рикардо!
  - Рикардо?
  - Рикардо,

training:
  n_samples: 50000
  n_samples_val: 10000

voices:
  primary:
    base_url: https://huggingface.co/rhasspy/piper-voices/resolve/main/ru/ru_RU
    models:
      ru_RU-dmitri-medium: dmitri/medium
      ru_RU-denis-medium: denis/medium
      ru_RU-irina-medium: irina/medium
      ru_RU-ruslan-medium: ruslan/medium

negative_phrases:
  - Рик
  - Рика
  - Рикар
  - Кардо
  - Ардо
  - Рико
  - Рикки
  - Рикарда
  - Риккардо
  - Бардо
  - Лардо
  - Карл
  - Кардинал
  - Помидор
  - Командо
  - Рекорд
  - Алиса
  - Яндекс
  - Джарвис
  - Компьютер
  - Привет
  - Слушай
  - Окей
  - Стоп
  - Хватит

microwakeword:
  architecture: mixednet
  spectrogram_length: 204
  stride: 3
  training_steps: 10000
  batch_size: 128
  learning_rate: 0.001
  positive_class_weight: 1.5
  negative_class_weight: 1.0
  probability_cutoff: 0.9
  sliding_window_size: 10
```

Команды:

```bash
git clone https://github.com/interkelstar/microwakeword-trainer
cd microwakeword-trainer
chmod +x setup_mww.sh && ./setup_mww.sh
cp example_ru_jarvis.yaml ricardo.yaml   # затем править под «Рикардо»
./run_mww.sh --config ricardo.yaml
# output: ru_ricardo_mww.tflite + ru_ricardo_mww.json
```

### 6.4 Лицензии голосов

Это важно, если модель когда-либо будет опубликована:

| Голос | Лицензия | Можно использовать |
|---|---|---|
| `ru_RU-dmitri-medium` | CC0 | да |
| `ru_RU-denis-medium` | CC0 | да |
| `ru_RU-irina-medium` | неизвестна | только личный эксперимент |
| `ru_RU-ruslan-medium` | CC BY-NC-SA 4.0 | только non-commercial, с attribution |

Для **распространяемой** модели: только CC0-голоса + собственные
записи с согласием. Для личного домашнего проекта допустимы все четыре,
но это надо явно зафиксировать в манифесте.

### 6.5 Метрики

| Метрика | Цель | Как измерять |
|---|---|---|
| Recall, тишина | ≥ 95% | 50 held-out реальных «Рикардо», 0.5/2/4 м |
| Recall, фон | ≥ 85% | ТВ/музыка/разговор на фоне |
| FP | ≤ 1/день в быту | 24 ч фонового аудио + логи |
| Latency | < 500 мс | от конца слова до `on_wake` |
| CPU | < 5% одного ядра | `top`/`pidstat` на станции |
| RAM | < 20 МБ | RSS Pulsar до/после |
| Model | 50–300 КБ | размер `.tflite` |

Публичные RU-модели на синтетике показывают всего ~60% clean recall и
1.5–22 FP/час; это не значит, что «Рикардо» будет таким же, но это
напоминание: **без реальных данных и FP-mining модель может быть
непригодной**. Не обещать «работает» до измерений.

### 6.6 Windows + RTX 5070: можно, но только через WSL2

Короткий ответ: **да, обучать можно на твоём ПК с RTX 5070, но не в
нативном Windows.** TensorFlow официально прекратил GPU-поддержку
native-Windows после 2.10; с 2.11 нужен WSL2. Trainer тоже Linux-only:
`setup_mww.sh` использует `apt`, Linux-бинарник Piper и Linux-пути.

Схема:

```text
Windows 11 + NVIDIA driver (Windows)
  └─ WSL2 Ubuntu 24.04
       ├─ CUDA toolkit for WSL (не драйвер!)
       ├─ Python 3.11/3.12 + venv
       └─ microwakeword-trainer + RTX 5070
```

Пошагово:

```powershell
# PowerShell от администратора
wsl --install -d Ubuntu-24.04
wsl --update
# перезагрузка, затем в Ubuntu:
```

```bash
nvidia-smi                     # GPU должен быть виден внутри WSL2
sudo apt update
sudo apt install -y python3.12 python3.12-venv python3-dev \
  build-essential ffmpeg git curl
git clone https://github.com/interkelstar/microwakeword-trainer
cd microwakeword-trainer
chmod +x setup_mww.sh && ./setup_mww.sh
source .venv/bin/activate
python -c "import tensorflow as tf; print(tf.__version__); print(tf.config.list_physical_devices('GPU'))"
./run_mww.sh --config ricardo.yaml
```

Важные нюансы для RTX 5070 (Blackwell, `sm_120`):

- Нужен свежий **Windows**-драйвер NVIDIA и CUDA 12.8+; Linux-драйвер
  внутри WSL ставить **нельзя**, достаточно Windows-драйвера.
- Официальные TensorFlow wheels для Blackwell исторически проблемные:
  часть сборок не содержит нативных `sm_120`-ядер. Тогда TF либо падает,
  либо уходит в PTX JIT (первый запуск долгий, тензорные ядра
  недогружены). Проверять `tf.config.list_physical_devices('GPU')` и
  маленький `matmul` до запуска обучения.
- Если GPU не заводится: не тратить дни. Модель крошечная, CPU-обучение
  на i7/16 потоках всё равно реально (8–14 ч). RTX 5070 — ускоритель,
  а не обязательное условие.
- Для совместимости с trainer иногда лучше `tensorflow==2.16.x`, но он
  не знает Blackwell; это конфликт. Практичный порядок: сначала
  `setup_mww.sh` как есть → проверить GPU → при ошибке либо custom
  Blackwell wheel, либо CPU-fallback. Не менять код trainer без нужды.
- Держать проект в `~/microwakeword-trainer` (ext4 WSL), **не** в
  `/mnt/c/...`: иначе feature extraction/IO будут медленными.
- 15 ГБ диска в WSL; проверить `df -h`. Windows sleep выключать на время
  обучения (`powercfg /change standby-timeout-ac 0`).
- GPU ускоряет только `train`; генерация 50k клипов и feature extraction
  всё равно CPU-heavy (несколько часов).

Итог: **RTX 5070 подходит**, если готов поставить WSL2 и повозиться с
CUDA/TensorFlow. Если нужен минимум возни — обучать на неттопе/CPU или
арендовать GPU на 1–2 часа; для одной модели разница не критична.

### 6.7 «Рикардо» — хороший и сложный кандидат

Хорошо: 3 слога, чёткая структура `ри-кар-до`, редкое слово в русской
бытовой речи, не пересекается с «Алиса»/«Яндекс».

Сложно: испанское имя `Ricardo` часто встречается в музыке, футболе,
ТВ; близкие `рик`, `кардо`, `рикарда`, `риккардо`; возможны срабатывания
на испанскую речь.

Меры: негативы на все части; FP-mining на латиноамериканскую музыку,
ТВ, подкасты; порог 0.9–0.95; окно 10. Если FP остаётся >1/день —
рассмотреть двухсловную фразу «Эй, Рикардо» (но это уже другое
обращение, менять только с согласия).

---

## 7. Патч Pulsar для MVP (v0.4.1 C++)

### 7.1 Что уже есть

- `AudioFrontend::set_streaming(true)` отдаёт `VQE_OMNI_WITH_GAIN` чанки
  512 сэмплов (16 кГц) через `on_audio`.
- `Controller::answer_wake(phrase, preroll)` уже принимает pre-roll.
- `AudioFrontend` умеет включать/выключать wake detection и command
  spotter.
- `api_server` перечисляет `cfg.wake_words`; HA сам создаёт select.
- `WakeArbiter` использует id `st_.wake_word` для договорённости между
  колонками.

### 7.2 Минимальные изменения

1. **`src/controller.h/cpp`**
   - `Settings`: разрешить `wake_word=ricardo`; добавить
     `wake_backend=gateway|ondevice`, `wake_gateway_host`,
     `wake_gateway_port`.
   - `cfg.wake_words`: добавить
     `{"ricardo", "Рикардо", {"ru"}}`.
   - `model_dir("ricardo")`: для gateway-режима вернуть `alisa` (нужен
     VQE-конвейер), но native detection выключить.
   - `on_active_wake_words`: при `ricardo` включить gateway и выключить
     native spotter; при `alisa`/`yandex` — наоборот.
   - `on_audio`: в idle добавлять чанки в ring buffer 0.5 с и отправлять
     в gateway; в `LISTENING`/`arbitrating` — как сейчас.
   - `on_wake`: для gateway-события вызвать `answer_wake("Рикардо",
     preroll)`; не реагировать на собственный TTS (`SPEAKING`/
     `announcing`), если вероятность ниже высокого порога.

2. **`src/wake_gateway.h/cpp`** (новый)
   - TCP-клиент, reconnect/backoff.
   - Handshake `HELLO <token>\n` → `OK\n`.
   - Отправка `PWK1` + `u32 len` + s16le PCM.
   - Приём `WAKE <prob>\n`, `PING`.
   - Отдельный writer-поток + bounded queue (например, 50 чанков);
     при переполнении/обрыве дропать старые чанки, **никогда не
     блокировать VQE/audio thread**.
   - Refractory 1 с после `WAKE`; сброс feature-буфера на gateway.
   - Если 10 с нет `PING` — лог `wake gateway offline`; опционально
     автоматический fallback на `alisa` (по настройке), с записью в HA
     persistent notification.

3. **`src/audio/frontend.h/cpp`**
   - Не ломать существующий streaming для HA.
   - Добавить режим «constant streaming» для gateway; включать только
     при `wake_word=ricardo`.
   - Для on-device позже — отдельный детектор в этом же processing
     thread, чтобы не плодить потоки.

4. **`src/esphome/api_server.cpp`**
   - Скорее всего, без изменений: `cfg.wake_words` уже отдаётся в HA.
   - Проверить, что HA select показывает `Рикардо` и переключение
     приходит в `on_active_wake_words`.

5. **`state.conf`**
   - `wake_word=ricardo`
   - `wake_backend=gateway`
   - `wake_gateway_host=192.0.2.26`
   - `wake_gateway_port=10310`
   - токен — отдельный файл `0600`, не в `state.conf` и не в docs.

### 7.3 Pre-roll

Сейчас `on_wake` вызывает `answer_wake(phrase, {})` — первые слоги
команды могут потеряться. Для custom-пути нужен ring buffer на 0.5 с
(8000 сэмплов @16 кГц):

- в idle: push каждого чанка;
- при `WAKE`: drain в `answer_wake`;
- в `LISTENING`/`arbitrating`: не дублировать, иначе аудио повторится.

Это обязательный пункт: «Рикардо, включи свет» без паузы должно
работать.

### 7.4 Gateway на `lab-host`

```bash
python3 -m venv /home/labuser/nettop/venvs/wake
. /home/labuser/nettop/venvs/wake/bin/activate
pip install pymicro-wakeword==2.5.0
```

`pymicro-wakeword` 2.5.0: Apache-2.0, wheels x86_64/armv7/aarch64,
Python 3.9–3.13, умеет `MicroWakeWord.from_config()` и
`MicroWakeWordFeatures.process_streaming()`. Модель и JSON кладутся в
`/home/labuser/nettop/data/wakewords/`.

Скелет:

```python
import asyncio, struct
from pathlib import Path
from pymicro_wakeword import MicroWakeWord, MicroWakeWordFeatures

MODEL = Path("/home/labuser/nettop/data/wakewords/ru_ricardo_mww.json")
mww = MicroWakeWord.from_config(MODEL)
feats = MicroWakeWordFeatures()
TOKEN = Path("/home/labuser/.config/station/wake-token").read_text().strip()

async def handle(reader, writer):
    hello = await reader.readline()
    if hello.strip() != f"HELLO {TOKEN}".encode():
        writer.close()
        return
    writer.write(b"OK\n")
    await writer.drain()
    while True:
        hdr = await reader.readexactly(8)
        if hdr[:4] != b"PWK1":
            break
        n = struct.unpack("<I", hdr[4:])[0]
        pcm = await reader.readexactly(n)
        for feature in feats.process_streaming(pcm):
            if mww.process_streaming(feature):
                writer.write(b"WAKE 0.97\n")
                await writer.drain()
                break

async def main():
    server = await asyncio.start_server(handle, "0.0.0.0", 10310)
    async with server:
        await server.serve_forever()

asyncio.run(main())
```

Systemd-юнит, `Restart=always`, `RestartSec=3`; порт `10310/tcp` только
LAN, firewall разрешает `192.0.2.13`; токен `0600`.

### 7.5 Переключение и fallback

- HA select: `Алиса`, `Яндекс`, `Рикардо`, `no_wake_word`.
- При `Рикардо`: native spotter выключен, gateway включён.
- При `Алиса`/`Яндекс`: gateway выключен, native spotter включён.
- Если gateway недоступен:
  - в логе ошибка;
  - HA persistent notification;
  - опция `auto_fallback_alisa` (по умолчанию **выключена**, чтобы не
    менять поведение без согласия);
  - при включённой — Pulsar переключает active wake word на `alisa` и
    обновляет HA select.
- Никогда не оставлять станцию без wake word молча.

---

## 8. On-device этап

### 8.1 Почему это реально

- `libtflite.so` уже есть, C API и нужные ops подтверждены.
- Модель ~50–100 КБ, вход `[1,3,40]` int8, выход uint8.
- На станции 4 ядра, ~700 МБ free; инференс раз в 30 мс.
- Pulsar уже делает более тяжёлый VQE и stock spotter.

### 8.2 Что добавить

1. **`src/audio/micro_features.{h,cpp}`**
   - Vendor TFLite Micro microfrontend C library + `kissfft`
     (Apache-2.0), либо адаптировать `pymicro-features/src`.
   - 40-bin log-mel, 30 мс окно, 10 мс шаг, PCAN AGC/noise reduction.
   - Golden-тест: features на WAV должны совпадать с Python
     `pymicro_features` (допуск на float/int8).

2. **`src/audio/custom_wake.{h,cpp}`**
   - `dlopen("/system/vendor/quasar/libs/libtflite.so", RTLD_NOW|RTLD_GLOBAL)`.
   - `TfLiteModelCreateFromFile`, затем **обязательно**
     `TfLiteCustomOpResolverCreate("FULL")` +
     `TfLiteInterpreterCreateWithCustomResolver`; обычный
     `TfLiteInterpreterCreate` на этой сборке падает.
   - `TfLiteInterpreterAllocateTensors`; вход `int8[1][stride][40]`;
     `TfLiteTensorCopyFromBuffer`, invoke каждые `stride` кадров; выход
     uint8 через `TfLiteTensorCopyToBuffer`.
   - Sliding window, cutoff, refractory 1.2 с.
   - `reset()` — **пересоздать interpreter**: resource variables
     (streaming state) иначе не очищаются; при ошибке — fallback.
   - Warm-up ~100 feature-слайсов, как в ESPHome/Jarvis; иначе первые
     ложные/пропущенные срабатывания.

3. **`src/audio/frontend.cpp`**
   - После VQE брать `VQE_OMNI` (без AGC) или
     `VQE_OMNI_WITH_GAIN` (с AGC) — **проверить оба**.
   - microfrontend имеет свой PCAN AGC; двойной AGC может ухудшить.
   - 512-сэмпловый чанк не делится на 160; нужен ring buffer 160
     сэмплов, обрабатывать по 3 feature-кадра на 30 мс.
   - Детектор в том же processing thread; если latency/CPU выйдет за
     бюджет — отдельный worker с bounded queue, но не блокировать VQE.

4. **`src/controller.cpp`**
   - `wake_word=ricardo` + `wake_backend=ondevice`: native spotter
     выключить, custom detector включить.
   - `model_dir` для `ricardo` — `/data/pulsar/wakewords/ricardo.tflite`.
   - `on_wake("Рикардо", level)` — тот же путь, что gateway.
   - `cfg.wake_words` + HA select — как в MVP.

5. **`src/model` / installer**
   - Модель + JSON в `/data/pulsar/wakewords/`.
   - При старте: если файла нет/битый — fallback `alisa`, лог, HA
     notification.
   - Не включать auto-update Pulsar, пока патч не upstream.

### 8.3 Gate перед встраиванием

Сторонний `jarvis.zip` уже подтвердил, что связка работает на Midi:
Yandex TFLite 2.12 + `FULL` resolver + microfrontend + streaming model.
Поэтому gate сокращается до проверки **нашей** сборки:

1. Собрать `tools/tflite_probe` под armhf (zig 0.14.1 или CI Pulsar).
2. Загрузить `jarvis_ru_B6_mww.tflite` (или `hey_jarvis`) через
   `libtflite.so` с `FULL` resolver, сделать 100 invoke, вывести
   input/output dims, среднее время.
3. Скормить `tools/tflite_probe`/offline WAV известный «Джарвис» и
   убедиться, что score растёт; это проверяет frontend + model, а не
   только загрузку.
4. Если работает — on-device путь подтверждён; если нет — fallback на
   vendor TFLite Micro (Apache-2.0), +3–5 дней.

Этот gate нужно пройти **до** обучения «Рикардо», чтобы не вкладывать
недели в модель, которую некуда поставить. Теперь это не «исследование
наугад», а воспроизведение уже работающего решения.

### 8.4 Референс `jarvis.zip`: что он доказал

Архив `jarvis.zip` (SHA-256
`3ef5f0431843f97d9259741a109b45d97c4c9249d2ab363f8d246238e64a8ed0`,
теперь `artifacts/reference/jarvis.zip`) — проект «Jarvis» на Go, который
работает на Midi **рядом со штатной Алисой** и использует русский
microWakeWord «Джарвис».

Что внутри:

| Компонент | Значение |
|---|---|
| Модель | `models/jarvis_ru_B6_mww.tflite`, 84 КБ, v2, RU, автор `morvex885` |
| Манифест | cutoff `0.97`, window `5`, arena `30000` |
| Вход | `[1, 3, 40]` int8; выход uint8 |
| Frontend | vendored TFLite Micro microfrontend из `pymicro-features 2.0.2` |
| Runtime | Yandex `libtflite.so` 2.12; `TfLiteCustomOpResolverCreate("FULL")` |
| Аудио | `vqe_0` через `yio_audio` IPC `127.0.0.1:9850` (не ALSA!) |
| CPU | ~1% одного ядра в тишине, ~20% при звуке |
| RAM | ~22–32 МБ RSS |
| Бинарник | готовый `jarvis-armhf` + Go/C++ исходники |

Главные выводы:

1. **On-device microWakeWord на Midi реально работает.** Это не теория:
   есть исходники, модель, бинарник и измеренные CPU/RAM.
2. **`TfLiteInterpreterCreate` использовать нельзя** — нужен
   `TfLiteCustomOpResolverCreate("FULL")` +
   `TfLiteInterpreterCreateWithCustomResolver`. Это самая важная
   техническая деталь, которой не было в нашем плане.
3. **Frontend должен быть точным**: 30 мс окно, 10 мс шаг, 40 mel
   (125–7500 Гц), noise reduction, PCAN, log scale; int8-квантование
   `(v*256+333)/666 - 128`; warm-up 100 слайсов; reset через пересоздание
   interpreter.
4. **Модель 84 КБ и cutoff 0.97** — хороший ориентир по размеру и
   порогу; «Джарвис» и «Рикардо» — похожие по длине/структуре слова.
5. **Аудио-путь другой.** Jarvis берёт `vqe_0` из `maind`/`aliced` по
   `yio_audio`, чтобы не открывать ALSA и не оглушать Алису. На нашей
   станции `maind` остановлен Pulsar'ом, порта `9850` нет — этот путь
   **не совместим** с текущим Pulsar. Нужно кормить детектор из
   `VQE_OMNI` Pulsar'а, а не открывать второй ALSA capture.
6. **Это не drop-in для Pulsar/HA.** Jarvis — отдельный ассистент
   (ChatGPT Live + Pi) рядом со штатной Алисой, а не патч Pulsar. Его
   бинарник не надо ставить на нашу станцию: он конфликтует с Pulsar и
   завязан на `yio_audio`.
7. **Лицензии.** `mwwfe/third_party/microfrontend` — Apache-2.0, его
   можно vendor'ить с attribution. У Go-кода и модели `jarvis_ru_B6_mww`
   в архиве нет явной лицензии; использовать как reference/для теста, не
   публиковать без разрешения автора (`morvex885`).

Что делать с этим:

- **Не** копировать Go-код целиком; взять `mwwfe` (Apache-2.0) и
  реализовать TFLite-обёртку в Pulsar по описанному API.
- **Использовать `jarvis_ru_B6_mww` как тестовую модель**: если наш
  Pulsar-детектор срабатывает на «Джарвис» с этой моделью, значит
  frontend/runtime собран правильно. Потом заменить модель на
  `ru_ricardo_mww`.
- **Не запускать `jarvis-armhf` на живой станции** без отдельного
  согласования: это чужой бинарник, он конфликтует с Pulsar и требует
  `maind`/`aliced`.
- Если автор даст training config/лицензию — можно сравнить его
  гиперпараметры с нашим `ricardo.yaml`; но модель «Рикардо» всё равно
  нужно обучать отдельно.

Этот архив **не отменяет** server-side MVP как быстрый способ проверить
качество модели, но **снимает главный риск on-device**: runtime и
frontend уже проверены на этом железе. Если пользователь готов на 1–2
недели интеграции, можно сразу идти on-device; если нужен быстрый
результат — server-side MVP, затем перенос.

---

## 9. HA/UX и совместимость

- HA select: `select.iandeks_stantsiia_midi_wake_word`; опции
  `no_wake_word`, `Алиса`, `Яндекс`, `Рикардо`.
- Pipeline `Рикардо` не меняется; wake word остаётся metadata.
- Фразы: «Рикардо, поставь будильник…», «Рикардо, стоп»,
  «Рикардо, включи…».
- Локальный command spotter (`стоп`, `громче`, `тише`, `пауза`,
  `продолжи`) остаётся; custom wake его не заменяет.
- Будильник: stop через «Рикардо, стоп»; проверить, что музыка
  продолжается после stop и после snooze.
- Таймеры: встроенные Assist-таймеры; проверить wake word во время
  звонка (wake word работает как stop).
- Несколько колонок: `WakeArbiter` сравнивает `wake` id. Если на одной
  станции `ricardo`, на другой `alisa`, они не договорятся. Для
  нескольких станций — ставить одну модель и один id на всех.
- TTS/announce: gateway слышит собственный TTS через VQE; нужен guard
  или высокий порог, иначе самозапуск. В MVP лучше игнорировать custom
  wake в `SPEAKING`/`announcing`; barge-in — кнопкой/stock «стоп».

---

## 10. План реализации

### Этап 0 — backup и PoC (1–2 дня)

- [ ] Бэкап `/data/pulsar`, `state.conf`, текущего binary; зафиксировать
      `wake_word=alisa` как откат.
- [ ] Собрать/запустить `tflite_probe` под armhf; проверить
      `libtflite.so` + microWakeWord-модель (`jarvis_ru_B6_mww` из
      `jarvis.zip` как контрольную).
- [ ] Записать 20–30 «Рикардо» на телефон/ноутбук для быстрой проверки
      пайплайна (не для финальных метрик).
- [ ] Решить: server-side MVP или сразу on-device. Runtime уже
      подтверждён сторонним проектом; остаётся качество модели.

**Gate:** probe загружает модель и делает invoke; откат Pulsar не нужен.

### Этап 1 — модель, baseline (3–5 дней + 8–14 ч CPU)

- [ ] Поднять trainer на неттопе (Linux) или Colab GPU.
- [ ] `ricardo.yaml`, 50k позитивов, 10k val, негативы.
- [ ] Обучить baseline; `test_mww.py` на 20–30 записях.
- [ ] Записать recall/FP; если < 90% — добавить реальные позитивы.
- [ ] Положить `.tflite` + `.json` в
      `/home/labuser/nettop/data/wakewords/`.

**Gate:** recall ≥ 90% на реальных записях, FP ≤ 1/час на негативах
(предварительно; финально — после FP-mining).

### Этап 2 — server-side MVP (3–5 дней)

- [ ] Патч `v0.4.1` C++: `wake_word=ricardo`, gateway client,
      ring buffer, pre-roll, fallback.
- [ ] Собрать armhf (`make ARCH=armhf` / CI), задеплоить с `.prev`.
- [ ] Запустить gateway, handshake, TCP health.
- [ ] HA select `Рикардо`; native spotter выключен.
- [ ] E2E: «Рикардо, какой будильник?», «Рикардо, стоп»,
      «Рикардо, включи свет» без паузы.
- [ ] Проверить TTS/музыку/ТВ; логировать вероятности.

**Gate:** wake работает, pre-roll не теряет первые слоги, нет
самозапуска от TTS; откат на `alisa` одной командой.

### Этап 3 — живая настройка (1–2 недели)

- [ ] 24–48 ч фонового аудио/логов; собрать FP и misses.
- [ ] FP-mining → `user_negatives/` → retrain/export.
- [ ] Подобрать cutoff/window; A/B на реальных условиях.
- [ ] Проверить 0.5/2/4 м, тишину, ТВ, музыку, несколько голосов.
- [ ] Записать метрики в `logs/status-*`.

**Gate:** recall ≥ 90% clean / ≥ 80% noisy, FP ≤ 1/день; иначе не
включать как default.

### Этап 4 — on-device (1–2 недели)

- [ ] Vendor microfrontend, `custom_wake`, TFLite wrapper.
- [ ] `tflite_probe` → реальный детектор на WAV.
- [ ] Встроить в Pulsar, собрать armhf, деплой с `.prev`.
- [ ] Сравнить latency/CPU/FP с gateway; убрать gateway.
- [ ] Проверить, что `libtflite.so` не конфликтует с Yandex VQE.

**Gate:** on-device не хуже gateway по recall/FP, CPU < 5%, нет
регрессий; иначе оставить gateway.

### Этап 5 — upstream/поддержка (параллельно)

- [ ] Оформить патч как fork/PR; не включать auto-update.
- [ ] Если v0.5.0 Rust стабилен — портировать backend отдельным PR.
- [ ] Документировать модель, лицензии, метрики, откат.

---

## 11. Критерии приёмки

- [ ] «Рикардо» срабатывает с 0.5/2/4 м в тишине.
- [ ] Срабатывает в ТВ/музыке с recall ≥ 80%.
- [ ] «Рикардо, <команда>» без паузы не теряет первые слоги.
- [ ] Нет самозапуска от TTS/announce/музыки.
- [ ] FP ≤ 1/день в быту; ≤ 1/12 ч в фоне.
- [ ] Задержка < 500 мс от конца слова.
- [ ] CPU < 5% одного ядра; RSS +<20 МБ.
- [ ] Native `Алиса` остаётся fallback в HA select.
- [ ] Gateway down → лог/уведомление, без зависания Pulsar.
- [ ] Откат: `wake_word=alisa` + native spotter, без перепрошивки.
- [ ] Будильник/таймер/музыка/Spotify/MA не регрессировали.
- [ ] Модель и данные не содержат секретов; аудио не уходит в интернет.

---

## 12. Риски и меры

| Риск | Вероятность | Мера |
|---|---|---|
| Модель плохо ловит голос пользователя | средняя | реальные записи, 4 TTS-голоса, fine-tune |
| FP на испанскую музыку/ТВ | высокая | негативы, FP-mining, cutoff 0.9–0.95, при необходимости «Эй, Рикардо» |
| Синтетика даёт плохой real-world recall | средняя | не обещать до теста; реальные held-out записи |
| Патч ломает голос/музыку | средняя | v0.4.1, `.prev`, E2E, откат `ctl.sh disable` |
| v0.5.0 Rust-порт нестабилен | средняя | не обновлять в рамках MVP; отдельная миграция |
| Gateway down → нет wake | низкая | native fallback + HA notification; on-device позже |
| Приватность: аудио на nettop | средняя | LAN-only, token, on-device; не в интернет |
| TFLite lib не подходит | низкая | PoC probe; fallback TFLite Micro |
| Двойной AGC (VQE + microfrontend) | средняя | тест `VQE_OMNI` vs `WITH_GAIN` |
| Несколько колонок не арбитрируются | средняя | одинаковый id/модель на всех |
| Лицензии голосов | низкая | CC0 + свои записи для публикации |
| Pulsar auto-update затрёт патч | средняя | auto-update off; pin/fork; PR upstream |

---

## 13. Что нужно решить пользователю

1. **Приоритет:** быстрый рабочий MVP server-side или сразу on-device
   (приватность/независимость, но дольше)?
2. **Аудио на nettop:** допустим ли постоянный LAN-поток 16 кГц
   (≈2.7 ГБ/сутки) на время MVP?
3. **Фраза:** оставляем одно слово «Рикардо» или, если FP будет высоким,
   готовы на «Эй, Рикардо»?
4. **Обучение:** ночь на неттопе (8–14 ч) или арендованный GPU
   (быстрее, но данные уходят на чужой хост)?
5. **Реальные записи:** кто участвует (пользователь/домочадцы), сколько
   голосов, согласие на локальные записи?
6. **Pulsar:** разрешаешь патч `v0.4.1` с `.prev` и без auto-update?
   v0.5.0 не трогаем до отдельной миграции.
7. **Модель:** публиковать ли `.tflite` (тогда только CC0-голоса +
   свои записи) или оставить локально?

---

## 14. Источники

Локальные:

- `docs/STATE.md`, `docs/PENDING.md`, `docs/OPERATIONS.md`,
  `docs/TTS-WAKE-WORD-RESEARCH.md`;
- `projects/station/upstream/pulsar/satellite/src/controller.cpp` (v0.4.1):
  `cfg.wake_words`, `model_dir`, `on_wake`, `on_active_wake_words`;
- `projects/station/upstream/pulsar/satellite/src/audio/frontend.{h,cpp}`:
  `set_streaming`, `on_audio`, VQE output channels;
- `projects/station/upstream/pulsar/satellite/src/esphome/api_server.cpp`:
  wake word list/select;
- `projects/station/upstream/pulsar/satellite/Makefile` (armhf, zig 0.14.1);
- живая станция: `/data/pulsar/state.conf`,
  `/system/vendor/quasar/libs/libtflite.so`,
  `/system/vendor/quasar/spotter_models/activation/{alisa,yandex,yasmina}`;
- `artifacts/reference/jarvis.zip` (SHA-256
  `3ef5f0431843f97d9259741a109b45d97c4c9249d2ab363f8d246238e64a8ed0`):
  `README.md`, `wake_mww.go`, `wake.go`, `mwwfe/`, `models/jarvis_ru_B6_mww.*`.

Внешние:

- microWakeWord: https://github.com/kahrendt/microWakeWord
- RU trainer: https://github.com/interkelstar/microwakeword-trainer
- `pymicro-wakeword` 2.5.0: https://pypi.org/project/pymicro-wakeword/
- `pymicro-features`: https://github.com/rhasspy/pymicro-features
- ESPHome microWakeWord: https://esphome.io/components/micro_wake_word
- HA wake words: https://www.home-assistant.io/voice_control/create_wake_word/
- HA openWakeWord approach: https://www.home-assistant.io/voice_control/about_wake_word
- Piper voices: https://huggingface.co/rhasspy/piper-voices
- Pulsar: https://github.com/Morvex885/pulsar
- Pulsar v0.5.0 release: https://github.com/Morvex885/pulsar/releases/tag/v0.5.0
- Pulsar Rust port commit #28:
  https://github.com/Morvex885/pulsar/pull/28
- Porcupine languages: https://picovoice.ai/products/voice/wake-word
- TensorFlow install / WSL2 GPU: https://www.tensorflow.org/install/pip
- NVIDIA CUDA on WSL: https://docs.nvidia.com/cuda/wsl-user-guide/
- Microsoft CUDA in WSL: https://learn.microsoft.com/windows/ai/directml/gpu-cuda-in-wsl

---

## Приложение A. Быстрый ответ на вопрос «как поменять wake word сейчас»

Сейчас — только `Алиса`/`Яндекс`/выключить:

```yaml
service: select.select_option
target:
  entity_id: select.iandeks_stantsiia_midi_wake_word
data:
  option: "Яндекс"   # или "Алиса", "no_wake_word"
```

«Рикардо» появится в этом select только после патча Pulsar и установки
модели. До этого select его не покажет, потому что HA строит список из
`cfg.wake_words` самого демона.
