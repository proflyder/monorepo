# TTS и wake word «Рикардо» — исследование и план

> **TTS superseded, 2026-10-26:** актуальный полный ресерч — [RESEARCH-RICARDO-TTS.md](RESEARCH-RICARDO-TTS.md), план — [PLAN-RICARDO-TTS.md](PLAN-RICARDO-TTS.md), выбранный пользователем голос и передача контекста — [RICARDO-TTS-HANDOFF.md](RICARDO-TTS-HANDOFF.md). Ниже сохранена предыдущая версия; рекомендации Silero CIS по умолчанию, auto-stress CIS и CUDA-only Qwen не считать актуальными. Пользователь исключил Kokoro и выбрал направление Eugene + RUAccent. Это не запись о развёртывании.

**Дата:** 2026-10-26  
**Статус:** research + implementation plan, **ничего в живом контуре не менялось**.  
**Область:** голос колонки (TTS) и активационная фраза (wake word). STT в этой
сессии не трогаем — им параллельно занимается другая сессия.

> **Wake word rev 2 (2026-10-26):** актуальное исследование и план по
> «Рикардо» перенесены в [`RESEARCH-RICARDO-WAKE-WORD.md`](RESEARCH-RICARDO-WAKE-WORD.md).
> Раздел 4 ниже — предыдущая версия; recommendation изменился: для MVP
> патчим **v0.4.1 C++**, а не v0.5.0 Rust; обновление до v0.5.0 — отдельная
> миграция. TTS-часть этого документа остаётся актуальной.

> Коротко: текущий голос — Piper `ru_RU-irina-medium` (женский, 22.05 кГц,
> среднее качество VITS). **Silero v5 CIS base больше не рекомендуется**:
> у него нет полноценного русского акцентора, ударения плохие (проверено
> 2026-10-26 на тексте с ловушками). Основной кандидат —
> **Kokoro-ru `dima` + RUAccent** (82M, 24 кГц, RTF ~0.11 на Mac CPU,
> OpenRAIL, ударения через RUAccent). Быстрый fallback —
> **Silero v5.6 RU `aidar` + RUAccent** (48 кГц, RTF ~0.01–0.03).
> Тест ударений: `artifacts/tts-samples/stress-test.html` (`artifacts/tts-samples/stress-test.html`, только локально).
> Wake word «Рикардо» **нельзя** получить настройкой: штатный споттер Pulsar
> использует закрытые яндексовские модели `alisa`/`yandex`/`yasmina`, а
> `words.txt` не меняет акустику. Нужен отдельный детектор. Рекомендуемый
> путь — **microWakeWord на неттопе + патч Pulsar**, с последующим переносом
> детектора на саму колонку.

---

## 1. Что есть сейчас (проверено)

### 1.1 Железо и контур

```text
Станция Midi 192.0.2.13
  Pulsar v0.4.1 (armhf, /data/pulsar)
    ALSA 4 mic + speaker ref → Yandex VQE (libyandex_evil.so)
      → wake spotter «Алиса»/«Яндекс» (dlopen)
      → ESPHome API :6053 → HA
    GStreamer ← HTTP-поток TTS/музыки

Неттоп lab-host 192.0.2.26
  HA Container 2026.10.4 :8123
  Whisper :10300 (fallback) / GigaAM ONNX (primary, другая сессия)
  Piper :10200 (ru_RU-irina-medium)
  Hermes :8642 → hermes-voice-proxy :8643
  Music Assistant :8095 / Sendspin :8927
```

Проверено на устройстве 2026-10-26:

| Факт | Значение |
|---|---|
| Pulsar | `v0.4.1`, `/data/pulsar/pulsar`, pid есть |
| `state.conf` | `wake_word=alisa` |
| Модели активации | `/system/vendor/quasar/spotter_models/activation/{alisa,yandex,yasmina}` |
| `alisa` | `acoustic_model.nnet` 1.9 МБ, `words.txt`: `filler 0`, `алиса 1` |
| `yandex` | `acoustic_model.nnet` 0.6 МБ, `words.txt`: `яндекс 1` |
| `yasmina` | `acoustic_model.nnet` 1.9 МБ, телефоны EN/AR (`Y AE1 S M IY1 N AH0`) |
| Своя модель | каталога `/data/pulsar/wakewords/` нет |
| HA pipeline | `Рикардо` (`01m5t7gvhp2c3bmvn3bw5emje7`) |
| TTS | `tts.piper`, голос `ru_RU-irina-medium`, контейнер `wyoming-piper:2.5.2`, `:10200` |
| Будильник | `script.alarm_announce` тоже вызывает `tts.piper` |
| STT | `stt.onnx_asr` primary, Whisper `medium-int8` fallback (меняется другой сессией) |

> Неттоп на момент исследования был недоступен по сети (`192.0.2.26` не
> отвечал), поэтому живые HA-настройки не перечитывались; таблица выше —
> канонические факты из `docs/STATE.md`, `projects/home-assistant/hermes/README.md` и логов
> `logs/status-20261026-*`, плюс проверка станции по ADB.

### 1.2 Почему текущий голос звучит плохо

Это не баг и не поломка. `ru_RU-irina-medium` — это:

- **женский** голос, а запрос — мужской;
- **VITS medium, 22.05 кГц**, то есть заметно более узкополосный и «пластиковый»,
  чем современные 48 кГц модели;
- **без управления просодией** (нет пауз/эмоций/акцентов);
- **со средним качеством ударений** в русском; Piper не умеет автоматически
  разрешать омографы так, как это делают современные RU-модели;
- **фиксированный**: в Piper нет выбора «красивый мужской тембр», только
  несколько готовых голосов (Denis/Dmitri/Ruslan), которые также синтетические.

То есть замена нужна не «починить», а **сменить движок и голос**.

### 1.3 Почему «Рикардо» нельзя просто вписать

Pulsar загружает **родные яндексовские** модели через `dlopen`
(`libyandex_evil.so`) и передаёт им каталог модели. В коде жёстко:

```rust
// satellite/crates/pulsar/src/controller/mod.rs (v0.5.0)
if st.wake_word != "alisa" && st.wake_word != "yandex" {
    st.wake_word = "alisa".into();
}
...
cfg.wake_words = vec![
    WakeWordInfo { id: "alisa",  phrase: "Алиса",  languages: ["ru"] },
    WakeWordInfo { id: "yandex", phrase: "Яндекс", languages: ["ru"] },
];
...
fn model_dir(id: &str) -> PathBuf {
    PathBuf::from(format!("/system/vendor/quasar/spotter_models/activation/{id}"))
}
```

Модель — не «словарь + акустика», а обученный `nnet` + verifier + TTS-timings.
`words.txt` лишь сопоставляет id фразы; замена строки на «рикардо» не меняет
акустическую модель. `yasmina` — это Yango/Yasmina для EN/AR, не русская
«Ясмина»; как drop-in она не подходит.

**Вывод:** нужен свой детектор. Это не «галочка в UI», а отдельная модель
(~50–300 КБ) + интеграция в Pulsar.

---

## 2. Требования и ограничения

| Требование | Следствие |
|---|---|
| Русский язык, мужской голос | Silero/MOSS/Chatterbox/XTTS; Piper — только fallback |
| Локально, без облака | Silero / MOSS / Chatterbox / Kokoro; не ElevenLabs/Yandex SpeechKit по умолчанию |
| Неттоп без GPU | CPU-only, RTF < 0.3 желательно; 14 ГБ RAM |
| Колонка 1 ГБ RAM, armhf | wake-детектор ≤ несколько МБ, без PyTorch на станции |
| Латентность ассистента | TTS first audio < 300–500 мс; wake < 300 мс |
| Лицензия | MIT/Apache/OpenRAIL; CC BY-NC допустим только для личного использования |
| Другие сессии | не трогать STT/Whisper/GigaAM; TTS добавлять отдельным сервисом |
| Откат | Piper остаётся на `:10200`; Silero на `:10209`; wake — native `alisa` |

---

## 3. TTS: варианты и выбор

### 3.1 Что важно для «красивого» голоса

1. **Тембр** — низкий/тёплый баритон, а не «диктор из 2010».
2. **Просодия** — паузы, лёгкие эмоции, вопрос/утверждение.
3. **Русские ударения и омографы** — `за́мок`/`замо́к`, `мо́локо`, `фотоси́нтез`.
4. **Частота дискретизации** — 48 кГц лучше для маленького динамика.
5. **Латентность** — короткие ответы (1–2 предложения) должны начинаться < 0.5 с.
6. **Интеграция** — Wyoming/OpenAI API в HA без своего кода.
7. **Лицензия** — MIT предпочтительно; CC BY-NC — только личный проект.

### 3.2 Сравнение (2026-10)

| Модель | Размер | RU | CPU RTF (ориентир) | Клонирование | Лицензия | HA-путь | Вердикт |
|---|---:|---|---:|---|---|---|---|
| **Piper** `ru_RU-*` | ~60 МБ | да | ~0.02 | нет | GPL-3.0 / CC | Wyoming (уже есть) | текущий, звучит синтетически |
| Silero v5 CIS base | 87 МБ | да, 29 голосов | ~0.04 | нет | **MIT** | Wyoming | **не использовать без RUAccent: плохие ударения** |
| **Silero v5.6 RU** | ~148 МБ | да, 5 голосов | ~0.01–0.03 | нет | CC BY-NC | Wyoming/свой | **быстрый fallback + RUAccent** |
| **Kokoro-ru** | 82M / 138 МБ ONNX | да, 3 голоса | **~0.11** | нет | OpenRAIL | свой Wyoming/OpenAI | **основной кандидат: `dima` + RUAccent** |
| MOSS-TTS-Nano | 100M / ~730 МБ ONNX | да, 20 языков | RTF ~1.16 на Mac CPU (тест) | да, 3–10 с | Apache-2.0 | свой OpenAI/Wyoming | **проверен: медленно/повторы, не подходит** |
| Chatterbox Multilingual V3 | 500 МБ | да, 23 языка | медленнее real-time на CPU | да | MIT | свой OpenAI | качество высокое, latency риск |
| XTTS-v2 | ~1.8 ГБ | да, 17 языков | CPU медленно | да | CPML (non-commercial) | свой | legacy, не для realtime |
| **Qwen3-TTS** 0.6B/1.7B | 0.6–1.7B | да, 10 языков | CUDA-only официально | да | Apache-2.0 | GPU/MLX | лучшее качество, но нужен GPU |
| ElevenLabs v3/Flash | cloud | да | ~75–330 мс TTFA | да | proprietary | HACS `openai_tts` | лучшее качество, но облако/деньги |
| Yandex SpeechKit | cloud | да, `filipp`/`ermil`/`zahar` | ~real-time | нет | proprietary | свой | отличный RU, но снова Яндекс/облако |

#### Что показал тест ударений (2026-10-26)

Текст с ловушками: «Привет! Я Рикардо. Проверим ударения: звонит, договор,
каталог, жалюзи, свёкла, щавель, торты, банты, аэропорты, бухгалтеры. На
двери висел замок, а вдали стоял старинный замок. Я стою дорого, но стою на
своём.»

| Вариант | Ударения | Вывод |
|---|---|---|
| Silero v5 CIS base `ru_alexandr` | **плохие** | CIS base рассчитан на явные `+`-метки; встроенного полноценного RU-акцентора/омографов нет |
| Silero v5 CIS base + **RUAccent** | хорошие | RUAccent заранее ставит `+`; звук тот же, но ударения исправлены |
| Silero v5.6 RU `aidar` (авто) | лучше, но проверить | новая RU-модель, свой акцентор |
| Silero v5.6 RU + **RUAccent** | хорошие | самый быстрый локальный вариант с контролем ударений |
| **Kokoro-ru `dima`** + RUAccent | хорошие | другой движок, 24 кГц, RTF ~0.11 на Mac CPU |
| MOSS-TTS-Nano (клон) | — | на CPU 90 с аудио на 210 символов (RTF ~1.16), повторы; **не подходит** |

Причина: `v5_cis_base_nostress` — это CIS-модель на 29 языков, а не
специализированная RU-модель. Она умеет читать `+`-метки, но не расставляет
их сама так, как это делает RUAccent. Поэтому «просто Silero CIS base» —
неправильный выбор для русского ассистента.

#### Silero v5.6 RU — быстрый fallback

- 5 голосов: `aidar` (мужской), `baya`, `kseniya`, `xenia`, `eugene`;
- 48 кГц, RTF ~0.008–0.03 после прогрева на Mac M3 Pro;
- лицензия **CC BY-NC** (для домашнего проекта ок, для коммерции — нет);
- с RUAccent ударения контролируемые; без него — проверить на тесте.

Тембры CIS base (если всё же использовать с RUAccent) по медианной F0:

| Голос | Медиана F0 |
|---|---:|
| `ru_safarhuja` | 106 Гц |
| `ru_alexandr` | 117 Гц |
| `ru_roman` | 121 Гц |
| `ru_dmitriy` | 122 Гц |
| `ru_bogdan` | 131 Гц |
| `ru_eduard` | 144 Гц |
| `ru_marat` | 146 Гц |
| `ru_igor` | 153 Гц |
| `ru_gamat` | 162 Гц |
| `ru_sibday` | 168 Гц |

#### Kokoro-ru — основной кандидат

- 82M параметров, 24 кГц, RTF ~0.10–0.11 на Mac CPU (9.8× realtime на
  ноутбучном CPU по заявлению автора);
- три голоса: `sveta`, `masha` (женские), **`dima`** (мужской);
- **RUAccent на входе**: ударения, ё, омографы, редукция гласных;
- лицензия OpenRAIL (weights), код Apache-2.0; голоса — согласованные
  студийные актёры;
- WER round-trip 2.50% против 4.38% у Piper (79 слов, ASR — не идеальная
  метрика, но показатель разборчивости);
- ограничения: тембр темнее исходных записей, детерминированная просодия,
  редкие огрехи омографов; один мужской голос.

Kokoro — не «drop-in Wyoming»: нужен свой маленький TTS-сервер (Wyoming или
OpenAI-совместимый), который вызывает `ru_g2p` + ONNX. Это ~200–300 строк
Python; зато ударения и preprocessing под нашим контролем.

#### MOSS-TTS-Nano — проверен, не подходит

- 100M параметров, Apache-2.0, 20 языков включая русский, 48 кГц stereo;
- zero-shot клонирование по 3–10 с референса;
- ONNX CPU-версия; на MacBook Air M4 разработчики заявляют real-time на
  одном ядре;
- **фактический тест 2026-10-26** на i7/M3 CPU с ONNX: 210 символов →
  90 с аудио, RTF ~1.16, слышимые повторы/«плывущая» просодия. Для
  ассистента с ответами 1–2 предложения это неприемлемо.
- Можно вернуться, если появится GPU или выйдет MOSS-TTS 2.0 с лучшей
  стабильностью; в текущий план не берём.

#### Chatterbox Multilingual V3

- 500M, MIT, 23+ языка включая русский, клонирование, watermarked;
- качество высокое (Resemble заявляет production-ready для RU, CER 1–5%);
- на CPU 500M, скорее всего, будет **медленнее real-time**; на i7-12650H
  можно ожидать ~0.3–1.0× real-time. Для коротких ответов latency 1–3 с —
  на грани; нужен бенчмарк.
- Если есть GPU (даже RTX 3060) — сильный кандидат.

#### Qwen3-TTS

- Apache-2.0, 0.6B/1.7B, русский, лучший WER среди открытых по техническому
  отчёту; клонирование 3 с;
- официально CUDA-only; на Mac есть MLX-порт, на неттопе без GPU — нет.
- Держим как future option при появлении GPU.

#### Облако

- **ElevenLabs v3/Flash** — лучшее качество/эмоции, русский 4.0/5 у носителей,
  Flash ~75 мс, v3 ~330 мс TTFA. Но текст ответа уходит в облако, подписка
  $5–22/мес, и это ломает локальную философию проекта.
- **Yandex SpeechKit** — лучший русский, голоса `filipp`, `ermil`, `zahar`;
  но это снова Яндекс и облако. Для лаборатории, которая разворачивает
  Pulsar именно чтобы уйти от Яндекса, — идеологически странно; только если
  пользователь явно выберет «качество важнее локальности».

### 3.3 Рекомендация

**Основной кандидат: Kokoro-ru `dima` + RUAccent.**

- 82M, 24 кГц, RTF ~0.11 на Mac CPU; на i7-12650H замерить, ожидаемо
  ~0.1–0.2;
- RUAccent на входе решает ударения/омографы/ё;
- OpenRAIL, один мужской голос `dima`;
- нужен свой маленький TTS-сервер (Wyoming/OpenAI) — это часть плана.

**Быстрый fallback: Silero v5.6 RU `aidar` + RUAccent.**

- 48 кГц, RTF ~0.01–0.03, тот же Wyoming-подход;
- лицензия CC BY-NC (для дома ок);
- если голос Silero не нравится, остаётся Kokoro.

**Silero v5 CIS base — не использовать как основной.** Причина — не
«плохая модель вообще», а отсутствие полноценного RU-акцентора: без
RUAccent ударения в тесте неверные. С RUAccent звук становится корректнее,
но тембр остаётся silero-«дикторским»; пользователь его уже отверг.

**Если ни Kokoro, ни Silero не устраивают:** Chatterbox V3 (нужен GPU или
готовность к latency 1–3 с), Qwen3-TTS (GPU) или облако (ElevenLabs /
Yandex SpeechKit). MOSS-TTS-Nano отклонён по результатам теста.

### 3.4 Сэмплы

Сгенерированы два текста:

- короткий: «Привет! Я Рикардо. Чем могу помочь?»
- длинный: «Сейчас в Москве плюс двадцать три градуса, небольшой ветер.
  Будильник на семь тридцать установлен.»

Файлы:

```text
artifacts/tts-samples/
  stress-test.html               # главный тест: ударения-ловушки, все кандидаты
  index.html                     # большой набор голосов (Silero/Piper)
  piper/                         # текущий Irina + мужские Denis/Dmitri/Ruslan
  silero-v5-cis/                 # 10 мужских голосов, short/long + stress
  silero-v5-cis-ruaccent/        # CIS + внешний RUAccent
  silero-v5.6-ru/                # aidar/eugene, auto + RUAccent
  kokoro-ru/                     # Dima, short/long/stress
  moss/                          # MOSS (для протокола; не подходит)
```

Слушать нужно **на самой колонке** (маленький динамик меняет тембр) и в
наушниках. Субъективную оценку «красивый/приятный» может дать только
пользователь; сэмплы — для этого.

### 3.5 Текстовая подготовка

Даже лучший TTS испортится, если скормить ему Markdown/URL/цифры:

- `hermes-voice-proxy` уже убирает URL, Markdown-ссылки, цитаты, списки;
- добавить в TTS-прокси/обёртку:
  - удаление `http(s)://…`, `**`, `__`, `#`, emoji;
  - нормализация чисел/сокращений (`23` → `двадцать три`, `т.е.` → `то есть`);
  - для имён — фиксированное ударение: `Рик+ардо` (проверить на сэмплах);
  - максимум 1–2 предложения/180 символов — уже задано в voice prompt Hermes.
- Silero v5 поддерживает SSML; не злоупотреблять `prosody`, иначе звучит
  неестественно. Начать с `rate=1.0`, `pitch=0`.
- Кэш HA: после смены движка/голоса очистить TTS-кэш (`/config/tts`) или
  использовать `cache: false` для проверки, иначе HA может отдавать старый
  аудиофайл.

### 3.6 Интеграция

> Делать после того, как другая сессия закончит правки STT, или как минимум
> не перезапускать `whisper`/`onnx-stt`. Добавляем **отдельный** сервис,
> Piper не трогаем.

#### A. Kokoro-ru (рекомендуемый путь)

Готового «русского Kokoro» Wyoming-сервера нет: существующие
(`lfventura/wyoming-kokoro`, `nordwestt/kokoro-wyoming`) используют
английский G2P. Нужен форк/свой маленький сервер, который заменяет G2P на
`ru_g2p.py` + RUAccent.

Состав:

```text
projects/home-assistant/tts-kokoro/
  Dockerfile
  server.py            # Wyoming: describe / synthesize / synthesize-start/chunk/stop
  ru_g2p.py            # из zaakirio/kokoro-ru (Apache-2.0)
  config.json          # vocab
  kokoro-config.json   # vocab для ru_g2p
  espeak-data/         # 11 МБ, acute-aware ru_dict
  models/
    model_dima_quantized.onnx   # 138 МБ
    dima.bin                    # style pack 510x256
```

Ключевые зависимости (пин):

```text
onnxruntime==1.31.*
ruaccent==1.5.8.*
misaki==0.7.*          # см. патч ниже
phonemizer==3.4.*
espeakng-loader==0.2.*
numpy, soundfile, wyoming
```

Патч `ru_g2p.py` под новый `misaki` (в 0.7 `EspeakG2P.__call__` возвращает
строку, а не `(строка, токены)`):

```python
ps = self._espeak(marked)
if isinstance(ps, tuple):
    ps = ps[0]
```

Сервер (скелет):

```python
from wyoming.tts import Synthesize, SynthesizeVoice
from wyoming.audio import AudioStart, AudioChunk, AudioStop

def synth(text: str) -> bytes:
    ipa, oov = g2p(text)                  # RUAccent + espeak + IPA
    ids = [vocab[c] for c in ipa if c in vocab]
    audio = sess.run(None, {
        "input_ids": np.array([[0, *ids, 0]], np.int64),
        "style": style[len(ipa) - 1][None],
        "speed": np.ones(1, np.float32),
    })[0].squeeze()
    return (audio * 32767).astype(np.int16).tobytes()
```

HA: Wyoming Protocol → `192.0.2.26:10209` → TTS entity → pipeline
`Рикардо` → voice `dima`. Piper остаётся на `:10200`.

Docker/Compose:

```yaml
  tts-kokoro:
    build: ./tts-kokoro
    container_name: tts-kokoro
    restart: unless-stopped
    network_mode: host
    volumes:
      - ./data/kokoro:/data          # кэш моделей RUAccent/Kokoro
    environment:
      WYOMING_PORT: "10209"
      KOKORO_VOICE: dima
      KOKORO_THREADS: "8"
```

Модели скачать заранее (HF): `model_dima_quantized.onnx`, `dima.bin`,
`config.json`, `kokoro-config.json`, `ru_g2p.py`, `espeak-data/`.
RUAccent при первом запуске тянет свой акцентор — закэшировать в
`/data/ruaccent`, иначе первый синтез будет долгим/офлайн сломается.

#### B. Silero v5.6 + RUAccent (быстрый fallback)

Если Kokoro-голос не нравится, но нужен 48 кГц и минимальная latency:

- использовать `v5_6_ru.pt` (`aidar`) вместо CIS base;
- перед синтезом прогнать RUAccent и передать `+`-метки с
  `put_accent=False`;
- готовый `wyoming-silero` этого не умеет (он сам ставит ударения), поэтому
  нужен такой же маленький сервер/патч, как в A;
- лицензия CC BY-NC — только личный проект.

#### Общие шаги

1. Запустить только новый сервис:
   ```bash
   cd /home/labuser/nettop
   docker compose up -d tts-kokoro
   docker compose logs -f tts-kokoro
   ss -lntp | grep 10209
   ```
2. HA: Settings → Devices & services → Add integration → Wyoming Protocol →
   `192.0.2.26:10209`. Появится TTS-сущность (проверить в Developer Tools →
   Actions).
3. Pipeline: Settings → Voice assistants → `Рикардо` → Text-to-speech →
   engine Kokoro → voice `dima` → language `ru` → Update. STT не трогаем.
4. Проверка:
   ```yaml
   action: tts.speak
   target:
     entity_id: tts.kokoro
   data:
     media_player_entity_id: media_player.iandeks_stantsiia_midi_media_player
     message: "Привет! Я Рикардо. Звонит, договор, каталог, жалюзи, свёкла, щавель, торты, банты."
     cache: false
   ```
5. Будильник: в `projects/home-assistant/ha-voice/scripts.yaml` заменить `entity_id: tts.piper`
   на новый TTS.
6. Откат: в pipeline выбрать `tts.piper` / `ru_RU-irina-medium`; контейнер
   Kokoro остановить. Piper остаётся на `:10200`.

### 3.7 Ожидаемые цифры

| Метрика | Piper Irina | Silero v5.6 + RUAccent | Kokoro Dima |
|---|---|---|---|
| Частота | 22.05 кГц | 48 кГц | 24 кГц |
| RTF CPU (Mac M3 Pro) | ~0.02 | 0.008–0.03 | **0.10–0.11** |
| Первый звук | ~0.1–0.3 с + сеть | ~0.1–0.3 с + сеть | ~0.3–0.6 с + сеть |
| RAM | ~0.3–0.5 ГБ | ~1–2 ГБ | ~1–2 ГБ |
| Русские ударения | средне | RUAccent | **RUAccent** |
| Тембр | женский, «телефонный» | мужской, 48 кГц | мужской, 24 кГц |

На неттопе i7-12650H (16 потоков, AVX2) замерить фактически. Если Kokoro
окажется медленным — поднять `intra_op_num_threads`, уменьшить
`max_duration`/чанки, либо использовать Silero v5.6.

---

## 4. Wake word «Рикардо»: варианты

> **Устарело (v1).** Актуальная версия — §10 в
> [`RESEARCH-RICARDO-WAKE-WORD.md`](RESEARCH-RICARDO-WAKE-WORD.md).
> В частности, план «сначала обновить Pulsar до v0.5.0» отменён: v0.5.0 —
> полный Rust-порт, его миграция не должна блокировать wake word. Для MVP
> используется v0.4.1 C++.

### 4.1 Что доступно

| Вариант | RU | On-device | Смена слова | Качество | Вердикт |
|---|---|---|---|---|---|
| Родной `alisa`/`yandex` | да | да | нельзя | отлично | оставить fallback |
| `yasmina` | нет (EN/AR) | да | нельзя | — | не подходит |
| **microWakeWord on-device** | да (обучаем) | да | пересборка/файл | 94% recall, 1/140 FP (RU «Джарвис») | лучший конечный путь |
| **microWakeWord server-side** | да | неттоп | файл модели | то же | **рекомендуемый первый путь** |
| openWakeWord server-side | да | неттоп | файл | ниже (RU «Джарвис» ~43%) | fallback |
| Porcupine | нет RU | да | платно | — | не подходит |
| Vosk/Whisper keyword | да | неттоп | грамматика | тяжёлый/латентный | не для wake |
| Кнопка | — | да | — | 100% | без hands-free |

### 4.2 Рекомендация

**Первый путь — server-side microWakeWord на `lab-host` + патч Pulsar.**

Почему:

1. **Не нужно реверс-инжинирить формат Яндекса.** Модель обучается открытым
   `microwakeword-trainer` и запускается через `pymicro_wakeword` (Apache-2.0,
   есть wheel для x86_64/armv7/aarch64).
2. **Лучшее качество для русского** среди открытых: streaming CNN, 50–300 КБ,
   в примере автора — 94% recall на 50 реальных срабатываниях и 1/140 FP
   после FP-mining. openWakeWord на том же наборе даёт ~43% recall.
3. **Смена слова = замена файла** `.tflite` + `.json`, без пересборки Pulsar.
4. **Неттоп уже обязателен** для HA/STT/LLM/TTS; если он лежит, ассистент всё
   равно не работает. Дополнительная зависимость почти бесплатна.
5. **Pulsar уже отдаёт VQE-аудио** (16 кГц, echo-cancelled). Патч — небольшой:
   стримить его на неттоп и принимать событие wake.

Почему не on-device сразу:

- нужно встроить TFLite-Micro/pymicro-подобный runtime в Rust/C++ Pulsar,
  собрать под armhf glibc 2.30, отладить streaming-state и модель;
- это 1–2 недели R&D, тогда как server-side даёт рабочий «Рикардо» за дни;
- модель и пайплайн те же, поэтому on-device можно сделать вторым этапом без
  переобучения.

**Быстрый workaround, если «Рикардо» нужен за вечер:** отдельный
ESP32-S3 / Home Assistant Voice PE рядом с колонкой с прошитой
microWakeWord-моделью. Он не использует 4 микрофона и VQE Midi, но даёт
hands-free «Рикардо» без патча Pulsar. Это временный костыль, не замена
интеграции в станцию.

### 4.3 Архитектура server-side

```text
Pulsar (станция)
  VQE_OMNI_WITH_GAIN, 16 кГц mono s16
    │  TCP 192.0.2.26:10310, framed PCM (32 мс чанки)
    ▼
wakeword-gateway (lab-host, Python + pymicro_wakeword)
    │  модель /data/wakewords/ru_ricardo_mww.tflite
    │  features → MicroWakeWord → probability
    │  WAKE <prob> по тому же TCP
    ▼
Pulsar: on_wake("Рикардо", level) → pre-roll 0.5 с → HA pipeline
```

Почему TCP, а не UDP: 32 КБ/с, LAN, потери недопустимы для streaming-модели;
TCP даёт порядок и автоматический reconnect. Задержка в LAN — единицы мс.

Приватность: аудио идёт только по локальной сети на свой неттоп; в интернет
ничего не уходит. Но это изменение модели угроз: раньше аудио не покидало
колонку до wake. Если это критично — on-device этап 2. В плане предусмотрен
флаг `wake_backend=gateway|ondevice`, чтобы можно было переключить.

### 4.4 Что патчить в Pulsar

> Патчить **v0.5.0 (Rust)**, а не старый v0.4.1 C++. v0.5.0 вышла 2026-10-25,
> это актуальная ветка; v0.4.1 — снимок. Сначала обновить демон, проверить
> голос/музыку/кнопки, потом форк.

Файлы и изменения:

1. **`satellite/crates/pulsar/src/settings.rs`**
   - `wake_word` принимает `alisa`, `yandex`, `ricardo`;
   - новый `wake_backend` (`native`/`gateway`), `wake_gateway_host`,
     `wake_gateway_port`, `wake_token` (последние — в state.conf или
     отдельном `/data/pulsar/wake.conf`).

2. **`satellite/crates/pulsar/src/controller/mod.rs`**
   - в `cfg.wake_words` добавить:
     ```rust
     WakeWordInfo { id: "ricardo".into(), phrase: "Рикардо".into(),
                    languages: vec!["ru".into()] }
     ```
   - при старте, если `wake_word == "ricardo"`:
     - frontend всё равно стартует с native-моделью `alisa` (VQE нужен для AEC);
     - `mic.set_detection(false)` — native-споттер не должен срабатывать;
     - `mic.set_streaming(true)` — VQE-аудио идёт в контроллер;
   - `FrontendEvent::Audio` в idle-фазе: писать в ring buffer 0.5 с и в
     gateway-клиент; в `Listening`/`arbitrating` — как сейчас в HA.

3. **`satellite/crates/pulsar/src/controller/voice.rs`**
   - `answer_wake(phrase, preroll)` уже есть; передавать туда ring buffer;
   - `on_wake` для custom: `phrase="Рикардо"`, `level_db` из RMS последних
     чанков (или из gateway);
   - guard: не триггерить, пока `phase == Speaking || announcing` (иначе
     собственный TTS может активировать); для barge-in оставить кнопку/«стоп».

4. **`satellite/crates/pulsar-audio/src/frontend.rs`**
   - ничего не ломать: `set_streaming(true)` уже отдаёт
     `VQE_OMNI_WITH_GAIN` чанки по 512 сэмплов;
   - при custom-режиме streaming включён постоянно, а не только после wake;
   - ring buffer можно держать в контроллере, не в frontend.

5. **`satellite/crates/pulsar/src/wake_gateway.rs`** (новый)
   - TCP-клиент, reconnect/backoff;
   - handshake `HELLO <token>\n` → `OK\n`;
   - отправка: `PWK1` + `u32 len` + s16le PCM;
   - приём: `WAKE <prob>\n`, `PING\n`;
   - **отдельный writer-поток и bounded-очередь** (например, 50 чанков):
     при переполнении/обрыве дропать старые чанки, но никогда не блокировать
     VQE/audio-поток и main loop;
   - события в controller через `Loop::post` / канал;
   - если 10 с нет `PING` — лог `wake gateway offline`, опционально
     fallback на native `alisa` (по настройке);
   - после `WAKE` — refractory 1 с и сброс feature-буфера, чтобы не было
     серии повторных срабатываний.

6. **`satellite/crates/pulsar/src/controller/entities.rs`**
   - `on_active_wake_words`: если `ricardo` — включить gateway-режим;
     если `alisa`/`yandex` — native; persist.

7. **Pre-roll**
   - Сейчас в v0.5.0 `answer_wake` вызывается с `Vec::new()`; PR #27
     («пре-ролл аудио вместе с wake word») ещё открыт. Для custom-пути
     обязательно держать 0.5 с последних чанков: иначе «Рикардо, включи
     свет» без паузы потеряет «включи».
   - Реализация: `VecDeque<i16>` на 8000 сэмплов (0.5 с @16 кГц), push в
     idle, drain в `answer_wake`.

8. **Обновление до v0.5.0 (до патча)**
   ```bash
   # бэкап текущего бинарника и state
   adb pull /data/pulsar/pulsar backups/pulsar-v0.4.1-armhf.bin
   adb pull /data/pulsar/state.conf backups/pulsar-state-v0.4.1.conf
   # установка релиза
   curl -LO https://github.com/Morvex885/pulsar/releases/download/v0.5.0/pulsar-armhf.bin
   sha256sum pulsar-armhf.bin   # сверить с SHA256SUMS релиза
   adb push pulsar-armhf.bin /data/pulsar/pulsar.new
   adb shell 'cd /data/pulsar && cp -f pulsar pulsar.prev && \
     mv pulsar.new pulsar && chmod +x pulsar && ./ctl.sh restart'
   adb shell '/data/pulsar/ctl.sh log 80'
   ```
   Проверить: wake «Алиса», TTS, музыка, кнопки, HA entities. Откат:
   `adb shell 'cd /data/pulsar && mv pulsar.prev pulsar && ./ctl.sh restart'`.
   Только после этого ставить форк с gateway-патчем.

9. **Сборка**
   - `cargo-zigbuild` + zig 0.14.1, target `armv7-unknown-linux-gnueabihf.2.30`;
   - `make ARCH=armhf` в `satellite/`;
   - CI-форк GitHub Actions — удобнее, чем локальный кросс-компилятор;
   - деплой:
     ```bash
     adb push pulsar-armhf.bin /data/pulsar/pulsar.new
     adb shell 'cd /data/pulsar && cp -f pulsar pulsar.prev && \
       mv pulsar.new pulsar && chmod +x pulsar && ./ctl.sh restart'
     ```
   - откат: `mv pulsar.prev pulsar && ./ctl.sh restart`.

### 4.5 Gateway на неттопе

Зависимости:

```bash
python3 -m venv /home/labuser/nettop/venvs/wake
. venv/bin/activate
pip install pymicro-wakeword
```

`pymicro-wakeword` 2.5.0: Apache-2.0, wheels x86_64/armv7/aarch64, Python
3.9–3.13. На неттопе x86_64 — без сборки.

Скелет `wake_gateway.py`:

```python
import asyncio, json, struct, logging
from pathlib import Path
from pymicro_wakeword import MicroWakeWord, MicroWakeWordFeatures

MODEL = Path("/home/labuser/nettop/data/wakewords/ru_ricardo_mww.tflite")
CONF  = MODEL.with_suffix(".json")
mww = MicroWakeWord.from_config(CONF)          # читает .json рядом
feats = MicroWakeWordFeatures()
TOKEN = Path("/home/labuser/.config/station/wake-token").read_text().strip()

async def handle(reader, writer):
    hello = await reader.readline()
    if hello.strip() != f"HELLO {TOKEN}".encode():
        writer.close(); return
    writer.write(b"OK\n"); await writer.drain()
    while True:
        hdr = await reader.readexactly(8)
        if hdr[:4] != b"PWK1":
            break
        n = struct.unpack("<I", hdr[4:])[0]
        pcm = await reader.readexactly(n)
        for feat in feats.process_streaming(pcm):
            if mww.process_streaming(feat):
                writer.write(b"WAKE 0.97\n")
                await writer.drain()
                break

async def main():
    srv = await asyncio.start_server(handle, "0.0.0.0", 10310)
    async with srv:
        await srv.serve_forever()

asyncio.run(main())
```

Systemd:

```ini
[Unit]
Description=Wake word gateway (Ricardo)
After=network-online.target
Wants=network-online.target

[Service]
User=labuser
WorkingDirectory=/home/labuser/nettop/wakeword
ExecStart=/home/labuser/nettop/venvs/wake/bin/python wake_gateway.py
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
```

Порты/доступ:

- `10310/tcp` — только LAN; firewall на неттопе (ufw/nftables) разрешить
  `192.0.2.13`;
- токен — 32 байта base64, `chmod 600`, не публиковать;
- health: `systemctl status wakeword-gateway`, лог с вероятностями.

### 4.6 Обучение модели «Рикардо»

Инструмент: `interkelstar/microwakeword-trainer` (MIT). Требования: Linux,
Python 3.9+, ~15 ГБ диска, GPU сильно рекомендуется (CPU 8–14 ч).

Конфиг `ricardo.yaml` (на основе `example_ru_jarvis.yaml`):

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

training:
  n_samples: 50000
  n_samples_val: 10000

voices:
  primary:
    base_url: https://huggingface.co/rhasspy/piper-voices/resolve/main/ru/ru_RU
    models:
      ru_RU-dmitri-medium: dmitri/medium
      ru_RU-denis-medium: denis/medium
      ru_RU-ruslan-medium: ruslan/medium
      ru_RU-irina-medium: irina/medium

negative_phrases:
  - Рик
  - Рика
  - Кардо
  - Рикарда
  - Рикар
  - Ардо
  - Рико
  - Риккардо
  - Ricardo
  - Алиса
  - Яндекс
  - Привет
  - Компьютер
  - Стоп
  - Хватит
  - Карл
  - Рига
  - Риск
  - Бардо
  - Лардо
  - Джарвис

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
# положить ricardo.yaml
./run_mww.sh --config ricardo.yaml
# выход: ru_ricardo_mww.tflite + ru_ricardo_mww.json
```

Где обучать:

- **GPU (рекомендуется):** Colab T4 / RunPod / Vast — генерация 50k клипов
  упирается в CPU (несколько часов), обучение на GPU — 0.5–3 ч; полный
  пайплайн ~6–12 ч, стоимость порядка $1–5. Это самый быстрый путь.
- **Локально:** неттоп i7-12650H, CPU, ночью; 8–14 ч, 15 ГБ диска. Не
  запускать одновременно с активным использованием Whisper/HA.
- **Mac M3:** TensorFlow/MPS менее предсказуем; если запускать, только
  через Linux-контейнер/Colab, не на macOS.

Данные:

- 50k синтетических позитивов на 4 голосах;
- 10k валидация;
- **реальные записи**: 50–200 «Рикардо» от пользователя/домочадцев, 16 кГц
  mono, разные дистанции/громкость/фон; положить в `record/` и включить
  `train_with_recordings: true` (или использовать только для eval, если
  шумно);
- негативы: фоновая музыка/ТВ/подкасты — 2–3 часа; FP-mining после первых
  дней.

Метрики:

- recall clean ≥ 95% (цель), noisy ≥ 85%;
- FP ≤ 0.5/час на 10 ч негативов; в быту ≤ 1 ложного срабатывания/день;
- latency: 10 мс features + модель, на i7 — единицы мс;
- после FP-mining: добавить реальные ложные клипы в `user_negatives/`,
  переобучить `features → train → export`.

### 4.7 Почему «Рикардо» — хороший и сложный кандидат

Хороший:

- 3 слога, чёткая структура `ри-кар-до`;
- редкое слово в русской бытовой речи;
- не пересекается с «Алиса»/«Яндекс».

Сложный:

- испанское имя `Ricardo` часто встречается в музыке/футболе/ТВ;
- близкие `рик`, `кардо`, `рикарда`, `риккардо`;
- возможны ложные срабатывания на испанскую речь.

Меры:

- негативы на все части и испанские варианты;
- FP-mining на музыку (особенно латиноамериканскую), ТВ, подкасты;
- порог 0.9–0.95, скользящее окно 10;
- при высоком FP — поднять порог, а не переобучать сразу; при низком recall —
  понизить до 0.85 и добавить реальные позитивы.

---

## 5. План внедрения

### Этап 0 — координация и бэкап (30 мин)

1. Согласовать с сессией STT окно: не менять pipeline `Рикардо` одновременно;
   TTS-стадию можно менять после её правок.
2. Бэкап:
   - станция: `/data/pulsar` + `state.conf` (уже есть
     `backups/station-data-pre-pulsar-*`);
   - неттоп: `./ctl.sh backup` (compose/.env/HA config/MA);
   - HA: снапшот pipeline/настроек.
3. Зафиксировать текущие `tts.piper` и `wake_word=alisa` как откат.

### Этап 1 — TTS MVP (0.5–1 день)

1. Собрать `tts-kokoro` (Wyoming-сервер с `ru_g2p` + RUAccent) на `:10209`;
   Piper оставить на `:10200`.
2. Предзагрузить `model_dima_quantized.onnx`, `dima.bin`, `config.json`,
   `kokoro-config.json`, `espeak-data/`, кэш RUAccent; проверить SHA256.
3. HA Wyoming integration → `192.0.2.26:10209`.
4. Pipeline `Рикардо` → TTS Kokoro → voice `dima`.
5. `tts.speak` на колонку: stress-текст + короткая/длинная фраза; сравнить
   с `artifacts/tts-samples/stress-test.html`.
6. Если Kokoro-голос не нравится — переключить сервер на Silero v5.6
   `aidar` + RUAccent (тот же интерфейс).
7. Обновить `alarm_announce` на новый TTS.
8. Замерить latency/RAM; записать в `logs/status-*`.

**Gate:** ударения в stress-тексте верные на слух; голос приятнее Irina;
будильник/announce работают; откат на Piper проверен.

### Этап 2 — voice lab (опционально, 1–2 дня)

1. Если ни Kokoro, ни Silero v5.6 не устраивают — Chatterbox V3 на GPU или
   Qwen3-TTS (GPU/MLX).
2. Для Chatterbox: клонировать 10 с референса (с согласием), замерить
   RTF/first-audio.
3. A/B на колонке; оставить победителя.
4. MOSS-TTS-Nano не брать: тест дал RTF ~1.16 и повторы.

### Этап 3 — модель «Рикардо» (1–3 дня)

1. Обучить `ru_ricardo_mww.tflite` (GPU или ночь на CPU).
2. `test_mww.py` на записях и негативах.
3. Порог/окно подобрать; задокументировать recall/FP.
4. Положить модель + JSON в `/home/labuser/nettop/data/wakewords/`.

**Gate:** recall ≥90% на реальных записях, FP ≤1/час на негативах.

### Этап 4 — Pulsar gateway patch (2–4 дня)

1. Обновить Pulsar до v0.5.0 (или собрать форк сразу с патчем).
2. Добавить `wake_gateway.rs`, streaming idle, ring buffer, pre-roll,
   `ricardo` в `WakeWordInfo`.
3. Собрать armhf, задеплоить с `.prev`.
4. Запустить gateway, проверить TCP/handshake.
5. В HA выбрать wake word `ricardo`; проверить, что native `alisa` не
   срабатывает.
6. E2E: «Рикардо, какой будильник?» → ответ Silero.
7. Проверить «Рикардо, включи свет» без паузы (pre-roll).
8. Проверить музыку/ТВ/тишину; логировать вероятности.

**Gate:** wake работает, pre-roll не теряет первые слоги, нет ложных
срабатываний на собственный TTS.

### Этап 5 — живая настройка (1–2 недели)

1. Собрать статистику: misses/FP, дистанции, фон.
2. FP-mining → переобучение.
3. Подобрать порог.
4. Обновить docs/STATE/PENDING (после завершения, не одновременно с STT).

### Этап 6 — on-device (опционально, 1–2 недели)

1. Взять ту же `.tflite`-модель.
2. Встроить tflite-micro/pymicro-подобный runtime в Pulsar (C++/Rust FFI).
3. Убрать gateway/стриминг, оставить локальный детектор.
4. Сравнить latency/CPU/FP; оставить лучший режим.

---

## 6. Критерии приёмки

### TTS

- [ ] На колонке звучит мужской голос, субъективно приятнее Irina.
- [ ] Stress-текст: звони́т, догово́р, катало́г, жалюзи́, свёкла,
  щаве́ль, то́рты, ба́нты, аэропо́рты, бухга́лтеры — верно.
- [ ] Омографы: замо́к/за́мок, сто́ю/стою́ — верно.
- [ ] Числа читаются словами, URL/Markdown не читаются.
- [ ] Нет URL/Markdown/эмодзи в озвучке.
- [ ] `tts.speak` и `assist_satellite.announce` работают.
- [ ] Будильник (`alarm_announce`) и таймеры озвучиваются новым голосом.
- [ ] Первый звук < 500 мс (после прогрева).
- [ ] RAM/CPU неттопа в норме; HA не тормозит.
- [ ] Откат на Piper — одна смена engine в pipeline.

### Wake word

- [ ] «Рикардо» срабатывает с 0.5/2/4 м, в тишине/ТВ/музыке.
- [ ] «Рикардо, <команда>» без паузы не теряет первые слоги (pre-roll).
- [ ] Нет ложных срабатываний на собственный TTS/музыку.
- [ ] Ложные срабатывания ≤1/день в быту.
- [ ] Native `alisa` остаётся доступным как fallback (select в HA).
- [ ] Gateway down → понятный лог/статус, нет зависания Pulsar.
- [ ] Откат: `wake_word=alisa` + native spotter, без перепрошивки.

---

## 7. Риски и меры

| Риск | Вероятность | Мера |
|---|---|---|
| «Рикардо» путается с испанской музыкой | средняя | негативы, FP-mining, порог 0.9–0.95 |
| Модель плохо ловит голос пользователя | средняя | реальные записи, 4 TTS-голоса, дообучение |
| Pulsar-патч ломает голос/музыку | средняя | форк, `.prev`, E2E, откат `ctl.sh disable` |
| Pulsar v0.5.0 имеет регрессии | низкая | сначала обновить и проверить без патча |
| Неттоп недоступен → wake не работает | низкая | native fallback / HA select; HA и так нужен |
| Приватность: аудио уходит на неттоп | средняя | LAN-only, токен, on-device этап 6 |
| Silero звучит «дикторски» | средняя | MOSS/Chatterbox/другой голос |
| TTS-контейнер ест RAM | низкая | лимит 2–3 ГБ, мониторинг |
| Конфликт с сессией STT | средняя | отдельный сервис/порт, координация pipeline |
| Лицензия CC BY-NC | низкая | для дома ок; для коммерции — MIT `v5_cis_base` |

---

## 8. Что нужно решить пользователю

1. **Голос:** послушать `artifacts/tts-samples/index.html` и выбрать
   `ru_alexandr` / `ru_eduard` / другой. Если ни один не «красив» — делаем
   MOSS-clone (нужен референс 10 с и согласие).
2. **Локальность:** подтвердить, что аудио можно стримить на неттоп для
   wake (LAN-only). Если нет — сразу on-device, дольше.
3. **Облако:** допустим ли ElevenLabs/Yandex SpeechKit как fallback ради
   качества, или только локально?
4. **Тренировка:** есть ли GPU/Colab или обучаем ночью на неттопе?
5. **Окно:** когда можно трогать pipeline `Рикардо`, чтобы не конфликтовать
   с сессией STT.
6. **Имя:** «Рикардо» — только wake word, или ассистент должен так же
   представляться в ответах? (Сейчас pipeline уже называется «Рикардо».)

---

## 9. Источники

Локальные:

- `docs/STATE.md`, `docs/ARCHITECTURE.md`, `docs/PENDING.md`,
  `docs/RESEARCH-OWN-ASSISTANT.md`;
- `projects/home-assistant/stack/docker-compose.yml`, `projects/home-assistant/stack/.env.example`,
  `projects/home-assistant/hermes/README.md`, `projects/home-assistant/ha-voice/scripts.yaml`;
- `projects/station/upstream/pulsar/README.md`, `projects/station/upstream/pulsar/AGENTS.md`;
- `projects/station/upstream/pulsar/satellite/src/audio/frontend.cpp`,
  `projects/station/upstream/pulsar/satellite/src/controller.cpp` (v0.4.1);
- upstream `Morvex885/pulsar` v0.5.0 (Rust), `pulsar-audio/src/frontend.rs`,
  `pulsar/src/controller/{mod,voice,entities}.rs`;
- `artifacts/tts-samples/` (сэмплы).

Внешние:

- Silero Models: https://github.com/snakers4/silero-models
- Silero v5 CIS base (MIT): https://github.com/snakers4/silero-models/blob/master/LICENSE_CIS
- `staastas/wyoming-silero`: https://github.com/staastas/wyoming-silero
- `wf1nder/silero-wyoming`: https://github.com/wf1nder/silero-wyoming
- Kokoro-ru: https://huggingface.co/zaakirio/kokoro-ru
- RUAccent: https://github.com/Den4ikAI/ruaccent
- `lfventura/wyoming-kokoro`: https://github.com/lfventura/wyoming-kokoro
- MOSS-TTS-Nano: https://github.com/OpenMOSS/MOSS-TTS-Nano
- Chatterbox: https://github.com/resemble-ai/chatterbox
- Qwen3-TTS: https://github.com/QwenLM/Qwen3-TTS
- microWakeWord: https://github.com/kahrendt/microWakeWord
- `microwakeword-trainer`: https://github.com/interkelstar/microwakeword-trainer
- `pymicro-wakeword`: https://pypi.org/project/pymicro-wakeword/
- openWakeWord: https://github.com/dscripka/openWakeWord
- HA Wyoming: https://www.home-assistant.io/integrations/wyoming/
- HA wake word: https://www.home-assistant.io/voice_control/about_wake_word
- ElevenLabs models: https://elevenlabs.io/docs/overview/models
- Yandex SpeechKit voices: https://aistudio.yandex.ru/docs/en/speechkit/tts/voices
