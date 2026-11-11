//! Поток обработки: VQE, спотеры, детектор, пре-ролл и события.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use pulsar_core::MusicFrame;
use pulsar_dsp::endpoint::EndpointPrior;
use pulsar_dsp::level::{
    LEVEL_CHUNKS, LEVEL_SAMPLES, LevelHistory, level_db, mean_square, percentile_db,
};
use pulsar_dsp::{Ring, SpectrumAnalyzer, to_s16_scaled, to_s16_slice};
use pulsar_model::{Capture as CaptureLayout, Preroll, PrerollRule};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{error, info, warn};

use crate::engine::{Stage, VQE_OMNI, VQE_OMNI_WITH_GAIN, WakeEngine, db_to_gain};
use crate::wake_events::logic;

use super::capture::{Pipe, deinterleave};
use super::cmd::FrontendCmd;
use super::ports::{Clock, Detection, Detector, WakeStore};
use super::state::{StateEvent, StateMachine};
use super::{FrontendEvent, FrontendStats, lock};

/// Pre-roll ring: 1.024 s at 16 kHz (32 chunks of 32 ms, 64 of 16 ms on the Mini 3 and Mini),
/// more than any wake word's confirmation delay.
const PREROLL_SAMPLES: usize = 16384;
const CHUNK_WAIT: Duration = Duration::from_millis(500);

#[derive(Default)]
struct DetectorChunk {
    hit: Option<f32>,
    mean: Option<f32>,
    hit_id: Option<String>,
    /// Чанк ушёл в детектор (для счётчика прослушанного времени).
    feeding: bool,
}

/// A struct with the channels and shared pieces of [`Processing`]; keeps `new`'s argument count
/// down and separates construction from the ports.
pub(super) struct ProcessingConfig {
    pub(super) capture: &'static CaptureLayout,
    pub(super) preroll: Option<&'static Preroll>,
    pub(super) cmd_rx: Receiver<FrontendCmd>,
    pub(super) tx: UnboundedSender<FrontendEvent>,
    pub(super) stats: Arc<Mutex<FrontendStats>>,
    pub(super) custom: bool,
}

/// The processing thread's state and buffers. Generic over the ports so tests can drive it with
/// scripts and inspect them; production instantiates it with `YandexEngine`, `RicardoDetector`,
/// `RecordingStore` and `SystemClock`.
pub(super) struct Processing<E: WakeEngine, D: Detector, S: WakeStore, C: Clock> {
    engine: E,
    detector: Option<D>,
    store: Option<S>,
    clock: C,
    pub(super) cmd_rx: Receiver<FrontendCmd>,
    pub(super) tx: UnboundedSender<FrontendEvent>,
    pub(super) stats: Arc<Mutex<FrontendStats>>,
    pub(super) preroll: Option<&'static Preroll>,
    state: StateMachine,
    pending_model: Option<PathBuf>,
    custom: bool,
    commands: bool,
    preroll_on: bool,
    music: bool,
    recording: bool,
    wake_missed: bool,
    custom_failed: bool,
    input: Vec<i16>,
    mics: Vec<f32>,
    spk: Vec<f32>,
    level_hist: LevelHistory,
    gain_hist: [f32; LEVEL_CHUNKS],
    gain_pos: usize,
    gain_filled: usize,
    ana: SpectrumAnalyzer,
    music_was_on: bool,
    ring: Ring<i16>,
    /// Звук для HA и пре-ролла: `VQE_OMNI` с «Microphone gain».
    pcm_chunk: Vec<i16>,
    /// Звук детектора Ricardo: прежний `VQE_OMNI_WITH_GAIN`, как до «Microphone gain».
    detector_pcm: Vec<i16>,
    /// «Microphone gain», дБ: усиление [`Self::pcm_chunk`].
    mic_gain: u32,
    /// Chunks since the wake spotter's 1st-stage hit, while its verifier waits.
    first_stage_age: Option<usize>,
    frame: usize,
    /// Capture frames per VQE chunk (ALSA period of the FIFO).
    pub(super) capture_chunk: usize,
    nch: usize,
    mic_ch: &'static [u32],
    spk_ch: &'static [u32],
    chunk_ms: u64,
}

impl<E: WakeEngine, D: Detector, S: WakeStore, C: Clock> Processing<E, D, S, C> {
    pub(super) fn new(
        cfg: ProcessingConfig,
        engine: E,
        detector: Option<D>,
        store: Option<S>,
        clock: C,
    ) -> Self {
        let frame = engine.frame();
        let capture_chunk = engine.chunk();
        let nch = cfg.capture.channels as usize;
        let mic_ch = cfg.capture.mic_channels;
        let spk_ch = cfg.capture.spk_channels;
        let ana = SpectrumAnalyzer::new(cfg.capture.rate, capture_chunk);
        if !ana.ok() {
            warn!("music analysis: unsupported capture layout");
        }
        Self {
            engine,
            detector,
            store,
            clock,
            cmd_rx: cfg.cmd_rx,
            tx: cfg.tx,
            stats: cfg.stats,
            preroll: cfg.preroll,
            state: StateMachine::new(true),
            pending_model: None,
            custom: cfg.custom,
            commands: false,
            preroll_on: true,
            music: false,
            recording: true,
            wake_missed: false,
            custom_failed: false,
            input: vec![0i16; capture_chunk * nch],
            mics: vec![0f32; capture_chunk * mic_ch.len()],
            spk: vec![0f32; capture_chunk * spk_ch.len()],
            level_hist: LevelHistory::new(),
            gain_hist: [0f32; LEVEL_CHUNKS],
            gain_pos: 0,
            gain_filled: 0,
            ana,
            music_was_on: false,
            ring: Ring::new(frame, PREROLL_SAMPLES),
            pcm_chunk: vec![0i16; frame],
            detector_pcm: vec![0i16; frame],
            mic_gain: 0,
            first_stage_age: None,
            frame,
            capture_chunk,
            nch,
            mic_ch,
            spk_ch,
            chunk_ms: frame as u64 * 1000 / u64::from(logic::SAMPLE_RATE),
        }
    }

    /// Один чанк: сначала команды, затем те же шаги, что у старого `processing`, каждый — со
    /// своим портом.
    pub(super) fn process_chunk(&mut self, input: &[i16]) -> Vec<FrontendEvent> {
        self.drain_commands();
        let mut events = Vec::new();
        self.split_channels(input);
        self.music_chunk(&mut events);
        if !self.engine.process(&self.mics, &self.spk) {
            self.ring.clear();
            self.reset_gain_window();
            return events;
        }
        self.level_chunk();
        let det = self.detector_chunk(&mut events);
        // Listening time is the audio the detector actually heard (contract v1).
        if det.feeding
            && let Some(store) = self.store.as_mut()
        {
            store.listening(self.chunk_ms, self.state.dry_run());
        }
        if det.hit.is_some() {
            self.custom_wake(&det, &mut events);
        } else if !self.custom && self.state.detecting() {
            self.stock_wake(&mut events);
        } else {
            self.first_stage_age = None;
        }
        if self.commands {
            for word in self.engine.feed_command() {
                events.push(FrontendEvent::Command { word });
            }
        }
        events
    }

    fn split_channels(&mut self, input: &[i16]) {
        deinterleave(
            input,
            self.nch,
            self.mic_ch,
            self.spk_ch,
            &mut self.mics,
            &mut self.spk,
        );
    }

    fn music_chunk(&mut self, events: &mut Vec<FrontendEvent>) {
        let music_on = self.music && self.ana.ok();
        if music_on && !self.music_was_on {
            self.ana.reset();
        }
        self.music_was_on = music_on;
        let mut mf = MusicFrame::default();
        if music_on && self.ana.process(&self.spk, self.spk_ch.len(), &mut mf) {
            events.push(FrontendEvent::Music(mf));
        }
    }

    fn level_chunk(&mut self) {
        // VQE_OMNI is cleaned of the station's own playback but has no AGC, so it keeps the
        // distance to the talker; VQE_OMNI_WITH_GAIN, which the spotter hears, normalizes it
        // away.
        if let Some(d) = self.engine.output(VQE_OMNI)
            && !d.is_empty()
        {
            self.level_hist.push(d);
        }
    }

    /// Ricardo, окно записи и `Audio` одного чанка.
    fn detector_chunk(&mut self, events: &mut Vec<FrontendEvent>) -> DetectorChunk {
        let mut out = DetectorChunk::default();
        let custom_active = self.custom;
        let detecting = self.state.detecting();
        let dry_run = self.state.dry_run();
        // В HA и в кольцо пре-ролла идёт VQE_OMNI — главный канал распознавания стока, с
        // усилением «Microphone gain»; см. docs/hardware.md, «Библиотеки Яндекса (C API)».
        // Уровень для арбитража считается по нему же без усиления (см. `level_chunk`).
        let Some(d) = self.engine.output(VQE_OMNI).filter(|d| !d.is_empty()) else {
            self.ring.clear();
            // A missing output is a gap: restart the prior window.
            self.reset_gain_window();
            return out;
        };
        self.gain_hist[self.gain_pos] = mean_square(d);
        self.gain_pos = (self.gain_pos + 1) % LEVEL_CHUNKS;
        self.gain_filled = (self.gain_filled + 1).min(LEVEL_CHUNKS);
        let stored = if d.len() == self.frame && self.frame > 0 {
            to_s16_scaled(d, db_to_gain(self.mic_gain), &mut self.pcm_chunk);
            self.ring.push_chunk(&self.pcm_chunk)
        } else {
            self.ring.clear();
            false
        };
        if !stored {
            return out;
        }
        let slot = &self.pcm_chunk[..];
        // Детектор Ricardo слышит прежний VQE_OMNI_WITH_GAIN: он обучен и проверен на нём, а
        // «Microphone gain» меняет только звук для HA и пре-ролл. Нет выхода — тот же чанк.
        let detector_slot: &[i16] = match self
            .engine
            .output(VQE_OMNI_WITH_GAIN)
            .filter(|w| w.len() == self.frame)
        {
            Some(w) => {
                to_s16_slice(w, &mut self.detector_pcm);
                &self.detector_pcm
            }
            None => slot,
        };
        // Детектор Ricardo слышит прежний очищенный VQE_OMNI_WITH_GAIN, как и стоковый спотер;
        // его подтверждение берёт пре-ролл из кольца (в HA он идёт с «Microphone gain»).
        if custom_active && detecting && !self.custom_failed {
            out.feeding = true;
            match self.detector.as_mut().map(|d| d.feed(detector_slot)) {
                Some(Ok(Detection { hit: true, score })) => {
                    pulsar_metrics::observe_wake_score(f64::from(score));
                    out.hit = Some(score);
                    out.mean = Some(score);
                }
                Some(Ok(Detection { hit: false, score })) => {
                    // The windowed mean of every chunk drives the near-miss band;
                    // negative means the detector window is still filling.
                    if score >= 0.0 {
                        pulsar_metrics::observe_wake_score(f64::from(score));
                    }
                    out.mean = (score >= 0.0).then_some(score);
                }
                Some(Err(e)) => {
                    self.custom_failed = true;
                    error!("Ricardo wake word: {e}");
                }
                None => {
                    self.custom_failed = true;
                    error!("Ricardo wake word: detector is not loaded");
                }
            }
        }
        // Сквозное кольцо 31 с: hit держит 3 с до и 1 с после, miss — последние
        // 30 с. Выключено при мьюте и без модели Ricardo.
        let recording = self.store.is_some()
            && self.recording
            && custom_active
            && detecting
            && !self.custom_failed;
        if recording {
            let now = self.clock.mono_ms();
            let utc = self.clock.utc_ms();
            let miss = std::mem::take(&mut self.wake_missed);
            let store = self.store.as_mut().expect("store is Some when recording");
            if miss {
                store.missed(utc);
            }
            out.hit_id = store.push(detector_slot, out.mean, out.hit, dry_run, now, utc);
        } else {
            // No ring without recording: a miss button pressed meanwhile is void.
            self.wake_missed = false;
            if let Some(store) = self.store.as_mut() {
                store.reset();
            }
        }
        if self.state.audio_on() {
            events.push(FrontendEvent::Audio(slot.to_vec()));
        }
        out
    }

    fn custom_wake(&mut self, det: &DetectorChunk, events: &mut Vec<FrontendEvent>) {
        let Some(score) = det.hit else {
            return;
        };
        pulsar_metrics::wake_detected();
        if self.state.dry_run() {
            // Recorded with the outcome `dry_run` by the writer; no earcon, no claim, no
            // dialog, and the music keeps playing.
            info!("Ricardo wake word detected (probability {score:.3}): dry run, not answering");
            return;
        }
        let level = self.level_hist.level().as_f32();
        let speech_end = self.level_hist.speech_end(self.frame);
        let endpoint_prior = endpoint_prior(&self.gain_hist, self.gain_filled);
        let rule = self.preroll_rule("ricardo");
        let n = preroll_chunks(rule, None).min(self.ring.chunks());
        let preroll = self.ring.tail(n);
        info!(
            "Ricardo wake word detected (probability {score:.3}); \
             wake timing: speech_end={} preroll={}",
            Chunks(speech_end),
            preroll.len() / self.frame
        );
        self.state.apply(StateEvent::Wake);
        events.push(FrontendEvent::Wake {
            phrase: "Рикардо".into(),
            level_db: level,
            preroll,
            endpoint_prior,
            event_id: det.hit_id.clone(),
        });
    }

    fn stock_wake(&mut self, events: &mut Vec<FrontendEvent>) {
        // Kept for the level window's 1.5 s, whatever the VQE chunk.
        self.first_stage_age = self
            .first_stage_age
            .map(|a| a + 1)
            .filter(|&a| a * self.frame < LEVEL_CHUNKS * LEVEL_SAMPLES);
        let spotted = self.engine.feed_wake();
        match spotted.stage {
            Stage::FirstStageHit => self.first_stage_age = Some(0),
            Stage::Rejected => self.first_stage_age = None,
            Stage::None => {}
        }
        if spotted.phrases.is_empty() {
            return;
        }
        let level = self.level_hist.level().as_f32();
        let speech_end = self.level_hist.speech_end(self.frame);
        let endpoint_prior = endpoint_prior(&self.gain_hist, self.gain_filled);
        let rule = self.preroll_rule(self.engine.wake_id());
        let n = preroll_chunks(rule, self.first_stage_age).min(self.ring.chunks());
        let preroll = self.ring.tail(n);
        info!(
            "wake timing: first_stage={} speech_end={} preroll={}",
            Chunks(self.first_stage_age),
            Chunks(speech_end),
            preroll.len() / self.frame
        );
        self.first_stage_age = None;
        for phrase in spotted.phrases {
            self.state.apply(StateEvent::Wake);
            events.push(FrontendEvent::Wake {
                phrase,
                level_db: level,
                preroll: preroll.clone(),
                endpoint_prior,
                event_id: None,
            });
        }
    }

    fn drain_commands(&mut self) {
        while let Ok(cmd) = self.cmd_rx.try_recv() {
            self.apply(cmd);
        }
        if let Some(dir) = self.pending_model.take() {
            self.first_stage_age = None;
            if !self.engine.set_wake_model(&dir) {
                error!("failed to switch wake word model to {}", dir.display());
            }
        }
    }

    fn apply(&mut self, cmd: FrontendCmd) {
        match cmd {
            FrontendCmd::SetModel(dir) => self.pending_model = dir,
            FrontendCmd::SetDetection(on) => self.state.apply(StateEvent::Detecting(on)),
            FrontendCmd::SetCustomWake(on) => self.custom = on,
            FrontendCmd::SetCommands(on) => self.commands = on,
            FrontendCmd::SetStreaming(on) => self.state.apply(StateEvent::Streaming(on)),
            FrontendCmd::SetPreroll(on) => self.preroll_on = on,
            FrontendCmd::SetMusic(on) => self.music = on,
            FrontendCmd::SetRecording(on) => self.recording = on,
            FrontendCmd::SetDryRun(on) => self.state.apply(StateEvent::DryRun(on)),
            FrontendCmd::SetMicGain(db) => {
                if db != self.mic_gain {
                    self.mic_gain = db;
                    // В пре-ролле одной команды уровень не скачет.
                    self.ring.clear();
                }
            }
            FrontendCmd::WakeHandled => self.state.apply(StateEvent::WakeHandled),
            FrontendCmd::Missed => self.wake_missed = true,
        }
    }

    /// Pre-roll rule for the active detector. The custom Ricardo detector has no stock model
    /// directory, so its id is not `YandexEngine::wake_id` (which still points at the native
    /// «Алиса» model kept for rollback) but the `ricardo` entry of `Preroll`.
    fn preroll_rule(&self, stock_id: &str) -> Option<PrerollRule> {
        if !self.preroll_on {
            return None;
        }
        let id = if self.custom { "ricardo" } else { stock_id };
        self.preroll?
            .rules
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.rule)
    }

    fn reset_gain_window(&mut self) {
        self.gain_hist = [0.0; LEVEL_CHUNKS];
        self.gain_pos = 0;
        self.gain_filled = 0;
    }
}

pub(super) fn processing_thread<E, D, S, C>(
    mut p: Processing<E, D, S, C>,
    running: &AtomicBool,
    pipe: &Pipe,
) where
    E: WakeEngine,
    D: Detector,
    S: WakeStore,
    C: Clock,
{
    let mut input = std::mem::take(&mut p.input);
    while running.load(Relaxed) {
        {
            let (mut f, _) = pipe
                .ready
                .wait_timeout_while(lock(&pipe.fifo), CHUNK_WAIT, |f| {
                    !f.has_chunk() && running.load(Relaxed)
                })
                .unwrap_or_else(PoisonError::into_inner);
            if !f.take_chunk(&mut input) {
                continue;
            }
            lock(&p.stats).fifo_frames = (f.len() / p.nch) as u64;
        }
        lock(&p.stats).chunks += 1;
        for ev in p.process_chunk(&input) {
            if p.tx.send(ev).is_err() {
                return;
            }
        }
    }
}

/// Wake levels for the local endpoint detector on the stream channel, once the window is full.
/// A partial window would count its zero padding as a -100 dB floor.
fn endpoint_prior(hist: &[f32; LEVEL_CHUNKS], filled: usize) -> Option<EndpointPrior> {
    (filled == LEVEL_CHUNKS).then(|| EndpointPrior {
        noise_db: percentile_db(hist, 0.10).as_f32(),
        speech_db: level_db(hist).as_f32(),
    })
}

/// Pre-roll length in chunks for `rule`, given the chunks since the 1st-stage hit.
fn preroll_chunks(rule: Option<PrerollRule>, first_stage_age: Option<usize>) -> usize {
    match (rule, first_stage_age) {
        (None, _) => 0,
        (Some(PrerollRule::Fixed(n)), _) => n as usize,
        (Some(PrerollRule::FirstStage { offset, .. }), Some(a)) => {
            (a as i64 + i64::from(offset)).max(0) as usize
        }
        (Some(PrerollRule::FirstStage { fallback, .. }), None) => fallback as usize,
    }
}

struct Chunks(Option<usize>);

impl std::fmt::Display for Chunks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some(n) => write!(f, "{n}"),
            None => f.write_str("-"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Spotted;
    use crate::frontend::state::FrontendState;
    use crate::frontend::test_support::*;
    use pulsar_dsp::{Ring, to_s16_slice};
    use pulsar_model::{WakePreroll, profile_from_compatible};

    #[test]
    fn music_analyzer_is_built_from_the_capture_chunk_of_every_model() {
        // The FFT window is fed by capture-sized chunks; passing the VQE output frame
        // (512/256) instead disables music analysis on every model (the MIDI regression).
        for profile in pulsar_model::ALL {
            let (chunk, frame) = match profile.identity.codename {
                "cucumber" => (3072, 512),
                "plum" | "station-mini2" => (1536, 512),
                "yandexmini" | "bergamot" => (768, 256),
                other => panic!("unexpected model {other}"),
            };
            let mut h = harness_with(
                &profile.audio.capture,
                profile.wake_preroll.as_ref(),
                ScriptedEngine::with_sizes(chunk, frame, Vec::new()),
                Option::<ScriptedDetector>::None,
                Option::<NullStore>::None,
                FixedClock::new(),
                true,
            );
            assert!(
                h.processing.ana.ok(),
                "{}: unsupported capture layout",
                profile.identity.codename
            );
            h.processing.music = true;
            let input = block(&profile.audio.capture, chunk, 0);
            // The FFT window is 512 samples at 16 kHz: two 16 ms chunks fill it on the Minis.
            let calls = 512 * (profile.audio.capture.rate as usize / 16_000) / chunk;
            let mut events = Vec::new();
            for _ in 0..calls {
                events.extend(h.processing.process_chunk(&input));
            }
            assert!(
                events
                    .iter()
                    .any(|ev| matches!(ev, FrontendEvent::Music(_))),
                "{}: no music frame",
                profile.identity.codename
            );
        }
    }

    #[test]
    fn wake_level_is_rounded_to_a_tenth_for_the_arbiter() {
        let level_for = |amp: f32| {
            let (mut h, _log, _clock) = harness(
                ScriptedEngine::new(steps(47, amp)),
                vec![],
                miss_then_hit(46, 0.95),
                None,
                true,
            );
            let mut level = None;
            for _ in 0..47 {
                if let Some((db, _, _)) = wake_of(&process(&mut h, 0)) {
                    level = Some(db);
                }
            }
            level.expect("Ricardo hit on the last chunk")
        };
        assert_eq!(level_for(0.1), -20.0);
        let at = |db: f64| 10f64.powf(db / 20.0) as f32;
        // Половина к нулю от нуля до 0,1 дБ: −32,46 -> −32,5, −32,44 -> −32,4. Это то же
        // округление, что уходит в UDP-арбитраж строкой `%.1f`.
        assert_eq!(level_for(at(-32.46)), -32.5);
        assert_eq!(level_for(at(-32.44)), -32.4);
    }

    #[test]
    fn ricardo_wake_carries_the_last_eight_chunks_from_the_ring() {
        let midi = profile_from_compatible(b"yandex,cucumber\0").unwrap();
        let (mut h, _log, _clock) = harness(
            ScriptedEngine::new(indexed_steps(33)),
            vec![],
            miss_then_hit(32, 0.95),
            midi.wake_preroll.as_ref(),
            true,
        );
        let mut wake = None;
        for _ in 0..33 {
            if let Some(w) = wake_of(&process(&mut h, 0)) {
                wake = Some(w);
            }
        }
        let (_, preroll, _) = wake.expect("hit on the 33rd chunk");
        // Fixed(8) for `ricardo`: the confirming chunk is 32, the window starts at 25.
        assert_eq!(preroll.len(), 8 * 512);
        assert_eq!(preroll[0], 25);
        assert_eq!(preroll[preroll.len() - 1], 32);
    }

    #[test]
    fn endpoint_prior_appears_only_after_a_full_level_window() {
        let mut det = misses(10);
        det.push(Ok(Detection {
            hit: true,
            score: 0.95,
        }));
        det.extend(misses(35));
        det.push(Ok(Detection {
            hit: true,
            score: 0.95,
        }));
        let (mut h, _log, _clock) =
            harness(ScriptedEngine::new(steps(47, 0.1)), vec![], det, None, true);
        let mut early = None;
        let mut full = None;
        for k in 0..47 {
            if let Some((_, _, prior)) = wake_of(&process(&mut h, 0)) {
                if k == 10 {
                    early = Some(prior);
                } else {
                    full = Some(prior);
                }
            }
        }
        assert_eq!(early, Some(None));
        let prior = full.expect("full window at the last chunk").expect("prior");
        assert_eq!(prior.noise_db, -20.0);
        assert_eq!(prior.speech_db, -20.0);
    }

    #[test]
    fn audio_flows_only_while_a_wake_is_pending_or_streaming() {
        let mut det = misses(2);
        det.push(Ok(Detection {
            hit: true,
            score: 0.95,
        }));
        det.extend(misses(4));
        let (mut h, _log, _clock) =
            harness(ScriptedEngine::new(steps(7, 0.0)), vec![], det, None, true);
        // До подтверждения владелец ничего не получает.
        assert_eq!(audio_count(&process(&mut h, 0)), 0);
        assert_eq!(audio_count(&process(&mut h, 1)), 0);
        // Подтверждающий чанк уже в пре-ролле, но ещё не в `Audio`.
        let hit = process(&mut h, 2);
        assert!(wake_of(&hit).is_some());
        assert_eq!(audio_count(&hit), 0);
        // `wake_handled` ещё не пришёл: следующий чанк продолжает пре-ролл.
        assert_eq!(audio_count(&process(&mut h, 3)), 1);
        h.cmd.send(FrontendCmd::WakeHandled).unwrap();
        assert_eq!(audio_count(&process(&mut h, 4)), 0);
        // Стрим держит поток открытым независимо от `Wake`.
        h.cmd.send(FrontendCmd::SetStreaming(true)).unwrap();
        assert_eq!(audio_count(&process(&mut h, 5)), 1);
        h.cmd.send(FrontendCmd::SetDetection(false)).unwrap();
        assert_eq!(audio_count(&process(&mut h, 6)), 1);
    }

    #[test]
    fn microphone_gain_scales_the_ha_stream_of_the_stock_channel() {
        let steps = vec![Step::outputs(512, 0.01, 0.5), Step::outputs(512, 0.01, 0.5)];
        let mut h = harness_with(
            midi(),
            None,
            ScriptedEngine::new(steps),
            Option::<ScriptedDetector>::None,
            Option::<NullStore>::None,
            FixedClock::new(),
            false,
        );
        h.cmd.send(FrontendCmd::SetStreaming(true)).unwrap();
        let first = |events: &[FrontendEvent]| {
            events
                .iter()
                .find_map(|ev| match ev {
                    FrontendEvent::Audio(pcm) => Some(pcm[0]),
                    _ => None,
                })
                .expect("Audio while streaming")
        };
        // 0 дБ: в HA идёт VQE_OMNI как есть, а не прежний VQE_OMNI_WITH_GAIN.
        assert_eq!(first(&process(&mut h, 0)), 328);
        h.cmd.send(FrontendCmd::SetMicGain(20)).unwrap();
        // +20 дБ — в десять раз; смена усиления очищает пре-ролл.
        assert_eq!(first(&process(&mut h, 0)), 3277);
    }

    #[test]
    fn dry_run_records_the_hit_and_stays_silent() {
        let mut det = misses(1);
        det.push(Ok(Detection {
            hit: true,
            score: 0.93,
        }));
        let (mut h, log, _clock) =
            harness(ScriptedEngine::new(steps(3, 0.0)), vec![], det, None, true);
        h.cmd.send(FrontendCmd::SetDryRun(true)).unwrap();
        assert!(process(&mut h, 0).is_empty());
        let events = process(&mut h, 1);
        assert!(wake_of(&events).is_none());
        assert_eq!(audio_count(&events), 0);
        let log = lock(&log);
        let (mean, hit, dry_run) = log.pushes[1];
        assert_eq!(mean, Some(0.93));
        assert_eq!(hit, Some(0.93));
        assert!(dry_run);
        assert_eq!(log.listens, [(32, true), (32, true)]);
    }

    #[test]
    fn missed_button_reaches_the_store_only_while_recording() {
        let (mut h, log, _clock) = harness(
            ScriptedEngine::new(steps(2, 0.0)),
            vec![],
            misses(2),
            None,
            true,
        );
        h.cmd.send(FrontendCmd::Missed).unwrap();
        process(&mut h, 0);
        assert_eq!(lock(&log).misses, 1);
        h.cmd.send(FrontendCmd::SetRecording(false)).unwrap();
        h.cmd.send(FrontendCmd::Missed).unwrap();
        process(&mut h, 1);
        let log = lock(&log);
        assert_eq!(log.misses, 1);
        assert!(log.resets >= 1);
        drop(log);
    }

    /// The stock rule anchors the pre-roll at the 1st-stage hit; a model switch must drop that
    /// anchor, and the Ricardo switch must select its own `Fixed(8)` rule.
    #[test]
    fn switching_model_and_detector_drops_the_old_preroll_anchor() {
        static RULES: &[WakePreroll] = &[
            WakePreroll {
                id: "alisa",
                rule: PrerollRule::FirstStage {
                    offset: 0,
                    fallback: 3,
                },
            },
            WakePreroll {
                id: "ricardo",
                rule: PrerollRule::Fixed(8),
            },
        ];
        static PREROLL: Preroll = Preroll { rules: RULES };

        let mut spotted = vec![
            Spotted {
                phrases: vec![],
                stage: Stage::FirstStageHit,
            },
            Spotted::default(),
            Spotted::default(),
            Spotted::default(),
            Spotted {
                phrases: vec!["алиса".into()],
                stage: Stage::None,
            },
            Spotted {
                phrases: vec!["алиса".into()],
                stage: Stage::None,
            },
        ];
        // The detector switch happens before the last chunk; its hit takes the custom rule.
        let (mut h, _log, _clock) = harness(
            ScriptedEngine::new(indexed_steps(40)),
            std::mem::take(&mut spotted),
            miss_then_hit(32, 0.95),
            Some(&PREROLL),
            false,
        );
        let mut first = None;
        for _ in 0..5 {
            if let Some(w) = wake_of(&process(&mut h, 0)) {
                first = Some(w);
            }
        }
        // 1st-stage at chunk 0, confirmation at chunk 4: age 4 + offset 0.
        assert_eq!(first.unwrap().1.len(), 4 * 512);

        // No 1st-stage before the confirmation after the switch: the fallback applies.
        h.cmd
            .send(FrontendCmd::SetModel(Some(PathBuf::from("/models/alisa"))))
            .unwrap();
        assert_eq!(wake_of(&process(&mut h, 0)).unwrap().1.len(), 3 * 512);

        // Switch to Ricardo: its rule is Fixed(8), and the stale stock anchor is gone.
        h.cmd.send(FrontendCmd::SetCustomWake(true)).unwrap();
        let mut custom = None;
        for _ in 0..33 {
            if let Some(w) = wake_of(&process(&mut h, 0)) {
                custom = Some(w);
            }
        }
        assert_eq!(custom.unwrap().1.len(), 8 * 512);
    }

    #[test]
    fn preroll_length_per_rule() {
        let first = |offset, fallback| Some(PrerollRule::FirstStage { offset, fallback });
        assert_eq!(preroll_chunks(None, Some(5)), 0);
        assert_eq!(preroll_chunks(Some(PrerollRule::Fixed(7)), None), 7);
        assert_eq!(preroll_chunks(Some(PrerollRule::Fixed(7)), Some(2)), 7);
        assert_eq!(preroll_chunks(first(3, 9), Some(4)), 7);
        assert_eq!(preroll_chunks(first(-3, 9), Some(4)), 1);
        // An anchor closer than the offset: nothing rather than the word itself.
        assert_eq!(preroll_chunks(first(-3, 9), Some(2)), 0);
        // No 1st-stage hit before the confirmation.
        assert_eq!(preroll_chunks(first(-3, 9), None), 9);
    }

    #[test]
    fn ricardo_preroll_rule_uses_the_custom_id() {
        let midi = profile_from_compatible(b"yandex,cucumber\0").unwrap();
        // The custom detector is active: the stock model id ("alisa", kept for rollback) must
        // not select the rule, or the Ricardo word loses its pre-roll.
        let h = harness_with(
            &midi.audio.capture,
            midi.wake_preroll.as_ref(),
            ScriptedEngine::new(vec![]),
            Option::<ScriptedDetector>::None,
            Option::<NullStore>::None,
            FixedClock::new(),
            true,
        );
        assert_eq!(
            h.processing.preroll_rule("alisa"),
            Some(PrerollRule::Fixed(8))
        );
        // Switched back to a stock spotter: the custom entry no longer applies.
        let h = harness_with(
            &midi.audio.capture,
            midi.wake_preroll.as_ref(),
            ScriptedEngine::new(vec![]),
            Option::<ScriptedDetector>::None,
            Option::<NullStore>::None,
            FixedClock::new(),
            false,
        );
        assert_eq!(
            h.processing.preroll_rule("alisa"),
            Some(PrerollRule::Fixed(2))
        );
    }

    #[test]
    fn ricardo_preroll_is_the_last_256_ms_of_the_ring() {
        const FRAME: usize = 512; // 16 kHz, one VQE chunk = 32 ms
        const RING_CHUNKS: usize = PREROLL_SAMPLES / FRAME;
        let midi = profile_from_compatible(b"yandex,cucumber\0").unwrap();
        let mut ring = Ring::new(FRAME, PREROLL_SAMPLES);
        let mut chunk = vec![0i16; FRAME];
        for k in 0..RING_CHUNKS {
            to_s16_slice(&[k as f32 / 32768.0; FRAME], &mut chunk);
            ring.push_chunk(&chunk);
        }
        let h = harness_with(
            &midi.audio.capture,
            midi.wake_preroll.as_ref(),
            ScriptedEngine::new(vec![]),
            Option::<ScriptedDetector>::None,
            Option::<NullStore>::None,
            FixedClock::new(),
            true,
        );
        let n = preroll_chunks(h.processing.preroll_rule("ricardo"), None);
        assert_eq!(n, 8);
        let preroll = ring.tail(n);
        assert_eq!(preroll.len(), n * FRAME);
        assert_eq!(preroll.len() / 16, 256, "milliseconds at 16 kHz");
        // Oldest first, ending with the chunk that confirmed the word.
        assert_eq!(preroll[0], (RING_CHUNKS - n) as i16);
        assert_eq!(preroll[preroll.len() - 1], (RING_CHUNKS - 1) as i16);
    }

    #[test]
    fn preroll_off_is_empty_but_the_wake_hold_is_separate() {
        let midi = profile_from_compatible(b"yandex,cucumber\0").unwrap();
        let mut h = harness_with(
            &midi.audio.capture,
            midi.wake_preroll.as_ref(),
            ScriptedEngine::new(vec![]),
            Option::<ScriptedDetector>::None,
            Option::<NullStore>::None,
            FixedClock::new(),
            true,
        );
        h.processing.apply(FrontendCmd::SetPreroll(false));
        assert_eq!(h.processing.preroll_rule("ricardo"), None);
        let mut ring = Ring::new(512, PREROLL_SAMPLES);
        ring.push_chunk(&[16384; 512]);
        assert!(ring.tail(0).is_empty());
        // `wake_pending` holds the stream open regardless of the pre-roll switch: the owner
        // raises it for every Wake and lowers it once after `set_streaming`.
        h.processing.state.apply(StateEvent::Wake);
        assert_eq!(h.processing.state.state(), FrontendState::WakePending(1));
        h.processing.apply(FrontendCmd::WakeHandled);
        assert_eq!(h.processing.state.state(), FrontendState::Detecting);
    }
}
