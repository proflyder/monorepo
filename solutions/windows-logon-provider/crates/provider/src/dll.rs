//! COM in-proc server entry points: exactly `DllGetClassObject` and
//! `DllCanUnloadNow` (`#[unsafe(no_mangle)]`, no `.def`).
//!
//! `DllCanUnloadNow` keeps the DLL mapped while a provider object or worker
//! thread is alive: a live worker can still run logon code.

use std::ffi::c_void;
use std::sync::atomic::{AtomicI32, Ordering};

use crate::bindings::Windows::Win32::Foundation::{
    CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_FAIL, E_INVALIDARG, S_FALSE, S_OK,
};
use crate::bindings::Windows::Win32::System::Com::{IClassFactory, IClassFactory_Impl};
use windows_core::{BOOL, GUID, HRESULT, IUnknown, Interface, Ref, Result as WinResult, implement};

use crate::ffi_guard::guard;
use crate::provider::{ACTIVE_WORKERS, PROVIDER_CLSID, Provider};

static DLL_REFS: AtomicI32 = AtomicI32::new(0);

pub fn dll_add_ref() {
    DLL_REFS.fetch_add(1, Ordering::SeqCst);
}

pub fn dll_release() {
    DLL_REFS.fetch_sub(1, Ordering::SeqCst);
}

pub fn dll_refs() -> i32 {
    DLL_REFS.load(Ordering::SeqCst)
}

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
