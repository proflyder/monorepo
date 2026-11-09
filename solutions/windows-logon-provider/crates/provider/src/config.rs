//! Registry configuration and the DPAPI-protected secret file.
//!
//! Value names, types, limits and error phrases are the install contract in
//! `README.md`. `FailCount` arithmetic is pure Rust and host-tested.

/// `HKLM\SOFTWARE\Home\KompLogon` (64-bit view).
pub const REGISTRY_KEY: &str = r"SOFTWARE\Home\KompLogon";
/// Fixed DPAPI entropy; `set-secret.ps1` uses the same bytes.
pub const SECRET_ENTROPY: &[u8] = b"home-komp-logon-v1";
pub const SECRET_PATH: &str = r"C:\ProgramData\Home\KompLogon\secret.bin";
pub const MAX_SECRET_FILE_BYTES: u64 = 64 * 1024;
/// Third failed logon disables the provider.
pub const FAILURE_DISABLE_THRESHOLD: u32 = 3;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    /// Normalised URL without a trailing slash.
    pub service_url: String,
    pub machine: String,
    pub user: String,
    pub domain: String,
}

/// Why the registry configuration cannot be used. Both variants are
/// log-safe: `Invalid` carries only a static phrase plus the value name.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    /// `Disabled=1`: do nothing at all.
    #[error("disabled")]
    Disabled,
    /// Missing or invalid values: log and stay closed.
    #[error("{0}")]
    Invalid(String),
}

impl ConfigError {
    /// Reason for log lines; `Disabled` has no extra detail, matching the
    /// installer contract.
    pub fn detail(&self) -> &str {
        match self {
            Self::Disabled => "",
            Self::Invalid(detail) => detail,
        }
    }
}

/// Registry values before validation. `None` means absent or unreadable; the
/// validation ladder names the first required field that is missing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(not(any(windows, test)), allow(dead_code))] // constructed by the registry loader
pub(crate) struct RawConfig {
    service_url: Option<String>,
    machine: Option<String>,
    user: Option<String>,
    domain: Option<String>,
    /// `Disabled=1` means "do nothing at all"; absent means enabled.
    disabled: Option<u32>,
}

/// Reason for log lines: `registry_value_invalid:<field>` keeps the name of
/// the first failing field, as the installer contract documents.
#[cfg_attr(not(any(windows, test)), allow(dead_code))] // used by the registry loader
fn value_invalid(field: &str) -> ConfigError {
    ConfigError::Invalid(format!("registry_value_invalid:{field}"))
}

/// Validates raw registry values. `Disabled` is checked before the URL and the
/// other required values, so a disabled provider logs `disabled` even when the
/// rest of the key is broken. `FailCount` (and `Disabled` itself) may be absent:
/// both are counters/switch values that default to 0.
#[cfg_attr(not(any(windows, test)), allow(dead_code))] // called by the registry loader
pub(crate) fn resolve_config(raw: &RawConfig) -> Result<Config, ConfigError> {
    if raw.disabled.unwrap_or(0) != 0 {
        return Err(ConfigError::Disabled);
    }
    let service_url = raw
        .service_url
        .as_deref()
        .ok_or_else(|| value_invalid("ServiceUrl"))?;
    let machine = raw
        .machine
        .as_deref()
        .ok_or_else(|| value_invalid("Machine"))?;
    let user = raw
        .user
        .as_deref()
        .ok_or_else(|| value_invalid("UserName"))?;
    let domain = raw
        .domain
        .as_deref()
        .ok_or_else(|| value_invalid("Domain"))?;
    let service_url = validate_config(service_url, machine, user, domain)
        .map_err(|detail| ConfigError::Invalid(detail.detail().to_owned()))?;
    Ok(Config {
        service_url,
        machine: machine.to_owned(),
        user: user.to_owned(),
        domain: domain.to_owned(),
    })
}

/// Next stored `FailCount`; saturates at 1000 like `RecordFailedLogon`.
pub fn next_fail_count(count: u32) -> u32 {
    if count < 1000 { count + 1 } else { count }
}

pub fn reaches_disable_threshold(count: u32) -> bool {
    count >= FAILURE_DISABLE_THRESHOLD
}

pub fn updated_fail_state(count: u32, disabled: u32) -> (u32, u32) {
    let count = next_fail_count(count);
    let disabled = if reaches_disable_threshold(count) {
        1
    } else {
        disabled
    };
    (count, disabled)
}

/// `SecureZeroMemory` for memory we do not own as a Rust slice (CoTaskMem
/// buffers, DPAPI output).
///
/// # Safety
///
/// `ptr` must be valid for writes of `len` bytes, and `len` must describe the
/// allocation that is wiped (the caller owns it and is about to free it).
pub unsafe fn secure_zero_raw(ptr: *mut u8, len: usize) {
    for index in 0..len {
        // SAFETY: the caller guarantees the range is writable.
        unsafe { core::ptr::write_volatile(ptr.add(index), 0) };
    }
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
}

#[cfg(windows)]
pub use imp::{load_config, load_secret, record_failed_logon};

#[cfg(windows)]
mod imp {
    use crate::bindings::Windows::Win32::Foundation::{
        CloseHandle, ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, GENERIC_READ, HLOCAL, LocalFree,
    };
    use crate::bindings::Windows::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CryptUnprotectData,
    };
    use crate::bindings::Windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, GetFileSizeEx, OPEN_EXISTING, ReadFile,
    };
    use crate::bindings::Windows::Win32::System::Registry::{
        HKEY, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, KEY_WOW64_64KEY, REG_DWORD,
        REG_ROUTINE_FLAGS, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RRF_SUBKEY_WOW6464KEY, RegCloseKey,
        RegGetValueW, RegOpenKeyExW, RegSetValueExW,
    };
    use windows_core::PCWSTR;
    use windows_core::w;

    use crate::wide::WideNul;

    use wakegrant_protocol::{MachineSecret, parse_secret_json};

    use zeroize::Zeroize;

    use super::{
        Config, ConfigError, MAX_SECRET_FILE_BYTES, REGISTRY_KEY, SECRET_ENTROPY, SECRET_PATH,
        secure_zero_raw,
    };

    const RRF_STRING: REG_ROUTINE_FLAGS =
        REG_ROUTINE_FLAGS(RRF_RT_REG_SZ.0 | RRF_SUBKEY_WOW6464KEY.0);
    const RRF_DWORD: REG_ROUTINE_FLAGS =
        REG_ROUTINE_FLAGS(RRF_RT_REG_DWORD.0 | RRF_SUBKEY_WOW6464KEY.0);

    /// Narrows a NUL-terminated UTF-16 buffer to UTF-8. A malformed surrogate
    /// is replaced with `?`, like `WideCharToMultiByte(..., 0, ...)`.
    fn utf8_from_wide(wide: &[u16]) -> String {
        let end = wide
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(wide.len());
        String::from_utf16_lossy(&wide[..end])
    }

    /// `Ok(Some(value))` when present, `Ok(None)` when absent, `Err(())` when
    /// the value exists but is unreadable or out of bounds.
    fn read_string(key: HKEY, name: PCWSTR) -> Result<Option<String>, ()> {
        let mut bytes = 0u32;
        // SAFETY: the out-parameter is valid for the duration of the call.
        let status = unsafe {
            RegGetValueW(
                key,
                PCWSTR::null(),
                name,
                RRF_STRING,
                None,
                None,
                Some(&raw mut bytes),
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if status != ERROR_SUCCESS || bytes == 0 || bytes > 8192 {
            return Err(());
        }

        let mut buffer = vec![0u16; bytes as usize / 2 + 1];
        // SAFETY: `buffer` has room for `bytes` plus one terminator unit, and
        // `RegGetValueW` re-reports the byte count it writes.
        let status = unsafe {
            RegGetValueW(
                key,
                PCWSTR::null(),
                name,
                RRF_STRING,
                None,
                Some(buffer.as_mut_ptr().cast()),
                Some(&raw mut bytes),
            )
        };
        if status != ERROR_SUCCESS {
            return Err(());
        }
        let units = (bytes as usize / 2).min(buffer.len());
        Ok(Some(utf8_from_wide(&buffer[..units])))
    }

    /// `Ok(Some(value))` when present, `Ok(None)` when absent, `Err(())` when
    /// the value exists but is unreadable or has the wrong type.
    fn read_dword_value(key: HKEY, name: PCWSTR) -> Result<Option<u32>, ()> {
        let mut value = 0u32;
        let mut bytes = size_of::<u32>() as u32;
        // SAFETY: `value` is a valid DWORD-sized out-parameter for
        // RRF_RT_REG_DWORD.
        let status = unsafe {
            RegGetValueW(
                key,
                PCWSTR::null(),
                name,
                RRF_DWORD,
                None,
                Some((&raw mut value).cast()),
                Some(&raw mut bytes),
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if status != ERROR_SUCCESS {
            return Err(());
        }
        Ok(Some(value))
    }

    fn close_key(key: HKEY) {
        // SAFETY: `key` is an open handle owned by the caller.
        unsafe {
            let _ = RegCloseKey(key);
        }
    }

    /// Reads an optional `REG_SZ` value; a missing or malformed value is
    /// `None`, which the validation ladder reports by field name.
    fn optional_string(key: HKEY, name: PCWSTR) -> Option<String> {
        read_string(key, name).ok().flatten()
    }

    /// Reads and validates the registry configuration.
    pub fn load_config() -> Result<Config, ConfigError> {
        let mut key = HKEY::default();
        let key_path = WideNul::new(REGISTRY_KEY);
        // SAFETY: `key` is a valid out-parameter; `key_path` is NUL-terminated.
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                key_path.as_pcwstr(),
                None,
                KEY_READ | KEY_WOW64_64KEY,
                &raw mut key,
            )
        };
        if status != ERROR_SUCCESS {
            return Err(ConfigError::Invalid("registry_key_missing".to_owned()));
        }

        // Absent `Disabled` means 0. A present but unreadable value is still an
        // error: it must not silently change the operator's switch.
        let Ok(disabled) = read_dword_value(key, w!("Disabled")) else {
            close_key(key);
            return Err(super::value_invalid("Disabled"));
        };
        let raw = super::RawConfig {
            service_url: optional_string(key, w!("ServiceUrl")),
            machine: optional_string(key, w!("Machine")),
            user: optional_string(key, w!("UserName")),
            domain: optional_string(key, w!("Domain")),
            disabled,
        };
        // `Disabled` and the required fields decide first; only a configuration
        // the provider would actually use is checked for a bad `FailCount`
        // (absent is 0, a present value of the wrong type is invalid).
        let resolved = super::resolve_config(&raw);
        let fail_count_invalid =
            resolved.is_ok() && read_dword_value(key, w!("FailCount")).is_err();
        close_key(key);
        if fail_count_invalid {
            return Err(super::value_invalid("FailCount"));
        }
        resolved
    }

    /// Reads, DPAPI-decrypts and parses the secret file. `Err` carries a short
    /// log-safe phrase (never the secret or the decrypted plaintext).
    pub fn load_secret() -> Result<MachineSecret, &'static str> {
        let secret_path = WideNul::new(SECRET_PATH);
        // SAFETY: `secret_path` is NUL-terminated; no security attributes.
        let Ok(file) = (unsafe {
            CreateFileW(
                secret_path.as_pcwstr(),
                GENERIC_READ.0,
                FILE_SHARE_READ,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
        }) else {
            return Err("secret_file_missing");
        };

        let mut size: i64 = 0;
        // SAFETY: `size` is a valid out-parameter for GetFileSizeEx.
        let size_ok = unsafe { GetFileSizeEx(file, &raw mut size) }.is_ok();
        if !size_ok || size <= 0 || size > MAX_SECRET_FILE_BYTES as i64 {
            // SAFETY: `file` is an open handle owned here.
            unsafe {
                let _ = CloseHandle(file);
            };
            return Err("secret_file_size_invalid");
        }

        let mut blob = vec![0u8; size as usize];
        let mut read = 0u32;
        // SAFETY: `blob` is exclusively borrowed for the call and `file` is open.
        let read_ok =
            unsafe { ReadFile(file, Some(blob.as_mut_slice()), Some(&raw mut read), None) }.is_ok();
        // SAFETY: `file` is an open handle owned here.
        unsafe {
            let _ = CloseHandle(file);
        };
        if !read_ok || read as usize != blob.len() {
            blob.zeroize();
            return Err("secret_file_unreadable");
        }

        let input = CRYPT_INTEGER_BLOB {
            cbData: blob.len() as u32,
            pbData: blob.as_mut_ptr(),
        };
        let entropy = CRYPT_INTEGER_BLOB {
            cbData: SECRET_ENTROPY.len() as u32,
            pbData: SECRET_ENTROPY.as_ptr().cast_mut(),
        };
        let mut output = CRYPT_INTEGER_BLOB::default();
        // SAFETY: both blobs point to live buffers; `output` is a valid
        // out-parameter. No prompt and no reserved pointer, as documented for `CryptUnprotectData`.
        let decrypted = unsafe {
            CryptUnprotectData(&input, None, Some(&entropy), None, None, 0, &raw mut output)
        };
        if decrypted.is_err() || output.pbData.is_null() || output.cbData == 0 {
            blob.zeroize();
            return Err("secret_dpapi_failed");
        }

        let parsed = {
            // SAFETY: DPAPI returned `output.cbData` bytes at `output.pbData`.
            let plaintext =
                unsafe { core::slice::from_raw_parts(output.pbData, output.cbData as usize) };
            core::str::from_utf8(plaintext).map_or_else(
                |_| Err("secret_json_invalid_utf8"),
                |text| parse_secret_json(text).map_err(wakegrant_protocol::Error::detail),
            )
        };

        // SAFETY: `output` is the DPAPI allocation, wiped before it is freed.
        unsafe {
            secure_zero_raw(output.pbData, output.cbData as usize);
            let _ = LocalFree(Some(HLOCAL(output.pbData.cast())));
        }
        blob.zeroize();
        parsed
    }

    /// Increments `FailCount` and sets `Disabled=1` at the third failure.
    /// Returns the stored values; `(0, false)` when the key cannot be updated.
    pub fn record_failed_logon() -> (u32, bool) {
        let mut key = HKEY::default();
        let key_path = WideNul::new(REGISTRY_KEY);
        // SAFETY: `key` is a valid out-parameter and `key_path` is
        // NUL-terminated.
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                key_path.as_pcwstr(),
                None,
                KEY_READ | KEY_SET_VALUE | KEY_WOW64_64KEY,
                &raw mut key,
            )
        };
        if status != ERROR_SUCCESS {
            return (0, false);
        }

        let fail_count = match read_dword_value(key, w!("FailCount")) {
            Ok(Some(value)) => value,
            Ok(None) => 0,
            Err(()) => {
                close_key(key);
                return (0, false);
            }
        };
        let disabled = match read_dword_value(key, w!("Disabled")) {
            Ok(Some(value)) => value,
            Ok(None) => 0,
            Err(()) => {
                close_key(key);
                return (0, false);
            }
        };
        let (count, disabled) = super::updated_fail_state(fail_count, disabled);

        let count_bytes = count.to_le_bytes();
        let disabled_bytes = disabled.to_le_bytes();
        // SAFETY: the values are live locals and the key is open for writing.
        unsafe {
            let _ = RegSetValueExW(key, w!("FailCount"), None, REG_DWORD, Some(&count_bytes));
            let _ = RegSetValueExW(key, w!("Disabled"), None, REG_DWORD, Some(&disabled_bytes));
        }
        close_key(key);

        (count, disabled != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeroize::Zeroize;

    #[test]
    fn fail_count_increments_and_saturates() {
        assert_eq!(next_fail_count(0), 1);
        assert_eq!(next_fail_count(1), 2);
        assert_eq!(next_fail_count(2), 3);
        assert_eq!(next_fail_count(999), 1000);
        assert_eq!(next_fail_count(1000), 1000);
        assert_eq!(next_fail_count(1001), 1001);
    }

    #[test]
    fn the_third_failure_disables() {
        assert!(!reaches_disable_threshold(1));
        assert!(!reaches_disable_threshold(2));
        assert!(reaches_disable_threshold(3));
        assert!(reaches_disable_threshold(42));
    }

    #[test]
    fn disabled_stays_set_once_reached() {
        assert_eq!(updated_fail_state(0, 0), (1, 0));
        assert_eq!(updated_fail_state(1, 0), (2, 0));
        assert_eq!(updated_fail_state(2, 0), (3, 1));
        assert_eq!(updated_fail_state(3, 0), (4, 1));
        assert_eq!(updated_fail_state(10, 1), (11, 1));
    }

    #[test]
    fn missing_optional_values_keep_the_configuration_valid() {
        let raw = RawConfig {
            service_url: Some("http://192.0.2.27:30130".to_owned()),
            machine: Some("komp".to_owned()),
            user: Some("pl".to_owned()),
            domain: Some(".".to_owned()),
            disabled: None,
        };
        let config = resolve_config(&raw).expect("counters are optional");
        assert_eq!(config.machine, "komp");
        assert_eq!(config.service_url, "http://192.0.2.27:30130");
    }

    #[test]
    fn disabled_is_checked_before_the_url() {
        let raw = RawConfig {
            service_url: Some("not a url".to_owned()),
            disabled: Some(1),
            ..RawConfig::default()
        };
        assert_eq!(resolve_config(&raw), Err(ConfigError::Disabled));
    }

    #[test]
    fn a_missing_required_value_names_its_field() {
        let raw = RawConfig {
            machine: Some("komp".to_owned()),
            user: Some("pl".to_owned()),
            domain: Some(".".to_owned()),
            ..RawConfig::default()
        };
        assert_eq!(
            resolve_config(&raw),
            Err(ConfigError::Invalid(
                "registry_value_invalid:ServiceUrl".to_owned()
            ))
        );
    }

    #[test]
    fn zeroing_clears_buffers() {
        let mut bytes = *b"secret";
        bytes.zeroize();
        assert_eq!(bytes, *b"\0\0\0\0\0\0");

        let mut wide = [1u16, 2, 3];
        wide.zeroize();
        assert_eq!(wide, [0, 0, 0]);

        let mut raw = [7u8; 4];
        // SAFETY: `raw` is a live 4-byte buffer.
        unsafe { secure_zero_raw(raw.as_mut_ptr(), raw.len()) };
        assert_eq!(raw, [0, 0, 0, 0]);
    }
}

/// Client-side service URL handling. The provider owns this because it is
/// registry/install configuration, not part of the shared wire protocol.
pub mod endpoint {
    use ::http::Uri;

    use wakegrant_protocol::Error;

    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct ServiceEndpoint {
        pub tls: bool,
        /// DNS name, IPv4 or IPv6 literal without brackets.
        pub host: String,
        pub port: u16,
        /// `""` or `"/path"` without a trailing slash.
        pub prefix: String,
    }

    fn is_lower_alnum_dash(c: u8) -> bool {
        c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-'
    }

    fn has_control_or_space(text: &str) -> bool {
        text.bytes().any(|c| c < 0x20 || c == 0x7f || c == b' ')
    }

    /// Parses `http://host[:port][/path]` or `https://...`. Rejects userinfo,
    /// queries, fragments, whitespace and control characters.
    pub fn parse_service_url(url: &str) -> Result<ServiceEndpoint, Error> {
        if url.len() > 512 {
            return Err(Error::new("service url too long"));
        }
        if has_control_or_space(url) {
            return Err(Error::new("service url contains whitespace"));
        }

        let (tls, default_port) = if url.starts_with("http://") {
            (false, 80)
        } else if url.starts_with("https://") {
            (true, 443)
        } else {
            return Err(Error::new(
                "service url must start with http:// or https://",
            ));
        };
        if url.bytes().any(|c| matches!(c, b'?' | b'#' | b'@')) {
            return Err(Error::new(
                "service url must not carry credentials, query or fragment",
            ));
        }

        let uri: Uri = url
            .parse()
            .map_err(|_| Error::new("service url is not a valid URI"))?;
        let authority = uri
            .authority()
            .ok_or(Error::new("service url without host"))?;
        let port = parse_port(authority.as_str(), default_port)?;

        let host = authority.host();
        let host = if let Some(rest) = host.strip_prefix('[') {
            let literal = rest
                .strip_suffix(']')
                .ok_or(Error::new("service url has an unterminated IPv6 literal"))?;
            if literal.is_empty() {
                return Err(Error::new("service url without host"));
            }
            if !literal
                .bytes()
                .all(|c| c.is_ascii_hexdigit() || c == b':' || c == b'.')
            {
                return Err(Error::new("service url has an invalid IPv6 literal"));
            }
            literal
        } else {
            if host.is_empty() {
                return Err(Error::new("service url without host"));
            }
            if host.len() > 253 {
                return Err(Error::new("service url host too long"));
            }
            if !host
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-')
            {
                return Err(Error::new("service url has an invalid host character"));
            }
            if host.starts_with('.') || host.ends_with('.') || host.contains("..") {
                return Err(Error::new("service url host has an empty label"));
            }
            host
        };

        let prefix = if uri.path() == "/" {
            ""
        } else {
            let prefix = uri.path().trim_end_matches('/');
            if prefix.contains("//") {
                return Err(Error::new("service url path has an empty segment"));
            }
            prefix
        };

        Ok(ServiceEndpoint {
            tls,
            host: host.to_owned(),
            port,
            prefix: prefix.to_owned(),
        })
    }

    /// Port after the host of an authority. Rejects non-numeric and out-of-range
    /// values instead of letting `http::Uri` silently truncate them.
    fn parse_port(authority: &str, default_port: u16) -> Result<u16, Error> {
        let port_text = if let Some(rest) = authority.strip_prefix('[') {
            let close = rest
                .find(']')
                .ok_or(Error::new("service url has an unterminated IPv6 literal"))?;
            &rest[close + 1..]
        } else {
            authority.rfind(':').map_or("", |colon| &authority[colon..])
        };
        let Some(text) = port_text.strip_prefix(':') else {
            return Ok(default_port);
        };
        if text.is_empty() {
            return Ok(default_port);
        }
        if text.len() > 5 || !text.bytes().all(|c| c.is_ascii_digit()) {
            return Err(Error::new("service url port is not numeric"));
        }
        let port: u16 = text
            .parse()
            .map_err(|_| Error::new("service url port out of range"))?;
        if port == 0 {
            return Err(Error::new("service url port out of range"));
        }
        Ok(port)
    }

    /// Validates the whole machine configuration and normalizes the URL. The
    /// error is log-safe.
    pub fn validate_config(
        service_url: &str,
        machine: &str,
        user: &str,
        domain: &str,
    ) -> Result<String, Error> {
        if machine.is_empty() || machine.len() > 32 {
            return Err(Error::new("machine must be 1..32 characters"));
        }
        if !machine.bytes().all(is_lower_alnum_dash) {
            return Err(Error::new("machine must match ^[a-z0-9-]{1,32}$"));
        }

        if user.is_empty() || user.len() > 256 {
            return Err(Error::new("UserName must be 1..256 characters"));
        }
        if user.bytes().any(|c| matches!(c, b'\\' | b'"' | b'@')) {
            return Err(Error::new("UserName must not contain '\\', '\"' or '@'"));
        }
        if has_control_or_space(user) {
            return Err(Error::new(
                "UserName contains whitespace or control characters",
            ));
        }

        if domain.is_empty() || domain.len() > 256 {
            return Err(Error::new("Domain must be 1..256 characters"));
        }
        if domain.bytes().any(|c| matches!(c, b'\\' | b'"')) {
            return Err(Error::new("Domain must not contain '\\' or '\"'"));
        }
        if has_control_or_space(domain) {
            return Err(Error::new(
                "Domain contains whitespace or control characters",
            ));
        }

        Ok(render(&parse_service_url(service_url)?, false))
    }

    /// Log-safe service URL for log lines: scheme, host and port only, no path.
    pub fn redact_service_url(url: &str) -> String {
        let Ok(endpoint) = parse_service_url(url) else {
            return "invalid-url".to_owned();
        };
        render(
            &ServiceEndpoint {
                prefix: String::new(),
                ..endpoint
            },
            true,
        )
    }

    /// `scheme://host[:port][prefix]`; `explicit_port` keeps the port even when it
    /// is the scheme default (the log always shows it).
    fn render(endpoint: &ServiceEndpoint, explicit_port: bool) -> String {
        let mut result = String::new();
        result.push_str(if endpoint.tls { "https://" } else { "http://" });
        push_host(&mut result, &endpoint.host);
        let default_port = if endpoint.tls { 443 } else { 80 };
        if explicit_port || endpoint.port != default_port {
            result.push(':');
            result.push_str(&endpoint.port.to_string());
        }
        result.push_str(&endpoint.prefix);
        result
    }

    fn push_host(out: &mut String, host: &str) {
        if host.contains(':') {
            out.push('[');
            out.push_str(host);
            out.push(']');
        } else {
            out.push_str(host);
        }
    }

    /// Request target path for an endpoint, e.g. `/v1/claim` or `/gw/v1/claim`.
    pub fn endpoint_path(endpoint: &ServiceEndpoint, path: &str) -> String {
        let mut result = endpoint.prefix.clone();
        if !path.starts_with('/') {
            result.push('/');
        }
        result.push_str(path);
        result
    }
}

pub use endpoint::{
    ServiceEndpoint, endpoint_path, parse_service_url, redact_service_url, validate_config,
};

#[cfg(test)]
mod endpoint_tests {
    use super::endpoint::{
        ServiceEndpoint, endpoint_path, parse_service_url, redact_service_url, validate_config,
    };

    #[test]
    fn service_urls_are_validated_and_normalized() {
        assert_eq!(
            validate_config("http://192.0.2.27:30130", "komp", "pl", ".").unwrap(),
            "http://192.0.2.27:30130"
        );
        assert_eq!(
            validate_config("https://home.example:8443/wake/", "komp-2", "pl", ".").unwrap(),
            "https://home.example:8443/wake"
        );
        assert_eq!(
            validate_config("http://[fd00::1]:30130", "komp", "pl", ".").unwrap(),
            "http://[fd00::1]:30130"
        );

        let invalid = [
            ("", "komp", "pl", "."),
            ("ftp://192.0.2.27", "komp", "pl", "."),
            ("192.0.2.27:30130", "komp", "pl", "."),
            ("http://", "komp", "pl", "."),
            ("http://192.0.2.27:0", "komp", "pl", "."),
            ("http://192.0.2.27:70000", "komp", "pl", "."),
            ("http://192.0.2.27:abc", "komp", "pl", "."),
            ("http://user:pass@192.0.2.27", "komp", "pl", "."),
            ("http://192.0.2.27/?x=1", "komp", "pl", "."),
            ("http://192.0.2.27/#frag", "komp", "pl", "."),
            ("http://192.0.2.27/a//b", "komp", "pl", "."),
            ("http://192.0.2.27 /x", "komp", "pl", "."),
            ("http://192.0.2.27", "", "pl", "."),
            ("http://192.0.2.27", "Komp", "pl", "."),
            ("http://192.0.2.27", "komp_1", "pl", "."),
            ("http://192.0.2.27", "komp", "", "."),
            ("http://192.0.2.27", "komp", "p l", "."),
            ("http://192.0.2.27", "komp", "a\\b", "."),
            ("http://192.0.2.27", "komp", "pl", ""),
            ("http://192.0.2.27", "komp", "pl", "a b"),
            (
                "http://192.0.2.27",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "pl",
                ".",
            ),
        ];
        for (url, machine, user, domain) in invalid {
            let error = validate_config(url, machine, user, domain)
                .expect_err(&format!("accepted {url} {machine} {user} {domain:?}"));
            assert!(!error.detail().is_empty());
        }

        let endpoint = parse_service_url("http://192.0.2.27:30130/wake").expect("valid url");
        assert_eq!(
            endpoint,
            ServiceEndpoint {
                tls: false,
                host: "192.0.2.27".to_owned(),
                port: 30130,
                prefix: "/wake".to_owned(),
            }
        );
        assert_eq!(endpoint_path(&endpoint, "/v1/claim"), "/wake/v1/claim");

        let endpoint = parse_service_url("https://example.lan").expect("valid url");
        assert_eq!(endpoint.port, 443);
        assert!(endpoint.tls);
        assert_eq!(endpoint_path(&endpoint, "/v1/claim"), "/v1/claim");

        let endpoint = parse_service_url("http://[fd00::1]").expect("valid url");
        assert_eq!(endpoint.host, "fd00::1");
        assert_eq!(endpoint.port, 80);

        assert_eq!(
            redact_service_url("http://192.0.2.27:30130/secret/path"),
            "http://192.0.2.27:30130"
        );
        assert_eq!(redact_service_url("not a url"), "invalid-url");
    }
}
