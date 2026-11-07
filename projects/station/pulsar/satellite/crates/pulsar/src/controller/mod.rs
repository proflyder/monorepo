//! Glue between Home Assistant (ESPHome API), the microphone front-end, playback and the hardware
//! UI (C++ controller.cpp). Voice-satellite behaviour follows OHF linux-voice-assistant
//! (satellite.py). The controller owns its state and every service handle; the services report
//! through channels, which `run` serves one event at a time (no re-entrant hooks as in C++).
//!
//! Everything the controller calls into is a port (`ports`); `wiring` starts the services and
//! builds `Deps`. The controller itself knows nothing about paths, FFI or the start-up order.

mod earcons;
mod entities;
mod logic;
mod media;
mod ports;
mod timers;
mod voice;
mod wiring;

use std::collections::HashMap;

use pulsar_arbiter::ArbiterEvent;
use pulsar_audio::{FrontendEvent, Outcome};
use pulsar_clock::ClockMusicStyle;
use pulsar_core::{ClockShot, Timers, UiState, now_ms};
use pulsar_esphome::{ApiEvent, LightState};
use pulsar_hw::{Effect, InputEvent, MusicStyle, Rgb, VoiceColor};
use pulsar_media::{Channel, PlayId, PlayerEvent};
use pulsar_model::DeviceProfile;
use pulsar_sendspin::SendspinEvent;
use pulsar_update::UpdateEvent;
use pulsar_zigbee::ZigbeeEvent;
use tokio::signal::unix::Signal;
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::info;

use crate::settings::{self, Store};
use logic::{Phase, RunBinding};

pub use wiring::prepare_process;

const MODELS: &str = "/system/vendor/quasar/spotter_models/activation/";
const COMMAND_MODEL: &str = "/system/vendor/quasar/spotter_models/command";
const RICARDO_MODEL: &str = "/data/pulsar/wakewords/ru_ricardo_mww.tflite";
/// Recorded wake events, beside `state.conf`.
const WAKE_EVENTS_DIR: &str = "wake-events";

const LISTEN_TIMEOUT_MS: u64 = 30_000;
const PROCESS_TIMEOUT_MS: u64 = 120_000;
// Updates: releases of this fork on GitHub, never upstream. The first check waits for the
// network after boot; auto_update stays off by default, so a custom binary is not replaced
// silently; an automatic install waits until the speaker is idle (it restarts the daemon).
const UPDATE_FIRST_CHECK_MS: u64 = 60_000;
const UPDATE_CHECK_PERIOD_MS: u64 = 6 * 3_600_000;
const UPDATE_IDLE_RETRY_MS: u64 = 10 * 60_000;
// The C++ LightSensor::on_lux default.
const LUX_PERIOD_MS: u64 = 2000;

/// Why `run` returned; the daemon exits 0 for each (supervise.sh starts it again).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitReason {
    /// SIGTERM / SIGINT.
    Signal,
    /// Home Assistant's Restart button.
    Restart,
    /// Another Zigbee role was chosen: it starts with the daemon.
    ZigbeeRoleChanged,
    /// A new release replaced the binary.
    Updated,
}

/// Loop timers of the C++ controller. Intended difference: C++ armed an independent timer on
/// every call, so with two error rings pending update_ui ran at the earlier deadline (and again
/// at the later one); here a re-arm of `UiAfterNotSubscribed` / `UiAfterError` moves the
/// deadline. Each firing runs the same idempotent update, so the final state is identical.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tk {
    /// The error ring of a wake word without a voice subscriber ends.
    UiAfterNotSubscribed,
    /// The error ring of a failed dialog ends.
    UiAfterError,
    /// Ends a LISTENING / PROCESSING phase HA never advances.
    Watchdog,
    /// В follow-up-реплике речь не началась за окно.
    FollowupWait,
    /// The shown running HA timer runs out.
    TimerTick,
    /// Next round of the timer ring.
    RingTick,
    UpdateCheck,
    /// An automatic install waits for idle.
    UpdateRetry,
    /// Exit after an installed update.
    UpdateExit,
}

/// The recorded Ricardo hit behind the current dialog, or behind a pending wake arbitration.
/// The sidecar's outcome is filled from it when the dialog ends (contract v1).
#[derive(Clone, Debug, Default)]
struct WakeRecord {
    id: String,
    /// STT_END text of the dialog; Some("") when HA recognized nothing.
    stt: Option<String>,
    cancelled: bool,
    error: bool,
    /// Another station answered this wake word.
    arbiter_lost: bool,
}

impl WakeRecord {
    fn new(id: String) -> Self {
        Self {
            id,
            ..Self::default()
        }
    }

    fn outcome(&self) -> Outcome {
        if self.arbiter_lost {
            Outcome::ArbiterLost
        } else if self.error {
            Outcome::Error
        } else if self.cancelled {
            Outcome::Cancelled
        } else {
            match &self.stt {
                Some(text) if !text.is_empty() => Outcome::Answered,
                _ => Outcome::EmptyStt,
            }
        }
    }
}

/// Id of one dialog: a completion or a sidecar outcome belongs to the dialog it started in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DialogId(u64);

impl DialogId {
    fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

/// Id of one HA announcement (assist or media player).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AnnounceId(u64);

impl AnnounceId {
    fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

/// Id of one closing earcon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EarconSeq(u64);

impl EarconSeq {
    fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

/// What a `play()` was for: the C++ completion callback and the values it captured.
#[derive(Clone, Copy, Debug)]
enum Pending {
    /// Pipeline TTS of dialog `dialog`.
    Tts { dialog: DialogId },
    /// Error / cancel earcon number `seq`.
    Earcon { seq: EarconSeq },
    /// HA (assist satellite) announcement number `seq`.
    Announce { conv: bool, seq: AnnounceId },
    /// Media player announcement number `seq`.
    MediaAnnounce { seq: AnnounceId },
    /// A round of the timer ring.
    Ring,
}

/// Event channels of the services; a closed one is dropped.
struct Rx {
    api: Option<UnboundedReceiver<ApiEvent>>,
    frontend: Option<UnboundedReceiver<FrontendEvent>>,
    player: Option<UnboundedReceiver<PlayerEvent>>,
    arbiter: Option<UnboundedReceiver<ArbiterEvent>>,
    input: Option<UnboundedReceiver<InputEvent>>,
    lux: Option<UnboundedReceiver<f32>>,
    sendspin: Option<UnboundedReceiver<SendspinEvent>>,
    zigbee: Option<UnboundedReceiver<ZigbeeEvent>>,
    updater: Option<UnboundedReceiver<UpdateEvent>>,
}

/// Next event of an optional channel; never ready without one.
async fn recv<T>(rx: &mut Option<UnboundedReceiver<T>>) -> Option<T> {
    match rx {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

/// Один готовый источник `select!`; разбор — в `run`.
enum Trigger {
    Timer(Tk),
    StateDue,
    Term(i32),
    Api(Option<ApiEvent>),
    Player(Option<PlayerEvent>),
    Frontend(Option<FrontendEvent>),
    Arbiter(Option<ArbiterEvent>),
    Input(Option<InputEvent>),
    Sendspin(Option<SendspinEvent>),
    Zigbee(Option<ZigbeeEvent>),
    Updater(Option<UpdateEvent>),
    Lux(Option<f32>),
}

pub struct Controller {
    // Service handles first: fields drop in declaration order, which is the C++ members'
    // reverse declaration order (Sendspin first, the API server last). Dropping the last handle
    // of a service stops it; the front-end joins its threads (up to ~200 ms of snd_pcm_wait plus
    // the chunk in progress), inside the teardown watchdog that main arms before the drop.
    deps: ports::Deps,
    rx: Rx,
    term: Signal,
    int: Signal,

    model: &'static DeviceProfile,
    st: Store,
    media: media::Policy,
    entity: entities::Publisher,
    timers_ha: timers::HaTimers,
    timers: Timers<Tk>,
    /// Completion of each `play()` whose end matters.
    pending: HashMap<PlayId, Pending>,
    exit: Option<ExitReason>,
    /// TZ last given to libc ("" = none yet).
    applied_tz: String,

    phase: Phase,
    /// Start of the current LISTENING phase (commands window).
    listen_started_ms: i64,
    /// Текущий диалог открыт wake word (не кнопкой, не продолжением диалога и не HA).
    wake_dialog: bool,
    /// Звонок выключили посреди этого диалога «стоп» спотера или wake word, а диалог пошёл дальше.
    ring_stopped_in_dialog: bool,
    hw_mic_muted: bool,
    error_shown: bool,
    // Wake arbitration: the window between our claim and the decision. Microphone audio and a
    // command word heard in it are held until the decision.
    arbitrating: bool,
    /// Our claim being decided; forgotten on cancel (a `Decided` already queued then still
    /// arrives and is ignored).
    arb_claim: Option<pulsar_arbiter::ClaimId>,
    arb_phrase: String,
    arb_level: f32,
    arb_audio: Vec<i16>,
    /// Samples in `arb_audio` that came with the wake event; the rest arrived after it (live
    /// post-wake speech held during arbitration).
    arb_boundary: usize,
    arb_endpoint: Option<pulsar_audio::endpoint::EndpointPrior>,
    /// Another station answers the wake word just heard.
    commands_blocked_until_ms: i64,
    run: RunBinding,
    /// Experimental local end-of-utterance detector (opt-in, see `PULSAR_LOCAL_ENDPOINTING`).
    endpoint: Option<pulsar_audio::endpoint::EndpointRun>,
    /// voice_request_stop already sent for this dialog's run.
    stop_sent: bool,
    /// Id of the last voice_request_start: VoiceStartFailed / VoiceAudioOverflow of another
    /// run are ignored.
    voice_run: u64,
    tts_url: String,
    tts_played: bool,
    /// A run's TTS_START left HA's assist satellite "responding" until AnnounceFinished, also
    /// when nothing plays here, and that report is still to be sent (`send_tts_report`).
    tts_report_owed: bool,
    followup: logic::Followup,
    /// An HA announcement (assist or media player) plays on ANNOUNCE.
    announcing: bool,
    closing_earcon: bool,
    earcon_seq: EarconSeq,
    // Completions act only for what is still current: `dialog` changes whenever a dialog starts
    // or ends, `announce_seq` whenever an HA announcement starts or is interrupted.
    dialog: DialogId,
    announce_seq: AnnounceId,
    /// When the HA media channel last paused (logic::owns_music).
    music_paused_ms: Option<i64>,
    /// When the Sendspin stream last ended: pause or stop in Music Assistant (logic::owns_music).
    sendspin_ended_ms: Option<i64>,
    /// A check whose `Checked` has not been handled yet (C++ `Updater::busy()` until its
    /// callback ran).
    pending_check: Option<pulsar_update::RequestId>,
    /// An install whose `Installed` has not been handled yet.
    install_id: Option<pulsar_update::RequestId>,
    /// Version being installed (for the messages of its outcome).
    install_version: String,
    sendspin_pairing_prompt: bool,
    /// The hit whose dialog is open.
    wake_rec: Option<WakeRecord>,
    /// The hit heard while this station arbitrates with its neighbours.
    wake_pending: Option<WakeRecord>,
}

/// Face from the option text; "None" / "" / unknown clear it (the stock "no face").
fn clock_face(name: &str) -> pulsar_clock::ClockFace {
    name.parse().unwrap_or(pulsar_clock::ClockFace::None)
}

/// Effect from `state.conf` / HA; unknown or "" is the plain colour.
fn light_effect(name: &str) -> Effect {
    name.parse().unwrap_or_default()
}

fn model_dir(id: &str) -> std::path::PathBuf {
    if id == "ricardo" {
        std::path::PathBuf::from(RICARDO_MODEL)
    } else {
        std::path::PathBuf::from(format!("{MODELS}{id}"))
    }
}

/// C++ `uint8_t(x * 255)`.
fn light_rgb(r: f32, g: f32, b: f32) -> Rgb {
    Rgb {
        r: (r * 255.0) as u8,
        g: (g * 255.0) as u8,
        b: (b * 255.0) as u8,
    }
}

/// `state.conf` option text → domain type, unknown text → the shipped default. One copy for the
/// controller and `wiring`.
fn music_viz(st: &settings::Settings) -> MusicStyle {
    st.music_viz.parse().unwrap_or(MusicStyle::All)
}

fn clock_music(st: &settings::Settings) -> ClockMusicStyle {
    st.clock_music.parse().unwrap_or_default()
}

fn voice_color(st: &settings::Settings) -> VoiceColor {
    st.voice_color.parse().unwrap_or_default()
}

fn zigbee_role(st: &settings::Settings) -> pulsar_zigbee::Mode {
    pulsar_zigbee::parse_mode(&st.zigbee_role)
}

impl Controller {
    /// Loads the settings and starts every service. Err (to be logged by the caller, exit 1):
    /// an invalid `PULSAR_PORT_OFFSET`, or the API server could not bind its port.
    pub fn start(
        model: &'static DeviceProfile,
        state_path: std::path::PathBuf,
    ) -> Result<Self, wiring::StartError> {
        let wired = wiring::wire(model, state_path)?;
        Ok(Self::new(wired))
    }

    /// Builds the controller from the wired services: no paths, no FFI, no start-up order.
    fn new(wired: wiring::Wired) -> Self {
        let wiring::Wired {
            deps,
            rx,
            term,
            int,
            model,
            store,
            media,
            applied_tz,
            hw_mic_muted,
            persist_on_start,
        } = wired;
        let mut c = Self {
            deps,
            rx,
            term,
            int,
            model,
            st: store,
            media,
            entity: entities::Publisher::new(),
            timers_ha: timers::HaTimers::new(),
            timers: Timers::new(),
            pending: HashMap::new(),
            exit: None,
            applied_tz,
            phase: Phase::Idle,
            listen_started_ms: 0,
            wake_dialog: false,
            ring_stopped_in_dialog: false,
            hw_mic_muted,
            error_shown: false,
            arbitrating: false,
            arb_claim: None,
            arb_phrase: String::new(),
            arb_level: -100.0,
            arb_audio: Vec::new(),
            arb_boundary: 0,
            arb_endpoint: None,
            commands_blocked_until_ms: 0,
            run: RunBinding::default(),
            endpoint: None,
            stop_sent: false,
            voice_run: 0,
            tts_url: String::new(),
            tts_played: false,
            tts_report_owed: false,
            followup: logic::Followup::default(),
            announcing: false,
            closing_earcon: false,
            earcon_seq: EarconSeq(0),
            dialog: DialogId(0),
            announce_seq: AnnounceId(0),
            music_paused_ms: None,
            sendspin_ended_ms: None,
            pending_check: None,
            install_id: None,
            install_version: String::new(),
            sendspin_pairing_prompt: false,
            wake_rec: None,
            wake_pending: None,
        };
        // The rest of the C++ start(), in its order: every call below only queues commands to
        // the services, which run once `run` is awaited.
        c.on_start(persist_on_start);
        c
    }

    /// The C++ tail of `start()`: published state, update schedule, gains, ring, boot shot.
    fn on_start(&mut self, persist_on_start: bool) {
        self.publish_update();
        self.timers.set_repeating(
            Tk::UpdateCheck,
            UPDATE_FIRST_CHECK_MS,
            UPDATE_CHECK_PERIOD_MS,
        );
        self.update_detection();
        self.apply_gains(); // a saved mute
        self.update_ui();
        self.deps.display.clock.play(ClockShot::Boot);
        if persist_on_start {
            self.persist();
        }
    }

    /// Serves the services' events until the daemon should exit. Leaving while an update
    /// installs: the updater cancels the install when its handle goes (the binary stays as it
    /// was, whereas C++ finished it), so `update_attempt` is cleared; left set, automatic
    /// updates would skip that release. An install that already succeeded (its `Installed` still
    /// queued) keeps it: the new binary clears it. A settings change still pending is written by
    /// `flush_settings` afterwards.
    pub async fn run(&mut self) -> ExitReason {
        let reason = loop {
            if let Some(r) = self.exit.take() {
                break r;
            }
            match self.next_event().await {
                Trigger::Timer(k) => self.on_timer(k),
                Trigger::StateDue => self.st.flush(),
                Trigger::Term(sig) => {
                    info!("signal {sig}, exiting");
                    break ExitReason::Signal;
                }
                Trigger::Api(ev) => self.on_api_event(ev),
                Trigger::Player(ev) => self.on_player_event(ev),
                Trigger::Frontend(ev) => self.on_frontend_event(ev),
                Trigger::Arbiter(ev) => self.on_arbiter_event(ev),
                Trigger::Input(ev) => self.on_input_event(ev),
                Trigger::Sendspin(ev) => self.on_sendspin_event(ev),
                Trigger::Zigbee(ev) => self.on_zigbee_event(ev),
                Trigger::Updater(ev) => self.on_updater_event(ev),
                Trigger::Lux(ev) => self.on_lux_event(ev),
            }
        };
        self.settle_interrupted_update();
        reason
    }

    /// Timers first (biased): keys due at one wake-up fire back to back (pulsar_core Timers
    /// contract), then the pending settings write, signals, and the services' events.
    async fn next_event(&mut self) -> Trigger {
        tokio::select! {
            biased;
            k = self.timers.next() => Trigger::Timer(k),
            () = self.st.due() => Trigger::StateDue,
            _ = self.term.recv() => Trigger::Term(libc::SIGTERM),
            _ = self.int.recv() => Trigger::Term(libc::SIGINT),
            ev = recv(&mut self.rx.api) => Trigger::Api(ev),
            ev = recv(&mut self.rx.player) => Trigger::Player(ev),
            ev = recv(&mut self.rx.frontend) => Trigger::Frontend(ev),
            ev = recv(&mut self.rx.arbiter) => Trigger::Arbiter(ev),
            ev = recv(&mut self.rx.input) => Trigger::Input(ev),
            ev = recv(&mut self.rx.sendspin) => Trigger::Sendspin(ev),
            ev = recv(&mut self.rx.zigbee) => Trigger::Zigbee(ev),
            ev = recv(&mut self.rx.updater) => Trigger::Updater(ev),
            lux = recv(&mut self.rx.lux) => Trigger::Lux(lux),
        }
    }

    fn on_api_event(&mut self, ev: Option<ApiEvent>) {
        match ev {
            Some(ev) => self.on_api(ev),
            None => self.rx.api = None,
        }
    }

    fn on_player_event(&mut self, ev: Option<PlayerEvent>) {
        match ev {
            Some(ev) => self.on_player(ev),
            None => self.rx.player = None,
        }
    }

    fn on_frontend_event(&mut self, ev: Option<FrontendEvent>) {
        match ev {
            Some(ev) => self.on_frontend(ev),
            None => self.rx.frontend = None,
        }
    }

    fn on_arbiter_event(&mut self, ev: Option<ArbiterEvent>) {
        match ev {
            Some(ArbiterEvent::Decided {
                claim_id,
                won,
                best,
                peers,
            }) => {
                if self.arbitrating && self.arb_claim == Some(claim_id) {
                    self.arbitration_done(won, &best, peers);
                }
            }
            None => self.rx.arbiter = None,
        }
    }

    fn on_input_event(&mut self, ev: Option<InputEvent>) {
        match ev {
            Some(ev) => self.on_input(ev),
            None => self.rx.input = None,
        }
    }

    fn on_sendspin_event(&mut self, ev: Option<SendspinEvent>) {
        match ev {
            Some(ev) => self.on_sendspin(ev),
            None => self.rx.sendspin = None,
        }
    }

    fn on_zigbee_event(&mut self, ev: Option<ZigbeeEvent>) {
        match ev {
            Some(ev) => self.on_zigbee(ev),
            None => self.rx.zigbee = None,
        }
    }

    fn on_updater_event(&mut self, ev: Option<UpdateEvent>) {
        match ev {
            Some(ev) => self.on_updater(ev),
            None => self.rx.updater = None,
        }
    }

    fn on_lux_event(&mut self, ev: Option<f32>) {
        match ev {
            Some(lux) => self.on_lux(lux),
            None => self.rx.lux = None,
        }
    }

    /// An install cut short by the exit: the binary stays as it was, clear the marker so
    /// automatic updates try that release again.
    fn settle_interrupted_update(&mut self) {
        let Some(id) = self.install_id else {
            return;
        };
        let mut installed = false;
        if let Some(rx) = &mut self.rx.updater {
            while let Ok(ev) = rx.try_recv() {
                if let UpdateEvent::Installed { id: i, result } = ev
                    && i == id
                {
                    installed = result.is_ok();
                }
            }
        }
        if !installed {
            info!(
                "update to {}: cut short by the exit, not installed",
                self.install_version
            );
            self.st.update_attempt.clear();
            self.persist();
        }
    }

    fn on_timer(&mut self, k: Tk) {
        match k {
            Tk::UiAfterNotSubscribed | Tk::UiAfterError => self.update_ui(),
            Tk::Watchdog => self.abort_dialog("no progress from Home Assistant, giving up"),
            Tk::FollowupWait => {
                info!("follow-up: no speech started, conversation ends");
                self.finish_interaction();
            }
            Tk::TimerTick => self.show_timers(),
            Tk::RingTick => self.ring_timer(),
            Tk::UpdateCheck => self.check_update(),
            Tk::UpdateRetry => self.maybe_auto_update(),
            // exit 0: supervise.sh starts the new binary
            Tk::UpdateExit => self.exit = Some(ExitReason::Updated),
        }
    }

    fn on_player(&mut self, ev: PlayerEvent) {
        match ev {
            // HA media state follows playback (C++ Player::on_state hook).
            PlayerEvent::State(ch, s) => {
                if ch == Channel::Music {
                    match s {
                        pulsar_media::PlayState::Paused => self.music_paused_ms = Some(now_ms()),
                        // HA media took the speaker: an ended Sendspin stream is not this
                        // station's music any more, whichever of the two events came first.
                        pulsar_media::PlayState::Loading | pulsar_media::PlayState::Playing => {
                            self.sendspin_ended_ms = None;
                        }
                        _ => {}
                    }
                }
                self.publish_media();
            }
            PlayerEvent::Done { id, ok } => {
                if let Some(p) = self.pending.remove(&id) {
                    self.play_done(p, ok);
                }
            }
        }
    }

    fn on_frontend(&mut self, ev: FrontendEvent) {
        match ev {
            FrontendEvent::Wake {
                phrase,
                level_db,
                preroll,
                endpoint_prior,
                event_id,
            } => {
                self.on_wake(&phrase, level_db, preroll, endpoint_prior, event_id);
                // After on_wake has switched streaming on where the wake word starts a request.
                self.deps.frontend.wake_handled();
            }
            FrontendEvent::Command { word } => self.on_command(&word),
            FrontendEvent::Audio(pcm) => {
                if self.phase == Phase::Listening {
                    self.on_listening_audio(pcm);
                } else if self.arbitrating {
                    // The request goes on with what was said right after the wake word.
                    self.arb_audio.extend_from_slice(&pcm);
                }
            }
            FrontendEvent::Music(f) => {
                self.deps.display.leds.music_frame(f, false);
                self.deps.display.clock.music_frame(f, false);
            }
        }
    }

    /// `st` changed: written a moment after the last change (`Store`). `volume=` follows
    /// `volume2=` on the curve of earlier builds.
    fn persist(&mut self) {
        self.st.volume_legacy = pulsar_core::volume::to_legacy(&self.model.volume, self.st.volume);
        self.st.changed();
    }

    /// `st` changed in a way that must survive a power cut or a crash right away: before a
    /// restart or a binary swap, or the microphone mute. True = on disk; on false the change
    /// stays pending (`Store`).
    #[must_use = "a write that must be on disk has to check it"]
    fn persist_now(&mut self) -> bool {
        self.st.volume_legacy = pulsar_core::volume::to_legacy(&self.model.volume, self.st.volume);
        self.st.write_now()
    }

    /// Writes a pending change now: main calls it once after `run`, when nothing changes any
    /// more.
    pub fn flush_settings(&mut self) {
        self.st.flush();
    }

    /// The supervisor will start this daemon again (the `enabled` file exists): promise the
    /// Sendspin server a reconnect before the client stops, otherwise its stop() sends
    /// `shutdown` and Music Assistant deliberately stops redialing this player.
    pub fn promise_sendspin_restart(&self) {
        if let Some(ss) = &self.deps.sendspin {
            ss.goodbye_restart();
        }
    }

    fn mic_muted(&self) -> bool {
        self.st.mic_muted || self.hw_mic_muted
    }

    fn sendspin_streaming(&self) -> bool {
        self.deps.sendspin.as_ref().is_some_and(|ss| ss.streaming())
    }

    fn zigbee_flashing(&self) -> bool {
        self.deps.radio.as_ref().is_some_and(|z| z.flashing())
    }

    /// The stored light as the ESPHome entity shows it.
    fn light_state(&self) -> LightState {
        let l = &self.st.light;
        LightState {
            on: l.on,
            brightness: l.brightness,
            r: l.r,
            g: l.g,
            b: l.b,
            effect: l.effect.clone(),
        }
    }

    /// HA command → the stored light (`settings::Light` has no foreign domain types).
    fn store_light(&mut self, l: LightState) {
        self.st.light = settings::Light {
            on: l.on,
            brightness: l.brightness,
            r: l.r,
            g: l.g,
            b: l.b,
            effect: l.effect,
        };
    }

    fn update_ui(&mut self) {
        self.deps.display.leds.set_muted(self.mic_muted());
        self.update_commands();
        let s = if self.timers_ha.ringing() {
            UiState::TimerRing
        } else {
            match self.phase {
                Phase::Listening => UiState::Listening,
                Phase::Processing => UiState::Thinking,
                Phase::Speaking => UiState::Speaking,
                Phase::Idle if self.arbitrating => UiState::Listening,
                Phase::Idle if self.announcing => UiState::Speaking,
                Phase::Idle => UiState::Idle,
            }
        };
        self.show_ui(s);
    }

    /// Ring and clock.
    fn show_ui(&self, s: UiState) {
        self.deps.display.leds.set_ui_state(s);
        self.deps.display.clock.set_ui_state(s);
    }

    fn update_detection(&self) {
        self.deps.frontend.set_detection(!self.mic_muted());
    }
}

#[cfg(test)]
mod tests;
