#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    Flac = 0,
    Opus = 1,
    Pcm = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisType {
    Beat = 0,
    Loudness = 1,
    Spectrum = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    Log = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Play = 0,
    Pause = 1,
    Stop = 2,
    Next = 3,
    Previous = 4,
}

pub struct Format {
    pub codec: Codec,
    pub channels: u8,
    pub rate: u32,
    pub bits: u8,
}

pub struct Visualizer<'a> {
    pub buffer_capacity: usize,
    pub types: &'a [VisType],
    pub rate_max: u16,
    pub bins: u8,
    pub scale: Scale,
    pub f_min: u16,
    pub f_max: u16,
    pub display_offset_ms: i32,
}
