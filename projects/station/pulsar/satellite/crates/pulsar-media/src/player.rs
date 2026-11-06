//! Audio playback through the GStreamer build bundled with the Yandex firmware into the
//! ALSA playback PCM (`Playback::pcm`: dmix + softvol). Two independent channels:
//!   Music    - media_player content, can be paused / ducked.
//!   Announce - TTS, announcements, earcons; plays over (ducked) music. Decoded by its playbin,
//!              played by the ANNOUNCE writer of `pulsar-playback` instead of an alsasink.
//!
//! Threading: a dedicated thread runs a GMainLoop on the default GMainContext; the bus watches and
//! timeouts live there, and so does the state machine. Handle methods marshal work into it with
//! g_idle_add; events go back through the channel `spawn` returns. GStreamer is initialised once
//! per process, and the default main context hosts one loop, so a second player is refused with
//! [`PlayerInitError::AlreadyStarted`].

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use thiserror::Error;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use tracing::error;

use pulsar_model::Playback;

use crate::backend;
use crate::core::{Channel, Exec, Machine, PlayId, PlayerEvent, Shared, with_machine};

/// Why the player could not start. A failed start gives a degraded handle (`spawn`), or the
/// error itself (`try_spawn`).
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
    /// None: degraded (init failed), every play() is Done(false) at once.
    exec: Option<Box<dyn Exec>>,
}

/// Player commands; the GLib thread stops when the last clone is dropped. The process keeps the
/// "started once" slot: a new player is refused for the lifetime of the process.
#[derive(Clone)]
pub struct PlayerHandle {
    inner: Rc<Inner>,
}

/// Starts the player on the playback's ALSA PCM (`Playback::pcm`) with its output gates. A
/// failed dlopen / gst_init is logged ("player init failed") and gives a degraded handle: every
/// play() ends with Done(false) at once and state() stays Idle.
pub fn spawn(playback: &'static Playback) -> (PlayerHandle, UnboundedReceiver<PlayerEvent>) {
    match start(playback) {
        Ok(v) => v,
        Err(e) => {
            // Every detail line (dlopen, gst_init, playbin, output thread, GLib loop, thread) is
            // already logged by backend/gst; these two name themselves.
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

/// As [`spawn`], but a failed start is returned instead of producing a degraded handle.
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

    /// Plays urls sequentially (http(s)://, file://, or an absolute path), replacing the
    /// channel's current playback. MUSIC retries an http(s) url that fails for network reasons
    /// (3 times, 1/2/4 s apart, resuming at the position reached) before moving on; local files
    /// and ANNOUNCE fail at once.
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

    /// MUSIC only; ANNOUNCE ignores pause and resume.
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

    /// Changed by the GLib thread only: right after a command it still reports the state from
    /// before it.
    pub fn state(&self, ch: Channel) -> crate::PlayState {
        self.inner.shared.state(ch)
    }

    /// Per-channel gain 0..1 (ducking MUSIC while listening / speaking), ramped while playing.
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
        // No firmware GStreamer on the host: spawn() gives the degraded handle.
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
