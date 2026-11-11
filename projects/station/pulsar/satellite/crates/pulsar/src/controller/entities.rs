//! HA connection and entities, buttons, the light sensor, Zigbee and updates (controller.cpp
//! "HA connection", "entities", "updates"). `Publisher` keeps the entity-facing state and
//! publishes it; HA option texts are parsed here only.

use pulsar_clock::ClockMusicStyle;
use pulsar_core::now_ms;
use pulsar_esphome::{
    ActionId, ApiEvent, BinarySensorId, ButtonId, EventId, LightState, NumberId, SelectId,
    SensorId, SwitchId, TextId, TextSensorId, UpdateCommand, UpdateState,
};
use pulsar_hw::{Button, CaseColor, InputEvent, MusicStyle, Press, VoiceColor};
use pulsar_media::Channel;
use pulsar_update::{Release, UpdateEvent};
use pulsar_zigbee::{Mode, ZigbeeEvent};
use tracing::{error, info, warn};

use super::logic::{self, AutoUpdate, cxx_clamp, cxx_max, cxx_min};
use super::ports::HaLink;
use super::{
    Controller, ExitReason, Tk, UPDATE_IDLE_RETRY_MS, clock_face, light_effect, light_rgb,
    model_dir,
};
use crate::settings::Settings;
use crate::version::VERSION;

/// Entity-facing state that survives HA reconnects: the clock's last text/face, the last
/// published illuminance, the release and update progress, the Zigbee status line.
pub struct Publisher {
    pub(super) clock_text: String,
    pub(super) clock_face: String,
    pub(super) last_lux_published: f32,
    pub(super) last_lux_ms: i64,
    pub(super) release: Option<Release>,
    pub(super) update_installing: bool,
    pub(super) update_progress: f32,
    pub(super) zigbee_status: String,
}

impl Publisher {
    pub fn new() -> Self {
        Self {
            clock_text: String::new(),
            clock_face: "None".into(),
            last_lux_published: -1.0,
            last_lux_ms: 0,
            release: None,
            update_installing: false,
            update_progress: 0.0,
            zigbee_status: String::new(),
        }
    }

    /// "Microphones" is on while the speaker listens: neither the mute from HA nor the mute
    /// button's.
    pub fn publish_microphones(&self, ha: &dyn HaLink, muted: bool) {
        ha.publish_switch(SwitchId::Microphones, !muted);
    }

    pub fn publish_update(&self, ha: &dyn HaLink) {
        let release = self.release.as_ref();
        let version = release.map_or("", |r| r.version().as_str());
        // latest == current hides the offer: nothing newer, or not checked yet.
        let offer = !version.is_empty() && pulsar_update::release_newer(version, VERSION).0;
        ha.publish_update(&UpdateState {
            in_progress: self.update_installing,
            has_progress: self.update_installing,
            progress: self.update_progress,
            current_version: VERSION.into(),
            latest_version: if offer { version } else { VERSION }.into(),
            title: "pulsar".into(),
            release_url: release.map_or_else(String::new, |r| r.page_url().to_owned()),
        });
    }

    /// The C++ `on_client_connected` order: HA keeps the entity list per device, so every
    /// publish and its order is an external contract.
    pub fn on_client_connected(&self, ha: &dyn HaLink, s: &ClientSnapshot) {
        ha.publish_switch(SwitchId::Clock, s.st.clock_on);
        ha.publish_switch(SwitchId::ClockAutoBrightness, s.st.clock_auto_brightness);
        ha.publish_switch(SwitchId::AutoUpdate, s.st.auto_update);
        ha.publish_switch(SwitchId::NearestAnswers, s.st.nearest_answers);
        ha.publish_switch(SwitchId::VoiceIndication, s.st.voice_indication);
        ha.publish_switch(SwitchId::WakePreroll, s.st.wake_preroll);
        ha.publish_switch(SwitchId::WakeEventRecording, s.st.wake_recording);
        ha.publish_switch(SwitchId::WakeDryRun, s.st.wake_dry_run);
        self.publish_update(ha);
        ha.publish_binary_sensor(BinarySensorId::HwMicMute, s.hw_mic_muted);
        ha.publish_number(NumberId::ClockBrightness, s.st.clock_brightness * 100.0);
        ha.publish_number(NumberId::ClockScroll, s.st.clock_scroll_ms as f32);
        ha.publish_text(TextId::ClockText, &self.clock_text);
        ha.publish_select(SelectId::ClockFace, &self.clock_face);
        ha.publish_number(NumberId::LedSpeed, s.st.led_speed);
        ha.publish_number(NumberId::FollowupDuration, s.st.followup_duration_s as f32);
        ha.publish_number(NumberId::AnnounceVolume, s.st.announce_volume * 100.0);
        ha.publish_select(SelectId::MusicViz, &s.st.music_viz);
        ha.publish_select(SelectId::ClockMusic, &s.st.clock_music);
        ha.publish_select(SelectId::CaseColor, s.case_color);
        ha.publish_select(SelectId::VoiceColor, &s.st.voice_color);
        if let Some((role, status)) = s.zigbee {
            ha.publish_select(SelectId::ZigbeeMode, role);
            // The live status, not the "zigbee: …" dedup cache of Changed: its cache stays as
            // it is (HEAD published `z.status().to_string()` here).
            ha.publish_text_sensor(TextSensorId::Zigbee, status);
        }
        ha.publish_light(s.light);
        if self.last_lux_published >= 0.0 {
            ha.publish_sensor(SensorId::Illuminance, self.last_lux_published);
        }
    }
}

/// Everything the entity list needs from the controller at `ClientConnected`.
pub struct ClientSnapshot<'a> {
    pub st: &'a Settings,
    pub light: &'a LightState,
    pub case_color: &'a str,
    pub hw_mic_muted: bool,
    /// Present when the Zigbee service runs: its select option label and live status line.
    pub zigbee: Option<(&'a str, &'a str)>,
}

impl Controller {
    pub(super) fn on_api(&mut self, ev: ApiEvent) {
        match ev {
            ApiEvent::ClientConnected { client_info } => {
                pulsar_metrics::ha_connected(true);
                pulsar_metrics::ha_reconnect();
                self.on_client_connected(&client_info);
            }
            ApiEvent::ClientDisconnected => {
                pulsar_metrics::ha_connected(false);
                warn!("Home Assistant disconnected");
                self.voice_link_lost();
            }
            ApiEvent::VoiceSubscribed(s) => self.on_voice_subscribed(s),
            ApiEvent::VoiceEvent { event, data } => self.on_voice_event(event, &data),
            ApiEvent::Announce(req) => self.on_announce(req),
            ApiEvent::Timer(t) => self.on_timer_event(t),
            ApiEvent::ActiveWakeWords(ids) => self.on_active_wake_words(ids),
            ApiEvent::MediaCommand(cmd) => self.on_media_command(cmd),
            ApiEvent::Switch(id, on) => self.on_switch(id, on),
            ApiEvent::Button(id) => self.on_button(id),
            ApiEvent::Light {
                state,
                transition_ms,
            } => self.on_light(state, transition_ms),
            ApiEvent::Number(id, v) => self.on_number(id, v),
            ApiEvent::Text(id, s) => self.on_text(id, s),
            ApiEvent::Select(id, s) => self.on_select(id, s),
            ApiEvent::Action { id, value, seconds } => self.on_action(id, value, seconds),
            ApiEvent::LedShow {
                effect,
                color,
                seconds,
            } => self.on_led_show(&effect, &color, seconds),
            ApiEvent::Time { epoch, posix_tz } => self.on_time(epoch, &posix_tz),
            ApiEvent::Restart => self.on_restart(),
            ApiEvent::VoiceStartFailed { run } => self.on_voice_start_failed(run),
            ApiEvent::VoiceAudioOverflow { run } => self.on_voice_audio_overflow(run),
            ApiEvent::Update(cmd) => self.on_update(cmd),
            ApiEvent::MdnsTxtChanged(txt) => {
                if let Some(m) = &self.deps.mdns {
                    m.set_txt("_esphomelib._tcp", txt);
                }
            }
        }
    }

    fn on_client_connected(&mut self, client_info: &str) {
        info!("Home Assistant connected: {client_info}");
        self.error_shown = false;
        self.publish_media();
        self.publish_microphones();
        let light = self.light_state();
        let zigbee = self.deps.radio.as_ref().map(|z| {
            (
                pulsar_zigbee::role(super::zigbee_role(&self.st)).label,
                z.status(),
            )
        });
        let zigbee = zigbee
            .as_ref()
            .map(|(role, status)| (*role, status.as_str()));
        self.entity.on_client_connected(
            &*self.deps.ha,
            &ClientSnapshot {
                st: &self.st,
                light: &light,
                case_color: self.case_color_option(),
                hw_mic_muted: self.hw_mic_muted,
                zigbee,
            },
        );
    }

    fn case_color_option(&self) -> &str {
        if self.st.led_case_color.is_empty() {
            "None"
        } else {
            &self.st.led_case_color
        }
    }

    fn on_time(&mut self, epoch: u32, tz: &str) {
        if !tz.is_empty() {
            // Only on a change: setenv races with threads reading the environment (GLib).
            if tz != self.applied_tz {
                pulsar_core::tz::set_tz(tz);
                tz.clone_into(&mut self.applied_tz);
            }
            if tz != self.st.tz {
                tz.clone_into(&mut self.st.tz);
                self.persist();
            }
        }
        info!("time from HA: epoch={epoch} tz='{tz}' (local clock now follows TZ)");
    }

    fn on_restart(&mut self) {
        // A restart cuts the upload short (the shutdown watchdog does not wait ~25 s); the radio
        // would be written again on the next start, but stays without Zigbee meanwhile.
        if self.zigbee_flashing() {
            warn!(
                "restart requested from Home Assistant: refused while the Zigbee radio is being written"
            );
            return;
        }
        warn!("restart requested from Home Assistant");
        self.exit = Some(ExitReason::Restart);
    }

    fn on_switch(&mut self, id: SwitchId, state: bool) {
        match id {
            SwitchId::Microphones => {
                self.st.mic_muted =
                    logic::sw_mute_after_switch(state, self.st.mic_muted, self.hw_mic_muted);
                if state && self.hw_mic_muted {
                    // The button's latch cuts the microphones in hardware: there is nothing to
                    // switch on, and HA must not stay "on" after asking for it.
                    warn!(
                        "mic held by hardware switch: Home Assistant asked for microphones on, publishing off"
                    );
                }
                if !state {
                    self.cancel_arbitration();
                    if self.session.is_listening() {
                        self.mark_wake_cancelled();
                        self.finish_interaction();
                    }
                }
                self.update_detection();
                self.update_ui();
                // The actual state, not the command: "on" under the button mute comes back off.
                // Published even when the stored mute did not change, otherwise HA keeps showing
                // the state it asked for.
                self.publish_microphones();
                // Privacy: a mute must not be undone by a power cut right after it. A failed
                // write is logged and stays pending; the mute itself is in effect either way.
                let _ = self.persist_now();
                return;
            }
            SwitchId::Clock => {
                self.st.clock_on = state;
                self.deps.display.clock.set_enabled(state);
                self.update_music();
            }
            SwitchId::ClockAutoBrightness => {
                self.st.clock_auto_brightness = state;
                self.deps.display.clock.set_auto_brightness(state);
            }
            SwitchId::AutoUpdate => self.st.auto_update = state,
            SwitchId::NearestAnswers => self.st.nearest_answers = state,
            SwitchId::VoiceIndication => {
                self.st.voice_indication = state;
                self.deps.display.leds.set_voice_indication(state);
                self.deps.display.clock.set_voice_indication(state);
            }
            SwitchId::WakePreroll => {
                self.st.wake_preroll = state;
                self.deps.frontend.set_preroll(state);
            }
            SwitchId::WakeEventRecording => {
                self.st.wake_recording = state;
                self.deps.frontend.set_wake_recording(state);
                info!(
                    "wake events: recording {}",
                    if state { "on" } else { "off" }
                );
                // Privacy: the choice must survive a power cut right after it.
                let _ = self.persist_now();
                self.deps.ha.publish_switch(id, state);
                return;
            }
            SwitchId::WakeDryRun => {
                self.st.wake_dry_run = state;
                self.deps.frontend.set_wake_dry_run(state);
                info!(
                    "wake events: dry run {}",
                    if state {
                        "on, hits are recorded but not answered"
                    } else {
                        "off"
                    }
                );
                let _ = self.persist_now();
                self.deps.ha.publish_switch(id, state);
                return;
            }
        }
        self.deps.ha.publish_switch(id, state);
        self.persist();
        if id == SwitchId::AutoUpdate {
            self.maybe_auto_update();
        }
    }

    fn publish_microphones(&self) {
        self.entity
            .publish_microphones(&*self.deps.ha, self.mic_muted());
    }

    fn on_number(&mut self, id: NumberId, value: f32) {
        match id {
            NumberId::ClockBrightness => {
                self.st.clock_brightness = cxx_min(100.0, cxx_max(0.0, value)) / 100.0;
                self.deps
                    .display
                    .clock
                    .set_brightness(self.st.clock_brightness);
                self.deps
                    .ha
                    .publish_number(id, self.st.clock_brightness * 100.0);
            }
            NumberId::ClockScroll => {
                // C++ int(lround(value / 50)) * 50 (Rust saturates where C++ is undefined).
                let steps = (value / 50.0).round() as i64 as i32;
                self.st.clock_scroll_ms = steps.wrapping_mul(50).clamp(100, 2000);
                self.deps
                    .display
                    .clock
                    .set_scroll_ms(self.st.clock_scroll_ms);
                self.deps
                    .ha
                    .publish_number(id, self.st.clock_scroll_ms as f32);
            }
            NumberId::FollowupDuration => {
                let Some(s) = pulsar_esphome::followup_duration_s(value) else {
                    return;
                };
                self.st.followup_duration_s = s;
                self.deps.ha.publish_number(id, s as f32);
                // Ожидание открыто и до STT_START, и без таймера: проверяется по флагу, не по Tk.
                if s == 0 && self.session.is_listening() && self.session.followup_awaiting_speech()
                {
                    info!("follow-up: turned off, conversation ends");
                    self.finish_interaction();
                }
            }
            NumberId::LedSpeed => {
                self.st.led_speed = cxx_clamp(value, 0.25, 4.0);
                self.deps.display.leds.set_speed(self.st.led_speed);
                self.deps.ha.publish_number(id, self.st.led_speed);
            }
            NumberId::AnnounceVolume => {
                self.st.announce_volume = cxx_min(100.0, cxx_max(0.0, value)) / 100.0;
                if self.media.set_announce_volume(self.st.announce_volume) {
                    self.apply_gains();
                }
                self.deps
                    .ha
                    .publish_number(id, self.st.announce_volume * 100.0);
            }
        }
        self.persist();
    }

    fn on_text(&mut self, id: TextId, value: String) {
        self.deps.display.clock.set_text(value.clone());
        self.deps.ha.publish_text(id, &value);
        // Text and face replace each other on the display; the clock already dropped the face.
        let clears_face = !value.is_empty() && self.entity.clock_face != "None";
        self.entity.clock_text = value;
        if clears_face {
            self.entity.clock_face = "None".into();
            self.deps
                .ha
                .publish_select(SelectId::ClockFace, &self.entity.clock_face);
        }
    }

    fn on_select(&mut self, id: SelectId, option: String) {
        match id {
            SelectId::ZigbeeMode => {
                if self.deps.radio.is_none() {
                    return;
                }
                let role = pulsar_zigbee::ROLES
                    .iter()
                    .find(|r| r.label == option)
                    .map_or(super::zigbee_role(&self.st), |r| r.mode);
                if role == super::zigbee_role(&self.st) {
                    return self
                        .deps
                        .ha
                        .publish_select(id, pulsar_zigbee::role(role).label);
                }
                // The role starts with the daemon, which writes the radio firmware it needs; the
                // coordinator's mDNS advertisement is only made in that role.
                warn!(
                    "zigbee: role {} chosen, restarting",
                    pulsar_zigbee::role(role).name
                );
                pulsar_zigbee::role(role)
                    .name
                    .clone_into(&mut self.st.zigbee_role);
                self.st.zigbee_flash_failed = 0;
                // Before the restart: the next start acts on the role. A failed write stays
                // pending for the exit flush after run().
                let _ = self.persist_now();
                self.deps
                    .ha
                    .publish_select(id, pulsar_zigbee::role(role).label);
                self.exit = Some(ExitReason::ZigbeeRoleChanged);
            }
            SelectId::MusicViz => {
                let current = super::music_viz(&self.st);
                let style: MusicStyle = option.parse().unwrap_or(current);
                self.deps.display.leds.set_music_style(style);
                self.deps.ha.publish_select(id, &option);
                style.as_str().clone_into(&mut self.st.music_viz);
                self.persist();
                self.update_music();
            }
            SelectId::ClockMusic => {
                let current = super::clock_music(&self.st);
                let style: ClockMusicStyle = option.parse().unwrap_or(current);
                self.deps.display.clock.set_music_style(style);
                self.deps.ha.publish_select(id, &option);
                style.as_str().clone_into(&mut self.st.clock_music);
                self.persist();
                self.update_music();
            }
            SelectId::CaseColor => {
                let stem = if option == "None" { "" } else { &option };
                self.deps.display.leds.set_case_color(CaseColor::new(stem));
                stem.clone_into(&mut self.st.led_case_color);
                let shown = self.case_color_option().to_owned();
                self.deps.ha.publish_select(id, &shown);
                self.persist();
            }
            SelectId::ClockFace => {
                self.deps.display.clock.set_face(clock_face(&option));
                self.deps.ha.publish_select(id, &option);
                let clears_text = option != "None" && !self.entity.clock_text.is_empty();
                self.entity.clock_face = option;
                if clears_text {
                    self.entity.clock_text.clear();
                    self.deps
                        .ha
                        .publish_text(TextId::ClockText, &self.entity.clock_text);
                }
            }
            SelectId::VoiceColor => {
                let current = super::voice_color(&self.st);
                let color: VoiceColor = option.parse().unwrap_or(current);
                self.deps.display.leds.set_voice_color(color);
                self.deps.ha.publish_select(id, color.as_str());
                color.as_str().clone_into(&mut self.st.voice_color);
                self.persist();
            }
        }
    }

    fn on_button(&mut self, id: ButtonId) {
        match id {
            ButtonId::WakeFalseAlarm => {
                info!("wake events: «ложное срабатывание» pressed");
                return self.deps.frontend.wake_event_false_alarm();
            }
            ButtonId::WakeMissed => {
                info!("wake events: «не услышала» pressed");
                return self.deps.frontend.wake_missed();
            }
            ButtonId::ZigbeeJoin | ButtonId::ZigbeeLeave => {}
        }
        let Some(z) = &self.deps.radio else {
            return;
        };
        if z.mode() != Mode::Router {
            warn!("zigbee: join/leave only apply to the router role");
            return;
        }
        match id {
            ButtonId::ZigbeeJoin => z.join(),
            ButtonId::ZigbeeLeave => z.leave(),
            ButtonId::WakeFalseAlarm | ButtonId::WakeMissed => unreachable!("handled above"),
        }
    }

    fn on_action(&mut self, id: ActionId, value: String, seconds: i32) {
        let text = id == ActionId::ClockShowText;
        info!(
            "clock: temporary {} '{value}' for {seconds} s",
            if text { "text" } else { "face" }
        );
        if text {
            self.deps.display.clock.show_temporary_text(value, seconds);
        } else {
            self.deps
                .display
                .clock
                .show_temporary_face(clock_face(&value), seconds);
        }
    }

    fn on_led_show(&mut self, effect: &str, color: &str, seconds: i32) {
        if effect.is_empty() && color.is_empty() {
            return self.deps.display.leds.clear_temporary();
        }
        let Some(rgb) = logic::parse_led_color(color) else {
            warn!("led_show: bad colour '{color}'");
            return;
        };
        info!(
            "leds: show '{effect}' #{:02x}{:02x}{:02x}, {seconds} s requested",
            rgb.r, rgb.g, rgb.b
        );
        self.deps
            .display
            .leds
            .show_temporary(light_effect(effect), rgb, seconds);
    }

    fn on_light(&mut self, l: LightState, transition_ms: u32) {
        self.deps.display.leds.set_user_light(
            l.on,
            light_rgb(l.r, l.g, l.b),
            l.brightness,
            light_effect(&l.effect),
            transition_ms,
        );
        self.store_light(l);
        self.persist();
        self.update_music();
    }

    fn on_active_wake_words(&mut self, ids: Vec<String>) {
        let Some(first) = ids.into_iter().next() else {
            return;
        };
        if !matches!(first.as_str(), "alisa" | "yandex" | "ricardo") {
            return;
        }
        if first == self.st.wake_word {
            return;
        }
        self.st.wake_word = first;
        info!("wake word -> {}", self.st.wake_word);
        let custom = self.st.wake_word == "ricardo";
        self.deps.frontend.set_custom_wake(custom);
        if !custom {
            self.deps.frontend.set_model(model_dir(&self.st.wake_word));
        }
        self.deps
            .ha
            .set_active_wake_words(vec![self.st.wake_word.clone()]);
        self.persist();
    }

    // ---------------- buttons, sensors, Zigbee ----------------

    pub(super) fn on_input(&mut self, ev: InputEvent) {
        match ev {
            InputEvent::Mute(m) => {
                if m {
                    self.cancel_arbitration();
                }
                self.hw_mic_muted = m;
                info!("hardware mic mute: {}", u8::from(m));
                self.deps
                    .ha
                    .publish_binary_sensor(BinarySensorId::HwMicMute, m);
                let sw_muted = logic::sw_mute_after_button(self.st.mic_muted, m);
                if sw_muted != self.st.mic_muted {
                    info!("mute button lifted the mute from Home Assistant");
                    self.st.mic_muted = sw_muted;
                    // A lost write leaves the speaker muted after a restart (the safe side).
                    self.persist();
                }
                self.publish_microphones();
                if m && self.session.is_listening() {
                    self.mark_wake_cancelled();
                    self.finish_interaction();
                }
                self.update_detection();
                self.update_ui();
            }
            InputEvent::MuteToggle => {
                let muted = !self.mic_muted();
                info!(
                    "mic mute button: {}",
                    if muted { "muted" } else { "unmuted" }
                );
                self.st.mic_muted = muted;
                if muted {
                    self.cancel_arbitration();
                    if self.session.is_listening() {
                        self.mark_wake_cancelled();
                        self.finish_interaction();
                    }
                }
                self.update_detection();
                self.update_ui();
                self.publish_microphones();
                // A write cut short by a power loss must not unmute the station.
                let _ = self.persist_now();
            }
            InputEvent::Button(Button::Action, p) => {
                self.deps.ha.fire_event(
                    EventId::Button,
                    if p == Press::Long {
                        "action_long"
                    } else {
                        "action"
                    },
                );
                if p == Press::Long {
                    return; // exposed to HA automations only
                }
                // Во время звонка кнопка действия его сбрасывает (короткое нажатие).
                if self.call_active {
                    if self.call_phase() != pulsar_call::CallPhase::Idle {
                        self.deps.call.hangup();
                    }
                    return;
                }
                if self.sendspin_pairing_prompt
                    && let Some(ss) = &self.deps.sendspin
                {
                    return ss.confirm_pairing_window();
                }
                self.action_button();
            }
            InputEvent::Button(b, _) => {
                let up = b == Button::VolumeUp;
                self.deps.ha.fire_event(
                    EventId::Button,
                    if up { "volume_up" } else { "volume_down" },
                );
                self.set_volume(
                    pulsar_core::volume::step_level(&self.model.volume, self.st.volume, up),
                    true,
                );
            }
        }
    }

    pub(super) fn on_lux(&mut self, lux: f32) {
        self.deps.display.clock.set_ambient_lux(lux);
        self.deps.display.leds.set_ambient_lux(lux);
        let now = now_ms();
        if logic::publish_lux(
            lux,
            self.entity.last_lux_published,
            self.entity.last_lux_ms,
            now,
        ) {
            self.deps.ha.publish_sensor(SensorId::Illuminance, lux);
            self.entity.last_lux_published = lux;
            self.entity.last_lux_ms = now;
        }
    }

    /// The status is read only here and when a client connects (as the C++ hook and
    /// on_client_connected); right after join()/leave() it may still be the previous one.
    pub(super) fn on_zigbee(&mut self, ev: ZigbeeEvent) {
        match ev {
            ZigbeeEvent::Changed => {
                let Some(z) = &self.deps.radio else {
                    return;
                };
                let s = z.status();
                if s == self.entity.zigbee_status {
                    return;
                }
                info!("zigbee: {s}");
                self.deps.ha.publish_text_sensor(TextSensorId::Zigbee, &s);
                self.entity.zigbee_status = s;
            }
            ZigbeeEvent::FlashOutcome { failed_version } => {
                if failed_version == self.st.zigbee_flash_failed {
                    return;
                }
                self.st.zigbee_flash_failed = failed_version;
                // An image the radio refused is not written again after a crash or restart. A
                // failed write stays pending (logged).
                let _ = self.persist_now();
            }
        }
    }

    // ---------------- updates ----------------

    pub(super) fn publish_update(&self) {
        self.entity.publish_update(&*self.deps.ha);
    }

    pub(super) fn check_update(&mut self) {
        // None: a request is in flight (C++ Updater::check returned silently).
        if let Some(id) = self.deps.updater.check() {
            self.pending_check = Some(id);
        }
    }

    fn release_version(&self) -> &str {
        self.entity
            .release
            .as_ref()
            .map_or("", |r| r.version().as_str())
    }

    fn idle_for_update(&self) -> bool {
        logic::idle_for_update(
            self.session.phase(),
            self.session.announcing(),
            self.timers_ha.ringing(),
            self.deps.player.state(Channel::Music),
            self.sendspin_streaming(),
            self.zigbee_flashing(),
        )
    }

    pub(super) fn maybe_auto_update(&mut self) {
        self.timers.cancel(Tk::UpdateRetry);
        match logic::auto_update(
            self.st.auto_update,
            self.release_version(),
            self.entity.update_installing,
            VERSION,
            &self.st.update_attempt,
            self.idle_for_update(),
        ) {
            AutoUpdate::No => {}
            AutoUpdate::WaitIdle => self.timers.set(Tk::UpdateRetry, UPDATE_IDLE_RETRY_MS),
            AutoUpdate::Install => self.install_update("automatic"),
        }
    }

    fn install_update(&mut self, why: &str) {
        // A check not handled yet would still replace the release (C++ busy() until then).
        if self.deps.updater.busy() || self.entity.update_installing || self.pending_check.is_some()
        {
            return;
        }
        if self.zigbee_flashing() {
            // The install ends with a restart.
            warn!("update: not while the Zigbee radio is being written");
            return;
        }
        let Ok(exe) = std::fs::read_link("/proc/self/exe") else {
            error!("update: cannot find the running binary");
            return;
        };
        let binary = std::path::PathBuf::from(logic::strip_deleted(exe.as_os_str()));
        let Some(r) = self.entity.release.clone() else {
            return;
        };
        warn!("installing release {} ({why})", r.version());
        let previous =
            std::mem::replace(&mut self.st.update_attempt, r.version().as_str().to_owned());
        // On disk before the updater can replace the binary: still set under another version
        // after a restart, it tells that the release was rolled back. Without it a bad release
        // would carry no marker and automatic updates would install it again.
        if !self.persist_now() {
            warn!(
                "update to {}: update_attempt not written to state.conf, install refused",
                r.version()
            );
            self.st.update_attempt = previous;
            self.install_failed();
            return;
        }
        self.entity.update_installing = true;
        self.deps.display.clock.set_updating(true);
        self.entity.update_progress = 0.0;
        self.publish_update();
        r.version().as_str().clone_into(&mut self.install_version);
        if let Some(id) = self.deps.updater.install(r, binary) {
            self.install_id = Some(id);
        }
    }

    fn on_update(&mut self, cmd: UpdateCommand) {
        match cmd {
            UpdateCommand::Check => self.check_update(),
            UpdateCommand::Install => {
                let v = self.release_version();
                if v.is_empty() || v == VERSION {
                    warn!("update: no other release to install");
                    return;
                }
                self.install_update("requested from Home Assistant");
            }
            UpdateCommand::None => {}
        }
    }

    /// Every Checked / Installed is handled (the C++ callbacks carried no sequence number);
    /// the ids only tell when the request in flight has been answered.
    pub(super) fn on_updater(&mut self, ev: UpdateEvent) {
        match ev {
            UpdateEvent::Checked { id, result } => {
                if self.pending_check == Some(id) {
                    self.pending_check = None;
                }
                match result {
                    Err(e) => info!("update check: {e}"),
                    Ok(r) => {
                        if r.version().as_str() != self.release_version() {
                            info!("latest release {} (running {VERSION})", r.version());
                        }
                        self.entity.release = Some(r);
                        self.publish_update();
                        self.maybe_auto_update();
                    }
                }
            }
            UpdateEvent::Progress { value, .. } => {
                self.entity.update_progress = value;
                self.publish_update();
            }
            UpdateEvent::Installed { id, result } => {
                if self.install_id == Some(id) {
                    self.install_id = None;
                }
                let version = std::mem::take(&mut self.install_version);
                match result {
                    Ok(()) => {
                        warn!("installed {version}, restarting");
                        self.entity.update_progress = 100.0;
                        self.publish_update();
                        self.timers.set(Tk::UpdateExit, 500);
                    }
                    Err(err) => {
                        error!("update to {version} failed: {err}");
                        if !err.fatal() {
                            // A network failure: the next check tries again.
                            self.st.update_attempt.clear();
                            // Left set by a crash, it would make automatic updates skip it. A
                            // failed write stays pending (logged).
                            let _ = self.persist_now();
                        }
                        self.install_failed();
                    }
                }
            }
        }
    }

    /// An install that ended without a new binary, or never started: HA sees the update entity
    /// idle again, the clock shows the time.
    fn install_failed(&mut self) {
        self.entity.update_installing = false;
        self.deps.display.clock.set_updating(false);
        self.publish_update();
    }
}
