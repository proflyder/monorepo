#[derive(Clone, Copy, Debug)]
pub struct StreamParams {
    pub codec: u8,
    pub rate: u32,
    pub channels: u8,
    pub bits: u8,
}

#[derive(Debug)]
pub enum LibEvent {
    Group {
        playing: Option<bool>,
        name: Option<String>,
    },
    TimeSync(f32),
    PairingStarted(String),
    PairingSucceeded(String),
    PairingFailed {
        server_id: String,
        reason: u8,
    },
    Trust {
        paired: bool,
    },
    PairingCode(String),
    PairingCodeCleared,
    PairingWindow(bool),
    StreamStart(Option<StreamParams>),
    StreamEnd,
    Volume(u8),
    Mute(bool),
    OutputDelay(u16),
    ControllerState {
        commands: usize,
        volume: u8,
        muted: bool,
    },
    Metadata {
        artist: Option<String>,
        title: Option<String>,
    },
    VisualizerStart {
        types: usize,
        rate_max: u16,
    },
    VisualizerEnd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SinkRequest {
    Configure { rate: u32, channels: u8, bits: u8 },
    Release,
}

pub fn sink_request(ev: &LibEvent) -> Option<SinkRequest> {
    match ev {
        LibEvent::StreamStart(Some(p)) => Some(SinkRequest::Configure {
            rate: p.rate,
            channels: p.channels,
            bits: p.bits,
        }),
        LibEvent::StreamEnd => Some(SinkRequest::Release),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_callbacks_request_the_sink() {
        let p = StreamParams {
            codec: 0,
            rate: 48000,
            channels: 2,
            bits: 16,
        };
        assert_eq!(
            sink_request(&LibEvent::StreamStart(Some(p))),
            Some(SinkRequest::Configure {
                rate: 48000,
                channels: 2,
                bits: 16
            })
        );
        assert_eq!(
            sink_request(&LibEvent::StreamEnd),
            Some(SinkRequest::Release)
        );
        assert_eq!(sink_request(&LibEvent::StreamStart(None)), None);
        assert_eq!(sink_request(&LibEvent::Volume(20)), None);
        assert_eq!(
            sink_request(&LibEvent::Group {
                playing: Some(false),
                name: None
            }),
            None
        );
    }
}
