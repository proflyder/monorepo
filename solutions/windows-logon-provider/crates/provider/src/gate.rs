//! Cold-boot gate: a grant may sign in only while no session of the configured
//! user exists and the machine booted recently.
//!
//! Windows 11 calls `SetUsageScenario(CPUS_LOGON)` when LogonUI appears after
//! `LockWorkStation`, not only after a cold boot. The scenario alone therefore
//! cannot tell a fresh start from an already running PC, so both decisions live
//! here: an existing WTS session of the account is a hard deny, and the uptime
//! bound is the second line of defence. The decision itself is pure and
//! host-tested; the WTS query is Windows-only.

/// Cold-boot window from `GetTickCount64()`: a grant is considered only within
/// the first 10 minutes after boot. Lives next to the worker budget in
/// [`crate::provider`].
pub use crate::provider::MAX_UPTIME_MS;

/// One session as reported by `WTSEnumerateSessionsW` +
/// `WTSQuerySessionInformationW`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WtsSession {
    pub session_id: u32,
    pub user: String,
}

/// Why a grant may or may not arm the tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogonGate {
    /// No session of the configured user, and the machine booted recently.
    Allow,
    /// The configured user already has a session in any state.
    SessionExists,
    /// The machine has been up for longer than [`MAX_UPTIME_MS`].
    Uptime,
    /// A WTS call failed: fail closed.
    QueryFailed,
    /// The registry configuration is missing or invalid: fail closed.
    ConfigUnavailable,
}

impl LogonGate {
    /// Short log-safe label used in `scenario cpus=logon gate=<label>`.
    pub fn label(self) -> &'static str {
        match self {
            Self::Allow => "ok",
            Self::SessionExists => "session_exists",
            Self::Uptime => "uptime",
            Self::QueryFailed => "session_query_failed",
            Self::ConfigUnavailable => "config_unavailable",
        }
    }
}

pub fn within_uptime(uptime_ms: u64) -> bool {
    uptime_ms < MAX_UPTIME_MS
}

/// Decides whether a grant may arm the tile.
///
/// `sessions` is `None` when the WTS query failed. Session 0 and entries with
/// an empty user name are ignored, as is the session state: an `Active`,
/// `Disconnected` or any other session of the user means the PC is already in
/// use. Only the user name is compared (case-insensitively): the WTS domain
/// may be a NetBIOS name, a full name or empty for a Microsoft account, so
/// requiring it to match would be fail-open.
pub fn decide(configured_user: &str, uptime_ms: u64, sessions: Option<&[WtsSession]>) -> LogonGate {
    let Some(sessions) = sessions else {
        return LogonGate::QueryFailed;
    };
    let session_exists = sessions.iter().any(|session| {
        session.session_id != 0
            && !session.user.is_empty()
            && session.user.eq_ignore_ascii_case(configured_user)
    });
    if session_exists {
        return LogonGate::SessionExists;
    }
    if !within_uptime(uptime_ms) {
        return LogonGate::Uptime;
    }
    LogonGate::Allow
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

    use crate::config::{Config, ConfigState, load_config};

    use super::{LogonGate, WtsSession, decide};

    /// One `WTSQuerySessionInformationW` string, `None` when the call fails.
    fn query_string(session_id: u32, info_class: WTS_INFO_CLASS) -> Option<String> {
        let mut buffer = PWSTR::null();
        let mut bytes = 0u32;
        // SAFETY: `buffer` and `bytes` are valid out-parameters; the WTS server
        // handle is a documented constant. The returned buffer is freed below.
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

    /// Enumerates every session except session 0. `None` on any WTS failure so
    /// the caller fails closed.
    fn query_sessions() -> Option<Vec<WtsSession>> {
        let mut info: *mut WTS_SESSION_INFOW = ptr::null_mut();
        let mut count = 0u32;
        // SAFETY: both out-parameters are valid; version 1 is the only
        // supported WTS API version.
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

    /// Runs both gates against the live system. Call it before starting the
    /// worker, so a denied gate never reaches the network.
    pub fn evaluate() -> LogonGate {
        let mut config = Config::default();
        let mut error = String::new();
        if load_config(&mut config, &mut error) != ConfigState::Ok {
            return LogonGate::ConfigUnavailable;
        }
        let Some(sessions) = query_sessions() else {
            return LogonGate::QueryFailed;
        };
        // SAFETY: GetTickCount64 takes no arguments.
        let uptime_ms = unsafe { GetTickCount64() };
        decide(&config.user, uptime_ms, Some(&sessions))
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

    #[test]
    fn no_sessions_allows_a_fresh_boot() {
        assert_eq!(decide("Alice", 1_000, Some(&[])), LogonGate::Allow);
    }

    #[test]
    fn the_configured_user_session_denies_in_every_state() {
        for sessions in [
            vec![session(1, "alice")],
            vec![session(2, "ALICE")],
            vec![session(1, "bob"), session(3, "Alice")],
        ] {
            assert_eq!(
                decide("Alice", 1_000, Some(&sessions)),
                LogonGate::SessionExists
            );
        }
    }

    #[test]
    fn only_the_user_name_matters_not_the_domain() {
        // The WTS domain may disagree with the configured domain (NetBIOS,
        // full name, Microsoft account), so the same name must still deny.
        let sessions = [session(1, "alice")];
        assert_eq!(
            decide("Alice", 1_000, Some(&sessions)),
            LogonGate::SessionExists
        );
    }

    #[test]
    fn another_users_session_does_not_deny() {
        let sessions = [session(1, "bob")];
        assert_eq!(decide("Alice", 1_000, Some(&sessions)), LogonGate::Allow);
    }

    #[test]
    fn session_zero_and_empty_names_are_ignored() {
        let sessions = [session(0, "alice"), session(1, "")];
        assert_eq!(decide("Alice", 1_000, Some(&sessions)), LogonGate::Allow);
    }

    #[test]
    fn uptime_is_the_second_gate() {
        assert_eq!(
            decide("Alice", MAX_UPTIME_MS - 1, Some(&[])),
            LogonGate::Allow
        );
        assert_eq!(decide("Alice", MAX_UPTIME_MS, Some(&[])), LogonGate::Uptime);
        assert_eq!(
            decide("Alice", MAX_UPTIME_MS * 10, Some(&[])),
            LogonGate::Uptime
        );
    }

    #[test]
    fn a_failed_query_denies() {
        assert_eq!(decide("Alice", 1_000, None), LogonGate::QueryFailed);
    }

    #[test]
    fn labels_are_log_safe() {
        assert_eq!(LogonGate::Allow.label(), "ok");
        assert_eq!(LogonGate::SessionExists.label(), "session_exists");
        assert_eq!(LogonGate::Uptime.label(), "uptime");
        assert_eq!(LogonGate::QueryFailed.label(), "session_query_failed");
        assert_eq!(LogonGate::ConfigUnavailable.label(), "config_unavailable");
    }
}
