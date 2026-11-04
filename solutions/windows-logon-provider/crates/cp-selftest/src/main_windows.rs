//! Windows half of `cp-selftest.exe`: tries the registered CLSID, otherwise
//! loads `RicardoKompLogon.dll` next to the executable. Output is `PASS`/`FAIL`;
//! exit codes are 0/1.

use std::ffi::c_void;
use std::ptr;

use crate::bindings::Windows::Win32::Foundation::{E_FAIL, E_NOTIMPL, S_OK};
use crate::bindings::Windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize, IClassFactory,
};
use crate::bindings::Windows::Win32::System::LibraryLoader::{
    GetModuleFileNameW, GetProcAddress, LoadLibraryW,
};
use crate::bindings::Windows::Win32::UI::Shell::{CPUS_CREDUI, CPUS_LOGON, ICredentialProvider};
use windows_core::{BOOL, GUID, HRESULT, IUnknown, Interface, PCWSTR, Result as WinResult, s};

/// `crates/provider/src/provider.rs` (`PROVIDER_CLSID`).
const PROVIDER_CLSID: GUID = GUID {
    data1: 0x9459_ccea,
    data2: 0x4ccc,
    data3: 0x4435,
    data4: [0x89, 0xbf, 0xe0, 0xe1, 0x32, 0xe0, 0xc2, 0x7f],
};

const DLL_NAME: &str = "RicardoKompLogon.dll";

fn code_of(result: &WinResult<()>) -> HRESULT {
    result.as_ref().err().map_or(S_OK, |error| error.code())
}

fn print(name: &str, hr: HRESULT) {
    println!(
        "  {:<28} 0x{:08x} ({})",
        name,
        hr.0 as u32,
        if hr.is_ok() { "ok" } else { "FAIL" }
    );
}

/// Builds `<directory of this executable>\RicardoKompLogon.dll` as UTF-16.
fn local_dll_path() -> Option<Vec<u16>> {
    let mut path = [0u16; 260]; // MAX_PATH
    // SAFETY: `path` is a writable buffer of `len` UTF-16 units.
    let length = unsafe { GetModuleFileNameW(None, &mut path) } as usize;
    if length == 0 || length >= path.len() {
        return None;
    }
    let slash = path[..length]
        .iter()
        .rposition(|&unit| unit == u16::from(b'\\'))?;
    let mut result = path[..=slash].to_vec();
    result.extend(DLL_NAME.encode_utf16());
    result.push(0);
    Some(result)
}

/// `CoCreateInstance` fallback: load the DLL next to the executable.
fn provider_from_dll_next_to_executable() -> WinResult<ICredentialProvider> {
    let path = local_dll_path().ok_or(E_FAIL)?;
    // SAFETY: `path` is a NUL-terminated wide string.
    let module = unsafe { LoadLibraryW(PCWSTR(path.as_ptr())) }?;

    // SAFETY: the module handle is valid and the name is a static C string.
    let symbol = unsafe { GetProcAddress(module, s!("DllGetClassObject")) };
    let Some(symbol) = symbol else {
        return Err(E_FAIL.into());
    };

    type GetClassObjectProc =
        unsafe extern "system" fn(*const GUID, *const GUID, *mut *mut c_void) -> HRESULT;
    // SAFETY: `DllGetClassObject` has exactly this signature.
    let get_class_object: GetClassObjectProc = unsafe { std::mem::transmute(symbol) };

    let mut factory = ptr::null_mut();
    // SAFETY: the out-parameter is valid and `IClassFactory::IID` is correct.
    let hr = unsafe { get_class_object(&PROVIDER_CLSID, &IClassFactory::IID, &mut factory) };
    if hr.is_err() {
        println!("  DllGetClassObject(CLSID) failed");
        return Err(windows_core::Error::from_hresult(hr));
    }
    // SAFETY: on success the callee returned an owned IClassFactory pointer.
    let factory = unsafe { IClassFactory::from_raw(factory) };
    // SAFETY: the factory is alive; the returned provider owns its reference.
    unsafe { factory.CreateInstance::<_, ICredentialProvider>(None::<&IUnknown>) }
}

pub fn run() -> i32 {
    println!("Ricardo komp logon provider self-test");

    // SAFETY: STA is the LogonUI threading model this tool mirrors.
    let com = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    println!("  {:<28} 0x{:08x}", "CoInitializeEx", com.0 as u32);

    // SAFETY: the CLSID is the registered in-proc provider.
    let mut provider = unsafe {
        CoCreateInstance::<_, ICredentialProvider>(
            &PROVIDER_CLSID,
            None::<&IUnknown>,
            CLSCTX_INPROC_SERVER,
        )
    };
    println!(
        "  {:<28} 0x{:08x} ({})",
        "CoCreateInstance",
        provider
            .as_ref()
            .err()
            .map_or(0u32, |error| error.code().0 as u32),
        if provider.is_ok() {
            "registered"
        } else {
            "falling back to local DLL"
        }
    );

    if provider.is_err() {
        provider = provider_from_dll_next_to_executable();
        println!(
            "  {:<28} 0x{:08x}",
            "LoadLibrary+DllGetClassObject",
            provider
                .as_ref()
                .err()
                .map_or(0u32, |error| error.code().0 as u32)
        );
    }

    let Ok(provider) = provider else {
        println!("FAIL: provider could not be created");
        if com.is_ok() {
            // SAFETY: balances the successful CoInitializeEx above.
            unsafe { CoUninitialize() };
        }
        return 1;
    };

    // SAFETY: the provider pointer is a live in-proc COM object.
    let hr = unsafe { provider.SetUsageScenario(CPUS_LOGON, 0) };
    print("SetUsageScenario(CPUS_LOGON)", code_of(&hr));

    // SAFETY: same live object.
    let hr = unsafe { provider.SetUsageScenario(CPUS_CREDUI, 0) };
    let code = code_of(&hr);
    println!(
        "  {:<28} 0x{:08x} ({})",
        "SetUsageScenario(CPUS_CREDUI)",
        code.0 as u32,
        if code == E_NOTIMPL {
            "E_NOTIMPL as required"
        } else {
            "UNEXPECTED"
        }
    );

    // SAFETY: same live object.
    let field_count = unsafe { provider.GetFieldDescriptorCount() }.unwrap_or(0);
    println!("  {:<28} {}", "GetFieldDescriptorCount", field_count);

    let mut count = 0u32;
    let mut default_index = 0u32;
    let mut auto_logon = BOOL(0);
    // SAFETY: all three out-parameters are valid locals.
    let hr = unsafe {
        provider.GetCredentialCount(&raw mut count, &raw mut default_index, &raw mut auto_logon)
    };
    match hr {
        Ok(()) => print("GetCredentialCount", S_OK),
        Err(error) => print("GetCredentialCount", error.code()),
    }
    println!(
        "  {:<28} count={} auto={} (0 until a grant is armed)",
        "credential count",
        count,
        auto_logon.as_bool() as i32
    );

    // Stop the poller immediately: the self-test must not consume a grant.
    // SAFETY: same live object.
    print("UnAdvise", code_of(&unsafe { provider.UnAdvise() }));

    drop(provider);
    if com.is_ok() {
        // SAFETY: balances the successful CoInitializeEx above.
        unsafe { CoUninitialize() };
    }

    let ok = com.is_ok() && field_count == 2 && count == 0;
    println!("{}", if ok { "PASS" } else { "FAIL" });
    if ok { 0 } else { 1 }
}
