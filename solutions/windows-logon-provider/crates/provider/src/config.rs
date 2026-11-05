//! Registry configuration and the DPAPI-protected secret file.
//!
//! Value names, types, limits and error phrases are the install contract in
//! `README.md`. `FailCount` arithmetic is pure Rust and host-tested.

/// `HKLM\SOFTWARE\Ricardo\KompLogon` (64-bit view).
pub const REGISTRY_KEY: &str = r"SOFTWARE\Ricardo\KompLogon";
/// Fixed DPAPI entropy; `set-secret.ps1` uses the same bytes.
pub const SECRET_ENTROPY: &[u8] = b"ricardo-komp-logon-v1";
pub const SECRET_PATH: &str = r"C:\ProgramData\Ricardo\KompLogon\secret.bin";
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

    use logon_protocol::{MachineSecret, parse_secret_json, validate_config};

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

    fn read_dword(key: HKEY, name: PCWSTR) -> Result<u32, ()> {
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
            return Ok(0);
        }
        if status != ERROR_SUCCESS {
            return Err(());
        }
        Ok(value)
    }

    fn close_key(key: HKEY) {
        // SAFETY: `key` is an open handle owned by the caller.
        unsafe {
            let _ = RegCloseKey(key);
        }
    }

    /// Reads one required `REG_SZ` value, recording
    /// `registry_value_invalid:<field>` on a missing or malformed value (the
    /// validation ladder keeps the name of the first failing field).
    fn value_invalid(field: &str) -> ConfigError {
        ConfigError::Invalid(format!("registry_value_invalid:{field}"))
    }

    fn required_string(key: HKEY, name: PCWSTR, field: &str) -> Result<String, ConfigError> {
        read_string(key, name)
            .ok()
            .flatten()
            .ok_or_else(|| value_invalid(field))
    }

    fn required_dword(key: HKEY, name: PCWSTR, field: &str) -> Result<u32, ConfigError> {
        read_dword(key, name).map_err(|()| value_invalid(field))
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

        let values = (|| {
            let service_url = required_string(key, w!("ServiceUrl"), "ServiceUrl")?;
            let machine = required_string(key, w!("Machine"), "Machine")?;
            let user = required_string(key, w!("UserName"), "UserName")?;
            let domain = required_string(key, w!("Domain"), "Domain")?;
            let disabled = required_dword(key, w!("Disabled"), "Disabled")?;
            let _fail_count = required_dword(key, w!("FailCount"), "FailCount")?;
            Ok::<_, ConfigError>((service_url, machine, user, domain, disabled))
        })();
        close_key(key);

        let (service_url, machine, user, domain, disabled) = values?;
        let service_url = validate_config(&service_url, &machine, &user, &domain)
            .map_err(|detail| ConfigError::Invalid(detail.detail().to_owned()))?;
        if disabled != 0 {
            return Err(ConfigError::Disabled);
        }
        Ok(Config {
            service_url,
            machine,
            user,
            domain,
        })
    }

    /// Reads, DPAPI-decrypts and parses the secret file. `Err` carries a short
    /// log-safe phrase (never the secret or the decrypted plaintext).
    pub fn load_secret() -> Result<MachineSecret, &'static str> {
        let secret_path = WideNul::new(SECRET_PATH);
        // SAFETY: `secret_path` is NUL-terminated; no security attributes.
        let file = match unsafe {
            CreateFileW(
                secret_path.as_pcwstr(),
                GENERIC_READ.0,
                FILE_SHARE_READ,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
        } {
            Ok(handle) => handle,
            Err(_) => return Err("secret_file_missing"),
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
            match core::str::from_utf8(plaintext) {
                Ok(text) => parse_secret_json(text).map_err(|error| error.detail()),
                Err(_) => Err("secret_json_invalid_utf8"),
            }
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

        let (Ok(fail_count), Ok(disabled)) = (
            read_dword(key, w!("FailCount")),
            read_dword(key, w!("Disabled")),
        ) else {
            close_key(key);
            return (0, false);
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
