//! Minimal WinHTTP JSON POST client.
//!
//! No proxy, no redirects, no cookies, no credential attachment: the wakegrant
//! MAC is the only authenticator and it must not be relayed anywhere. Timeouts
//! and limits are the ones documented in the README (2/2/4/4 s, 16 KiB body).

use core::ffi::c_void;

use windows::Win32::Networking::WinHttp::{
    WINHTTP_ACCESS_TYPE_NO_PROXY, WINHTTP_DISABLE_COOKIES, WINHTTP_DISABLE_REDIRECTS,
    WINHTTP_FLAG_SECURE, WINHTTP_OPEN_REQUEST_FLAGS, WINHTTP_OPTION_DISABLE_FEATURE,
    WINHTTP_OPTION_REDIRECT_POLICY, WINHTTP_OPTION_REDIRECT_POLICY_NEVER,
    WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect,
    WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable, WinHttpQueryHeaders,
    WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetOption,
    WinHttpSetTimeouts,
};
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::core::{PCWSTR, w};

use logon_protocol::ServiceEndpoint;

use crate::provider::{widen, widen_nul};

const RESOLVE_TIMEOUT_MS: i32 = 2000;
const CONNECT_TIMEOUT_MS: i32 = 2000;
const SEND_TIMEOUT_MS: i32 = 4000;
const RECEIVE_TIMEOUT_MS: i32 = 4000;
const MAX_RESPONSE_BYTES: usize = 16 * 1024;
const USER_AGENT: &str = "RicardoKompLogon/1.0 (Windows LogonUI)";
const HEADERS: &str = "Content-Type: application/json\r\nAccept: application/json\r\n";

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
) -> Result<(i32, Vec<u8>), &'static str> {
    let host = widen(&endpoint.host);
    let target = widen(path);
    if host.is_empty() || target.is_empty() {
        return Err("invalid_target");
    }

    // SAFETY: all arguments are valid; WINHTTP_ACCESS_TYPE_NO_PROXY forbids a
    // proxy, as documented in `README.md`.
    let session = WinHttpHandle(unsafe {
        WinHttpOpen(
            PCWSTR(widen_nul(USER_AGENT).as_ptr()),
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
            RECEIVE_TIMEOUT_MS,
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

    // SAFETY: `host` is a NUL-terminated wide string; `session` is open.
    let connection = WinHttpHandle(unsafe {
        WinHttpConnect(session.0, PCWSTR(host.as_ptr()), endpoint.port, 0)
    });
    if connection.0.is_null() {
        return Err("connect_failed");
    }

    let flags = if endpoint.tls {
        WINHTTP_FLAG_SECURE
    } else {
        WINHTTP_OPEN_REQUEST_FLAGS(0)
    };
    // SAFETY: `target` is NUL-terminated; the connection handle is open.
    let request = WinHttpHandle(unsafe {
        WinHttpOpenRequest(
            connection.0,
            w!("POST"),
            PCWSTR(target.as_ptr()),
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

/// `POST path` with a JSON body. Never panics; every failure is a log-safe
/// `error` string and `ok = false`.
pub fn post_json(endpoint: &ServiceEndpoint, path: &str, body: &[u8]) -> HttpResult {
    // SAFETY: GetTickCount64 takes no arguments.
    let started = unsafe { GetTickCount64() };
    let result = post_json_inner(endpoint, path, body);
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
