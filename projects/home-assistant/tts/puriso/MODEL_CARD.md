---
license: other
license_name: cc-by-nc-sa-4.0-and-openrail-m
license_link: https://huggingface.co/dhammagift/piper-ru-puriso/blob/main/LICENSE-OpenRAIL-M
language:
- ru
tags:
- text-to-speech
- piper
- russian
pipeline_tag: text-to-speech
---

# Puriso: a Russian male voice for Piper (medium, 22 050 Hz)

Puriso is a Russian male voice for [Piper](https://github.com/OHF-Voice/piper1-gpl). Its name is Pali for "man". It was
made for [Dhamma.Gift](https://dhamma.gift), where it reads Russian translations of the Pali Canon. Piper has only a few
Russian voices, and we hope this one is useful beyond our project.

## How it was made

- **Base:** fine-tuned from Piper's [ru_RU-ruslan-medium](https://huggingface.co/rhasspy/piper-voices/tree/main/ru/ru_RU/ruslan/medium).
- **Teacher:** trained on Russian speech synthesized by Supertone's
  [Supertonic](https://huggingface.co/supertone-oss-archive/supertonic-3) voice **M1**. The input text carried our own
  stress marks, so the stresses are taught, not guessed. In other words, Puriso is a distillation of M1 into a small,
  fast Piper model that runs on a CPU, a phone or in the browser.
- **Phonemes:** espeak-ng `ru`, as in any Piper Russian voice. Dhamma.Gift adds its own fixes for Pali names in Cyrillic
  and for a few stresses ([dhammagift/pali-tts](https://github.com/dhammagift/pali-tts), `respell.py`). The voice works
  without them too.

## Listen

Upstream sample names are listed below; audio is not vendored in this repository.

- `samples/dn22-tak-ya-slyshal.mp3`: «Так я слышал. Однажды Благословенный проживал в стране Куру…»
- `samples/dn22-sozercanie.mp3`: «Вот, монахи, монах пребывает в созерцании тела как тела…»

## Use

```bash
pip install piper-tts
python -m piper -m ru_puriso2-medium.onnx -f out.wav -- "Так я слышал."
```

## Licence

Both of the following apply:
- **CC BY-NC-SA 4.0**, from the base voice's dataset (the [RUSLAN corpus](https://ruslan-corpus.github.io/)): attribution,
  non-commercial, share-alike.
- **BigScience OpenRAIL-M**: as a model trained on a Supertonic model's output, Puriso is a Derivative of the Model under
  that licence. Its use-based restrictions (Attachment A, in [LICENSE-OpenRAIL-M](LICENSE-OpenRAIL-M)) apply to you and
  to anything you build from Puriso. Changes from the original: a new voice distilled from M1's output, see above.

Made by Dhamma.Gift. Attribution: "Puriso voice by Dhamma.Gift, based on Piper ruslan and Supertonic M1".
