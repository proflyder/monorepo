//! Сценарные тесты контроллера на фейковых портах: настоящие `Controller`, `Store` и `select`
//! не участвуют — только домены и вызовы портов.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use pulsar_arbiter::ClaimId;
use pulsar_audio::Outcome;
use pulsar_clock::{ClockFace, ClockMusicStyle};
use pulsar_core::{MusicFrame, UiState};
use pulsar_esphome::{
    AnnounceRequest, ApiEvent, BinarySensorId, ButtonId, EventId, LightState, MediaState, NumberId,
    SelectId, SensorId, SwitchId, TextId, TextSensorId, TimerEvent, TimerInfo, UpdateState,
};
use pulsar_hw::{CaseColor, Effect, MusicStyle, Rgb, VoiceColor};
use pulsar_media::{Channel, PlayId, PlayState};
use pulsar_sendspin::Transport;
use pulsar_update::{Release, RequestId};
use tokio::sync::mpsc::unbounded_channel;

use super::ports::{
    Arbiter, Clock, Display, Frontend, HaLink, Leds, Mdns, Mixer, Player, Radio, Sendspin, Updater,
};
use super::{Controller, Pending, Tk, wiring};
use crate::settings::Store;

/// One ordered log of every port call.
#[derive(Default)]
struct Rec {
    calls: RefCell<Vec<String>>,
}

impl Rec {
    fn push(&self, call: String) {
        self.calls.borrow_mut().push(call);
    }

    fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.calls.borrow_mut())
    }
}

struct FakePlayer {
    rec: Rc<Rec>,
    music: Rc<Cell<PlayState>>,
    next: Cell<u64>,
}

impl Player for FakePlayer {
    fn play(&self, ch: Channel, urls: Vec<String>) -> PlayId {
        let id = PlayId(self.next.get() + 1);
        self.next.set(id.0);
        self.rec.push(format!("player.play {ch:?} {urls:?}"));
        id
    }

    fn pause(&self, ch: Channel) {
        self.rec.push(format!("player.pause {ch:?}"));
    }

    fn resume(&self, ch: Channel) {
        self.rec.push(format!("player.resume {ch:?}"));
    }

    fn stop(&self, ch: Channel) {
        self.rec.push(format!("player.stop {ch:?}"));
    }

    fn state(&self, ch: Channel) -> PlayState {
        if ch == Channel::Music {
            self.music.get()
        } else {
            PlayState::Idle
        }
    }

    fn set_channel_gain(&self, ch: Channel, gain: f32) {
        self.rec.push(format!("player.gain {ch:?} {gain}"));
    }
}

struct FakeMixer {
    rec: Rc<Rec>,
}

impl Mixer for FakeMixer {
    fn available(&self) -> bool {
        true
    }

    fn volume(&self) -> f32 {
        0.5
    }

    fn set_volume(&mut self, v: f32) {
        self.rec.push(format!("mixer.volume {v}"));
    }

    fn set_muted(&mut self, m: bool) {
        self.rec.push(format!("mixer.muted {m}"));
    }
}

struct FakeHa {
    rec: Rc<Rec>,
    subscribed: Rc<Cell<bool>>,
    run: Cell<u64>,
}

impl HaLink for FakeHa {
    fn voice_subscribed(&self) -> bool {
        self.subscribed.get()
    }

    fn voice_request_start(&self, phrase: &str) -> u64 {
        let run = self.run.get() + 1;
        self.run.set(run);
        self.rec.push(format!("ha.voice_start {run} '{phrase}'"));
        run
    }

    fn voice_request_stop(&self) {
        self.rec.push("ha.voice_stop".into());
    }

    fn voice_send_audio(&self, pcm: Vec<i16>) {
        self.rec.push(format!("ha.audio {}", pcm.len()));
    }

    fn voice_send_audio_end(&self) {
        self.rec.push("ha.audio_end".into());
    }

    fn voice_announce_finished(&self, success: bool) {
        self.rec.push(format!("ha.announce_finished {success}"));
    }

    fn voice_answer_finished(&self, success: bool) {
        self.rec.push(format!("ha.answer_finished {success}"));
    }

    fn set_active_wake_words(&self, ids: Vec<String>) {
        self.rec.push(format!("ha.active_wake_words {ids:?}"));
    }

    fn mdns_txt(&self) -> Vec<String> {
        Vec::new()
    }

    fn publish_media(&self, st: MediaState, volume: f32, muted: bool) {
        self.rec
            .push(format!("publish_media {st:?} {volume} {muted}"));
    }

    fn publish_switch(&self, id: SwitchId, on: bool) {
        self.rec.push(format!("publish_switch {id:?} {on}"));
    }

    fn publish_binary_sensor(&self, id: BinarySensorId, on: bool) {
        self.rec.push(format!("publish_binary {id:?} {on}"));
    }

    fn publish_sensor(&self, id: SensorId, v: f32) {
        self.rec.push(format!("publish_sensor {id:?} {v}"));
    }

    fn publish_number(&self, id: NumberId, v: f32) {
        self.rec.push(format!("publish_number {id:?} {v}"));
    }

    fn publish_text(&self, id: TextId, s: &str) {
        self.rec.push(format!("publish_text {id:?} '{s}'"));
    }

    fn publish_select(&self, id: SelectId, s: &str) {
        self.rec.push(format!("publish_select {id:?} '{s}'"));
    }

    fn publish_text_sensor(&self, id: TextSensorId, s: &str) {
        self.rec.push(format!("publish_text_sensor {id:?} '{s}'"));
    }

    fn publish_light(&self, st: &LightState) {
        self.rec.push(format!("publish_light {st:?}"));
    }

    fn fire_event(&self, id: EventId, event_type: &str) {
        self.rec.push(format!("fire_event {id:?} '{event_type}'"));
    }

    fn publish_update(&self, st: &UpdateState) {
        self.rec
            .push(format!("publish_update {}", st.latest_version));
    }
}

struct FakeFrontend {
    rec: Rc<Rec>,
}

impl Frontend for FakeFrontend {
    fn set_model(&self, dir: PathBuf) {
        self.rec.push(format!("frontend.model {}", dir.display()));
    }

    fn set_detection(&self, on: bool) {
        self.rec.push(format!("frontend.detection {on}"));
    }

    fn set_custom_wake(&self, on: bool) {
        self.rec.push(format!("frontend.custom_wake {on}"));
    }

    fn set_commands(&self, on: bool) {
        self.rec.push(format!("frontend.commands {on}"));
    }

    fn set_streaming(&self, on: bool) {
        self.rec.push(format!("frontend.streaming {on}"));
    }

    fn wake_handled(&self) {
        self.rec.push("frontend.wake_handled".into());
    }

    fn set_preroll(&self, on: bool) {
        self.rec.push(format!("frontend.preroll {on}"));
    }

    fn set_music_analysis(&self, on: bool) {
        self.rec.push(format!("frontend.music_analysis {on}"));
    }

    fn set_wake_recording(&self, on: bool) {
        self.rec.push(format!("frontend.wake_recording {on}"));
    }

    fn set_wake_dry_run(&self, on: bool) {
        self.rec.push(format!("frontend.wake_dry_run {on}"));
    }

    fn wake_missed(&self) {
        self.rec.push("frontend.wake_missed".into());
    }

    fn wake_event_outcome(&self, id: &str, outcome: Outcome, stt_text: Option<&str>) {
        self.rec
            .push(format!("frontend.outcome {id} {outcome:?} {stt_text:?}"));
    }

    fn wake_event_false_alarm(&self) {
        self.rec.push("frontend.false_alarm".into());
    }
}

struct FakeArbiter;

impl Arbiter for FakeArbiter {
    fn ok(&self) -> bool {
        false
    }

    fn claim(&self, _wake: &str, _level_db: f32, _busy: bool, _decide: bool) -> ClaimId {
        unreachable!("the fake arbiter never arbitrates")
    }

    fn cancel(&self) {}
}

struct FakeLeds {
    rec: Rc<Rec>,
}

impl Leds for FakeLeds {
    fn set_ui_state(&self, s: UiState) {
        self.rec.push(format!("leds.ui {s:?}"));
    }

    fn set_muted(&self, muted: bool) {
        self.rec.push(format!("leds.muted {muted}"));
    }

    fn set_user_light(&self, on: bool, color: Rgb, brightness: f32, effect: Effect, ms: u32) {
        self.rec.push(format!(
            "leds.light {on} {color:?} {brightness} {effect:?} {ms}"
        ));
    }

    fn show_volume(&self, v: f32) {
        self.rec.push(format!("leds.volume {v}"));
    }

    fn set_ambient_lux(&self, lux: f32) {
        self.rec.push(format!("leds.lux {lux}"));
    }

    fn show_timer_started(&self) {
        self.rec.push("leds.timer_started".into());
    }

    fn show_temporary(&self, effect: Effect, color: Rgb, seconds: i32) {
        self.rec
            .push(format!("leds.temporary {effect:?} {color:?} {seconds}"));
    }

    fn clear_temporary(&self) {
        self.rec.push("leds.clear_temporary".into());
    }

    fn set_voice_indication(&self, on: bool) {
        self.rec.push(format!("leds.voice_indication {on}"));
    }

    fn set_voice_color(&self, color: VoiceColor) {
        self.rec.push(format!("leds.voice_color {color:?}"));
    }

    fn set_speed(&self, k: f32) {
        self.rec.push(format!("leds.speed {k}"));
    }

    fn set_music_style(&self, style: MusicStyle) {
        self.rec.push(format!("leds.music_style {style:?}"));
    }

    fn set_media_playing(&self, playing: bool) {
        self.rec.push(format!("leds.media {playing}"));
    }

    fn music_frame(&self, _f: MusicFrame, from_sendspin: bool) {
        self.rec.push(format!("leds.music_frame {from_sendspin}"));
    }

    fn set_case_color(&self, color: CaseColor) {
        self.rec.push(format!("leds.case_color {color:?}"));
    }
}

struct FakeClock {
    rec: Rc<Rec>,
}

impl Clock for FakeClock {
    fn set_enabled(&self, on: bool) {
        self.rec.push(format!("clock.enabled {on}"));
    }

    fn set_brightness(&self, b: f32) {
        self.rec.push(format!("clock.brightness {b}"));
    }

    fn set_ambient_lux(&self, lux: f32) {
        self.rec.push(format!("clock.lux {lux}"));
    }

    fn set_auto_brightness(&self, on: bool) {
        self.rec.push(format!("clock.auto_brightness {on}"));
    }

    fn set_timer(&self, ms_left: i64, running: bool) {
        self.rec.push(format!("clock.timer {ms_left} {running}"));
    }

    fn flash_number(&self, value: i32) {
        self.rec.push(format!("clock.flash {value}"));
    }

    fn show_code(&self, digits: String) {
        self.rec.push(format!("clock.code '{digits}'"));
    }

    fn clear_code(&self) {
        self.rec.push("clock.clear_code".into());
    }

    fn set_text(&self, utf8: String) {
        self.rec.push(format!("clock.text '{utf8}'"));
    }

    fn set_face(&self, face: ClockFace) {
        self.rec.push(format!("clock.face {face:?}"));
    }

    fn set_scroll_ms(&self, ms: i32) {
        self.rec.push(format!("clock.scroll {ms}"));
    }

    fn show_temporary_text(&self, utf8: String, seconds: i32) {
        self.rec.push(format!("clock.tmp_text '{utf8}' {seconds}"));
    }

    fn show_temporary_face(&self, face: ClockFace, seconds: i32) {
        self.rec.push(format!("clock.tmp_face {face:?} {seconds}"));
    }

    fn set_ui_state(&self, s: UiState) {
        self.rec.push(format!("clock.ui {s:?}"));
    }

    fn set_voice_indication(&self, on: bool) {
        self.rec.push(format!("clock.voice_indication {on}"));
    }

    fn play(&self, s: pulsar_core::ClockShot) {
        self.rec.push(format!("clock.play {s:?}"));
    }

    fn set_updating(&self, on: bool) {
        self.rec.push(format!("clock.updating {on}"));
    }

    fn set_music_style(&self, style: ClockMusicStyle) {
        self.rec.push(format!("clock.music_style {style:?}"));
    }

    fn set_media_playing(&self, playing: bool) {
        self.rec.push(format!("clock.media {playing}"));
    }

    fn music_frame(&self, _f: MusicFrame, from_sendspin: bool) {
        self.rec.push(format!("clock.music_frame {from_sendspin}"));
    }
}

struct FakeUpdater {
    rec: Rc<Rec>,
}

impl Updater for FakeUpdater {
    fn busy(&self) -> bool {
        false
    }

    fn check(&self) -> Option<RequestId> {
        None
    }

    fn install(&self, release: Release, binary: PathBuf) -> Option<RequestId> {
        self.rec.push(format!(
            "updater.install {} {}",
            release.version(),
            binary.display()
        ));
        None
    }

    fn add_extra_asset(&self, name: &str, path: PathBuf) {
        self.rec
            .push(format!("updater.asset {name} {}", path.display()));
    }
}

struct FakeSendspin {
    rec: Rc<Rec>,
    streaming: Rc<Cell<bool>>,
    connected: Rc<Cell<bool>>,
}

impl Sendspin for FakeSendspin {
    fn streaming(&self) -> bool {
        self.streaming.get()
    }

    fn connected(&self) -> bool {
        self.connected.get()
    }

    fn set_duck_gain(&self, g: f32) {
        self.rec.push(format!("sendspin.gain {g}"));
    }

    fn report_volume(&self, volume: f32, muted: bool) {
        self.rec.push(format!("sendspin.volume {volume} {muted}"));
    }

    fn transport(&self, t: Transport) -> bool {
        self.rec.push(format!("sendspin.transport {t:?}"));
        true
    }

    fn yield_to_local(&self) {
        self.rec.push("sendspin.yield".into());
    }

    fn confirm_pairing_window(&self) {
        self.rec.push("sendspin.pairing".into());
    }

    fn goodbye_restart(&self) {
        self.rec.push("sendspin.goodbye".into());
    }
}

struct FakeMdns;

impl Mdns for FakeMdns {
    fn set_txt(&self, _service: &str, _txt: Vec<String>) {}
}

struct FakeRadio;

impl Radio for FakeRadio {
    fn mode(&self) -> pulsar_zigbee::Mode {
        pulsar_zigbee::Mode::Off
    }

    fn flashing(&self) -> bool {
        false
    }

    fn status(&self) -> String {
        "router: joined 0x0001".into()
    }

    fn join(&self) {}

    fn leave(&self) {}
}

/// Everything a test needs to inspect.
struct Fixture {
    c: Controller,
    rec: Rc<Rec>,
    dir: PathBuf,
    music: Rc<Cell<PlayState>>,
    streaming: Rc<Cell<bool>>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Fixture {
    fn path(&self) -> PathBuf {
        self.dir.join("state.conf")
    }

    fn calls(&self) -> Vec<String> {
        self.rec.take()
    }
}

fn controller() -> Fixture {
    build(None)
}

fn controller_with_radio() -> Fixture {
    build(Some(Box::new(FakeRadio)))
}

fn build(radio: Option<Box<dyn Radio>>) -> Fixture {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "pulsar-controller-{}-{}",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let rec = Rc::new(Rec::default());
    let music = Rc::new(Cell::new(PlayState::Idle));
    let streaming = Rc::new(Cell::new(false));
    let connected = Rc::new(Cell::new(false));
    let subscribed = Rc::new(Cell::new(false));
    let model = pulsar_model::profile_from_compatible(b"yandex,cucumber\0").unwrap();
    let store = Store::load(dir.join("state.conf"));
    let wired = wiring::Wired {
        deps: super::ports::Deps {
            sendspin: Some(Box::new(FakeSendspin {
                rec: rec.clone(),
                streaming: streaming.clone(),
                connected,
            })),
            radio,
            updater: Box::new(FakeUpdater { rec: rec.clone() }),
            arbiter: Box::new(FakeArbiter),
            _input: None,
            display: Display {
                clock: Box::new(FakeClock { rec: rec.clone() }),
                leds: Box::new(FakeLeds { rec: rec.clone() }),
            },
            mixer: Box::new(FakeMixer { rec: rec.clone() }),
            player: Box::new(FakePlayer {
                rec: rec.clone(),
                music: music.clone(),
                next: Cell::new(0),
            }),
            frontend: Box::new(FakeFrontend { rec: rec.clone() }),
            mdns: Some(Box::new(FakeMdns)),
            ha: Box::new(FakeHa {
                rec: rec.clone(),
                subscribed,
                run: Cell::new(0),
            }),
        },
        rx: super::Rx {
            api: Some(unbounded_channel().1),
            frontend: Some(unbounded_channel().1),
            player: Some(unbounded_channel().1),
            arbiter: Some(unbounded_channel().1),
            input: None,
            lux: None,
            sendspin: Some(unbounded_channel().1),
            zigbee: None,
            updater: Some(unbounded_channel().1),
        },
        term: tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap(),
        int: tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()).unwrap(),
        model,
        store,
        media: super::media::Policy::new(&model.volume),
        applied_tz: String::new(),
        hw_mic_muted: false,
        persist_on_start: false,
    };
    let c = Controller::new(wired);
    Fixture {
        c,
        rec,
        dir,
        music,
        streaming,
    }
}

#[tokio::test]
async fn mute_zeroes_the_gains_and_is_written_after_a_second() {
    let mut f = controller();
    f.calls();
    f.c.set_muted(true);
    assert!(f.c.st.muted);
    let calls = f.calls();
    assert!(calls.contains(&"mixer.muted true".to_owned()), "{calls:?}");
    assert!(
        calls.contains(&"player.gain Music 0".to_owned()),
        "{calls:?}"
    );
    assert!(
        calls.contains(&"player.gain Announce 0".to_owned()),
        "{calls:?}"
    );
    assert!(calls.contains(&"sendspin.gain 0".to_owned()), "{calls:?}");
    // Deferred: nothing on disk before the delay.
    assert!(!f.path().exists());
    f.c.flush_settings();
    let text = std::fs::read_to_string(f.path()).unwrap();
    assert!(text.contains("\nmuted=1\n"), "{text}");
    assert!(text.starts_with("volume2=0.5\n"), "{text}");
}

#[tokio::test]
async fn volume_feedback_unmutes_and_shows_the_rounded_level() {
    let mut f = controller();
    f.c.st.muted = true;
    f.calls();
    f.c.set_volume(0.4, true);
    assert!(!f.c.st.muted);
    let calls = f.calls();
    assert!(calls.contains(&"mixer.muted false".to_owned()), "{calls:?}");
    assert!(calls.contains(&"mixer.volume 0.4".to_owned()), "{calls:?}");
    assert!(calls.contains(&"leds.volume 0.4".to_owned()), "{calls:?}");
    assert!(calls.contains(&"clock.flash 40".to_owned()), "{calls:?}");
    // A click plays the volume earcon on ANNOUNCE.
    assert!(
        calls
            .iter()
            .any(|c| c.starts_with("player.play Announce") && c.contains("volume")),
        "{calls:?}"
    );
}

#[tokio::test]
async fn announce_ducks_music_and_its_end_restores_the_gain() {
    let mut f = controller();
    f.music.set(PlayState::Playing);
    f.calls();
    f.c.on_api(ApiEvent::Announce(AnnounceRequest {
        media_id: "/a.mp3".into(),
        text: "hello".into(),
        preannounce_media_id: String::new(),
        start_conversation: false,
    }));
    assert!(f.c.announcing);
    let calls = f.calls();
    // Midi: hardware volume, so the master gain is 1 and the duck is exactly DUCK_GAIN.
    assert!(
        calls.contains(&"player.gain Music 0.15".to_owned()),
        "{calls:?}"
    );
    let seq = f.c.announce_seq;
    f.c.play_done(Pending::Announce { conv: false, seq }, true);
    assert!(!f.c.announcing);
    let calls = f.calls();
    assert!(
        calls.contains(&"player.gain Music 1".to_owned()),
        "{calls:?}"
    );
}

#[tokio::test]
async fn client_connected_publishes_entities_in_the_cpp_order() {
    let mut f = controller();
    f.calls();
    f.c.on_api(ApiEvent::ClientConnected {
        client_info: "test".into(),
    });
    let calls: Vec<String> = f
        .calls()
        .into_iter()
        .filter(|c| c.starts_with("publish_"))
        .collect();
    let head = &calls[..calls
        .iter()
        .position(|c| c.starts_with("publish_light"))
        .expect("light published")];
    assert_eq!(
        &head[..22],
        &[
            "publish_media Idle 0.5 false",
            "publish_switch Microphones true",
            "publish_switch Clock true",
            "publish_switch ClockAutoBrightness true",
            "publish_switch AutoUpdate false",
            "publish_switch NearestAnswers false",
            "publish_switch VoiceIndication true",
            "publish_switch WakePreroll true",
            "publish_switch WakeEventRecording true",
            "publish_switch WakeDryRun false",
            "publish_update unknown",
            "publish_binary HwMicMute false",
            "publish_number ClockBrightness 100",
            "publish_number ClockScroll 400",
            "publish_text ClockText ''",
            "publish_select ClockFace 'None'",
            "publish_number LedSpeed 1",
            "publish_number FollowupDuration 10",
            "publish_select MusicViz 'All styles'",
            "publish_select ClockMusic 'Off'",
            "publish_select CaseColor 'None'",
            "publish_select VoiceColor 'Violet'",
        ]
    );
    // No Zigbee service: neither entity is published.
    assert!(!calls.iter().any(|c| c.contains("Zigbee")), "{calls:?}");
}

#[tokio::test]
async fn client_connected_publishes_the_live_zigbee_status() {
    let mut f = controller_with_radio();
    assert!(f.c.entity.zigbee_status.is_empty());
    f.calls();
    f.c.on_api(ApiEvent::ClientConnected {
        client_info: "test".into(),
    });
    let calls = f.calls();
    assert!(
        calls.contains(&"publish_select ZigbeeMode 'Off'".to_owned()),
        "{calls:?}"
    );
    // HEAD published `z.status().to_string()` here, not the Changed-dedup cache.
    assert!(
        calls.contains(&"publish_text_sensor Zigbee 'router: joined 0x0001'".to_owned()),
        "{calls:?}"
    );
    assert!(f.c.entity.zigbee_status.is_empty());
}

#[tokio::test]
async fn ha_commands_act_on_ports_and_are_written_late() {
    let mut f = controller();
    f.calls();
    f.c.on_api(ApiEvent::Switch(SwitchId::Clock, false));
    f.c.on_api(ApiEvent::Number(NumberId::LedSpeed, 2.0));
    f.c.on_api(ApiEvent::Select(SelectId::MusicViz, "Disco".into()));
    f.c.on_api(ApiEvent::Button(ButtonId::WakeMissed));
    let calls = f.calls();
    for want in [
        "clock.enabled false",
        "publish_switch Clock false",
        "leds.speed 2",
        "publish_number LedSpeed 2",
        "leds.music_style Disco",
        "publish_select MusicViz 'Disco'",
        "frontend.wake_missed",
    ] {
        assert!(
            calls.iter().any(|c| c == want),
            "missing {want:?} in {calls:?}"
        );
    }
    assert_eq!(f.c.st.music_viz, "Disco");
    // All changes are one deferred write.
    assert!(!f.path().exists());
    f.c.flush_settings();
    let text = std::fs::read_to_string(f.path()).unwrap();
    assert!(text.contains("\nclock_on=0\n"), "{text}");
    assert!(text.contains("\nled_speed=2\n"), "{text}");
    assert!(text.contains("\nmusic_viz=Disco\n"), "{text}");
}

#[tokio::test]
async fn mask_and_privacy_switches_write_at_once() {
    let mut f = controller();
    f.calls();
    f.c.on_api(ApiEvent::Switch(SwitchId::Microphones, false));
    // Privacy: the mute is on disk immediately.
    assert!(f.path().exists());
    assert!(
        std::fs::read_to_string(f.path())
            .unwrap()
            .contains("\nmic_muted=1\n")
    );
    assert!(f.c.st.mic_muted);
    let calls = f.calls();
    assert!(
        calls.contains(&"publish_switch Microphones false".to_owned()),
        "{calls:?}"
    );
    assert!(!calls.contains(&"frontend.detection true".to_owned()));
    assert!(calls.contains(&"frontend.detection false".to_owned()));
}

#[tokio::test]
async fn ha_timer_shows_rings_and_yields_to_speech() {
    let mut f = controller();
    f.calls();
    f.c.on_api(ApiEvent::Timer(TimerInfo {
        event: TimerEvent::Started,
        id: "a".into(),
        name: "tea".into(),
        total_seconds: 10,
        seconds_left: 10,
        is_active: true,
    }));
    let calls = f.calls();
    assert!(
        calls.contains(&"leds.timer_started".to_owned()),
        "{calls:?}"
    );
    assert!(
        calls.contains(&"clock.timer 10000 true".to_owned()),
        "{calls:?}"
    );
    assert!(f.c.timers.is_set(Tk::TimerTick));

    f.c.on_api(ApiEvent::Timer(TimerInfo {
        event: TimerEvent::Finished,
        id: "a".into(),
        name: "tea".into(),
        total_seconds: 10,
        seconds_left: 0,
        is_active: false,
    }));
    assert!(f.c.timers_ha.ringing());
    let calls = f.calls();
    assert!(
        calls
            .iter()
            .any(|c| c.starts_with("player.play Announce") && c.contains("timer")),
        "{calls:?}"
    );
    // Speech takes the channel: the ring yields, then resumes after the answer.
    f.c.phase = super::logic::Phase::Speaking;
    f.c.ring_timer();
    assert!(!f.calls().iter().any(|c| c.starts_with("player.play")));
    f.c.phase = super::logic::Phase::Idle;
    f.c.resume_ring();
    assert!(
        f.calls()
            .iter()
            .any(|c| c.starts_with("player.play Announce") && c.contains("timer"))
    );
    // The wake word stops the ring.
    f.c.stop_timer_ring();
    assert!(!f.c.timers_ha.ringing());
}

#[tokio::test]
async fn owns_music_follows_sendspin_and_the_player() {
    let mut f = controller();
    // Music Assistant streams: "пауза" is this station's and goes to the Sendspin server.
    f.streaming.set(true);
    f.calls();
    f.c.on_command("пауза");
    let calls = f.calls();
    assert!(
        calls.contains(&"sendspin.transport Pause".to_owned()),
        "{calls:?}"
    );
    assert!(
        !calls.contains(&"player.pause Music".to_owned()),
        "{calls:?}"
    );
    // Nothing plays and no stream ended recently: the word is not this station's.
    f.streaming.set(false);
    f.calls();
    f.c.on_command("пауза");
    let calls = f.calls();
    assert!(!calls.iter().any(|c| c.starts_with("player.")), "{calls:?}");
    assert!(
        !calls.iter().any(|c| c.starts_with("sendspin.")),
        "{calls:?}"
    );
}
