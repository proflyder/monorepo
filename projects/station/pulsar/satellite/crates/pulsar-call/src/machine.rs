use crate::CallEvent;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallPhase {
    Idle,
    Ringing,
    Active,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HangupReason {
    Unspecified,
    Hangup,
    Declined,
    Busy,
    Canceled,
    Timeout,
    Unreachable,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignalEvent {
    Ringing { call_id: String, caller: String },
    Active { call_id: String },
    Ended { call_id: String },
    Connecting { call_id: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Answer {
        call_id: String,
    },
    Hangup {
        call_id: String,
        reason: HangupReason,
    },
    Emit(CallEvent),
}

#[derive(Debug)]
pub struct Machine {
    phase: CallPhase,
    call_id: String,
    muted: bool,
    answered: bool,
}

impl Default for Machine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine {
    #[must_use]
    pub fn new() -> Self {
        Self {
            phase: CallPhase::Idle,
            call_id: String::new(),
            muted: false,
            answered: false,
        }
    }

    #[must_use]
    pub fn phase(&self) -> CallPhase {
        self.phase
    }

    #[cfg(test)]
    #[must_use]
    pub fn call_id(&self) -> &str {
        &self.call_id
    }

    #[must_use]
    pub fn muted(&self) -> bool {
        self.muted
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
    }

    pub fn on_signal(&mut self, event: SignalEvent) -> Vec<Action> {
        match event {
            SignalEvent::Ringing { call_id, caller } => match self.phase {
                CallPhase::Idle => {
                    self.phase = CallPhase::Ringing;
                    self.call_id = call_id;
                    self.answered = false;
                    self.muted = false;
                    vec![Action::Emit(CallEvent::Incoming { caller })]
                }
                CallPhase::Ringing | CallPhase::Active => vec![Action::Hangup {
                    call_id,
                    reason: HangupReason::Busy,
                }],
            },
            SignalEvent::Connecting { call_id } => {
                let _ = call_id;
                Vec::new()
            }
            SignalEvent::Active { call_id } => {
                if self.phase != CallPhase::Idle && self.call_id == call_id {
                    self.phase = CallPhase::Active;
                    vec![Action::Emit(CallEvent::Active)]
                } else {
                    Vec::new()
                }
            }
            SignalEvent::Ended { call_id } => {
                if self.phase == CallPhase::Idle || self.call_id != call_id {
                    return Vec::new();
                }
                self.phase = CallPhase::Idle;
                self.call_id.clear();
                self.answered = false;
                vec![Action::Emit(CallEvent::Ended)]
            }
        }
    }

    pub fn ring_elapsed(&mut self) -> Vec<Action> {
        if self.phase != CallPhase::Ringing || self.answered {
            return Vec::new();
        }
        self.answered = true;
        vec![Action::Answer {
            call_id: self.call_id.clone(),
        }]
    }

    pub fn local_hangup(&mut self) -> Vec<Action> {
        if self.phase == CallPhase::Idle {
            return Vec::new();
        }
        let call_id = std::mem::take(&mut self.call_id);
        self.phase = CallPhase::Idle;
        self.answered = false;
        vec![
            Action::Hangup {
                call_id,
                reason: HangupReason::Hangup,
            },
            Action::Emit(CallEvent::Ended),
        ]
    }

    pub fn media_failed(&mut self) -> Vec<Action> {
        if self.phase == CallPhase::Idle {
            return Vec::new();
        }
        let call_id = std::mem::take(&mut self.call_id);
        self.phase = CallPhase::Idle;
        self.answered = false;
        vec![
            Action::Hangup {
                call_id,
                reason: HangupReason::Unreachable,
            },
            Action::Emit(CallEvent::Ended),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ringing() -> Machine {
        let mut m = Machine::new();
        let actions = m.on_signal(SignalEvent::Ringing {
            call_id: "c1".into(),
            caller: "phone".into(),
        });
        assert_eq!(
            actions,
            vec![Action::Emit(CallEvent::Incoming {
                caller: "phone".into()
            })]
        );
        m
    }

    #[test]
    fn incoming_rings_then_answers_once_then_activates() {
        let mut m = ringing();
        assert_eq!(m.phase(), CallPhase::Ringing);
        assert_eq!(
            m.ring_elapsed(),
            vec![Action::Answer {
                call_id: "c1".into()
            }]
        );
        assert!(m.ring_elapsed().is_empty());
        assert_eq!(
            m.on_signal(SignalEvent::Active {
                call_id: "c1".into()
            }),
            vec![Action::Emit(CallEvent::Active)]
        );
        assert_eq!(m.phase(), CallPhase::Active);
        assert_eq!(
            m.on_signal(SignalEvent::Ended {
                call_id: "c1".into()
            }),
            vec![Action::Emit(CallEvent::Ended)]
        );
        assert_eq!(m.phase(), CallPhase::Idle);
    }

    #[test]
    fn mute_only_affects_the_uplink_flag() {
        let mut m = ringing();
        assert!(!m.muted());
        m.set_muted(true);
        assert!(m.muted());
        m.ring_elapsed();
        m.on_signal(SignalEvent::Active {
            call_id: "c1".into(),
        });
        assert_eq!(m.phase(), CallPhase::Active);
        assert!(m.muted());
    }

    #[test]
    fn the_action_button_ends_the_call() {
        let mut m = ringing();
        m.ring_elapsed();
        m.on_signal(SignalEvent::Active {
            call_id: "c1".into(),
        });
        assert_eq!(
            m.local_hangup(),
            vec![
                Action::Hangup {
                    call_id: "c1".into(),
                    reason: HangupReason::Hangup
                },
                Action::Emit(CallEvent::Ended)
            ]
        );
        assert_eq!(m.phase(), CallPhase::Idle);
        assert!(m.local_hangup().is_empty());
    }

    #[test]
    fn a_second_incoming_call_is_refused_as_busy() {
        let mut m = ringing();
        assert_eq!(
            m.on_signal(SignalEvent::Ringing {
                call_id: "c2".into(),
                caller: "phone".into()
            }),
            vec![Action::Hangup {
                call_id: "c2".into(),
                reason: HangupReason::Busy
            }]
        );
        assert_eq!(m.phase(), CallPhase::Ringing);
        assert_eq!(m.call_id(), "c1");
    }

    #[test]
    fn events_for_another_call_are_ignored() {
        let mut m = ringing();
        assert!(
            m.on_signal(SignalEvent::Active {
                call_id: "other".into()
            })
            .is_empty()
        );
        assert!(
            m.on_signal(SignalEvent::Ended {
                call_id: "other".into()
            })
            .is_empty()
        );
        assert_eq!(m.phase(), CallPhase::Ringing);
    }

    #[test]
    fn a_failed_media_teardown_ends_the_call_once() {
        let mut m = ringing();
        m.ring_elapsed();
        assert_eq!(
            m.media_failed(),
            vec![
                Action::Hangup {
                    call_id: "c1".into(),
                    reason: HangupReason::Unreachable
                },
                Action::Emit(CallEvent::Ended)
            ]
        );
        assert!(m.media_failed().is_empty());
        assert_eq!(m.phase(), CallPhase::Idle);
    }
}
