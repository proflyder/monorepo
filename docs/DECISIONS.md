# DECISIONS — ключевые решения

Факты — `STATE.md`; восстановление — `REBUILD.md`; секреты — `SENSITIVE-DATA.md`.

1. **Pulsar вместо штатной Алисы.** Полный контроль, локальный STT/TTS,
   HA Assist, egress-quarantine. `maind` остановлен; гибрид не поддерживается.
2. **Proxmox + HAOS VM + Ubuntu/K3s VM.** HAOS — Supervisor/add-ons;
   K3s — voice/Hermes/observability. Terraform — IaC, отдельные обновления.
3. **Wi-Fi 4-address/WDS uplink, Ethernet fallback.** Wi-Fi остаётся на PVE,
   `vmbr0` сохраняет L2; VM IP/URL не меняются. NAT/PCI passthrough не нужны.
   Оба uplink одновременно в мост не подключать.
4. **Tailscale Operator, не публичный HA.** Нет port-forward, TLS `*.ts.net`,
   ingress `/` → HAOS и `/grafana/` → Grafana. MFA в HA, ACL только `443`.
5. **STT/TTS локально.** GigaAM primary, Whisper fallback, Piper Puriso;
   Hermes — только свободная речь. Облако видит текст, но не аудио.
6. **Hermes не владеет state.** Таймеры/будильники/громкость/музыка — HA;
   локальные intents работают без LLM.
7. **MA + Spotify Connect.** MA — локальный хаб/плейлисты; Connect — телефон;
   Pulsar — Sendspin-плеер. Один аккаунт = один stream-slot.
8. **BLE gateway — аварийный provisioning.** Работает без Wi-Fi/облака;
   GATT SSID/PSK/token; 1/3/4/5. Старый QR/DPP провижининг удалён.
9. **OTA/ADB fortress + egress guard.** Защита от OTA/сброса/закрытия ADB;
   публичный WAN и Яндекс закрыты. Factory reset сносит защиту.
10. **Расписания — HA-native.** Будильники в `local_todo`, таймеры в `timer`,
    напоминания/заметки в `local_todo`; голос + dashboard. Legacy single-slot
    отключён.
11. **Recovery — локально + offsite.** HA/K3s/PVE/станция; проверять SHA-256.
    Factory-образа Midi нет; OTA-блок не заменяет recovery.
12. **Монорепозиторий + Bazel.** Go/Rust/mobile; hermetic build; тесты рядом
    с кодом. Rust — DSP/инференс, Go — control plane.

Меняется решение — обновить этот файл и `STATE.md`.
