//! Протокол ESPHome Native API без ввода-вывода.
//!
//! Крейт владеет контрактом провода: protobuf-сообщения (prost), plaintext- и Noise-фрейминг,
//! протокольная `Session`, устойчивая таблица сущностей с ключами и `Capabilities`. Сервер в
//! `pulsar-esphome` добавляет сокеты и состояние.

#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

pub mod api_key;
pub mod entity;
pub mod frame;
pub mod pb;
pub mod session;
pub mod timezone;

/// Версия API в HelloResponse. 1.15 — поддержаны DeviceCapabilitiesRequest/Response.
pub const API_MAJOR: u32 = 1;
pub const API_MINOR: u32 = 15;
pub const ESPHOME_VERSION: &str = "2026.9.0";
pub const PROJECT_NAME: &str = "pulsar.satellite";

#[cfg(test)]
mod tests;
