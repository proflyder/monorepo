# Wake-word model

The Ricardo build uses the on-device microWakeWord model produced by the
station lab. The model is intentionally not committed to this public fork:
its training voices and recordings still need a license review.

Put the model here before installing from a checkout. The reference file below
is the original v1 baseline; the station currently runs the private, pinned v28
through the model-only deploy profile (`projects/station/configs/wakeword-personal.json`).
Do not copy personal weights into this directory: they stay in the private registry.

```text
ru_ricardo_mww.tflite   # v1 baseline: 86,240 bytes, SHA-256 f1ba6dca7c380b584d5e4e1d83f9fd2144e09f4ef9a68135e206f7aab5fd96b2
```

`pulsar-install` copies every `*.tflite` from this directory to
`/data/pulsar/wakewords/` and verifies the SHA-256 during the upload. A release
archive without this directory keeps the model already installed on the
station; a fresh install without it starts with no «Рикардо» detector and logs
that fact.

The daemon path is fixed:

```text
/data/pulsar/wakewords/ru_ricardo_mww.tflite
```

The detector uses the firmware's `libtflite.so` and the vendored
microfrontend, so neither the model nor the Yandex runtime is redistributed
here. The C++ bridge and its license notes are in
`crates/pulsar-audio/ricardo/`.

Model versions, data and the from-scratch recipe are documented in the
monorepo `docs/WAKE-WORD.md`.
