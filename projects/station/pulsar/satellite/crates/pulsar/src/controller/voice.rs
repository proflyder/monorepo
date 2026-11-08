//! Voice pipeline, wake word arbitration, announcements and local voice commands
//! (controller.cpp "voice pipeline", on_command).

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
use super::{Controller, Pending, Tk, WakeRecord};

impl Controller {
    fn arbitration_on(&self) -> bool {
        self.st.nearest_answers && self.deps.arbiter.ok()
    }

    /// Plays `urls` on ANNOUNCE; `done` runs on its end. Spoken announcements and TTS answers
    /// follow «Громкость объявлений»; earcons and the ring keep the master volume.
    pub(super) fn play_announce(&mut self, urls: Vec<String>, done: Pending) {
        let speech = matches!(done, Pending::Tts { .. } | Pending::Announce { .. });
        self.set_announce_speech(speech);
        let id = self.deps.player.play(Channel::Announce, urls);
        self.pending.insert(id, done);
    }

    /// The C++ completion callbacks of `play()`.
    pub(super) fn play_done(&mut self, p: Pending, ok: bool) {
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
                // Exactly once per announcement, also when cut short.
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

    // ---------------- connection ----------------

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

    /// No more events come for the current run, and a stop would reach whichever client
    /// subscribes next. A dialog still listening or thinking ends now. An answer already playing
    /// is fetched over HTTP independently of the API link, so it plays out (still ducked) and its
    /// end closes the dialog; a stream that dies with HA ends it through the player instead. The
    /// AnnounceFinished owed for the answer belongs to the lost client: the next one could be
    /// waiting for an announcement, which that message would end.
    pub(super) fn voice_link_lost(&mut self) {
        self.session.reset_run();
        self.session.reset_followup();
        self.session.drop_tts_report();
        if self.session.in_voice_request() {
            self.mark_wake_cancelled();
            self.finish_interaction();
        }
    }

    // ---------------- wake word ----------------

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
        // The recorded hit this wake word belongs to (contract v1); the sidecar's outcome is
        // filled when the dialog ends. A wake word that only stops a timer starts no dialog.
        let rec = event_id.map(WakeRecord::new);
        if self.timers_ha.ringing() {
            // The wake word works as a stop word while ringing. A ringing station takes the
            // turn so that its neighbours do not start a dialog.
            self.cancel_arbitration();
            if self.arbitration_on() {
                self.deps
                    .arbiter
                    .claim(&self.st.wake_word, level_db, true, false);
            }
            return self.stop_timer_ring();
        }
        // As stock Alice: a repeated wake word replaces the call in progress instead of being
        // ignored. The old call is ended only when this station starts the new dialog
        // (answer_wake): a station that loses wake arbitration keeps its answer, and its busy
        // claim still counts SPEAKING. A pending claim is replaced by this wake word.
        self.cancel_arbitration();
        // Without the HA pipeline this station could win and stay silent: it does not take part.
        if !self.arbitration_on() || !self.deps.ha.voice_subscribed() {
            return self.answer_wake(phrase, &preroll, &[], endpoint_prior, rec);
        }
        let busy = self.session.is_speaking() || self.session.announcing();
        // A station that lost the previous wake word may win this one: the new claim replaces
        // the blocked turn.
        let claim = self
            .deps
            .arbiter
            .claim(&self.st.wake_word, level_db, busy, true);
        // Everything after this point is live post-wake speech held during the decision; the
        // detector must not treat it as pre-roll evidence.
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
        // A repeated wake word: a LISTENING / PROCESSING dialog is ended, its run stopped, and
        // RunBinding keeps the replaced run's late RUN_END from ending the new dialog. While
        // SPEAKING the answer is only cut, as before (the new start replaces its dialog id).
        // ESPHome pipeline events carry no run id: a run replaced before its RUN_START cannot be
        // told apart from the new one if that RUN_START was still on the wire.
        if self.session.in_voice_request() {
            info!("wake word: replacing the active dialog");
            self.interrupt_speech();
            self.mark_wake_cancelled();
            self.finish_interaction();
        } else if self.session.is_speaking() || self.session.announcing() {
            // The cut answer's completion never closes its dialog (the id changes below), so its
            // sidecar gets the outcome here.
            self.mark_wake_cancelled();
            self.finish_wake_rec();
            self.interrupt_speech(); // barge-in: cut the answer short
        }
        self.session.set_record(rec);
        self.start_pipeline(phrase, true, preroll, live, endpoint_prior);
        // No dialog started (HA is gone): the hit still gets an outcome instead of being
        // overwritten by the next one.
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
                // The recorded hit still gets its outcome: another station answered.
                self.finish_wake(record);
                // A dialog that was already listening keeps streaming: this wake word replaced
                // nothing unless this station answers it.
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
                // Its command spotter would still act on "Алиса, громче" if it sounds or owns music.
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
                // Capture keeps streaming into the new dialog: switching it off here could drop
                // the chunk between the preroll and live audio.
                self.answer_wake(&phrase, &preroll, &live, endpoint, record);
                if !self.session.is_listening() {
                    self.deps.frontend.set_streaming(false); // no dialog started (e.g. HA gone)
                }
            }
        }
    }

    pub(super) fn cancel_arbitration(&mut self) {
        if !self.wake.is_claiming() {
            return;
        }
        // The pending hit started no dialog: its event ended with the cancelled arbitration.
        let record = self.wake.cancel();
        self.finish_wake(record);
        self.deps.arbiter.cancel();
        if !self.session.is_listening() {
            self.deps.frontend.set_streaming(false);
        }
    }

    // ---------------- pipeline ----------------

    /// The only way into LISTENING (the watchdog bounds it). `preroll` came with the wake event;
    /// `live` is post-wake audio held during wake arbitration (empty otherwise).
    pub(super) fn start_pipeline(
        &mut self,
        phrase: &str,
        earcon: bool,
        preroll: &[i16],
        live: &[i16],
        endpoint_prior: Option<EndpointPrior>,
    ) {
        // The button, a continued conversation or HA's start_conversation win.
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
        self.session.begin(now_ms(), !phrase.is_empty(), endpoint);
        self.update_duck();
        if earcon {
            let sound = earcons::path(self.model.earcons.wake);
            self.set_announce_speech(false);
            self.deps.player.play(Channel::Announce, vec![sound]);
        }
        // HA applies AnnounceFinished by forcing idle: an answer's report must not arrive over
        // the new dialog's listening state.
        self.send_tts_report(false);
        // A failed send comes back as VoiceStartFailed / VoiceAudioOverflow of this run.
        let run = self.deps.ha.voice_request_start(phrase);
        self.session.set_voice_run(run);
        // HA drops audio queued before the start and queues what follows it on the same
        // connection.
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
        // HA finished the phrase first (STT_VAD_END/STT_END), the dialog was cancelled or a
        // repeated wake replaced it: drop the run so no stale end can follow.
        self.session.stop_listening();
        // A wake word heard meanwhile keeps its audio.
        self.deps.frontend.set_streaming(self.wake.is_claiming());
    }

    /// One live 32 ms chunk while the dialog listens. Sends it to HA and feeds the optional
    /// local detector; a detected end is deferred while the wake earcon is playing.
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

    /// Graceful `VoiceAssistantAudio.end`: finish the phrase on what HA already has, without
    /// aborting the run. Exactly once per dialog; a stale or replaced dialog sends nothing.
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
        // Stop feeding the microphone, but keep the run: HA finishes STT and the answer.
        self.stop_streaming();
        self.arm_watchdog();
        self.update_ui();
    }

    // ---------------- wake-event sidecars ----------------

    /// The running dialog was not ended by its own run: re-wake, local cancel, mute, lost link.
    pub(super) fn mark_wake_cancelled(&mut self) {
        self.session.mark_wake_cancelled();
    }

    /// Writes the outcome of `rec` to its sidecar (no-op without a recorded hit).
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
            // The run of a dialog that already ended; it is being stopped. A TTS_START that got
            // out before the stop still leaves HA waiting for the answer's report.
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
            // Any other type, including one newer than this build, only re-arms the watchdog.
            _ => {}
        }
        self.arm_watchdog();
    }

    fn on_stt_start(&mut self) {
        if self.session.is_listening()
            && let Some(ms) = self.session.followup_window_ms(self.st.followup_duration_s)
        {
            self.timers.set(Tk::FollowupWait, ms);
        }
    }

    /// STT кончился без слова-остановки: фраза уходит в HA.
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
        // HA accepts one wake word phrase per 2 s across satellites: another one answers.
        if code == "duplicate_wake_up_detected" {
            info!("pipeline: another satellite answers this wake word (HA duplicate wake-up)");
            self.session.record_arbiter_lost();
            self.stop_streaming();
        } else if code == "stt-no-text-recognized" {
            // Empty STT after a wake word: the user changed their mind or spoke too softly.
            // Treat it as a silent cancel, not a station error.
            info!("pipeline: no speech recognized, treating as cancel");
            self.session.record_stt("");
            self.stop_streaming();
        } else if self.session.followup_silent(code) {
            info!("follow-up: nothing said, conversation ends");
            self.stop_streaming();
        } else {
            warn!("pipeline error: {} {}", code, get(data, "message"));
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
        self.update_ui();
        self.play_announce(vec![url], Pending::Tts { dialog });
    }

    /// A cut-short or failed answer does not reopen the microphone, nor does a ringing timer.
    fn tts_finished(&mut self, ok: bool) {
        self.send_tts_report(ok);
        let followup = self.session.followup();
        if followup.should_continue(
            ok,
            self.st.followup_duration_s,
            self.timers_ha.ringing() || self.mic_muted(),
        ) {
            self.start_pipeline("", false, &[], &[], None);
            if self.session.is_listening() {
                self.session.set_followup(followup.next_turn());
            }
            return;
        }
        self.finish_interaction();
    }

    /// Sends the AnnounceFinished owed since the run's TTS_START, at most once. Dropped while an
    /// assist announcement's completion is pending (also when cut short): its own
    /// AnnounceFinished ends HA's wait as well, and one more would end the announcement's wait
    /// early. Media player announcements send none.
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
        self.timers.cancel(Tk::FollowupWait);
        // The dialog may end before its run does (answer cut short, cancel, timeout): stop the
        // run so HA does not go on generating; whatever it still sends is dropped while idle.
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

    /// The dialog failed without a pipeline run to end it: error ring, then idle.
    pub(super) fn abort_dialog(&mut self, why: &str) {
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
        self.session.no_run_follows(); // no run follows
        self.abort_dialog("Home Assistant refused to start it");
    }

    pub(super) fn on_voice_audio_overflow(&mut self, run: u64) {
        if run != self.session.voice_run() || !self.session.in_voice_request() {
            return;
        }
        self.session.mark_stop_sent(); // by the server itself
        self.abort_dialog("network too slow for the microphone audio");
    }

    /// Without it a run HA never ends (lost connection state, a hung agent) would leave the ring
    /// listening, the microphone streaming and the wake word ignored.
    fn arm_watchdog(&mut self) {
        self.timers.cancel(Tk::Watchdog);
        if let Some(ms) = self.session.watchdog_ms() {
            self.timers.set(Tk::Watchdog, ms);
        }
    }

    /// Cuts the TTS answer or HA announcement short. An interrupted announcement ends here (its
    /// completion then only reports to HA); the dialog of an interrupted answer is ended or
    /// replaced by the caller, otherwise by the TTS completion.
    pub(super) fn interrupt_speech(&mut self) {
        self.deps.player.stop(Channel::Announce);
        if self.session.announcing() {
            self.session.interrupt_announce();
        }
    }

    /// Error and cancel sounds outlive the dialog they close; music stays ducked until they end.
    pub(super) fn play_closing_earcon(&mut self, path: String) {
        let seq = self.session.start_earcon();
        self.play_announce(vec![path], Pending::Earcon { seq });
    }

    /// Отменяет диалог LISTENING / PROCESSING: короткое нажатие кнопки действия, «стоп» после окна
    /// wake word и «стоп» из текста STT в диалоге не от wake word. Объявление, играющее поверх
    /// диалога, обрывается вместе с ним.
    fn cancel_dialog(&mut self) {
        self.interrupt_speech();
        self.play_closing_earcon(earcons::path(self.model.earcons.cancel));
        self.mark_wake_cancelled();
        self.finish_interaction();
    }

    /// The Button's short press.
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

    // ---------------- announcements ----------------

    /// Announcements do not take over the dialog phase, and their end leaves any dialog alone. HA
    /// cancels its own running pipeline before it announces (assist_satellite
    /// async_internal_announce), so a dialog still active here is either ending (its RUN_END or
    /// the watchdog follows) or was started by HA after the announcement.
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

    // ---------------- local commands ----------------

    /// Stock command spotter ("стоп", "хватит", "пауза", "продолжи", "громче", "тише", "дальше",
    /// "назад"): on while something plays, off in silence and during a wake request. Which words
    /// act where is logic::command_allowed.
    pub(super) fn update_commands(&self) {
        // Stock commands without the wake word stay available while something plays,
        // as in the C++ build. During a wake request they are off: «Рикардо, стоп»
        // must reach Home Assistant so it can stop an alarm and restore the music,
        // instead of being swallowed as a local pause.
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

    /// True when something was playing. Reads the player and Sendspin states before the
    /// commands (neither changes them synchronously, as in C++).
    fn music_pause(&self) -> bool {
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

    fn music_resume(&self) {
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
        // A wake request owns the command. Sending "стоп" to Home Assistant is what
        // stops a ringing alarm and restores the paused music; local handling would
        // only pause the music and cancel the pipeline. This also covers a callback
        // queued just before update_commands() switched the spotter off.
        if self.session.in_voice_request() || self.wake.is_claiming() {
            let after_wake =
                logic::after_wake(self.session.phase(), now, self.session.listen_started_ms());
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

    /// «стоп» / «хватит» от командного спотера.
    fn stop_word(&mut self) {
        if self.timers_ha.ringing() {
            self.stop_timer_ring();
        } else if self.session.is_speaking() {
            self.mark_wake_cancelled();
            self.interrupt_speech();
            self.finish_interaction();
        } else if self.session.announcing() {
            // A dialog listening meanwhile goes on.
            self.mark_wake_cancelled();
            self.interrupt_speech();
            self.update_duck();
            self.update_ui();
        } else if self.music_pause() {
            // «стоп» = пауза, как в стоке; см. docs/voice.md, «Командный спотер».
            self.deps.display.clock.play(ClockShot::Pause);
        }
    }
}

fn get<'a>(data: &'a HashMap<String, String>, key: &str) -> &'a str {
    data.get(key).map_or("", String::as_str)
}
