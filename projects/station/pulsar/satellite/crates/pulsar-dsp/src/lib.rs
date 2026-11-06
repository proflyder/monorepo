//! Чистый DSP звукового тракта: кольцевые буферы, уровни dBFS, FFT-спектр, endpoint-детектор и
//! WAV-контейнер. Ни FFI, ни модели станции, ни потоков: всё покрыто хостовыми тестами, включая
//! golden-данные спектра.
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

pub mod convert;
pub mod endpoint;
pub mod level;
mod ring;
pub mod spectrum;
pub mod wav;

pub use convert::{to_s16, to_s16_slice};
pub use level::{LEVEL_CHUNKS, LEVEL_SAMPLES, LEVEL_TOP, LevelDb, LevelHistory};
pub use ring::Ring;
pub use spectrum::SpectrumAnalyzer;
