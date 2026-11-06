//! Состояние фронтенда: явная машина переходов без атомиков.

/// What the front-end is doing now; [`StateMachine::state`] derives it from the flags the
/// controller sets. `WakePending` carries the number of `Wake` events not yet handled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum FrontendState {
    /// Детекция выключена (программный мьют): микрофон читается, wake-событий нет.
    #[default]
    Idle,
    /// Детектор слушает; необработанных `Wake` нет.
    Detecting,
    /// Отправлено `pending` событий `Wake`, владелец ещё не позвал `wake_handled`: `Audio`
    /// продолжает пре-ролл без дыры.
    WakePending(u32),
    /// Владелец держит поток открытым (`set_streaming`).
    Streaming,
    /// Рикардо слушает, но срабатывание только записывается (`wake_dry_run`).
    DryRun,
}

/// Один переход машины состояния.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StateEvent {
    Detecting(bool),
    Streaming(bool),
    DryRun(bool),
    /// Отправлено ещё одно `Wake`.
    Wake,
    /// Владелец обработал одно `Wake`.
    WakeHandled,
}

/// Состояние фронтенда на потоке обработки: три переключателя и счётчик необработанных `Wake`.
/// Атомиков нет: порядок задаёт FIFO канала команд, а порядок событий — канал событий
/// (`UnboundedSender`/`Receiver`), поэтому протокол `wake_pending` выражен типом
/// [`FrontendState::WakePending`], а не парой `Acquire`/`AcqRel`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct StateMachine {
    detecting: bool,
    streaming: bool,
    dry_run: bool,
    pending: u32,
}

impl StateMachine {
    pub(super) fn new(detecting: bool) -> Self {
        Self {
            detecting,
            streaming: false,
            dry_run: false,
            pending: 0,
        }
    }

    /// Чистый переход: без ввода-вывода и блокировок.
    pub(super) fn apply(&mut self, ev: StateEvent) {
        match ev {
            StateEvent::Detecting(on) => self.detecting = on,
            StateEvent::Streaming(on) => self.streaming = on,
            StateEvent::DryRun(on) => self.dry_run = on,
            // `saturating_add`: счётчик — это «сколько Wake не обработано», а не номер события.
            StateEvent::Wake => self.pending = self.pending.saturating_add(1),
            // Лишний `wake_handled` не уводит счётчик ниже нуля и не ошибка.
            StateEvent::WakeHandled => self.pending = self.pending.saturating_sub(1),
        }
    }

    pub(super) fn detecting(&self) -> bool {
        self.detecting
    }

    pub(super) fn dry_run(&self) -> bool {
        self.dry_run
    }

    /// `Audio` идёт, пока владелец не обработал все `Wake`, либо пока включён стрим.
    pub(super) fn audio_on(&self) -> bool {
        matches!(
            self.state(),
            FrontendState::Streaming | FrontendState::WakePending(_)
        )
    }

    pub(super) fn state(&self) -> FrontendState {
        if self.pending > 0 {
            FrontendState::WakePending(self.pending)
        } else if self.streaming {
            FrontendState::Streaming
        } else if !self.detecting {
            FrontendState::Idle
        } else if self.dry_run {
            FrontendState::DryRun
        } else {
            FrontendState::Detecting
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_machine_is_a_pure_transition_table() {
        let mut s = StateMachine::new(true);
        assert_eq!(s.state(), FrontendState::Detecting);
        s.apply(StateEvent::Detecting(false));
        assert_eq!(s.state(), FrontendState::Idle);
        // Даже при выключенной детекции необработанный Wake держит `Audio` (он уже отправлен).
        s.apply(StateEvent::Wake);
        assert_eq!(s.state(), FrontendState::WakePending(1));
        assert!(s.audio_on());
        s.apply(StateEvent::Wake);
        assert_eq!(s.state(), FrontendState::WakePending(2));
        s.apply(StateEvent::WakeHandled);
        assert_eq!(s.state(), FrontendState::WakePending(1));
        s.apply(StateEvent::WakeHandled);
        assert_eq!(s.state(), FrontendState::Idle);
        // Лишний `wake_handled` не уводит счётчик ниже нуля.
        s.apply(StateEvent::WakeHandled);
        assert_eq!(s.state(), FrontendState::Idle);
        assert!(!s.audio_on());
        s.apply(StateEvent::Detecting(true));
        s.apply(StateEvent::DryRun(true));
        assert_eq!(s.state(), FrontendState::DryRun);
        // Стрим важнее тестового режима: он уже держит канал открытым.
        s.apply(StateEvent::Streaming(true));
        assert_eq!(s.state(), FrontendState::Streaming);
        s.apply(StateEvent::Streaming(false));
        assert_eq!(s.state(), FrontendState::DryRun);
        s.apply(StateEvent::DryRun(false));
        assert_eq!(s.state(), FrontendState::Detecting);
    }
}
