use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use home_actor::Handle;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tracing::{debug, info, warn};

use crate::codec::{Codec, OpusCodec, Upsampler};
use crate::jitter::{JitterBuffer, Playout};
use crate::machine::{Action, CallPhase, Machine, SignalEvent};
use crate::rtc::{Peer, PeerOutput};
use crate::signaling::{SignalError, Signaling, WatchMessage};
use crate::{CallConfig, CallEvent, FRAME_MS, RING_MS};

#[derive(Clone, Copy, Debug, Default)]
pub struct SinkStats {
    pub queued_ms: u32,
    pub written_ms: u64,
    pub partial: u64,
    pub overflow: u64,
    pub xrun: u64,
}

pub trait Downlink: Send + Sync {
    fn write(&self, pcm: &[i16]);
    fn flush(&self);
    fn stats(&self) -> SinkStats {
        SinkStats::default()
    }
    fn queued_ms(&self) -> u32 {
        self.stats().queued_ms
    }
}

enum Cmd {
    Mute(bool),
    Hangup,
    Uplink(Vec<i16>),
}

#[derive(Clone)]
pub struct CallHandle {
    cmds: Handle<Cmd>,
    phase: Rc<Cell<CallPhase>>,
    muted: Rc<Cell<bool>>,
    enabled: bool,
}

impl CallHandle {
    #[must_use]
    pub fn available(&self) -> bool {
        self.enabled
    }

    #[must_use]
    pub fn phase(&self) -> CallPhase {
        self.phase.get()
    }

    #[must_use]
    pub fn muted(&self) -> bool {
        self.muted.get()
    }

    pub fn set_muted(&self, muted: bool) {
        self.muted.set(muted);
        let _ = self.cmds.send(Cmd::Mute(muted));
    }

    pub fn hangup(&self) {
        let _ = self.cmds.send(Cmd::Hangup);
    }

    pub fn push_uplink(&self, pcm: &[i16]) {
        if !self.muted.get() {
            let _ = self.cmds.send(Cmd::Uplink(pcm.to_vec()));
        }
    }
}

pub fn spawn(
    config: Option<CallConfig>,
    downlink: Option<Arc<dyn Downlink>>,
) -> (CallHandle, UnboundedReceiver<CallEvent>) {
    let (commands, commands_rx) = home_actor::channel();
    let (events, events_rx) = unbounded_channel();
    let phase = Rc::new(Cell::new(CallPhase::Idle));
    let muted = Rc::new(Cell::new(false));
    let (Some(config), Some(downlink)) = (config, downlink) else {
        return (
            CallHandle {
                cmds: commands,
                phase,
                muted,
                enabled: false,
            },
            events_rx,
        );
    };
    let handle = CallHandle {
        cmds: commands,
        phase: Rc::clone(&phase),
        muted: Rc::clone(&muted),
        enabled: true,
    };
    pulsar_core::supervisor().spawn_local(
        "call",
        run(config, downlink, commands_rx, events, Rc::clone(&phase)),
    );
    (handle, events_rx)
}

async fn run(
    config: CallConfig,
    downlink: Arc<dyn Downlink>,
    mut commands: UnboundedReceiver<Cmd>,
    events: UnboundedSender<CallEvent>,
    phase: Rc<Cell<CallPhase>>,
) {
    let signaling = match Signaling::new(&config) {
        Ok(signaling) => signaling,
        Err(error) => {
            warn!("call: disabled: {error}");
            while let Some(command) = commands.recv().await {
                if matches!(command, Cmd::Hangup) {
                    break;
                }
            }
            return;
        }
    };
    let ctx = Ctx {
        signaling: &signaling,
        events: &events,
        phase: &phase,
        downlink: &downlink,
    };
    let mut backoff = Duration::from_secs(1);
    loop {
        match session(&ctx, &mut commands).await {
            Ok(()) => {}
            Err(error) => warn!("call: signaling: {error}"),
        }
        phase.set(CallPhase::Idle);
        downlink.flush();
        let _ = events.send(CallEvent::Ended);
        tokio::select! {
            () = tokio::time::sleep(backoff) => {}
            command = commands.recv() => {
                if command.is_none() {
                    return;
                }
            }
        }
        backoff = (backoff * 2).min(Duration::from_secs(30));
    }
}

struct Ctx<'a> {
    signaling: &'a Signaling,
    events: &'a UnboundedSender<CallEvent>,
    phase: &'a Rc<Cell<CallPhase>>,
    downlink: &'a Arc<dyn Downlink>,
}

impl Ctx<'_> {
    async fn apply(&self, actions: Vec<Action>, machine: &mut Machine, media: &mut Option<Media>) {
        for action in actions {
            match action {
                Action::Answer { call_id } => {
                    if let Err(error) = self.signaling.answer(&call_id).await {
                        warn!("call: answer failed: {error}");
                        let _ = machine.media_failed();
                        self.teardown(media);
                        self.phase.set(CallPhase::Idle);
                        let _ = self.events.send(CallEvent::Ended);
                    }
                }
                Action::Hangup { call_id, reason } => {
                    if let Err(error) = self.signaling.hangup(&call_id, reason).await {
                        debug!("call: hangup failed: {error}");
                    }
                    self.teardown(media);
                }
                Action::Emit(event) => {
                    self.phase.set(machine.phase());
                    if event == CallEvent::Ended {
                        self.teardown(media);
                    }
                    if event == CallEvent::Active {
                        info!("call: active");
                    }
                    let _ = self.events.send(event);
                }
            }
        }
    }

    fn teardown(&self, media: &mut Option<Media>) {
        if let Some(mut m) = media.take() {
            m.peer.close();
        }
        self.downlink.flush();
    }
}

type OpusPacket = Vec<u8>;

struct Timeline {
    anchor: Option<Anchor>,
}

#[derive(Clone, Copy)]
struct Anchor {
    time: u64,
    seq: u64,
    samples: u64,
}

impl Timeline {
    fn new() -> Self {
        Self { anchor: None }
    }

    fn seq(&mut self, time: u64, samples: usize) -> u64 {
        let samples = samples.max(1) as u64;
        let Some(anchor) = self.anchor else {
            self.anchor = Some(Anchor {
                time,
                seq: 0,
                samples,
            });
            return 0;
        };
        let step = anchor.samples.max(1);
        let (frames, forward) = if time >= anchor.time {
            ((time - anchor.time + step / 2) / step, true)
        } else {
            ((anchor.time - time + step / 2) / step, false)
        };
        if frames == 0 {
            return anchor.seq;
        }
        if forward {
            let seq = anchor.seq + frames;
            self.anchor = Some(Anchor { time, seq, samples });
            seq
        } else {
            anchor.seq.saturating_sub(frames)
        }
    }
}

const DOWNLINK_TARGET_MS: u32 = 60;
const MAX_FRAMES_PER_PUMP: usize = 5;
const REPORT_EVERY: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Default)]
struct PlayoutCounters {
    decode_errors: u64,
    plc: u64,
    fec: u64,
}

struct DownlinkPlayback<'a, C: Codec> {
    codec: &'a mut C,
    counters: &'a mut PlayoutCounters,
}

impl<C: Codec> Playout<OpusPacket> for DownlinkPlayback<'_, C> {
    type Output = Vec<i16>;

    fn frame(&mut self, packet: &OpusPacket) -> Vec<i16> {
        if let Some(pcm) = self.codec.decode(packet) {
            pcm
        } else {
            self.counters.decode_errors += 1;
            pulsar_metrics::call_decode_error();
            self.codec.conceal()
        }
    }

    fn lost(&mut self, next: Option<&OpusPacket>) -> Vec<i16> {
        if let Some(next) = next
            && let Some(pcm) = self.codec.decode_fec(next)
        {
            self.counters.fec += 1;
            pulsar_metrics::call_fec();
            return pcm;
        }
        self.counters.plc += 1;
        pulsar_metrics::call_plc();
        self.codec.conceal()
    }
}

fn playout<C: Codec>(
    jitter: &mut JitterBuffer<OpusPacket>,
    codec: &mut C,
    downlink: &dyn Downlink,
    counters: &mut PlayoutCounters,
) {
    let mut playback = DownlinkPlayback { codec, counters };
    for _ in 0..MAX_FRAMES_PER_PUMP {
        if downlink.queued_ms() >= DOWNLINK_TARGET_MS {
            break;
        }
        let now = pulsar_core::now_ms();
        let Some(pcm) = jitter.pop(now, &mut playback) else {
            break;
        };
        downlink.write(&pcm);
    }
}

struct Media {
    peer: Peer,
    codec: OpusCodec,
    jitter: JitterBuffer<OpusPacket>,
    upsampler: Upsampler,
    timeline: Timeline,
    rx_rtp: u64,
    seq_gap: u64,
    counters: PlayoutCounters,
    pushed: u64,
    late: u64,
    dup: u64,
    max_seq: Option<u64>,
    last_report: Instant,
}

impl Media {
    fn drive(&mut self) -> bool {
        self.peer.handle_timeout(Instant::now());
        let Ok((outputs, _deadline)) = self.peer.drain() else {
            return false;
        };
        for output in outputs {
            match output {
                PeerOutput::Connected => debug!("call: media connected"),
                PeerOutput::Disconnected => {
                    debug!("call: media disconnected");
                    return false;
                }
                PeerOutput::Audio { data, time } => {
                    self.rx_rtp += 1;
                    pulsar_metrics::call_rx_packet();
                    let Some(samples) = self.codec.packet_samples(&data) else {
                        self.counters.decode_errors += 1;
                        pulsar_metrics::call_decode_error();
                        continue;
                    };
                    let seq = self.timeline.seq(time, samples);
                    if let Some(max) = self.max_seq
                        && seq > max
                    {
                        self.seq_gap += seq - max - 1;
                        pulsar_metrics::call_packet_lost(seq - max - 1);
                    }
                    self.max_seq = Some(self.max_seq.map_or(seq, |max| max.max(seq)));
                    let now = pulsar_core::now_ms();
                    match self.jitter.push(seq, data, now) {
                        crate::jitter::Push::Queued => self.pushed += 1,
                        crate::jitter::Push::Late => self.late += 1,
                        crate::jitter::Push::Duplicate => self.dup += 1,
                    }
                }
            }
        }
        true
    }

    fn pump(&mut self, downlink: &Arc<dyn Downlink>, active: bool) {
        if active {
            playout(
                &mut self.jitter,
                &mut self.codec,
                downlink.as_ref(),
                &mut self.counters,
            );
        }
        self.report(downlink);
    }

    fn report(&mut self, downlink: &Arc<dyn Downlink>) {
        if self.last_report.elapsed() < REPORT_EVERY {
            return;
        }
        self.last_report = Instant::now();
        let sink = downlink.stats();
        info!(
            "call: rx_rtp={} seq_gap={} jitter_ms={} pushed={} late={} dup={} plc={} fec={} \
             decode_errors={} depth={} target={} queue_ms={} written_ms={} partial={} overflow={} \
             xrun={}",
            self.rx_rtp,
            self.seq_gap,
            self.jitter.depth_ms(),
            self.pushed,
            self.late,
            self.dup,
            self.counters.plc,
            self.counters.fec,
            self.counters.decode_errors,
            self.jitter.depth(),
            self.jitter.target_ms(),
            sink.queued_ms,
            sink.written_ms,
            sink.partial,
            sink.overflow,
            sink.xrun,
        );
    }

    fn uplink(&mut self, pcm: &[i16]) {
        if !self.peer.is_connected() {
            return;
        }
        self.upsampler.push(pcm);
        while let Some(frame) = self.upsampler.take_frame() {
            if let Some(packet) = self.codec.encode(&frame) {
                let _ = self.peer.write_audio(&packet, Instant::now());
            }
        }
        self.peer.handle_timeout(Instant::now());
        let _ = self.peer.drain();
    }
}

async fn session(ctx: &Ctx<'_>, commands: &mut UnboundedReceiver<Cmd>) -> Result<(), SignalError> {
    let _ = ctx.signaling.register("Колонка").await;
    let mut watch = ctx.signaling.watch().await?;
    debug!("call: signaling stream open");
    if let Ok(ice) = ctx.signaling.ice_config().await {
        debug!("call: {} ICE servers configured", ice.ice_servers.len());
    }
    let mut machine = Machine::new();
    let mut media: Option<Media> = None;
    let mut ring_deadline: Option<Instant> = None;
    let mut ticker = tokio::time::interval(Duration::from_millis(u64::from(FRAME_MS)));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        let ring = async {
            match ring_deadline {
                Some(deadline) => {
                    tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
                }
                None => std::future::pending().await,
            }
        };
        let receiver = media.as_ref().map(|m| m.peer.receiver());
        let incoming = async {
            match &receiver {
                Some(receiver) => receiver.recv().await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            biased;
            event = watch.next() => {
                let Some(message) = event? else {
                    return Err(SignalError::Closed);
                };
                match message {
                    WatchMessage::Call(signal) => {
                        if let SignalEvent::Ringing { .. } = &signal {
                            ring_deadline = Some(Instant::now() + Duration::from_millis(RING_MS));
                        }
                        let actions = machine.on_signal(signal);
                        ctx.apply(actions, &mut machine, &mut media).await;
                        if machine.phase() == CallPhase::Idle {
                            ring_deadline = None;
                        }
                    }
                    WatchMessage::Offer { call_id, sdp } => {
                        match open_media(sdp, call_id).await {
                            Ok((peer, answer, call_id)) => {
                                ctx.signaling.send_sdp(&call_id, answer).await?;
                                let _ = ctx.signaling.send_ice_end(&call_id).await;
                                if let Some(m) = new_media(peer, call_id) {
                                    media = Some(m);
                                } else {
                                    warn!("call: opus codec unavailable");
                                    let actions = machine.media_failed();
                                    ctx.apply(actions, &mut machine, &mut media).await;
                                }
                            }
                            Err(error) => {
                                debug!("call: media negotiation failed: {error}");
                                let actions = machine.media_failed();
                                ctx.apply(actions, &mut machine, &mut media).await;
                            }
                        }
                    }
                    WatchMessage::Ice { candidate, .. } => {
                        if let Some(m) = &mut media
                            && m.peer.add_remote_candidate(&candidate).is_err()
                        {
                            debug!("call: rejected remote ICE candidate");
                        }
                    }
                    WatchMessage::IceEnd { .. } | WatchMessage::Other => {}
                }
            }
            command = commands.recv() => {
                let Some(command) = command else { return Ok(()) };
                match command {
                    Cmd::Mute(m) => machine.set_muted(m),
                    Cmd::Uplink(pcm) => {
                        if machine.phase() == CallPhase::Active
                            && !machine.muted()
                            && let Some(m) = &mut media
                        {
                            m.uplink(&pcm);
                        }
                    }
                    Cmd::Hangup => {
                        let actions = machine.local_hangup();
                        ctx.apply(actions, &mut machine, &mut media).await;
                        ring_deadline = None;
                    }
                }
            }
            () = ring, if ring_deadline.is_some() => {
                ring_deadline = None;
                let actions = machine.ring_elapsed();
                ctx.apply(actions, &mut machine, &mut media).await;
            }
            packet = incoming, if media.is_some() => {
                if let (Ok((packet, source, destination)), Some(m)) = (packet, media.as_mut()) {
                    m.peer.handle_input(&packet, source, destination);
                    if !m.drive() {
                        ctx.downlink.flush();
                        let actions = machine.media_failed();
                        ctx.apply(actions, &mut machine, &mut media).await;
                    }
                }
            }
            _ = ticker.tick() => {
                let active = machine.phase() == CallPhase::Active;
                let lost = media.as_mut().is_some_and(|m| {
                    if !m.drive() {
                        return true;
                    }
                    m.pump(ctx.downlink, active);
                    false
                });
                if lost {
                    ctx.downlink.flush();
                    let actions = machine.media_failed();
                    ctx.apply(actions, &mut machine, &mut media).await;
                }
            }
        }
    }
}

async fn open_media(
    sdp: String,
    call_id: String,
) -> Result<(Peer, String, String), crate::rtc::PeerError> {
    let mut peer = Peer::bind().await?;
    let answer = peer.accept_offer(&sdp)?;
    Ok((peer, answer, call_id))
}

fn new_media(peer: Peer, _call_id: String) -> Option<Media> {
    debug!(
        "call: media opened (local port {}, jitter target {} ms)",
        peer.local_addr().port(),
        JitterBuffer::<OpusPacket>::new(FRAME_MS).target_ms()
    );
    Some(Media {
        peer,
        codec: OpusCodec::new()?,
        jitter: JitterBuffer::new(FRAME_MS),
        upsampler: Upsampler::new(),
        timeline: Timeline::new(),
        rx_rtp: 0,
        seq_gap: 0,
        counters: PlayoutCounters::default(),
        pushed: 0,
        late: 0,
        dup: 0,
        max_seq: None,
        last_report: Instant::now(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CALL_RATE, FRAME_SAMPLES};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[derive(Default)]
    struct FakeDownlink {
        written: Mutex<Vec<i16>>,
        flushed: Mutex<u32>,
        queued_ms: AtomicU32,
    }

    impl FakeDownlink {
        fn drain(&self, ms: u32) {
            if self.queued_ms.load(Ordering::Relaxed) > ms {
                self.queued_ms.fetch_sub(ms, Ordering::Relaxed);
            } else {
                self.queued_ms.store(0, Ordering::Relaxed);
            }
        }

        fn frames(&self) -> usize {
            self.written.lock().unwrap().len() / FRAME_SAMPLES
        }
    }

    impl Downlink for FakeDownlink {
        fn write(&self, pcm: &[i16]) {
            let ms = u32::try_from(pcm.len() * 1000 / CALL_RATE as usize).unwrap_or(u32::MAX);
            self.written.lock().unwrap().extend_from_slice(pcm);
            self.queued_ms.fetch_add(ms, Ordering::Relaxed);
        }

        fn flush(&self) {
            *self.flushed.lock().unwrap() += 1;
            self.queued_ms.store(0, Ordering::Relaxed);
        }

        fn queued_ms(&self) -> u32 {
            self.queued_ms.load(Ordering::Relaxed)
        }
    }

    struct FakeCodec;

    impl Codec for FakeCodec {
        fn encode(&mut self, _pcm: &[i16]) -> Option<Vec<u8>> {
            None
        }

        fn decode(&mut self, packet: &[u8]) -> Option<Vec<i16>> {
            Some(vec![i16::from(*packet.first()?); FRAME_SAMPLES])
        }

        fn decode_fec(&mut self, _packet: &[u8]) -> Option<Vec<i16>> {
            None
        }

        fn conceal(&mut self) -> Vec<i16> {
            vec![-1; FRAME_SAMPLES]
        }

        fn packet_samples(&self, _packet: &[u8]) -> Option<usize> {
            Some(FRAME_SAMPLES)
        }
    }

    fn packet(seq: u8) -> OpusPacket {
        vec![seq]
    }

    #[test]
    fn a_late_tick_refills_the_output_queue_in_one_pass() {
        let sink = FakeDownlink::default();
        let mut codec = FakeCodec;
        let mut counters = PlayoutCounters::default();
        let mut jitter = JitterBuffer::new(FRAME_MS);
        for seq in 0..6u64 {
            jitter.push(seq, packet(seq as u8), 0);
        }
        playout(&mut jitter, &mut codec, &sink, &mut counters);
        assert_eq!(sink.frames(), 3);
        assert_eq!(sink.queued_ms(), DOWNLINK_TARGET_MS);

        sink.drain(20);
        playout(&mut jitter, &mut codec, &sink, &mut counters);
        assert_eq!(sink.frames(), 4);
        assert_eq!(sink.queued_ms(), DOWNLINK_TARGET_MS);

        playout(&mut jitter, &mut codec, &sink, &mut counters);
        assert_eq!(sink.frames(), 4);

        let written = sink.written.lock().unwrap();
        for seq in 0..4 {
            assert_eq!(written[seq * FRAME_SAMPLES], seq as i16);
        }
    }

    #[test]
    fn steady_state_writes_frames_below_the_jitter_target() {
        let sink = FakeDownlink::default();
        let mut codec = FakeCodec;
        let mut counters = PlayoutCounters::default();
        let mut jitter = JitterBuffer::new(FRAME_MS);
        jitter.push(0, packet(0), 0);
        jitter.push(1, packet(1), 0);
        playout(&mut jitter, &mut codec, &sink, &mut counters);
        assert_eq!(sink.frames(), 2, "both frames must play out");
        playout(&mut jitter, &mut codec, &sink, &mut counters);
        assert_eq!(sink.frames(), 2);
        assert_eq!(counters.plc, 0);
    }

    #[test]
    fn a_gap_produces_one_concealed_frame() {
        let sink = FakeDownlink::default();
        let mut codec = FakeCodec;
        let mut counters = PlayoutCounters::default();
        let mut jitter = JitterBuffer::new(FRAME_MS);
        for seq in [0u64, 2, 3, 4] {
            jitter.push(seq, packet(seq as u8), 0);
        }
        playout(&mut jitter, &mut codec, &sink, &mut counters);
        assert_eq!(sink.frames(), 3);
        assert_eq!(counters.plc, 1);
        assert_eq!(counters.fec, 0);
    }

    #[test]
    fn the_timeline_uses_the_actual_frame_duration() {
        let mut timeline = Timeline::new();
        assert_eq!(timeline.seq(0, 960), 0);
        assert_eq!(timeline.seq(960, 960), 1);
        assert_eq!(timeline.seq(1920, 960), 2);
        assert_eq!(timeline.seq(2880, 2880), 3);
        assert_eq!(timeline.seq(5760, 2880), 4);
        assert_eq!(timeline.seq(8640, 480), 5);
        assert_eq!(timeline.seq(9120, 480), 6);
    }

    #[test]
    fn the_timeline_counts_lost_and_reordered_frames() {
        let mut timeline = Timeline::new();
        assert_eq!(timeline.seq(0, 960), 0);
        assert_eq!(timeline.seq(2880, 960), 3);
        assert_eq!(timeline.seq(960, 960), 1);
        assert_eq!(timeline.seq(2880, 960), 3);
    }

    #[test]
    fn a_disabled_handle_never_reports_a_call() {
        let (handle, _events) = spawn(None, None);
        assert!(!handle.available());
        assert_eq!(handle.phase(), CallPhase::Idle);
        handle.set_muted(true);
        handle.hangup();
        handle.push_uplink(&[0; 32]);
        assert_eq!(handle.phase(), CallPhase::Idle);
    }

    #[test]
    fn the_handle_tracks_mute_and_uplink_immediately() {
        let (handle, _events) = spawn(None, None);
        assert!(!handle.muted());
        handle.set_muted(true);
        assert!(handle.muted());
        handle.set_muted(false);
        assert!(!handle.muted());
    }

    #[test]
    fn the_fake_downlink_records_pcm_and_flushes() {
        let sink = Arc::new(FakeDownlink::default());
        sink.write(&[1, 2, 3]);
        sink.flush();
        assert_eq!(*sink.written.lock().unwrap(), vec![1, 2, 3]);
        assert_eq!(*sink.flushed.lock().unwrap(), 1);
    }
}
