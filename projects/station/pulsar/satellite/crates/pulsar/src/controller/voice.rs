use std::collections::HashMap;

use pulsar_audio::endpoint::{EndpointPrior, EndpointRun, pcm_level_db};
use pulsar_core::{ClockShot, UiState, now_ms};
use pulsar_esphome::{AnnounceRequest, VaEvent};
use pulsar_media::{Channel, PlayState};
use pulsar_sendspin::Transport;
use tracing::{debug, info, warn};

use super::earcons;
use super::logic::{self, Phase, VoiceCommand};
use super::wake::{ClaimToken, Decision};
use super::{Controller, Pending, RunMetric, Tk, WakeRecord};

impl Controller {
    fn arbitration_on(&self) -> bool {
        self.st.nearest_answers && self.deps.arbiter.ok()
    }

    pub(super) fn play_announce(&mut self, urls: Vec<String>, done: Pending) {
        let speech = matches!(done, Pending::Tts { .. } | Pending::Announce { .. });
        self.set_announce_speech(speech);
        pulsar_metrics::playback_started(done.playback_kind());
        let id = self.deps.player.play(Channel::Announce, urls);
        self.pending.insert(id, done);
    }

    pub(super) fn play_done(&mut self, p: Pending, ok: bool) {
        pulsar_metrics::playback_finished(p.playback_kind(), ok);
        match p {
            Pending::Tts { dialog } => {
                if dialog == self.session.current_id() && self.session.is_speaking() {
                    self.tts_finished(ok);
                }
                self.resume_ring();
            }
            Pending::Earcon { seq } => {
                if !self.session.finish_earcon(seq) {
                    return;
                }
                self.update_duck();
            }
            Pending::Announce { conv, seq } => {
                self.deps.ha.voice_announce_finished(ok);
                if self.session.finish_announce(seq) {
                    if ok
                        && conv
                        && self.session.phase() == Phase::Idle
                        && !self.timers_ha.ringing()
                        && !self.mic_muted()
                    {
                        self.start_pipeline("", false, &[], &[], None);
                    }
                    self.end_announce();
                }
                self.resume_ring();
            }
            Pending::MediaAnnounce { seq } => {
                if self.session.finish_announce(seq) {
                    self.end_announce();
                }
                self.resume_ring();
            }
            Pending::Ring => {
                if self.timers_ha.ringing() && !self.timers.is_set(Tk::RingTick) {
                    self.timers.set(Tk::RingTick, 300);
                }
            }
        }
    }

    pub(super) fn on_voice_subscribed(&mut self, subscribed: bool) {
        info!(
            "voice assistant {}",
            if subscribed {
                "subscribed"
            } else {
                "unsubscribed"
            }
        );
        if !subscribed {
            self.voice_link_lost();
        }
    }

    pub(super) fn voice_link_lost(&mut self) {
        self.session.reset_run();
        self.session.reset_followup();
        self.session.drop_tts_report();
        if self.session.in_voice_request() {
            self.mark_wake_cancelled();
            self.finish_interaction();
        }
    }

    pub(super) fn on_wake(
        &mut self,
        phrase: &str,
        level_db: f32,
        preroll: Vec<i16>,
        endpoint_prior: Option<EndpointPrior>,
        event_id: Option<String>,
    ) {
        if self.mic_muted() {
            return;
        }
        info!("wake word '{phrase}', level {level_db:.1} dB");
        let rec = event_id.map(WakeRecord::new);
        if self.timers_ha.ringing() {
            self.cancel_arbitration();
            if self.arbitration_on() {
                self.deps
                    .arbiter
                    .claim(&self.st.wake_word, level_db, true, false);
            }
            return self.stop_timer_ring();
        }
        self.cancel_arbitration();
        if !self.arbitration_on() || !self.deps.ha.voice_subscribed() {
            return self.answer_wake(phrase, &preroll, &[], endpoint_prior, rec);
        }
        let busy = self.session.is_speaking() || self.session.announcing();
        let claim = self
            .deps
            .arbiter
            .claim(&self.st.wake_word, level_db, busy, true);
        self.wake.start(
            claim,
            phrase.to_owned(),
            level_db,
            preroll,
            endpoint_prior,
            rec,
        );
        self.deps.frontend.set_streaming(true);
        self.update_ui();
    }

    fn answer_wake(
        &mut self,
        phrase: &str,
        preroll: &[i16],
        live: &[i16],
        endpoint_prior: Option<EndpointPrior>,
        rec: Option<WakeRecord>,
    ) {
        if self.timers_ha.ringing() {
            return self.stop_timer_ring();
        }
        if self.session.in_voice_request() {
            info!("wake word: replacing the active dialog");
            self.interrupt_speech();
            self.mark_wake_cancelled();
            self.finish_interaction();
        } else if self.session.is_speaking() || self.session.announcing() {
            self.mark_wake_cancelled();
            self.finish_wake_rec();
            self.interrupt_speech();
            self.count_current_run();
        }
        self.session.set_record(rec);
        self.start_pipeline(phrase, true, preroll, live, endpoint_prior);
        if !self.session.is_listening() {
            self.mark_wake_cancelled();
            self.finish_wake_rec();
        }
    }

    pub(super) fn on_arbiter_decided(
        &mut self,
        claim_id: ClaimToken,
        won: bool,
        best: &pulsar_arbiter::Claim,
        peers: usize,
    ) {
        let Some(decision) = self.wake.decided(claim_id, won, best, peers) else {
            return;
        };
        match decision {
            Decision::Lost {
                best,
                level_db,
                record,
            } => {
                self.finish_wake(record);
                if !self.session.is_listening() {
                    self.deps.frontend.set_streaming(false);
                }
                info!(
                    "wake word: {} answers ({}{:.1} dB, ours {:.1} dB)",
                    best.station,
                    if best.busy { "busy, " } else { "" },
                    best.level_db,
                    level_db
                );
                self.wake.lost(now_ms());
                self.update_ui();
            }
            Decision::Won {
                phrase,
                preroll,
                live,
                endpoint,
                record,
                peers,
            } => {
                info!("wake word: this station answers ({peers} other station(s) heard it)");
                self.answer_wake(&phrase, &preroll, &live, endpoint, record);
                if !self.session.is_listening() {
                    self.deps.frontend.set_streaming(false);
                }
            }
        }
    }

    pub(super) fn cancel_arbitration(&mut self) {
        if !self.wake.is_claiming() {
            return;
        }
        let record = self.wake.cancel();
        self.finish_wake(record);
        self.deps.arbiter.cancel();
        if !self.session.is_listening() {
            self.deps.frontend.set_streaming(false);
        }
    }

    pub(super) fn start_pipeline(
        &mut self,
        phrase: &str,
        earcon: bool,
        preroll: &[i16],
        live: &[i16],
        endpoint_prior: Option<EndpointPrior>,
    ) {
        self.cancel_arbitration();
        self.timers.cancel(Tk::FollowupWait);
        if !self.deps.ha.voice_subscribed() {
            warn!("wake ignored: Home Assistant voice assistant not subscribed");
            self.show_ui(UiState::Error);
            self.timers.set(Tk::UiAfterNotSubscribed, 1500);
            return;
        }
        info!("starting voice pipeline (wake='{phrase}')");
        self.wake.clear();
        let endpoint = if self.local_endpointing {
            endpoint_prior.and_then(|p| EndpointRun::new(p, preroll, live))
        } else {
            None
        };
        if let Some(r) = endpoint.as_ref() {
            let p = r.prior();
            let (on, off) = r.thresholds();
            debug!(
                "local endpointing: noise {:.1} dB, wake {:.1} dB, onset {:.1} dB, release {:.1} dB",
                p.noise_db, p.speech_db, on, off
            );
        }
        let started = now_ms();
        self.session.begin(started, !phrase.is_empty(), endpoint);
        self.run_metric = Some(RunMetric {
            started_ms: started,
            wake: !phrase.is_empty(),
            answer_started: false,
        });
        self.run_result = None;
        self.update_duck();
        match logic::wake_cue(earcon, self.st.wake_sound) {
            logic::WakeCue::Keep => {}
            logic::WakeCue::Sound => {
                let sound = earcons::path(self.model.earcons.wake);
                self.set_announce_speech(false);
                self.deps.player.play(Channel::Announce, vec![sound]);
            }
            logic::WakeCue::Cut => self.deps.player.stop(Channel::Announce),
        }
        self.send_tts_report(false);
        let run = self.deps.ha.voice_request_start(phrase);
        self.session.set_voice_run(run);
        for chunk in preroll.chunks(512) {
            self.deps.ha.voice_send_audio(chunk.to_vec());
        }
        for chunk in live.chunks(512) {
            self.deps.ha.voice_send_audio(chunk.to_vec());
        }
        self.deps.frontend.set_streaming(true);
        self.arm_watchdog();
        self.update_ui();
    }

    fn stop_streaming(&mut self) {
        self.timers.cancel(Tk::FollowupWait);
        self.session.stop_listening();
        self.deps.frontend.set_streaming(self.wake.is_claiming());
    }

    pub(super) fn on_listening_audio(&mut self, pcm: Vec<i16>) {
        self.session.push_listen_level(pcm_level_db(&pcm));
        self.deps.ha.voice_send_audio(pcm);
        let earcon = matches!(
            self.deps.player.state(Channel::Announce),
            PlayState::Playing | PlayState::Loading
        );
        if !earcon && self.session.is_listening() && self.session.take_endpoint_end() {
            self.send_endpoint_end();
        }
    }

    fn send_endpoint_end(&mut self) {
        if self.session.stop_sent() || !self.session.is_listening() {
            return;
        }
        if let Some(r) = self.session.endpoint() {
            let p = r.prior();
            let (on, off) = r.thresholds();
            info!(
                "voice pipeline: local end (experimental), noise {:.1} dB, wake {:.1} dB, \
                 onset {:.1} dB, release {:.1} dB",
                p.noise_db, p.speech_db, on, off
            );
        }
        self.deps.ha.voice_send_audio_end();
        self.stop_streaming();
        self.arm_watchdog();
        self.update_ui();
    }

    pub(super) fn mark_wake_cancelled(&mut self) {
        self.session.mark_wake_cancelled();
    }

    fn finish_wake(&mut self, rec: Option<WakeRecord>) {
        let Some(rec) = rec else { return };
        self.deps
            .frontend
            .wake_event_outcome(&rec.id, rec.outcome(), rec.stt.as_deref());
    }

    fn finish_wake_rec(&mut self) {
        let rec = self.session.take_record();
        self.finish_wake(rec);
    }

    pub(super) fn on_voice_event(&mut self, ev: VaEvent, data: &HashMap<String, String>) {
        let code = ev.code();
        if ev != VaEvent::IntentProgress {
            info!("voice event {code}");
        }
        if !self.session.accept(ev) {
            debug!("voice event {code}: from the replaced run, ignored");
            return;
        }
        if self.session.phase() == Phase::Idle {
            if ev == VaEvent::TtsStart {
                self.session.owe_tts_report();
                self.send_tts_report(false);
            }
            debug!("voice event {code}: dialog already ended, ignored");
            return;
        }
        self.session.followup_on_event(
            ev,
            data.get("continue_conversation").is_some_and(|v| v == "1"),
        );
        match ev {
            VaEvent::RunStart => self.session.set_tts_url(get(data, "url")),
            VaEvent::SttStart => self.on_stt_start(),
            VaEvent::SttVadStart => self.timers.cancel(Tk::FollowupWait),
            VaEvent::SttVadEnd => self.finish_phrase(),
            VaEvent::SttEnd => self.on_stt_end(get(data, "text")),
            VaEvent::IntentStart => self.on_intent_start(),
            VaEvent::IntentProgress => {
                if get(data, "tts_start_streaming") == "1" {
                    self.play_tts();
                }
            }
            VaEvent::TtsStart => self.session.owe_tts_report(),
            VaEvent::TtsEnd => {
                self.session.update_tts_url(get(data, "url"));
                self.play_tts();
            }
            VaEvent::Error => self.on_voice_error(data),
            VaEvent::RunEnd => {
                self.stop_streaming();
                if !self.session.tts_played() {
                    self.finish_interaction();
                }
            }
            _ => {}
        }
        self.arm_watchdog();
    }

    fn on_stt_start(&mut self) {
        if let Some(run) = self.run_metric
            && run.wake
        {
            pulsar_metrics::observe_wake_to_stt((now_ms() - run.started_ms) as f64 / 1000.0);
        }
        if self.session.is_listening()
            && let Some(ms) = self.session.followup_window_ms(self.st.followup_duration_s)
        {
            self.timers.set(Tk::FollowupWait, ms);
        }
    }

    fn finish_phrase(&mut self) {
        self.stop_streaming();
        self.update_ui();
    }

    fn on_stt_end(&mut self, text: &str) {
        self.session.record_stt(text);
        if self.session.in_voice_request()
            && logic::stt_stop_action(text, self.session.wake_dialog())
                == logic::SttStopAction::Cancel
        {
            info!("voice command: stop word in STT text");
            if self.timers_ha.ringing() {
                self.stop_timer_ring();
            }
            return self.cancel_dialog();
        }
        self.finish_phrase();
    }

    fn on_intent_start(&mut self) {
        if self.session.is_listening() {
            self.stop_streaming();
        }
        self.update_ui();
    }

    fn on_voice_error(&mut self, data: &HashMap<String, String>) {
        let code = get(data, "code");
        if code == "duplicate_wake_up_detected" {
            info!("pipeline: another satellite answers this wake word (HA duplicate wake-up)");
            self.session.record_arbiter_lost();
            self.stop_streaming();
        } else if code == "stt-no-text-recognized" {
            info!("pipeline: no speech recognized, treating as cancel");
            self.session.record_stt("");
            self.stop_streaming();
        } else if self.session.followup_silent(code) {
            info!("follow-up: nothing said, conversation ends");
            self.stop_streaming();
        } else {
            warn!("pipeline error: {} {}", code, get(data, "message"));
            self.run_result = Some(pulsar_metrics::PipelineResult::Error);
            self.session.record_error();
            self.stop_streaming();
            self.error_shown = true;
            self.show_ui(UiState::Error);
            let sound = earcons::path(self.model.earcons.error);
            self.play_closing_earcon(sound);
        }
    }

    fn play_tts(&mut self) {
        let Some((dialog, url)) = self.session.take_tts() else {
            return;
        };
        self.observe_answer_start();
        self.update_ui();
        self.play_announce(vec![url], Pending::Tts { dialog });
    }

    fn tts_finished(&mut self, ok: bool) {
        self.send_tts_report(ok);
        let followup = self.session.followup();
        if followup.should_continue(
            ok,
            self.st.followup_duration_s,
            self.timers_ha.ringing() || self.mic_muted(),
        ) {
            self.count_current_run();
            self.start_pipeline("", false, &[], &[], None);
            if self.session.is_listening() {
                self.session.set_followup(followup.next_turn());
            }
            return;
        }
        self.finish_interaction();
    }

    fn send_tts_report(&mut self, ok: bool) {
        if !self.session.take_tts_report()
            || self
                .pending
                .values()
                .any(|p| matches!(p, Pending::Announce { .. }))
        {
            return;
        }
        info!("pipeline: answer finished reported to HA (ok={ok})");
        self.deps.ha.voice_answer_finished(ok);
    }

    pub(super) fn finish_interaction(&mut self) {
        self.count_current_run();
        self.timers.cancel(Tk::FollowupWait);
        if self.session.stop_due() {
            self.deps.ha.voice_request_stop();
            self.session.mark_stop_sent();
        }
        self.session.no_run_follows();
        self.send_tts_report(false);
        self.session.finish();
        self.deps.frontend.set_streaming(self.wake.is_claiming());
        self.finish_wake_rec();
        self.update_duck();
        self.arm_watchdog();
        if self.error_shown {
            self.error_shown = false;
            self.timers.set(Tk::UiAfterError, 1200);
        } else {
            self.update_ui();
        }
    }

    pub(super) fn abort_dialog(&mut self, why: &str) {
        self.run_result
            .get_or_insert(pulsar_metrics::PipelineResult::Error);
        warn!("voice pipeline: {why}");
        self.error_shown = true;
        self.show_ui(UiState::Error);
        self.mark_wake_cancelled();
        self.finish_interaction();
    }

    pub(super) fn on_voice_start_failed(&mut self, run: u64) {
        if run != self.session.voice_run() || !self.session.in_voice_request() {
            return;
        }
        self.session.no_run_follows();
        self.abort_dialog("Home Assistant refused to start it");
    }

    pub(super) fn on_voice_audio_overflow(&mut self, run: u64) {
        if run != self.session.voice_run() || !self.session.in_voice_request() {
            return;
        }
        self.session.mark_stop_sent();
        self.abort_dialog("network too slow for the microphone audio");
    }

    fn observe_answer_start(&mut self) {
        let now = now_ms();
        if let Some(latency) = self
            .run_metric
            .as_mut()
            .and_then(|run| run.answer_latency(now))
        {
            pulsar_metrics::observe_wake_to_answer(latency);
        }
    }

    fn count_current_run(&mut self) {
        let result = self
            .run_result
            .take()
            .unwrap_or(pulsar_metrics::PipelineResult::Ok);
        if self.run_metric.take().is_some() {
            pulsar_metrics::pipeline_finished(result);
        }
    }

    fn arm_watchdog(&mut self) {
        self.timers.cancel(Tk::Watchdog);
        if let Some(ms) = self.session.watchdog_ms() {
            self.timers.set(Tk::Watchdog, ms);
        }
    }

    pub(super) fn interrupt_speech(&mut self) {
        self.deps.player.stop(Channel::Announce);
        if self.session.announcing() {
            self.session.interrupt_announce();
        }
    }

    pub(super) fn play_closing_earcon(&mut self, path: String) {
        let seq = self.session.start_earcon();
        self.play_announce(vec![path], Pending::Earcon { seq });
    }

    fn cancel_dialog(&mut self) {
        self.interrupt_speech();
        self.play_closing_earcon(earcons::path(self.model.earcons.cancel));
        self.mark_wake_cancelled();
        self.finish_interaction();
    }

    pub(super) fn action_button(&mut self) {
        if self.timers_ha.ringing() {
            return self.stop_timer_ring();
        }
        if self.session.in_voice_request() {
            return self.cancel_dialog();
        }
        if self.session.is_speaking() || self.session.announcing() {
            self.mark_wake_cancelled();
            self.interrupt_speech();
            return self.finish_interaction();
        }
        self.start_pipeline("", true, &[], &[], None);
    }

    pub(super) fn on_announce(&mut self, req: AnnounceRequest) {
        info!(
            "announce: '{}'{}",
            req.text,
            if self.session.phase() == Phase::Idle {
                ""
            } else {
                " (during a dialog)"
            }
        );
        let mut urls = Vec::with_capacity(2);
        if !req.preannounce_media_id.is_empty() {
            urls.push(req.preannounce_media_id);
        }
        urls.push(req.media_id);
        let seq = self.session.start_announce();
        self.update_duck();
        self.update_ui();
        self.publish_media();
        self.play_announce(
            urls,
            Pending::Announce {
                conv: req.start_conversation,
                seq,
            },
        );
    }

    pub(super) fn end_announce(&mut self) {
        self.update_duck();
        self.update_ui();
        self.publish_media();
    }

    pub(super) fn update_commands(&self) {
        let playing = matches!(
            self.deps.player.state(Channel::Music),
            PlayState::Playing | PlayState::Loading
        ) || self.sendspin_streaming()
            || self.session.announcing()
            || self.timers_ha.ringing()
            || self.session.is_speaking();
        let dialog = self.session.in_voice_request() || self.wake.is_claiming();
        self.deps
            .frontend
            .set_commands(!self.mic_muted() && playing && !dialog);
    }

    fn owns_music(&self) -> bool {
        self.session.owns_music(
            now_ms(),
            self.deps.player.state(Channel::Music),
            self.sendspin_streaming(),
            self.deps.sendspin.as_ref().is_some_and(|ss| ss.connected()),
        )
    }

    pub(super) fn music_pause(&self) -> bool {
        let mut paused = matches!(
            self.deps.player.state(Channel::Music),
            PlayState::Playing | PlayState::Loading
        );
        if paused {
            self.deps.player.pause(Channel::Music);
        }
        if let Some(ss) = &self.deps.sendspin
            && ss.streaming()
        {
            ss.transport(Transport::Pause);
            paused = true;
        }
        paused
    }

    pub(super) fn music_resume(&self) {
        if self.deps.player.state(Channel::Music) == PlayState::Paused {
            self.deps.player.resume(Channel::Music);
        } else if let Some(ss) = &self.deps.sendspin {
            ss.transport(Transport::Play);
        }
    }

    pub(super) fn on_command(&mut self, word: &str) {
        if self.mic_muted() {
            return;
        }
        let Some(c) = logic::command_of(word) else {
            return;
        };
        let now = now_ms();
        if self.wake.blocked(now) {
            info!("voice command '{word}' ignored: another station answers this wake word");
            return;
        }
        if self.session.in_voice_request() || self.wake.is_claiming() {
            let after_wake = logic::after_wake(
                self.session.phase(),
                self.session.wake_dialog(),
                now,
                self.session.listen_started_ms(),
            );
            info!(
                "voice command '{word}' left to Home Assistant during a voice request{}",
                if after_wake { " (after wake word)" } else { "" }
            );
            return;
        }
        let sounding =
            self.session.is_speaking() || self.session.announcing() || self.timers_ha.ringing();
        if !logic::command_allowed(c, false, false, sounding, self.owns_music()) {
            info!("voice command '{word}' ignored: not for this station");
            return;
        }
        info!("voice command '{word}'");
        match c {
            VoiceCommand::Stop => self.stop_word(),
            VoiceCommand::Pause => {
                if self.music_pause() {
                    self.deps.display.clock.play(ClockShot::Pause);
                }
            }
            VoiceCommand::Resume => self.music_resume(),
            VoiceCommand::Louder | VoiceCommand::Quieter => {
                let up = c == VoiceCommand::Louder;
                self.set_volume(pulsar_core::volume::step_alice(self.st.volume, up), true);
            }
            VoiceCommand::Next | VoiceCommand::Prev => {
                let (t, shot) = if c == VoiceCommand::Next {
                    (Transport::Next, ClockShot::Next)
                } else {
                    (Transport::Previous, ClockShot::Previous)
                };
                if self
                    .deps
                    .sendspin
                    .as_ref()
                    .is_some_and(|ss| ss.transport(t))
                {
                    self.deps.display.clock.play(shot);
                } else {
                    info!("'{word}' ignored: no Sendspin queue to skip in");
                }
            }
        }
    }

    fn stop_word(&mut self) {
        if self.timers_ha.ringing() {
            self.stop_timer_ring();
        } else if self.session.is_speaking() {
            self.mark_wake_cancelled();
            self.interrupt_speech();
            self.finish_interaction();
        } else if self.session.announcing() {
            self.mark_wake_cancelled();
            self.interrupt_speech();
            self.update_duck();
            self.update_ui();
        } else if self.music_pause() {
            self.deps.display.clock.play(ClockShot::Pause);
        }
    }
}

fn get<'a>(data: &'a HashMap<String, String>, key: &str) -> &'a str {
    data.get(key).map_or("", String::as_str)
}
