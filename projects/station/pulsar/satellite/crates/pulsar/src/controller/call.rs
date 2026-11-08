//! Звонок в контроллере: звонок на колонку — «интерком», колонка отвечает сама.
//!
//! Входящий звонок переводит станцию в [`logic::Phase::Call`]: спотеры выключены, музыка и
//! Sendspin на паузе, очищенный VQE-поток уходит в uplink. Мут микрофона глушит только
//! uplink (downlink слышен), кнопка действия сбрасывает звонок. Завершение возвращает всё
//! как было: спотеры, музыка, обычное кольцо.
//!
//! Аудио звонка никуда не пишется; в логах только факт входящего/завершения.

use pulsar_call::{CallEvent, CallPhase};
use pulsar_media::Channel;
use tracing::info;

use super::{Controller, earcons};

impl Controller {
    pub(super) fn on_call_event(&mut self, ev: Option<CallEvent>) {
        let Some(ev) = ev else {
            self.rx.call = None;
            return;
        };
        match ev {
            CallEvent::Incoming { .. } => {
                if self.call_active {
                    return;
                }
                self.call_active = true;
                info!("call: incoming, station answers");
                // Один поток VQE на всех: во время звонка он идёт в uplink, а не в HA.
                self.deps.frontend.set_streaming(true);
                self.deps.frontend.set_detection(false);
                self.deps.frontend.set_commands(false);
                self.deps.call.set_muted(self.mic_muted());
                self.call_paused_music = self.music_pause();
                // Короткий сигнал: отдельного «звонкового» ассета в прошивке нет, берём
                // таймерный earcon (~1.5 с) — он не пересекается с речью и узнаваем.
                self.deps.player.play(
                    Channel::Announce,
                    vec![earcons::path(self.model.earcons.timer)],
                );
                self.update_ui();
            }
            CallEvent::Active => {
                if self.call_active {
                    info!("call: media active");
                    self.update_ui();
                }
            }
            CallEvent::Ended => {
                if !self.call_active {
                    return;
                }
                self.call_active = false;
                info!("call: ended");
                self.deps.frontend.set_streaming(false);
                self.update_detection();
                self.update_commands();
                if self.call_paused_music {
                    self.call_paused_music = false;
                    self.music_resume();
                }
                self.update_ui();
                self.publish_media();
            }
        }
    }

    /// Идёт ли звонок (для тестов и проверок портов).
    #[cfg(test)]
    pub(super) fn in_call(&self) -> bool {
        self.call_active
    }

    /// Фаза звонка из порта (Idle, когда модуль выключен).
    pub(super) fn call_phase(&self) -> CallPhase {
        self.deps.call.phase()
    }
}
