//! Голосовая сессия: диалог с Home Assistant, follow-up, TTS-отчёт, объявления и earcons
//! (controller.cpp "voice pipeline"). Фаза с данными вместо набора флагов.

use std::mem;

use pulsar_audio::endpoint::EndpointRun;
use pulsar_esphome::VaEvent;
use pulsar_media::PlayState;

use super::logic::{self, Followup, Phase, RunBinding};
use super::{AnnounceId, DialogId, EarconSeq, LISTEN_TIMEOUT_MS, PROCESS_TIMEOUT_MS, WakeRecord};

/// Фаза диалога с тем, что существует только в ней: локальный детектор конца фразы живёт лишь
/// пока слушаем. `played` сбрасывается новым RUN_START: HA может прислать второй ответ в том же
/// диалоге, и следующий TTS_END должен заиграть снова.
pub enum DialogPhase {
    Listening { endpoint: Option<EndpointRun> },
    Processing,
    Speaking { played: bool },
}

/// Открытый диалог. Всё остальное в `Session` переживает его конец: привязка run, follow-up,
/// долг отчёта и `stop_sent` относятся к моменту завершения и к следующему диалогу.
pub struct Dialog {
    phase: DialogPhase,
    started_ms: i64,
    /// Диалог открыт wake word (а не кнопкой, продолжением или HA).
    wake: bool,
    tts_url: String,
}

/// Одна голосовая сессия станции. `dialog` = None — покой.
#[derive(Default)]
pub struct Session {
    /// Id последнего диалога; растёт и при старте, и при конце, поэтому completion старого
    /// диалога не может сработать для нового.
    current_id: DialogId,
    dialog: Option<Dialog>,
    /// Pipeline events не несут id run: связывает события с заменённым run.
    run: RunBinding,
    followup: Followup,
    /// voice_request_stop уже отправлен для run этого диалога.
    stop_sent: bool,
    /// Отчёт AnnounceFinished, который HA ждёт от TTS_START, ещё не отправлен.
    report_owed: bool,
    /// Играет объявление HA (assist или media player); от диалога не зависит.
    announce: Option<AnnounceId>,
    announce_seq: AnnounceId,
    /// Играет закрывающий earcon (ошибка/отмена); держит музыку приглушённой.
    earcon: Option<EarconSeq>,
    earcon_seq: EarconSeq,
    /// Id последнего voice_request_start.
    voice_run: u64,
    /// Записанное срабатывание, чей диалог открыт: живёт через follow-up-реплики до конца
    /// разговора, а не только до смены диалога.
    record: Option<WakeRecord>,
    /// Когда канал HA-медиа встал на паузу и когда кончился поток Sendspin.
    music_paused_ms: Option<i64>,
    sendspin_ended_ms: Option<i64>,
}

impl Session {
    pub fn phase(&self) -> Phase {
        match self.dialog.as_ref().map(|d| &d.phase) {
            None => Phase::Idle,
            Some(DialogPhase::Listening { .. }) => Phase::Listening,
            Some(DialogPhase::Processing) => Phase::Processing,
            Some(DialogPhase::Speaking { .. }) => Phase::Speaking,
        }
    }

    pub fn current_id(&self) -> DialogId {
        self.current_id
    }

    /// Диалог слушает микрофон (wake word, кнопка, follow-up или HA).
    pub fn is_listening(&self) -> bool {
        matches!(
            self.dialog.as_ref().map(|d| &d.phase),
            Some(DialogPhase::Listening { .. })
        )
    }

    pub fn is_speaking(&self) -> bool {
        matches!(
            self.dialog.as_ref().map(|d| &d.phase),
            Some(DialogPhase::Speaking { .. })
        )
    }

    /// Ответ этого диалога уже начал играть (HEAD `tts_played`).
    pub fn tts_played(&self) -> bool {
        matches!(
            self.dialog.as_ref().map(|d| &d.phase),
            Some(DialogPhase::Speaking { played: true })
        )
    }

    /// LISTENING / PROCESSING: запрос ещё не дошёл до ответа.
    pub fn in_voice_request(&self) -> bool {
        matches!(self.phase(), Phase::Listening | Phase::Processing)
    }

    pub fn announcing(&self) -> bool {
        self.announce.is_some()
    }

    pub fn closing_earcon(&self) -> bool {
        self.earcon.is_some()
    }

    pub fn wake_dialog(&self) -> bool {
        self.dialog.as_ref().is_some_and(|d| d.wake)
    }

    pub fn listen_started_ms(&self) -> i64 {
        self.dialog.as_ref().map_or(0, |d| d.started_ms)
    }

    /// Watchdog фазы: HA, не продвинувший run, не должен оставить микрофон включённым.
    pub fn watchdog_ms(&self) -> Option<u64> {
        match self.phase() {
            Phase::Listening => Some(LISTEN_TIMEOUT_MS),
            Phase::Processing => Some(PROCESS_TIMEOUT_MS),
            _ => None,
        }
    }

    // ---------------- run binding ----------------

    pub fn accept(&mut self, ev: VaEvent) -> bool {
        self.run.accept(ev)
    }

    /// Событие завершило диалог, но run этого диалога ещё активен.
    pub fn stop_due(&self) -> bool {
        (self.run.run_active || self.run.awaiting_run_start) && !self.stop_sent
    }

    pub fn mark_stop_sent(&mut self) {
        self.stop_sent = true;
    }

    pub fn stop_sent(&self) -> bool {
        self.stop_sent
    }

    /// HA отказался запускать run: ждать RUN_START больше нечего.
    pub fn no_run_follows(&mut self) {
        self.run.awaiting_run_start = false;
    }

    pub fn reset_run(&mut self) {
        self.run.reset();
    }

    pub fn voice_run(&self) -> u64 {
        self.voice_run
    }

    pub fn set_voice_run(&mut self, run: u64) {
        self.voice_run = run;
    }

    // ---------------- диалог ----------------

    /// Начало диалога: единственный вход в LISTENING.
    pub fn begin(&mut self, now: i64, wake: bool, endpoint: Option<EndpointRun>) {
        self.current_id = self.current_id.next();
        self.stop_sent = false;
        self.followup = Followup::default();
        self.run.start();
        self.dialog = Some(Dialog {
            phase: DialogPhase::Listening { endpoint },
            started_ms: now,
            wake,
            tts_url: String::new(),
        });
    }

    /// Конец диалога.
    pub fn finish(&mut self) {
        self.run.awaiting_run_start = false;
        self.current_id = self.current_id.next();
        self.followup = Followup::default();
        self.dialog = None;
    }

    /// HA закончил фразу первым, диалог отменён или его заменили: детектор конца фразы
    /// снимается, LISTENING становится PROCESSING.
    pub fn stop_listening(&mut self) {
        let Some(d) = &mut self.dialog else {
            return;
        };
        if let DialogPhase::Listening { endpoint } = &mut d.phase {
            if let Some(r) = endpoint {
                r.cancel();
            }
            d.phase = DialogPhase::Processing;
        }
    }

    pub fn endpoint(&self) -> Option<&EndpointRun> {
        match self.dialog.as_ref().map(|d| &d.phase) {
            Some(DialogPhase::Listening { endpoint }) => endpoint.as_ref(),
            _ => None,
        }
    }

    pub fn push_listen_level(&mut self, level_db: f32) {
        if let Some(DialogPhase::Listening { endpoint: Some(r) }) =
            self.dialog.as_mut().map(|d| &mut d.phase)
        {
            r.push(level_db, true);
        }
    }

    pub fn take_endpoint_end(&mut self) -> bool {
        match self.dialog.as_mut().map(|d| &mut d.phase) {
            Some(DialogPhase::Listening { endpoint }) => {
                endpoint.as_mut().is_some_and(EndpointRun::take_end)
            }
            _ => false,
        }
    }

    // ---------------- TTS ----------------

    /// RUN_START: новый run сбрасывает «ответ уже играл», как `tts_played = false` в HEAD.
    pub fn set_tts_url(&mut self, url: &str) {
        let Some(d) = &mut self.dialog else {
            return;
        };
        url.clone_into(&mut d.tts_url);
        if let DialogPhase::Speaking { played } = &mut d.phase {
            *played = false;
        }
    }

    /// TTS_END: URL ответа, без сброса уже сыгранного ответа.
    pub fn update_tts_url(&mut self, url: &str) {
        if url.is_empty() {
            return;
        }
        if let Some(d) = &mut self.dialog {
            url.clone_into(&mut d.tts_url);
        }
    }

    /// Ответ начинает играть; None — уже играет или URL ещё нет.
    pub fn take_tts(&mut self) -> Option<(DialogId, String)> {
        let Some(d) = &mut self.dialog else {
            return None;
        };
        if matches!(d.phase, DialogPhase::Speaking { played: true }) || d.tts_url.is_empty() {
            return None;
        }
        d.phase = DialogPhase::Speaking { played: true };
        Some((self.current_id, mem::take(&mut d.tts_url)))
    }

    pub fn owe_tts_report(&mut self) {
        self.report_owed = true;
    }

    /// Забирает долг отчёта; true — отправить AnnounceFinished.
    pub fn take_tts_report(&mut self) -> bool {
        mem::take(&mut self.report_owed)
    }

    pub fn drop_tts_report(&mut self) {
        self.report_owed = false;
    }

    // ---------------- follow-up ----------------

    pub fn followup_on_event(&mut self, ev: VaEvent, continue_conversation: bool) {
        self.followup.on_event(ev, continue_conversation);
    }

    pub fn followup(&self) -> Followup {
        self.followup
    }

    pub fn set_followup(&mut self, followup: Followup) {
        self.followup = followup;
    }

    pub fn followup_awaiting_speech(&self) -> bool {
        self.followup.awaiting_speech()
    }

    pub fn followup_window_ms(&self, seconds: u32) -> Option<u64> {
        self.followup.window_ms(seconds)
    }

    pub fn followup_silent(&self, code: &str) -> bool {
        self.followup.silent(code)
    }

    pub fn reset_followup(&mut self) {
        self.followup = Followup::default();
    }

    // ---------------- записанное срабатывание ----------------

    pub fn set_record(&mut self, record: Option<WakeRecord>) {
        self.record = record;
    }

    pub fn mark_wake_cancelled(&mut self) {
        if let Some(rec) = &mut self.record {
            rec.cancelled = true;
        }
    }

    pub fn record_stt(&mut self, text: &str) {
        if let Some(rec) = &mut self.record {
            rec.stt = Some(text.to_owned());
        }
    }

    pub fn record_arbiter_lost(&mut self) {
        if let Some(rec) = &mut self.record {
            rec.arbiter_lost = true;
        }
    }

    pub fn record_error(&mut self) {
        if let Some(rec) = &mut self.record {
            rec.error = true;
        }
    }

    /// Записанное срабатывание текущего разговора; None — диалог не от wake word.
    pub fn take_record(&mut self) -> Option<WakeRecord> {
        self.record.take()
    }

    // ---------------- объявления и earcons ----------------

    #[cfg(test)]
    pub fn announce_id(&self) -> Option<AnnounceId> {
        self.announce
    }

    pub fn start_announce(&mut self) -> AnnounceId {
        let seq = self.announce_seq.next();
        self.announce_seq = seq;
        self.announce = Some(seq);
        seq
    }

    /// Завершение объявления `seq`; false — это уже не текущее объявление.
    pub fn finish_announce(&mut self, seq: AnnounceId) -> bool {
        if self.announce == Some(seq) {
            self.announce = None;
            true
        } else {
            false
        }
    }

    pub fn interrupt_announce(&mut self) {
        self.announce = None;
    }

    pub fn start_earcon(&mut self) -> EarconSeq {
        let seq = self.earcon_seq.next();
        self.earcon_seq = seq;
        self.earcon = Some(seq);
        seq
    }

    /// Конец earcon `seq`; false — играет уже другой.
    pub fn finish_earcon(&mut self, seq: EarconSeq) -> bool {
        if self.earcon == Some(seq) {
            self.earcon = None;
            true
        } else {
            false
        }
    }

    // ---------------- музыка этой станции ----------------

    pub fn on_music_state(&mut self, s: PlayState, now: i64) {
        match s {
            PlayState::Paused => self.music_paused_ms = Some(now),
            // HA media took the speaker: an ended Sendspin stream is not this station's music
            // any more, whichever of the two events came first.
            PlayState::Loading | PlayState::Playing => self.sendspin_ended_ms = None,
            _ => {}
        }
    }

    pub fn on_sendspin_stream(&mut self, active: bool, taken_over: bool, now: i64) {
        self.sendspin_ended_ms = (!active && !taken_over).then_some(now);
    }

    pub fn owns_music(
        &self,
        now: i64,
        music: PlayState,
        sendspin_streaming: bool,
        sendspin_connected: bool,
    ) -> bool {
        logic::owns_music(
            now,
            music,
            self.music_paused_ms,
            sendspin_streaming,
            sendspin_connected,
            self.sendspin_ended_ms,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsar_audio::endpoint::EndpointPrior;

    #[test]
    fn listening_reports_a_local_end_once_and_processing_gets_the_long_watchdog() {
        let prior = EndpointPrior {
            noise_db: -60.0,
            speech_db: -20.0,
        };
        let run = EndpointRun::new(prior, &[], &[]).expect("thresholds");
        let mut s = Session::default();
        s.begin(0, false, Some(run));
        assert_eq!(s.phase(), Phase::Listening);
        assert_eq!(s.watchdog_ms(), Some(LISTEN_TIMEOUT_MS));
        for _ in 0..6 {
            s.push_listen_level(-25.0);
        }
        for _ in 0..31 {
            s.push_listen_level(-60.0);
        }
        assert!(s.take_endpoint_end());
        assert!(!s.take_endpoint_end(), "one graceful end per dialog");
        s.stop_listening();
        assert_eq!(s.phase(), Phase::Processing);
        assert_eq!(s.watchdog_ms(), Some(PROCESS_TIMEOUT_MS));
    }
}
