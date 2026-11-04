//! Standard COM in-proc server entry points.
//!
//! Exports are exactly `DllGetClassObject` and `DllCanUnloadNow` (the `.def`
//! file of the C++ build is no longer needed: only `#[unsafe(no_mangle)]`
//! functions become PE exports).
//!
//! `DllCanUnloadNow` returns `S_FALSE` while a provider object is alive
//! (`DLL_REFS`) or a worker thread is still running (`ACTIVE_WORKERS`). A live
//! worker means logon code could still run, so the module must stay mapped until
//! the thread exits (at most one poll budget).

use std::ffi::c_void;
use std::sync::atomic::{AtomicI32, Ordering};

use windows::Win32::Foundation::{
    CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_FAIL, E_INVALIDARG, S_FALSE, S_OK,
};
use windows::Win32::System::Com::{IClassFactory, IClassFactory_Impl};
use windows::core::{
    BOOL, GUID, HRESULT, IUnknown, Interface, Ref, Result as WinResult, implement,
};

use crate::ffi_guard::guard;
use crate::provider::{ACTIVE_WORKERS, PROVIDER_CLSID, Provider};

static DLL_REFS: AtomicI32 = AtomicI32::new(0);

/// Called by every live provider object and by `IClassFactory::LockServer`.
pub fn dll_add_ref() {
    DLL_REFS.fetch_add(1, Ordering::SeqCst);
}

/// Matches [`dll_add_ref`].
pub fn dll_release() {
    DLL_REFS.fetch_sub(1, Ordering::SeqCst);
}

/// `DllCanUnloadNow` reference count, exposed for tests/verification.
pub fn dll_refs() -> i32 {
    DLL_REFS.load(Ordering::SeqCst)
}

/// RAII holder for one DLL reference. The guard lives in the provider object,
/// so the count is decremented when the last COM reference is released, even
/// if construction panics after the increment.
pub struct DllRef;

impl DllRef {
    pub fn acquire() -> Self {
        dll_add_ref();
        Self
    }
}

impl Drop for DllRef {
    fn drop(&mut self) {
        dll_release();
    }
}

/// Class factory: one live object per `DllGetClassObject` call, exactly like the
/// C++ `ClassFactory`.
#[implement(IClassFactory)]
struct ClassFactory {}

impl IClassFactory_Impl for ClassFactory_Impl {
    fn CreateInstance(
        &self,
        punkouter: Ref<'_, IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut c_void,
    ) -> WinResult<()> {
        guard(
            || {
                if ppvobject.is_null() {
                    return Err(E_INVALIDARG.into());
                }
                // SAFETY: `ppvobject` is writable per the COM contract.
                unsafe { ppvobject.write(std::ptr::null_mut()) };
                if !punkouter.is_null() {
                    return Err(CLASS_E_NOAGGREGATION.into());
                }
                // SAFETY: forwarded to the COM QI contract checked above.
                unsafe { Provider::create_instance(riid, ppvobject) }.ok()
            },
            || Err(E_FAIL.into()),
        )
    }

    fn LockServer(&self, flock: BOOL) -> WinResult<()> {
        guard(
            || {
                if flock.as_bool() {
                    dll_add_ref();
                } else {
                    dll_release();
                }
                Ok(())
            },
            || Err(E_FAIL.into()),
        )
    }
}

/// `DllGetClassObject`: exactly the provider CLSID, otherwise
/// `CLASS_E_CLASSNOTAVAILABLE`. The out pointer is always cleared first.
///
/// # Safety
///
/// COM calling convention: `ppv` is null or writable; `rclsid`/`riid` are null
/// or point to valid GUIDs.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    guard(
        || {
            if ppv.is_null() {
                return E_INVALIDARG;
            }
            // SAFETY: `ppv` is writable per the COM contract.
            unsafe { ppv.write(std::ptr::null_mut()) };
            if rclsid.is_null() || riid.is_null() {
                return E_INVALIDARG;
            }
            // SAFETY: both pointers were checked above.
            if unsafe { *rclsid } != PROVIDER_CLSID {
                return CLASS_E_CLASSNOTAVAILABLE;
            }
            let factory: IClassFactory = ClassFactory {}.into();
            // SAFETY: `riid` is a valid IID and `ppv` is writable.
            unsafe { factory.query(riid, ppv) }
        },
        || E_FAIL,
    )
}

/// `DllCanUnloadNow`: `S_OK` only when no provider object is alive and no worker
/// thread is running.
#[unsafe(no_mangle)]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    guard(
        || {
            if dll_refs() > 0 || ACTIVE_WORKERS.load(Ordering::SeqCst) > 0 {
                S_FALSE
            } else {
                S_OK
            }
        },
        // Safe value on panic: keep the DLL mapped.
        || S_FALSE,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dll_refs_are_balanced() {
        let before = dll_refs();
        {
            let _first = DllRef::acquire();
            assert_eq!(dll_refs(), before + 1);
            let _second = DllRef::acquire();
            assert_eq!(dll_refs(), before + 2);
        }
        assert_eq!(dll_refs(), before);
    }
}
