//! Main-loop event handling and the service actor. The callbacks of sendspin-cpp run on its main
//! loop thread; [`EventHandler`] is their single handler: it applies the sink request, logs the
//! event, publishes the protocol state and records the stream edges whose 1 s hold the actor
//! applies. The actor itself only runs the library loop, the timers and the handle commands, so
//! it is testable without sendspin-cpp (the tests use [`FakeLink`] and a scripted edge queue).

use std::rc::Rc;

use pulsar_core::Timers;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::{debug, error, info, warn};

use crate::SendspinEvent;
use crate::event::{LibEvent, StreamParams, sink_request};
use crate::proto::Command;
use crate::state::{Key, Status, StreamHold};
use crate::visualizer::Visualizer;
use crate::{Transport, volume_percent};

/// sendspin-cpp's client surface the actor uses. Implemented by `ffi::Client<Ctx>` on the
/// stations, by a fake in the tests.
pub trait Link {
    fn run_loop(&self) -> bool;
    fn is_connected(&self) -> bool;
    fn server_info(&self) -> Option<(String, String)>;
    fn volume(&self) -> u8;
    fn muted(&self) -> bool;
    fn update_volume(&self, volume: u8);
    fn update_muted(&self, muted: bool);
    fn confirm_pairing_window(&self);
}

/// Sink operations the stream callbacks must perform before the library's sync task writes
/// (see [`sink_request`]). `sink::Sink` implements it on the stations.
pub trait SinkPort: Send + Sync {
    fn configure(&self, rate: u32, channels: u8, bits: u8);
    fn release(&self);
}

/// One edge of the stream state the actor applies to [`StreamHold`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamEdge {
    Start,
    End,
    Group(bool),
}

/// One item of the single event queue the callbacks fill and the actor drains in order. Pairing,
/// volume and mute events and stream edges keep the order the library delivered them in.
#[derive(Clone, Debug, PartialEq)]
pub enum Outgoing {
    Event(SendspinEvent),
    Stream(StreamEdge),
}

/// The handler's ordered queue and the channel the published `SendspinEvent`s go to.
pub trait StreamEvents: Send + Sync {
    fn take(&self) -> Vec<Outgoing>;
    fn send(&self, ev: SendspinEvent);
}

/// One place for every library event: sink request, log, published protocol state and the stream
/// edges. Called from the callback threads; none of its methods calls back into the library.
pub struct EventHandler {
    pub status: std::sync::Arc<Status>,
    sink: std::sync::Arc<dyn SinkPort>,
    tx: UnboundedSender<SendspinEvent>,
    queue: std::sync::Mutex<Vec<Outgoing>>,
    visualizer: Visualizer,
}

impl EventHandler {
    pub fn new(
        status: std::sync::Arc<Status>,
        sink: std::sync::Arc<dyn SinkPort>,
        tx: UnboundedSender<SendspinEvent>,
    ) -> Self {
        Self {
            status,
            sink,
            tx,
            queue: std::sync::Mutex::new(Vec::new()),
            visualizer: Visualizer::default(),
        }
    }

    fn send_event(&self, ev: SendspinEvent) {
        if self.status.enabled() {
            self.push(Outgoing::Event(ev));
        }
    }

    fn edge(&self, e: StreamEdge) {
        // Edges are kept even while the owner tears the service down: the actor still finishes
        // the held stream end (log, streaming=false).
        self.push(Outgoing::Stream(e));
    }

    fn push(&self, item: Outgoing) {
        self.queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(item);
    }

    /// A main loop event from the library.
    pub fn event(&self, ev: LibEvent) {
        match sink_request(&ev) {
            Some(crate::event::SinkRequest::Configure {
                rate,
                channels,
                bits,
            }) => self.sink.configure(rate, channels, bits),
            Some(crate::event::SinkRequest::Release) => self.sink.release(),
            None => {}
        }
        match ev {
            LibEvent::Group { playing, name } => {
                info!(
                    "sendspin: group {}{}{}",
                    name.as_deref().unwrap_or("-"),
                    if playing.is_some() { " state=" } else { "" },
                    match playing {
                        Some(true) => "playing",
                        Some(false) => "stopped",
                        None => "",
                    }
                );
                if let Some(playing) = playing {
                    self.edge(StreamEdge::Group(playing));
                }
            }
            LibEvent::TimeSync(error) => debug!("sendspin: time sync error {error:.0} us"),
            LibEvent::PairingStarted(id) => info!("sendspin: pairing started by server {id}"),
            LibEvent::PairingSucceeded(id) => info!("sendspin: paired with server {id}"),
            LibEvent::PairingFailed { server_id, reason } => warn!(
                "sendspin: pairing with {server_id} failed: {}",
                abort_reason(reason)
            ),
            LibEvent::Trust { paired } => info!(
                "sendspin: connection trust {}",
                if paired { "paired" } else { "unpaired" }
            ),
            LibEvent::PairingCode(code) => {
                info!(
                    "sendspin: PAIRING CODE {}-{} (enter it in Music Assistant)",
                    code.get(..3).unwrap_or(&code),
                    code.get(3..).unwrap_or("")
                );
                self.send_event(SendspinEvent::PairingCode(code));
            }
            LibEvent::PairingCodeCleared => {
                info!("sendspin: pairing code withdrawn");
                self.send_event(SendspinEvent::PairingCode(String::new()));
            }
            LibEvent::PairingWindow(open) => {
                if open {
                    info!(
                        "sendspin: pairing needs confirmation on the device: press the action button"
                    );
                }
                self.send_event(SendspinEvent::PairingWindow(open));
            }
            LibEvent::StreamStart(None) => {
                warn!("sendspin: stream start without complete parameters");
            }
            LibEvent::StreamStart(Some(p)) => {
                log_stream_start(p);
                self.edge(StreamEdge::Start);
            }
            LibEvent::StreamEnd => self.edge(StreamEdge::End),
            LibEvent::Volume(v) => {
                info!("sendspin: server volume {v}");
                self.send_event(SendspinEvent::Volume(f32::from(v) / 100.0));
            }
            LibEvent::Mute(m) => {
                info!("sendspin: server mute {}", u8::from(m));
                self.send_event(SendspinEvent::Mute(m));
            }
            LibEvent::OutputDelay(ms) => info!("sendspin: output delay {ms} ms"),
            LibEvent::ControllerState {
                commands,
                volume,
                muted,
            } => debug!(
                "sendspin: controller state: {commands} commands, group volume {volume}, muted {}",
                u8::from(muted)
            ),
            LibEvent::Metadata { artist, title } => {
                if artist.is_some() || title.is_some() {
                    info!(
                        "sendspin: now playing: {} - {}",
                        artist.as_deref().unwrap_or("?"),
                        title.as_deref().unwrap_or("?")
                    );
                }
            }
            LibEvent::VisualizerStart { types, rate_max } => {
                info!("sendspin: visualizer stream start, {types} types, {rate_max} fps");
            }
            LibEvent::VisualizerEnd => info!("sendspin: visualizer stream end"),
        }
    }

    pub fn loudness(&self, v: u16) {
        self.visualizer.loudness(v);
    }

    pub fn beat(&self) {
        self.visualizer.beat();
    }

    /// Visualizer drain thread: one `Music` event at the LED rate when the service is enabled.
    pub fn spectrum(&self, bins: &[u16]) {
        let Some(frame) = self.visualizer.frame(bins) else {
            return;
        };
        if self.status.enabled() {
            let _ = self.tx.send(SendspinEvent::Music(frame));
        }
    }
}

impl<T: StreamEvents + ?Sized> StreamEvents for std::sync::Arc<T> {
    fn take(&self) -> Vec<Outgoing> {
        (**self).take()
    }

    fn send(&self, ev: SendspinEvent) {
        (**self).send(ev);
    }
}

impl StreamEvents for EventHandler {
    fn take(&self) -> Vec<Outgoing> {
        std::mem::take(
            &mut *self
                .queue
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }

    fn send(&self, ev: SendspinEvent) {
        if self.status.enabled() {
            let _ = self.tx.send(ev);
        }
    }
}

fn log_stream_start(p: StreamParams) {
    info!(
        "sendspin: stream start {} {} Hz/{} ch/{} bit",
        codec_name(p.codec),
        p.rate,
        p.channels,
        p.bits
    );
}

/// The first `n` characters of an id, "…" if cut.
fn hex_tail(s: &str, n: usize) -> String {
    match s.char_indices().nth(n) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_owned(),
    }
}

fn codec_name(codec: u8) -> &'static str {
    ["flac", "opus", "pcm", "?"][usize::from(codec.min(3))]
}

/// sendspin::SendspinPairAbortReason.
fn abort_reason(r: u8) -> &'static str {
    match r {
        0 => "attempt_timeout",
        1 => "concurrent_attempt",
        2 => "method_not_supported",
        3 => "pairing_code_mismatch",
        4 => "user_cancelled",
        _ => "unknown",
    }
}

/// Handle commands handled by the actor.
#[derive(Clone, Copy)]
pub enum Cmd {
    ReportVolume { volume: f32, muted: bool },
    ConfirmPairingWindow,
}

/// The service actor (runtime thread).
pub struct Actor<L: Link, E: StreamEvents> {
    link: Rc<L>,
    edges: E,
    status: std::sync::Arc<Status>,
    hold: StreamHold,
    timers: Timers<Key>,
    connected: bool,
    loop_failed_logged: bool,
}

impl<L: Link, E: StreamEvents> Actor<L, E> {
    pub fn new(link: Rc<L>, edges: E, status: std::sync::Arc<Status>) -> Self {
        Self {
            link,
            edges,
            hold: StreamHold::new(std::sync::Arc::clone(&status)),
            status,
            timers: Timers::new(),
            connected: false,
            loop_failed_logged: false,
        }
    }

    pub async fn run(mut self, mut cmds: UnboundedReceiver<Cmd>) {
        self.timers.set_repeating(Key::Tick, TICK_MS, TICK_MS);
        loop {
            tokio::select! {
                biased;
                key = self.timers.next() => match key {
                    Key::Tick => self.poll(),
                    Key::StreamEnd => {
                        let changed = self.hold.finish(&mut self.timers);
                        self.stream(changed);
                    }
                },
                cmd = cmds.recv() => match cmd {
                    Some(cmd) => self.command(cmd),
                    None => break,
                },
            }
        }
        self.teardown();
    }

    fn stream(&self, changed: Option<bool>) {
        if let Some(active) = changed {
            self.edges.send(SendspinEvent::Stream(active));
        }
    }

    fn poll(&mut self) {
        if !self.link.run_loop() && !self.loop_failed_logged {
            self.loop_failed_logged = true;
            error!("sendspin: client loop threw an exception");
        }
        for item in self.edges.take() {
            match item {
                Outgoing::Event(ev) => self.edges.send(ev),
                Outgoing::Stream(edge) => {
                    let changed = match edge {
                        StreamEdge::Start => self.hold.start(&mut self.timers),
                        StreamEdge::End => self.hold.end(&mut self.timers, self.status.enabled()),
                        StreamEdge::Group(playing) => self.hold.group(playing, &mut self.timers),
                    };
                    self.stream(changed);
                }
            }
        }
        let c = self.link.is_connected();
        if c == self.connected {
            return;
        }
        self.connected = c;
        if c {
            let info = self.link.server_info();
            info!(
                "sendspin: server connected: {} ({})",
                info.as_ref().map_or("?", |(name, _)| name.as_str()),
                info.as_ref().map_or("?".into(), |(_, id)| hex_tail(id, 12))
            );
        } else {
            info!("sendspin: server disconnected");
            let changed = self.hold.finish(&mut self.timers);
            self.stream(changed);
        }
    }

    fn command(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::ReportVolume { volume, muted } => {
                let v = volume_percent(volume);
                if v != self.link.volume() {
                    self.link.update_volume(v);
                }
                if muted != self.link.muted() {
                    self.link.update_muted(muted);
                }
            }
            Cmd::ConfirmPairingWindow => {
                info!("sendspin: pairing window confirmed by the operator");
                self.link.confirm_pairing_window();
            }
        }
    }

    /// The owner is gone: goodbye to the server and the library's threads joined (inside the
    /// client's drop).
    fn teardown(self) {
        let Self {
            link,
            edges,
            status,
            mut hold,
            mut timers,
            ..
        } = self;
        status.disable();
        drop(link);
        // The client's stop() delivers its last stream edges inside the drop: finish the held
        // end (log, streaming=false) as before, without publishing events.
        for item in edges.take() {
            if let Outgoing::Stream(edge) = item {
                match edge {
                    StreamEdge::Start => {
                        hold.start(&mut timers);
                    }
                    StreamEdge::End => {
                        hold.end(&mut timers, false);
                    }
                    StreamEdge::Group(playing) => {
                        hold.group(playing, &mut timers);
                    }
                }
            }
        }
    }
}

pub const TICK_MS: u64 = 10;

/// The handle's local surface for `transport` (kept here to avoid a busy import in the service).
pub fn transport_command(t: Transport) -> (Command, &'static str) {
    match t {
        Transport::Play => (Command::Play, "play"),
        Transport::Pause => (Command::Pause, "pause"),
        Transport::Stop => (Command::Stop, "stop"),
        Transport::Next => (Command::Next, "next"),
        Transport::Previous => (Command::Previous, "previous"),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::event::StreamParams;

    #[derive(Default)]
    struct FakeSink {
        configured: AtomicBool,
        released: AtomicBool,
    }

    impl SinkPort for FakeSink {
        fn configure(&self, _: u32, _: u8, _: u8) {
            self.configured.store(true, Ordering::SeqCst);
        }

        fn release(&self) {
            self.released.store(true, Ordering::SeqCst);
        }
    }

    fn handler() -> (
        Arc<EventHandler>,
        UnboundedReceiver<SendspinEvent>,
        Arc<FakeSink>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let sink = Arc::new(FakeSink::default());
        let h = Arc::new(EventHandler::new(
            Status::new(),
            Arc::clone(&sink) as Arc<dyn SinkPort>,
            tx,
        ));
        (h, rx, sink)
    }

    // The library's sync task writes its priming silence right after on_stream_start returns,
    // before the actor runs: the sink must take it by then.
    #[test]
    fn stream_callbacks_configure_the_sink_before_the_actor_runs() {
        let (h, _rx, sink) = handler();
        assert!(!sink.configured.load(Ordering::SeqCst));
        h.event(LibEvent::StreamStart(Some(StreamParams {
            codec: 0,
            rate: 48000,
            channels: 2,
            bits: 16,
        })));
        assert!(sink.configured.load(Ordering::SeqCst));
        h.event(LibEvent::StreamEnd);
        assert!(sink.released.load(Ordering::SeqCst));
        // Incomplete parameters leave the sink alone, as in C++.
        h.event(LibEvent::StreamStart(None));
        assert_eq!(
            h.take(),
            vec![
                Outgoing::Stream(StreamEdge::Start),
                Outgoing::Stream(StreamEdge::End)
            ]
        );
    }

    #[test]
    fn one_edge_per_stream_event_and_a_teardown_disables_frames() {
        let (h, mut rx, _sink) = handler();
        h.event(LibEvent::Group {
            playing: Some(false),
            name: None,
        });
        h.loudness(65535);
        h.beat();
        h.spectrum(&[0, 65535]);
        assert!(matches!(rx.try_recv(), Ok(SendspinEvent::Music(_))));
        h.status.disable();
        h.spectrum(&[0, 65535]);
        assert!(rx.try_recv().is_err());
        assert_eq!(h.take(), vec![Outgoing::Stream(StreamEdge::Group(false))]);
        assert!(h.take().is_empty());
    }

    // `Mutex` is used by the edge queue; the test imports it to prove the handler is Sync.
    #[test]
    fn event_handler_is_sync() {
        fn assert_sync<T: Sync>() {}
        assert_sync::<EventHandler>();
        let _ = Mutex::new(());
    }

    #[derive(Default)]
    struct FakeLink {
        volume: std::cell::Cell<u8>,
        muted: std::cell::Cell<bool>,
        confirmations: std::cell::Cell<u32>,
        connected: std::cell::Cell<bool>,
    }

    impl Link for FakeLink {
        fn run_loop(&self) -> bool {
            true
        }
        fn is_connected(&self) -> bool {
            self.connected.get()
        }
        fn server_info(&self) -> Option<(String, String)> {
            None
        }
        fn volume(&self) -> u8 {
            self.volume.get()
        }
        fn muted(&self) -> bool {
            self.muted.get()
        }
        fn update_volume(&self, volume: u8) {
            self.volume.set(volume);
        }
        fn update_muted(&self, muted: bool) {
            self.muted.set(muted);
        }
        fn confirm_pairing_window(&self) {
            self.confirmations.set(self.confirmations.get() + 1);
        }
    }

    // The callback queue keeps the library's event order, so the controller sees PairingCode /
    // Volume / Mute and Stream(..) in exactly the order they arrived.
    #[test]
    fn the_actor_delivers_callbacks_in_library_order() {
        let (h, mut rx, _sink) = handler();
        h.event(LibEvent::Volume(20));
        h.event(LibEvent::StreamStart(Some(StreamParams {
            codec: 0,
            rate: 48000,
            channels: 2,
            bits: 16,
        })));
        h.event(LibEvent::PairingCode("123456".into()));
        let link = FakeLink::default();
        let mut actor = Actor::new(Rc::new(link), Arc::clone(&h), Arc::clone(&h.status));
        actor.poll();
        assert_eq!(
            [
                rx.try_recv().unwrap(),
                rx.try_recv().unwrap(),
                rx.try_recv().unwrap()
            ],
            [
                SendspinEvent::Volume(0.2),
                SendspinEvent::Stream(true),
                SendspinEvent::PairingCode("123456".into())
            ]
        );
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn actor_commands_touch_the_link_only_on_a_change() {
        let link = Rc::new(FakeLink::default());
        let mut actor = Actor::new(Rc::clone(&link), Arc::new(EmptyEdges), Status::new());
        actor.command(Cmd::ReportVolume {
            volume: 0.2,
            muted: false,
        });
        assert_eq!(link.volume.get(), 20);
        actor.command(Cmd::ReportVolume {
            volume: 0.2,
            muted: true,
        });
        assert_eq!(link.volume.get(), 20);
        assert!(link.muted.get());
        actor.command(Cmd::ConfirmPairingWindow);
        assert_eq!(link.confirmations.get(), 1);
    }

    struct EmptyEdges;

    impl StreamEvents for EmptyEdges {
        fn take(&self) -> Vec<Outgoing> {
            Vec::new()
        }
        fn send(&self, _: SendspinEvent) {}
    }
}
