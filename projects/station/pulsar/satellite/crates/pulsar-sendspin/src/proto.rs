//! Plain protocol values shared by the actor and the C ABI layer (no FFI types here, so the
//! actor builds and its tests run on the host).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    Flac = 0,
    Opus = 1,
    Pcm = 2,
}

/// sendspin::VisualizerDataType (the ones the service asks for).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisType {
    Beat = 0,
    Loudness = 1,
    Spectrum = 3,
}

/// sendspin::VisualizerSpectrumScale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    Log = 1,
}

/// sendspin::SendspinControllerCommand (transport subset).
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
