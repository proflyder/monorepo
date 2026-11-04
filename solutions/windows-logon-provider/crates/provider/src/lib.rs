//! Windows Credential Provider (in-proc COM server) — stage 1 skeleton.
//!
//! LogonUI loads this DLL and calls the two exported entry points. The COM
//! class, credential and worker implementation follow in the next stage; for
//! now `DllGetClassObject` reports that no class is available, so the provider
//! never contributes a tile and the normal sign-in screen is untouched.
//!
//! Panic policy: the workspace release profile keeps `panic = "unwind"` (the
//! crate is `cdylib`, and a panic must be caught at the COM boundary in the
//! next stage instead of aborting LogonUI).

use core::ffi::c_void;

// Windows builds use the official windows-rs types. The crate is `cdylib`
// only for Windows, but `cargo clippy`/`cargo test` on the build host still
// compile this file, so a small ABI-compatible fallback keeps the same
// signatures without pulling in `windows-future` (which is Windows-only).
#[cfg(windows)]
use windows::Win32::Foundation::S_OK;
#[cfg(windows)]
use windows::core::{GUID, HRESULT};

#[cfg(not(windows))]
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GUID {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

#[cfg(not(windows))]
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HRESULT(pub i32);

#[cfg(not(windows))]
const S_OK: HRESULT = HRESULT(0);

/// `CLASS_E_CLASSNOTAVAILABLE`; `windows` exposes it only with the
/// `Win32_System_Com` feature and it is a fixed HRESULT from `winerror.h`.
const CLASS_E_CLASSNOTAVAILABLE: HRESULT = HRESULT(0x8004_0111_u32 as i32);

// Compile-time check that the portable protocol crate is part of this build and
// cross-compiles for the Windows target. The linker may drop it until the
// worker uses it in stage 2.
const _: fn(&[u8]) -> String = logon_protocol::b64_encode;

/// `DllGetClassObject` must return `CLASS_E_CLASSNOTAVAILABLE` until the
/// provider class exists. The out pointer is always cleared so LogonUI never
/// sees a stale interface.
///
/// # Safety
///
/// The caller follows the COM contract: `ppv` is either null or points to a
/// writable `*mut c_void`. `rclsid`/`riid` are only read when non-null.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllGetClassObject(
    _rclsid: *const GUID,
    _riid: *const GUID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    if !ppv.is_null() {
        // SAFETY: the COM caller guarantees a writable out pointer.
        unsafe { *ppv = core::ptr::null_mut() };
    }
    CLASS_E_CLASSNOTAVAILABLE
}

/// `DllCanUnloadNow`: nothing is cached yet, so the DLL may be unloaded.
#[unsafe(no_mangle)]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    S_OK
}
