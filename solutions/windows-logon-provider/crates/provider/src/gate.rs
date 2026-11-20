use wakegrant_protocol::Purpose;

pub use crate::provider::MAX_UPTIME_MS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OffReason {
    Uptime,
    QueryFailed,
    UnlockWithoutSession,
    ConfigUnavailable,
    Disabled,
}

impl OffReason {
    pub fn label(self) -> &'static str {
        match self {
            Self::Uptime => "uptime",
            Self::QueryFailed => "session_query_failed",
            Self::UnlockWithoutSession => "unlock_without_session",
            Self::ConfigUnavailable => "config_unavailable",
            Self::Disabled => "disabled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Logon,
    Unlock,
    Off(OffReason),
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Logon => "logon",
            Self::Unlock => "unlock",
            Self::Off(reason) => reason.label(),
        }
    }
}

pub fn within_uptime(uptime_ms: u64) -> bool {
    uptime_ms < MAX_UPTIME_MS
}

pub fn decide_mode(
    scenario: Purpose,
    session_exists: bool,
    uptime_ms: u64,
    query_ok: bool,
) -> Mode {
    if !query_ok {
        return Mode::Off(OffReason::QueryFailed);
    }
    match scenario {
        Purpose::Logon => {
            if session_exists {
                Mode::Unlock
            } else if within_uptime(uptime_ms) {
                Mode::Logon
            } else {
                Mode::Off(OffReason::Uptime)
            }
        }
        Purpose::Unlock => {
            if session_exists {
                Mode::Unlock
            } else {
                Mode::Off(OffReason::UnlockWithoutSession)
            }
        }
    }
}

pub fn mode_for_sessions(
    scenario: Purpose,
    configured_user: &str,
    uptime_ms: u64,
    sessions: Option<&[WtsSession]>,
) -> Mode {
    let Some(sessions) = sessions else {
        return decide_mode(scenario, false, uptime_ms, false);
    };
    decide_mode(
        scenario,
        session_exists(configured_user, sessions),
        uptime_ms,
        true,
    )
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WtsSession {
    pub session_id: u32,
    pub user: String,
}

pub fn session_exists(configured_user: &str, sessions: &[WtsSession]) -> bool {
    sessions.iter().any(|session| {
        session.session_id != 0
            && !session.user.is_empty()
            && session.user.eq_ignore_ascii_case(configured_user)
    })
}

#[cfg(windows)]
pub use imp::evaluate;

#[cfg(windows)]
mod imp {
    use core::ptr;
    use windows_core::PWSTR;

    use crate::bindings::Windows::Win32::System::RemoteDesktop::{
        WTS_CURRENT_SERVER_HANDLE, WTS_INFO_CLASS, WTS_SESSION_INFOW, WTSEnumerateSessionsW,
        WTSFreeMemory, WTSQuerySessionInformationW, WTSUserName,
    };
    use crate::bindings::Windows::Win32::System::SystemInformation::GetTickCount64;

    use crate::config::{ConfigError, load_config};

    use wakegrant_protocol::Purpose;

    use super::{Mode, OffReason, WtsSession, mode_for_sessions};

    fn query_string(session_id: u32, info_class: WTS_INFO_CLASS) -> Option<String> {
        let mut buffer = PWSTR::null();
        let mut bytes = 0u32;
        // SAFETY: `buffer` and `bytes` are valid out-parameters; the WTS server handle is a documented constant. The returned buffer is freed below.
        let result = unsafe {
            WTSQuerySessionInformationW(
                Some(WTS_CURRENT_SERVER_HANDLE),
                session_id,
                info_class,
                &raw mut buffer,
                &raw mut bytes,
            )
        };
        if result.is_err() || buffer.is_null() {
            if !buffer.is_null() {
                // SAFETY: WTS allocated `buffer` with its own heap.
                unsafe { WTSFreeMemory(buffer.0.cast()) };
            }
            return None;
        }
        let units = bytes as usize / 2;
        let text = if units == 0 {
            String::new()
        } else {
            // SAFETY: WTS returned `units` UTF-16 units at `buffer`.
            let wide = unsafe { core::slice::from_raw_parts(buffer.0, units) };
            let end = wide
                .iter()
                .position(|&unit| unit == 0)
                .unwrap_or(wide.len());
            String::from_utf16_lossy(&wide[..end])
        };
        // SAFETY: the buffer came from WTSQuerySessionInformationW.
        unsafe { WTSFreeMemory(buffer.0.cast()) };
        Some(text)
    }

    fn query_sessions() -> Option<Vec<WtsSession>> {
        let mut info: *mut WTS_SESSION_INFOW = ptr::null_mut();
        let mut count = 0u32;
        // SAFETY: both out-parameters are valid; version 1 is the only supported WTS API version.
        let result = unsafe {
            WTSEnumerateSessionsW(
                Some(WTS_CURRENT_SERVER_HANDLE),
                0,
                1,
                &raw mut info,
                &raw mut count,
            )
        };
        if result.is_err() {
            return None;
        }
        if info.is_null() {
            return Some(Vec::new());
        }
        let sessions = (|| {
            // SAFETY: WTS returned `count` entries at `info`.
            let entries = unsafe { core::slice::from_raw_parts(info, count as usize) };
            let mut sessions = Vec::with_capacity(entries.len());
            for entry in entries {
                if entry.SessionId == 0 {
                    continue;
                }
                let user = query_string(entry.SessionId, WTSUserName)?;
                sessions.push(WtsSession {
                    session_id: entry.SessionId,
                    user,
                });
            }
            Some(sessions)
        })();
        // SAFETY: WTS allocated the session array; it is freed exactly once.
        unsafe { WTSFreeMemory(info.cast()) };
        sessions
    }

    pub fn evaluate(scenario: Purpose) -> Mode {
        let config = match load_config() {
            Ok(config) => config,
            Err(ConfigError::Disabled) => return Mode::Off(OffReason::Disabled),
            Err(ConfigError::Invalid(_)) => return Mode::Off(OffReason::ConfigUnavailable),
        };
        let Some(sessions) = query_sessions() else {
            return Mode::Off(OffReason::QueryFailed);
        };
        // SAFETY: GetTickCount64 takes no arguments.
        let uptime_ms = unsafe { GetTickCount64() };
        mode_for_sessions(scenario, &config.user, uptime_ms, Some(&sessions))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(session_id: u32, user: &str) -> WtsSession {
        WtsSession {
            session_id,
            user: user.to_owned(),
        }
    }

    fn logon(user: &str, uptime: u64, sessions: Option<&[WtsSession]>) -> Mode {
        mode_for_sessions(Purpose::Logon, user, uptime, sessions)
    }

    fn unlock(user: &str, uptime: u64, sessions: Option<&[WtsSession]>) -> Mode {
        mode_for_sessions(Purpose::Unlock, user, uptime, sessions)
    }

    #[test]
    fn no_sessions_and_a_fresh_boot_select_logon() {
        assert_eq!(logon("Alice", 1_000, Some(&[])), Mode::Logon);
        assert_eq!(
            unlock("Alice", 1_000, Some(&[])),
            Mode::Off(OffReason::UnlockWithoutSession)
        );
    }

    #[test]
    fn the_configured_user_session_selects_unlock_in_every_state() {
        for sessions in [
            vec![session(1, "alice")],
            vec![session(2, "ALICE")],
            vec![session(1, "bob"), session(3, "Alice")],
        ] {
            assert_eq!(logon("Alice", 1_000, Some(&sessions)), Mode::Unlock);
            assert_eq!(unlock("Alice", 1_000, Some(&sessions)), Mode::Unlock);
        }
    }

    #[test]
    fn uptime_does_not_matter_for_unlock() {
        let sessions = [session(1, "alice")];
        assert_eq!(
            logon("Alice", MAX_UPTIME_MS * 10, Some(&sessions)),
            Mode::Unlock
        );
        assert_eq!(
            unlock("Alice", MAX_UPTIME_MS * 10, Some(&sessions)),
            Mode::Unlock
        );
    }

    #[test]
    fn only_the_user_name_matters_not_the_domain() {
        let sessions = [session(1, "alice")];
        assert_eq!(logon("Alice", 1_000, Some(&sessions)), Mode::Unlock);
    }

    #[test]
    fn another_users_session_does_not_select_unlock() {
        let sessions = [session(1, "bob")];
        assert_eq!(logon("Alice", 1_000, Some(&sessions)), Mode::Logon);
        assert_eq!(
            unlock("Alice", 1_000, Some(&sessions)),
            Mode::Off(OffReason::UnlockWithoutSession)
        );
    }

    #[test]
    fn session_zero_and_empty_names_are_ignored() {
        let sessions = [session(0, "alice"), session(1, "")];
        assert_eq!(logon("Alice", 1_000, Some(&sessions)), Mode::Logon);
    }

    #[test]
    fn uptime_is_the_second_logon_gate() {
        assert_eq!(logon("Alice", MAX_UPTIME_MS - 1, Some(&[])), Mode::Logon);
        assert_eq!(
            logon("Alice", MAX_UPTIME_MS, Some(&[])),
            Mode::Off(OffReason::Uptime)
        );
        assert_eq!(
            logon("Alice", MAX_UPTIME_MS * 10, Some(&[])),
            Mode::Off(OffReason::Uptime)
        );
    }

    #[test]
    fn a_failed_query_turns_everything_off() {
        assert_eq!(
            logon("Alice", 1_000, None),
            Mode::Off(OffReason::QueryFailed)
        );
        assert_eq!(
            unlock("Alice", 1_000, None),
            Mode::Off(OffReason::QueryFailed)
        );
    }

    #[test]
    fn the_pure_decision_matches_the_table() {
        assert_eq!(decide_mode(Purpose::Logon, false, 1_000, true), Mode::Logon);
        assert_eq!(decide_mode(Purpose::Logon, true, 1_000, true), Mode::Unlock);
        assert_eq!(
            decide_mode(Purpose::Logon, false, MAX_UPTIME_MS, true),
            Mode::Off(OffReason::Uptime)
        );
        assert_eq!(
            decide_mode(Purpose::Unlock, true, MAX_UPTIME_MS * 10, true),
            Mode::Unlock
        );
        assert_eq!(
            decide_mode(Purpose::Unlock, false, 1_000, true),
            Mode::Off(OffReason::UnlockWithoutSession)
        );
        for scenario in [Purpose::Logon, Purpose::Unlock] {
            assert_eq!(
                decide_mode(scenario, true, 1_000, false),
                Mode::Off(OffReason::QueryFailed)
            );
        }
    }

    #[test]
    fn labels_are_log_safe() {
        assert_eq!(Purpose::Logon.as_str(), "logon");
        assert_eq!(Purpose::Unlock.as_str(), "unlock");
        assert_eq!(Mode::Logon.label(), "logon");
        assert_eq!(Mode::Unlock.label(), "unlock");
        assert_eq!(Mode::Off(OffReason::Uptime).label(), "uptime");
        assert_eq!(
            Mode::Off(OffReason::QueryFailed).label(),
            "session_query_failed"
        );
        assert_eq!(
            Mode::Off(OffReason::UnlockWithoutSession).label(),
            "unlock_without_session"
        );
        assert_eq!(
            Mode::Off(OffReason::ConfigUnavailable).label(),
            "config_unavailable"
        );
        assert_eq!(Mode::Off(OffReason::Disabled).label(), "disabled");
    }
}
