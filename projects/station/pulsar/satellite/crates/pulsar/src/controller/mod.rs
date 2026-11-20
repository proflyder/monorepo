mod call;
mod earcons;
mod entities;
mod logic;
mod media;
mod ports;
mod session;
mod timers;
mod voice;
mod wake;
mod wiring;

use std::collections::HashMap;

use pulsar_arbiter::ArbiterEvent;
use pulsar_audio::{FrontendEvent, Outcome};
use pulsar_call::CallEvent;
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
use session::Session;
use wake::Arbitration;

pub use wiring::prepare_process;

const MODELS: &str = "/system/vendor/quasar/spotter_models/activation/";
const COMMAND_MODEL: &str = "/system/vendor/quasar/spotter_models/command";
const RICARDO_MODEL: &str = "/data/pulsar/wakewords/ru_ricardo_mww.tflite";
const WAKE_EVENTS_DIR: &str = "wake-events";

const LISTEN_TIMEOUT_MS: u64 = 30_000;
const PROCESS_TIMEOUT_MS: u64 = 120_000;
const UPDATE_FIRST_CHECK_MS: u64 = 60_000;
const UPDATE_CHECK_PERIOD_MS: u64 = 6 * 3_600_000;
const UPDATE_IDLE_RETRY_MS: u64 = 10 * 60_000;
const LUX_PERIOD_MS: u64 = 2000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitReason {
    Signal,
    Restart,
    ZigbeeRoleChanged,
    Updated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tk {
    UiAfterNotSubscribed,
    UiAfterError,
    Watchdog,
    FollowupWait,
    TimerTick,
    RingTick,
    UpdateCheck,
    UpdateRetry,
    UpdateExit,
}

#[derive(Clone, Debug, Default)]
struct WakeRecord {
    id: String,
    stt: Option<String>,
    cancelled: bool,
    error: bool,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct DialogId(u64);

impl DialogId {
    fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct AnnounceId(u64);

impl AnnounceId {
    fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct EarconSeq(u64);

impl EarconSeq {
    fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

#[derive(Clone, Copy, Debug)]
enum Pending {
    Tts { dialog: DialogId },
    Earcon { seq: EarconSeq },
    Announce { conv: bool, seq: AnnounceId },
    MediaAnnounce { seq: AnnounceId },
    Ring,
}

impl Pending {
    fn playback_kind(self) -> pulsar_metrics::PlaybackKind {
        use pulsar_metrics::PlaybackKind;
        match self {
            Self::Tts { .. } => PlaybackKind::Tts,
            Self::Announce { .. } => PlaybackKind::Announce,
            Self::MediaAnnounce { .. } => PlaybackKind::MediaAnnounce,
            Self::Earcon { .. } => PlaybackKind::Earcon,
            Self::Ring => PlaybackKind::Ring,
        }
    }
}

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
    call: Option<UnboundedReceiver<CallEvent>>,
}

async fn recv<T>(rx: &mut Option<UnboundedReceiver<T>>) -> Option<T> {
    match rx {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

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
    Call(Option<CallEvent>),
    Lux(Option<f32>),
}

#[derive(Clone, Copy, Debug)]
struct RunMetric {
    started_ms: i64,
    wake: bool,
    answer_started: bool,
}

impl RunMetric {
    fn answer_latency(&mut self, now_ms: i64) -> Option<f64> {
        if !self.wake || self.answer_started {
            return None;
        }
        self.answer_started = true;
        Some((now_ms - self.started_ms) as f64 / 1000.0)
    }
}

pub struct Controller {
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
    pending: HashMap<PlayId, Pending>,
    exit: Option<ExitReason>,
    applied_tz: String,

    session: Session,
    wake: Arbitration,
    hw_mic_muted: bool,
    local_endpointing: bool,
    error_shown: bool,
    pending_check: Option<pulsar_update::RequestId>,
    install_id: Option<pulsar_update::RequestId>,
    install_version: String,
    sendspin_pairing_prompt: bool,
    run_metric: Option<RunMetric>,
    run_result: Option<pulsar_metrics::PipelineResult>,
    call_started_ms: i64,
    call_active: bool,
    call_paused_music: bool,
}

fn local_endpointing_enabled() -> bool {
    matches!(
        std::env::var("PULSAR_LOCAL_ENDPOINTING").as_deref(),
        Ok("1")
    )
}

fn clock_face(name: &str) -> pulsar_clock::ClockFace {
    name.parse().unwrap_or(pulsar_clock::ClockFace::None)
}

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

fn light_rgb(r: f32, g: f32, b: f32) -> Rgb {
    Rgb {
        r: (r * 255.0) as u8,
        g: (g * 255.0) as u8,
        b: (b * 255.0) as u8,
    }
}

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
    pub fn start(
        model: &'static DeviceProfile,
        state_path: std::path::PathBuf,
    ) -> Result<Self, wiring::StartError> {
        let wired = wiring::wire(model, state_path)?;
        Ok(Self::new(wired))
    }

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
            session: Session::default(),
            wake: Arbitration::default(),
            run_metric: None,
            run_result: None,
            call_started_ms: 0,
            hw_mic_muted,
            local_endpointing: local_endpointing_enabled(),
            error_shown: false,
            pending_check: None,
            install_id: None,
            install_version: String::new(),
            sendspin_pairing_prompt: false,
            call_active: false,
            call_paused_music: false,
        };
        c.on_start(persist_on_start);
        c
    }

    fn on_start(&mut self, persist_on_start: bool) {
        self.publish_update();
        self.timers.set_repeating(
            Tk::UpdateCheck,
            UPDATE_FIRST_CHECK_MS,
            UPDATE_CHECK_PERIOD_MS,
        );
        self.update_detection();
        self.apply_gains();
        self.update_ui();
        self.deps.display.clock.play(ClockShot::Boot);
        if persist_on_start {
            self.persist();
        }
    }

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
                Trigger::Call(ev) => self.on_call_event(ev),
                Trigger::Lux(ev) => self.on_lux_event(ev),
            }
        };
        self.settle_interrupted_update();
        reason
    }

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
            ev = recv(&mut self.rx.call) => Trigger::Call(ev),
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
            }) => self.on_arbiter_decided(claim_id.into(), won, &best, peers),
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
            Tk::Watchdog => {
                self.run_result = Some(pulsar_metrics::PipelineResult::Timeout);
                self.abort_dialog("no progress from Home Assistant, giving up");
            }
            Tk::FollowupWait => {
                info!("follow-up: no speech started, conversation ends");
                self.finish_interaction();
            }
            Tk::TimerTick => self.show_timers(),
            Tk::RingTick => self.ring_timer(),
            Tk::UpdateCheck => self.check_update(),
            Tk::UpdateRetry => self.maybe_auto_update(),
            Tk::UpdateExit => self.exit = Some(ExitReason::Updated),
        }
    }

    fn on_player(&mut self, ev: PlayerEvent) {
        match ev {
            PlayerEvent::State(ch, s) => {
                if ch == Channel::Music {
                    self.session.on_music_state(s, now_ms());
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
            FrontendEvent::Wake { .. } if self.call_active => {
                self.deps.frontend.wake_handled();
            }
            FrontendEvent::Wake {
                phrase,
                level_db,
                preroll,
                endpoint_prior,
                event_id,
            } => {
                self.on_wake(&phrase, level_db, preroll, endpoint_prior, event_id);
                self.deps.frontend.wake_handled();
            }
            FrontendEvent::Command { word } => self.on_command(&word),
            FrontendEvent::Audio(pcm) => {
                if self.call_active {
                    self.deps.call.push_uplink(&pcm);
                } else if self.session.is_listening() {
                    self.on_listening_audio(pcm);
                } else {
                    self.wake.push_audio(&pcm);
                }
            }
            FrontendEvent::Music(f) => {
                self.deps.display.leds.music_frame(f, false);
                self.deps.display.clock.music_frame(f, false);
            }
        }
    }

    fn persist(&mut self) {
        self.st.volume_legacy = pulsar_core::volume::to_legacy(&self.model.volume, self.st.volume);
        self.st.changed();
    }

    #[must_use = "a write that must be on disk has to check it"]
    fn persist_now(&mut self) -> bool {
        self.st.volume_legacy = pulsar_core::volume::to_legacy(&self.model.volume, self.st.volume);
        self.st.write_now()
    }

    pub fn flush_settings(&mut self) {
        self.st.flush();
    }

    pub fn promise_sendspin_restart(&self) {
        if let Some(ss) = &self.deps.sendspin {
            ss.goodbye_restart();
        }
    }

    fn mic_muted(&self) -> bool {
        self.st.mic_muted || self.hw_mic_muted
    }

    fn phase(&self) -> logic::Phase {
        if self.call_active {
            logic::Phase::Call
        } else {
            self.session.phase()
        }
    }

    fn sendspin_streaming(&self) -> bool {
        self.deps.sendspin.as_ref().is_some_and(|ss| ss.streaming())
    }

    fn zigbee_flashing(&self) -> bool {
        self.deps.radio.as_ref().is_some_and(|z| z.flashing())
    }

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
        pulsar_metrics::set_mute(self.hw_mic_muted, self.st.mic_muted);
        pulsar_metrics::set_volume(f64::from(self.st.volume));
        self.deps.display.leds.set_muted(self.mic_muted());
        self.update_commands();
        if self.call_active && self.deps.call.muted() != self.mic_muted() {
            self.deps.call.set_muted(self.mic_muted());
        }
        let s = if self.timers_ha.ringing() {
            UiState::TimerRing
        } else {
            match self.phase() {
                logic::Phase::Listening => UiState::Listening,
                logic::Phase::Processing => UiState::Thinking,
                logic::Phase::Speaking => UiState::Speaking,
                logic::Phase::Call => UiState::Call,
                logic::Phase::Idle if self.wake.is_claiming() => UiState::Listening,
                logic::Phase::Idle if self.session.announcing() => UiState::Speaking,
                logic::Phase::Idle => UiState::Idle,
            }
        };
        self.show_ui(s);
    }

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
