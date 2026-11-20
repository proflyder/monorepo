#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum FrontendState {
    #[default]
    Idle,
    Detecting,
    WakePending(u32),
    Streaming,
    DryRun,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StateEvent {
    Detecting(bool),
    Streaming(bool),
    DryRun(bool),
    Wake,
    WakeHandled,
}

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

    pub(super) fn apply(&mut self, ev: StateEvent) {
        match ev {
            StateEvent::Detecting(on) => self.detecting = on,
            StateEvent::Streaming(on) => self.streaming = on,
            StateEvent::DryRun(on) => self.dry_run = on,
            StateEvent::Wake => self.pending = self.pending.saturating_add(1),
            StateEvent::WakeHandled => self.pending = self.pending.saturating_sub(1),
        }
    }

    pub(super) fn detecting(&self) -> bool {
        self.detecting
    }

    pub(super) fn dry_run(&self) -> bool {
        self.dry_run
    }

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
        s.apply(StateEvent::Wake);
        assert_eq!(s.state(), FrontendState::WakePending(1));
        assert!(s.audio_on());
        s.apply(StateEvent::Wake);
        assert_eq!(s.state(), FrontendState::WakePending(2));
        s.apply(StateEvent::WakeHandled);
        assert_eq!(s.state(), FrontendState::WakePending(1));
        s.apply(StateEvent::WakeHandled);
        assert_eq!(s.state(), FrontendState::Idle);
        s.apply(StateEvent::WakeHandled);
        assert_eq!(s.state(), FrontendState::Idle);
        assert!(!s.audio_on());
        s.apply(StateEvent::Detecting(true));
        s.apply(StateEvent::DryRun(true));
        assert_eq!(s.state(), FrontendState::DryRun);
        s.apply(StateEvent::Streaming(true));
        assert_eq!(s.state(), FrontendState::Streaming);
        s.apply(StateEvent::Streaming(false));
        assert_eq!(s.state(), FrontendState::DryRun);
        s.apply(StateEvent::DryRun(false));
        assert_eq!(s.state(), FrontendState::Detecting);
    }
}
