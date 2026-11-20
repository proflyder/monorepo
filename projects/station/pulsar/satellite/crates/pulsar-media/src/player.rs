use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use thiserror::Error;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use tracing::error;

use pulsar_model::Playback;

use crate::backend;
use crate::core::{Channel, Exec, Machine, PlayId, PlayerEvent, Shared, with_machine};

#[derive(Clone, Debug, Error)]
pub enum PlayerInitError {
    #[error("cannot load the firmware GStreamer")]
    Load,
    #[error("gst_init failed")]
    GstInit,
    #[error("cannot build a playbin")]
    Pipeline,
    #[error("player: ch{0} has no output sink")]
    NoOutputSink(usize),
    #[error("cannot start the ANNOUNCE output")]
    Output,
    #[error("cannot create the GLib main loop")]
    MainLoop,
    #[error("player thread: {0}")]
    Thread(String),
    #[error("player: already started, one player per process")]
    AlreadyStarted,
}

struct Inner {
    shared: Arc<Shared>,
    next_id: Cell<u64>,
    exec: Option<Box<dyn Exec>>,
}

#[derive(Clone)]
pub struct PlayerHandle {
    inner: Rc<Inner>,
}

pub fn spawn(playback: &'static Playback) -> (PlayerHandle, UnboundedReceiver<PlayerEvent>) {
    match start(playback) {
        Ok(v) => v,
        Err(e) => {
            match &e {
                PlayerInitError::AlreadyStarted | PlayerInitError::NoOutputSink(_) => {
                    error!("{e}");
                }
                _ => {}
            }
            error!("player init failed");
            let (tx, rx) = unbounded_channel();
            let shared = Arc::new(Shared::new(tx));
            (PlayerHandle::new(shared, None), rx)
        }
    }
}

pub fn try_spawn(
    playback: &'static Playback,
) -> Result<(PlayerHandle, UnboundedReceiver<PlayerEvent>), PlayerInitError> {
    start(playback)
}

fn start(
    playback: &'static Playback,
) -> Result<(PlayerHandle, UnboundedReceiver<PlayerEvent>), PlayerInitError> {
    let (tx, rx) = unbounded_channel();
    let shared = Arc::new(Shared::new(tx));
    let process = backend::start(&shared, playback)?;
    Ok((PlayerHandle::new(shared, Some(Box::new(process))), rx))
}

impl PlayerHandle {
    pub(crate) fn new(shared: Arc<Shared>, exec: Option<Box<dyn Exec>>) -> Self {
        Self {
            inner: Rc::new(Inner {
                shared,
                next_id: Cell::new(1),
                exec,
            }),
        }
    }

    fn invoke(&self, f: impl FnOnce(&mut Machine) + Send + 'static) {
        if let Some(exec) = &self.inner.exec {
            exec.invoke(Box::new(move || {
                with_machine(f);
            }));
        }
    }

    pub fn play(&self, ch: Channel, urls: Vec<String>) -> PlayId {
        let id = PlayId(self.inner.next_id.get());
        self.inner.next_id.set(id.0 + 1);
        self.inner.shared.supersede(ch, Some(id));
        if self.inner.exec.is_none() {
            self.inner.shared.complete(ch, id, false);
        }
        self.invoke(move |m| m.play(ch, id, urls));
        id
    }

    pub fn pause(&self, ch: Channel) {
        self.invoke(move |m| m.pause(ch));
    }

    pub fn resume(&self, ch: Channel) {
        self.invoke(move |m| m.resume(ch));
    }

    pub fn stop(&self, ch: Channel) {
        self.inner.shared.supersede(ch, None);
        self.invoke(move |m| m.stop(ch));
    }

    pub fn state(&self, ch: Channel) -> crate::PlayState {
        self.inner.shared.state(ch)
    }

    pub fn set_channel_gain(&self, ch: Channel, gain: f32) {
        let gain = crate::core::clamp_gain(gain);
        self.invoke(move |m| m.set_gain(ch, gain));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Channel, PlayId, PlayState, PlayerEvent};

    #[test]
    fn degraded_player_fails_every_play_at_once() {
        let profile = pulsar_model::profile_from_compatible(b"yandex,cucumber\0").unwrap();
        let (h, mut rx) = spawn(&profile.audio.playback);
        let a = h.play(Channel::Music, vec!["/a.mp3".into()]);
        assert_eq!(
            rx.try_recv().unwrap(),
            PlayerEvent::Done { id: a, ok: false }
        );
        h.pause(Channel::Music);
        h.stop(Channel::Music);
        h.set_channel_gain(Channel::Music, 0.5);
        let b = h.play(Channel::Announce, Vec::new());
        assert_eq!(b, PlayId(2));
        assert_eq!(
            rx.try_recv().unwrap(),
            PlayerEvent::Done { id: b, ok: false }
        );
        assert!(rx.try_recv().is_err());
        assert_eq!(h.state(Channel::Music), PlayState::Idle);
        assert_eq!(h.state(Channel::Announce), PlayState::Idle);
    }
}
