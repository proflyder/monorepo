//! Доменные типы интерфейса вместо строк: эффекты кольца, стили музыки часов и кольца, лица
//! часов, цвет корпуса. Разбор строк — через [`FromStr`], список опций — [`Effect::ALL`],
//! [`MusicStyle::ALL`], [`ClockMusicStyle::ALL`], [`ClockFace::ALL`].

use std::convert::Infallible;
use std::fmt;
use std::str::FromStr;

use crate::font;
use crate::led_fx;

/// Эффект пользовательского света и `show_temporary`; `Solid` — ровный цвет, в опциях HA его
/// нет (там он зовётся "None").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Effect {
    #[default]
    Solid,
    Rainbow,
    Breathe,
    ColorLoop,
    Comet,
    Twinkle,
    Fire,
    Aurora,
    LavaLamp,
    Candle,
    NightLight,
}

impl Effect {
    /// В порядке опций ESPHome (`effect_names()`), без `Solid`.
    pub const ALL: [Self; 10] = [
        Self::Rainbow,
        Self::Breathe,
        Self::ColorLoop,
        Self::Comet,
        Self::Twinkle,
        Self::Fire,
        Self::Aurora,
        Self::LavaLamp,
        Self::Candle,
        Self::NightLight,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Solid => "",
            Self::Rainbow => "Rainbow",
            Self::Breathe => "Breathe",
            Self::ColorLoop => "Color loop",
            Self::Comet => "Comet",
            Self::Twinkle => "Twinkle",
            Self::Fire => "Fire",
            Self::Aurora => "Aurora",
            Self::LavaLamp => "Lava lamp",
            Self::Candle => "Candle",
            Self::NightLight => "Night light",
        }
    }
}

impl FromStr for Effect {
    /// Неизвестное имя — ошибка; граница решает сама (сейчас это сплошной цвет).
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Ok(Self::Solid);
        }
        Self::ALL.into_iter().find(|e| e.as_str() == s).ok_or(())
    }
}

/// Стиль визуализации музыки кольца (select «Music visualization»).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MusicStyle {
    #[default]
    Off,
    All,
    Spectrum,
    Pulse,
    Disco,
    Polar,
}

impl MusicStyle {
    pub const ALL: [Self; 6] = [
        Self::Off,
        Self::All,
        Self::Spectrum,
        Self::Pulse,
        Self::Disco,
        Self::Polar,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::All => "All styles",
            Self::Spectrum => "Spectrum",
            Self::Pulse => "Pulse",
            Self::Disco => "Disco",
            Self::Polar => "Polar",
        }
    }

    /// Конкретный стиль рендера; None у "Off" и "All styles" (ими занимается композитор).
    pub fn style(self) -> Option<led_fx::Style> {
        match self {
            Self::Off | Self::All => None,
            Self::Spectrum => Some(led_fx::Style::Spectrum),
            Self::Pulse => Some(led_fx::Style::Pulse),
            Self::Disco => Some(led_fx::Style::Disco),
            Self::Polar => Some(led_fx::Style::Polar),
        }
    }
}

impl FromStr for MusicStyle {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL.into_iter().find(|e| e.as_str() == s).ok_or(())
    }
}

/// Стиль музыки часов (select «Clock music»): набор другой, чем у кольца.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClockMusicStyle {
    #[default]
    Off,
    Levels,
}

impl ClockMusicStyle {
    pub const ALL: [Self; 2] = [Self::Off, Self::Levels];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Levels => "Levels",
        }
    }
}

impl FromStr for ClockMusicStyle {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL.into_iter().find(|e| e.as_str() == s).ok_or(())
    }
}

/// Лицо часов: индексы статичных лиц, «Blink», «All faces» и «None» (нет лица).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockFace {
    None,
    Static(u8),
    Blink,
    All,
}

impl ClockFace {
    /// В порядке опций ESPHome (`face_names()`), без `None`.
    pub const ALL: [Self; 19] = [
        Self::Static(0),
        Self::Static(1),
        Self::Static(2),
        Self::Static(3),
        Self::Static(4),
        Self::Static(5),
        Self::Static(6),
        Self::Static(7),
        Self::Static(8),
        Self::Static(9),
        Self::Static(10),
        Self::Static(11),
        Self::Static(12),
        Self::Static(13),
        Self::Static(14),
        Self::Static(15),
        Self::Static(16),
        Self::Blink,
        Self::All,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Static(i) => font::FACE_NAMES[usize::from(i)],
            Self::Blink => "Blink",
            Self::All => "All faces",
        }
    }

    /// Индекс `font::face_glyphs`; None у «None».
    pub fn index(self) -> Option<i32> {
        match self {
            Self::None => None,
            Self::Static(i) => Some(i32::from(i)),
            Self::Blink => Some(font::FACE_BLINK),
            Self::All => Some(font::FACE_ALL),
        }
    }
}

impl FromStr for ClockFace {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() || s == "None" {
            return Ok(Self::None);
        }
        Self::ALL.into_iter().find(|e| e.as_str() == s).ok_or(())
    }
}

/// Цвет корпуса: имя файла `/system/vendor/quasar/ledconfigs/<stem>.json`; пусто — без
/// коррекции. Набор зависит от прошивки, поэтому списка `ALL` нет.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CaseColor(Option<String>);

impl CaseColor {
    pub fn new(stem: &str) -> Self {
        Self((!stem.is_empty()).then(|| stem.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_deref().unwrap_or("")
    }
}

impl FromStr for CaseColor {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(s))
    }
}

impl fmt::Display for CaseColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
