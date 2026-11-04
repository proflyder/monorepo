//! Minimal WinHTTP JSON POST client.
//!
//! No proxy, no redirects, no cookies, no credential attachment: the wakegrant
//! MAC is the only authenticator and it must not be relayed anywhere. Timeouts
//! and limits are the ones documented in the README (2/2/4/4 s by default,
//! 16 KiB body). The unlock claim raises only the receive timeout so it can
//! outlast the server's 25 s long-poll.
//!
//! Only the argument construction is host-portable; the WinHTTP calls are
//! behind `cfg(windows)`.

use logon_protocol::ServiceEndpoint;

use crate::wide::WideNul;

/// One HTTP exchange. `ok` means the transport succeeded (any HTTP status).
#[derive(Debug, Default)]
pub struct HttpResult {
    pub ok: bool,
    pub status: i32,
    pub body: Vec<u8>,
    /// Log-safe reason when `ok` is false.
    pub error: &'static str,
    pub elapsed_ms: u32,
}

/// The two NUL-terminated wide strings `WinHttpConnect`/`WinHttpOpenRequest`
/// require.
#[derive(Debug)]
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
pub(crate) struct RequestTargets {
    pub(crate) host: WideNul,
    pub(crate) target: WideNul,
}

/// Builds the WinHTTP arguments. Both are always NUL-terminated and non-empty;
/// an empty host or path is rejected before any Win32 call.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
pub(crate) fn request_targets(
    endpoint: &ServiceEndpoint,
    path: &str,
) -> Result<RequestTargets, &'static str> {
    if endpoint.host.is_empty() || path.is_empty() {
        return Err("invalid_target");
    }
    Ok(RequestTargets {
        host: WideNul::new(&endpoint.host),
        target: WideNul::new(path),
    })
}

#[cfg(windows)]
pub use winhttp::{post_json, post_json_with_receive_timeout};

#[cfg(windows)]
mod winhttp {
    use core::ffi::c_void;

    use crate::bindings::Windows::Win32::Networking::WinHttp::{
        WINHTTP_ACCESS_TYPE_NO_PROXY, WINHTTP_DISABLE_COOKIES, WINHTTP_DISABLE_REDIRECTS,
        WINHTTP_FLAG_SECURE, WINHTTP_OPEN_REQUEST_FLAGS, WINHTTP_OPTION_DISABLE_FEATURE,
        WINHTTP_OPTION_REDIRECT_POLICY, WINHTTP_OPTION_REDIRECT_POLICY_NEVER,
        WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect,
        WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable, WinHttpQueryHeaders,
        WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetOption,
        WinHttpSetTimeouts,
    };
    use crate::bindings::Windows::Win32::System::SystemInformation::GetTickCount64;
    use windows_core::{PCWSTR, w};

    use logon_protocol::ServiceEndpoint;

    use super::{HttpResult, request_targets};
    use crate::wide::WideNul;

    const RESOLVE_TIMEOUT_MS: i32 = 2000;
    const CONNECT_TIMEOUT_MS: i32 = 2000;
    const SEND_TIMEOUT_MS: i32 = 4000;
    const RECEIVE_TIMEOUT_MS: i32 = 4000;
    const MAX_RESPONSE_BYTES: usize = 16 * 1024;
    const USER_AGENT: &str = "RicardoKompLogon/1.0 (Windows LogonUI)";
    // The headers are passed with an explicit character count, so (unlike the
    // host and the target) they are not required to be NUL-terminated.
    const HEADERS: &str = "Content-Type: application/json\r\nAccept: application/json\r\n";

    /// Closes a WinHTTP handle exactly once.
    struct WinHttpHandle(*mut c_void);

    impl Drop for WinHttpHandle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: the handle was returned by WinHttpOpen and is owned here.
                unsafe {
                    let _ = WinHttpCloseHandle(self.0);
                }
            }
        }
    }

    fn post_json_inner(
        endpoint: &ServiceEndpoint,
        path: &str,
        body: &[u8],
        receive_timeout_ms: i32,
    ) -> Result<(i32, Vec<u8>), &'static str> {
        let targets = request_targets(endpoint, path)?;
        let user_agent = WideNul::new(USER_AGENT);

        // SAFETY: all arguments are valid; WINHTTP_ACCESS_TYPE_NO_PROXY forbids a
        // proxy, as documented in `README.md`.
        let session = WinHttpHandle(unsafe {
            WinHttpOpen(
                user_agent.as_pcwstr(),
                WINHTTP_ACCESS_TYPE_NO_PROXY,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            )
        });
        if session.0.is_null() {
            return Err("session_open_failed");
        }
        // SAFETY: `session` is an open WinHTTP session handle.
        unsafe {
            let _ = WinHttpSetTimeouts(
                session.0,
                RESOLVE_TIMEOUT_MS,
                CONNECT_TIMEOUT_MS,
                SEND_TIMEOUT_MS,
                receive_timeout_ms,
            );
        }

        let disable = (WINHTTP_DISABLE_REDIRECTS | WINHTTP_DISABLE_COOKIES).to_le_bytes();
        // SAFETY: an open session handle and a live DWORD buffer.
        unsafe {
            let _ = WinHttpSetOption(
                Some(session.0),
                WINHTTP_OPTION_DISABLE_FEATURE,
                Some(&disable),
            );
        }

        // SAFETY: `targets.host` is NUL-terminated by construction; `session` is
        // open.
        let connection = WinHttpHandle(unsafe {
            WinHttpConnect(session.0, targets.host.as_pcwstr(), endpoint.port, 0)
        });
        if connection.0.is_null() {
            return Err("connect_failed");
        }

        let flags = if endpoint.tls {
            WINHTTP_FLAG_SECURE
        } else {
            WINHTTP_OPEN_REQUEST_FLAGS(0)
        };
        // SAFETY: `targets.target` is NUL-terminated by construction; the
        // connection handle is open.
        let request = WinHttpHandle(unsafe {
            WinHttpOpenRequest(
                connection.0,
                w!("POST"),
                targets.target.as_pcwstr(),
                PCWSTR::null(),
                PCWSTR::null(),
                core::ptr::null(),
                flags,
            )
        });
        if request.0.is_null() {
            return Err("request_open_failed");
        }

        // Belt and braces: a redirect must never carry the mac to another host.
        let redirect_policy = WINHTTP_OPTION_REDIRECT_POLICY_NEVER.to_le_bytes();
        let disable = (WINHTTP_DISABLE_REDIRECTS | WINHTTP_DISABLE_COOKIES).to_le_bytes();
        // SAFETY: the request handle is open and the option buffers are live.
        unsafe {
            let _ = WinHttpSetOption(
                Some(request.0),
                WINHTTP_OPTION_REDIRECT_POLICY,
                Some(&redirect_policy),
            );
            let _ = WinHttpSetOption(
                Some(request.0),
                WINHTTP_OPTION_DISABLE_FEATURE,
                Some(&disable),
            );
        }

        let headers: Vec<u16> = HEADERS.encode_utf16().collect();
        // SAFETY: `headers` and `body` are live for the duration of the call.
        let sent = unsafe {
            WinHttpSendRequest(
                request.0,
                Some(&headers),
                Some(body.as_ptr().cast()),
                body.len() as u32,
                body.len() as u32,
                0,
            )
        };
        if sent.is_err() {
            return Err("send_failed");
        }
        // SAFETY: the request handle is open.
        if unsafe { WinHttpReceiveResponse(request.0, core::ptr::null_mut()) }.is_err() {
            return Err("receive_failed");
        }

        let mut status = 0u32;
        let mut status_size = core::mem::size_of::<u32>() as u32;
        // SAFETY: `status` is a DWORD-sized out-parameter; the name and index are
        // ignored for WINHTTP_QUERY_STATUS_CODE.
        let status_ok = unsafe {
            WinHttpQueryHeaders(
                request.0,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                PCWSTR::null(),
                Some((&raw mut status).cast()),
                &raw mut status_size,
                core::ptr::null_mut(),
            )
        };
        if status_ok.is_err() {
            return Err("status_query_failed");
        }

        let mut buffer = vec![0u8; 1024];
        let mut response = Vec::new();
        loop {
            let mut available = 0u32;
            // SAFETY: the request handle is open.
            if unsafe { WinHttpQueryDataAvailable(request.0, &raw mut available) }.is_err() {
                return Err("read_failed");
            }
            if available == 0 || response.len() >= MAX_RESPONSE_BYTES {
                break;
            }
            let available = (available as usize).min(MAX_RESPONSE_BYTES);
            if available > buffer.len() {
                buffer.resize(available, 0);
            }
            let mut read = 0u32;
            // SAFETY: `buffer` is at least `available` bytes long.
            if unsafe {
                WinHttpReadData(
                    request.0,
                    buffer.as_mut_ptr().cast(),
                    available as u32,
                    &raw mut read,
                )
            }
            .is_err()
            {
                return Err("read_failed");
            }
            if read == 0 {
                break;
            }
            let room = MAX_RESPONSE_BYTES.saturating_sub(response.len());
            response.extend_from_slice(&buffer[..(read as usize).min(room)]);
            if response.len() >= MAX_RESPONSE_BYTES {
                break;
            }
        }

        Ok((status as i32, response))
    }

    /// `POST path` with a JSON body and the documented 4 s receive timeout.
    /// Never panics; every failure is a log-safe `error` string and `ok = false`.
    pub fn post_json(endpoint: &ServiceEndpoint, path: &str, body: &[u8]) -> HttpResult {
        post_json_with_receive_timeout(endpoint, path, body, RECEIVE_TIMEOUT_MS)
    }

    /// Same as [`post_json`], with an explicit receive timeout for the unlock
    /// long-poll claim. The send timeout stays at 4 s: the claim body is tiny.
    pub fn post_json_with_receive_timeout(
        endpoint: &ServiceEndpoint,
        path: &str,
        body: &[u8],
        receive_timeout_ms: i32,
    ) -> HttpResult {
        // SAFETY: GetTickCount64 takes no arguments.
        let started = unsafe { GetTickCount64() };
        let result = post_json_inner(endpoint, path, body, receive_timeout_ms);
        // SAFETY: same as above.
        let elapsed_ms = unsafe { GetTickCount64() }.saturating_sub(started) as u32;

        match result {
            Ok((status, body)) => HttpResult {
                ok: true,
                status,
                body,
                error: "",
                elapsed_ms,
            },
            Err(error) => HttpResult {
                ok: false,
                status: 0,
                body: Vec::new(),
                error,
                elapsed_ms,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::request_targets;
    use logon_protocol::ServiceEndpoint;

    fn endpoint(host: &str) -> ServiceEndpoint {
        ServiceEndpoint {
            host: host.to_owned(),
            port: 8443,
            tls: true,
            prefix: String::new(),
        }
    }

    #[test]
    fn the_host_and_target_are_nul_terminated() {
        let targets = request_targets(&endpoint("home.example"), "/v1/challenge").unwrap();
        assert_eq!(targets.host.as_slice().last(), Some(&0));
        assert_eq!(targets.target.as_slice().last(), Some(&0));
        assert_eq!(
            targets
                .host
                .as_slice()
                .iter()
                .filter(|&&unit| unit == 0)
                .count(),
            1
        );
        assert_eq!(
            targets
                .target
                .as_slice()
                .iter()
                .filter(|&&unit| unit == 0)
                .count(),
            1
        );
        assert_eq!(
            String::from_utf16(targets.host.text()).unwrap(),
            "home.example"
        );
        assert_eq!(
            String::from_utf16(targets.target.text()).unwrap(),
            "/v1/challenge"
        );
    }

    #[test]
    fn an_empty_host_or_path_is_rejected() {
        assert_eq!(
            request_targets(&endpoint(""), "/v1/challenge").unwrap_err(),
            "invalid_target"
        );
        assert_eq!(
            request_targets(&endpoint("home.example"), "").unwrap_err(),
            "invalid_target"
        );
    }
}
