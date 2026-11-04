#![allow(
    non_snake_case,
    non_upper_case_globals,
    non_camel_case_types,
    dead_code,
    clippy::all
)]

pub mod Windows {
    pub mod Win32 {
        pub mod Foundation {
            pub const E_FAIL: windows_core::HRESULT = windows_core::HRESULT(0x80004005_u32 as _);
            pub const E_NOTIMPL: windows_core::HRESULT = windows_core::HRESULT(0x80004001_u32 as _);
            pub type FARPROC = Option<unsafe extern "system" fn() -> isize>;
            #[repr(transparent)]
            #[derive(Clone, Copy, Debug, PartialEq, Eq)]
            pub struct HINSTANCE(pub *mut core::ffi::c_void);
            impl HINSTANCE {
                pub fn is_invalid(&self) -> bool {
                    self.0.is_null()
                }
            }
            impl windows_core::Free for HINSTANCE {
                #[inline]
                unsafe fn free(&mut self) {
                    if !self.is_invalid() {
                        windows_link::link!("kernel32.dll" "system" fn FreeLibrary(hlibmodule : *mut core::ffi::c_void) -> i32);
                        unsafe {
                            FreeLibrary(self.0);
                        }
                    }
                }
            }
            impl Default for HINSTANCE {
                fn default() -> Self {
                    unsafe { core::mem::zeroed() }
                }
            }
            impl windows_core::imp::CanInto<HMODULE> for HINSTANCE {}
            impl From<HINSTANCE> for HMODULE {
                fn from(value: HINSTANCE) -> Self {
                    Self(value.0)
                }
            }
            #[repr(transparent)]
            #[derive(Clone, Copy, Debug, PartialEq, Eq)]
            pub struct HMODULE(pub *mut core::ffi::c_void);
            impl HMODULE {
                pub fn is_invalid(&self) -> bool {
                    self.0.is_null()
                }
            }
            impl windows_core::Free for HMODULE {
                #[inline]
                unsafe fn free(&mut self) {
                    if !self.is_invalid() {
                        windows_link::link!("kernel32.dll" "system" fn FreeLibrary(hlibmodule : *mut core::ffi::c_void) -> i32);
                        unsafe {
                            FreeLibrary(self.0);
                        }
                    }
                }
            }
            impl Default for HMODULE {
                fn default() -> Self {
                    unsafe { core::mem::zeroed() }
                }
            }
            impl windows_core::imp::CanInto<HINSTANCE> for HMODULE {}
            impl From<HMODULE> for HINSTANCE {
                fn from(value: HMODULE) -> Self {
                    Self(value.0)
                }
            }
            pub const S_OK: windows_core::HRESULT = windows_core::HRESULT(0x0_u32 as _);
        }
        pub mod System {
            pub mod Com {
                #[inline]
                pub unsafe fn CoCreateInstance<P1, T>(
                    rclsid: *const windows_core::GUID,
                    punkouter: P1,
                    dwclscontext: CLSCTX,
                ) -> windows_core::Result<T>
                where
                    P1: windows_core::Param<windows_core::IUnknown>,
                    T: windows_core::Interface,
                {
                    windows_link::link!("ole32.dll" "system" fn CoCreateInstance(rclsid : *const windows_core::GUID, punkouter : * mut core::ffi::c_void, dwclscontext : CLSCTX, riid : *const windows_core::GUID, ppv : *mut *mut core::ffi::c_void) -> windows_core::HRESULT);
                    let mut result__ = core::ptr::null_mut();
                    unsafe {
                        CoCreateInstance(
                            rclsid,
                            punkouter.param().abi(),
                            dwclscontext,
                            &T::IID,
                            &mut result__,
                        )
                        .and_then(|| windows_core::Type::from_abi(result__))
                    }
                }
                #[inline]
                pub unsafe fn CoInitializeEx(
                    pvreserved: Option<*const core::ffi::c_void>,
                    dwcoinit: COINIT,
                ) -> windows_core::HRESULT {
                    windows_link::link!("ole32.dll" "system" fn CoInitializeEx(pvreserved : *const core::ffi::c_void, dwcoinit : u32) -> windows_core::HRESULT);
                    unsafe {
                        CoInitializeEx(
                            pvreserved.unwrap_or(core::mem::zeroed()) as _,
                            dwcoinit.0 as _,
                        )
                    }
                }
                #[inline]
                pub unsafe fn CoUninitialize() {
                    windows_link::link!("ole32.dll" "system" fn CoUninitialize());
                    unsafe { CoUninitialize() }
                }
                #[repr(transparent)]
                #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
                pub struct CLSCTX(pub u32);
                impl CLSCTX {
                    pub const fn contains(&self, other: Self) -> bool {
                        self.0 & other.0 == other.0
                    }
                }
                impl core::ops::BitOr for CLSCTX {
                    type Output = Self;
                    fn bitor(self, other: Self) -> Self {
                        Self(self.0 | other.0)
                    }
                }
                impl core::ops::BitAnd for CLSCTX {
                    type Output = Self;
                    fn bitand(self, other: Self) -> Self {
                        Self(self.0 & other.0)
                    }
                }
                impl core::ops::BitOrAssign for CLSCTX {
                    fn bitor_assign(&mut self, other: Self) {
                        self.0.bitor_assign(other.0)
                    }
                }
                impl core::ops::BitAndAssign for CLSCTX {
                    fn bitand_assign(&mut self, other: Self) {
                        self.0.bitand_assign(other.0)
                    }
                }
                impl core::ops::Not for CLSCTX {
                    type Output = Self;
                    fn not(self) -> Self {
                        Self(self.0.not())
                    }
                }
                pub const CLSCTX_INPROC_SERVER: CLSCTX = CLSCTX(1u32);
                #[repr(transparent)]
                #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
                pub struct COINIT(pub i32);
                impl COINIT {
                    pub const fn contains(&self, other: Self) -> bool {
                        self.0 & other.0 == other.0
                    }
                }
                impl core::ops::BitOr for COINIT {
                    type Output = Self;
                    fn bitor(self, other: Self) -> Self {
                        Self(self.0 | other.0)
                    }
                }
                impl core::ops::BitAnd for COINIT {
                    type Output = Self;
                    fn bitand(self, other: Self) -> Self {
                        Self(self.0 & other.0)
                    }
                }
                impl core::ops::BitOrAssign for COINIT {
                    fn bitor_assign(&mut self, other: Self) {
                        self.0.bitor_assign(other.0)
                    }
                }
                impl core::ops::BitAndAssign for COINIT {
                    fn bitand_assign(&mut self, other: Self) {
                        self.0.bitand_assign(other.0)
                    }
                }
                impl core::ops::Not for COINIT {
                    type Output = Self;
                    fn not(self) -> Self {
                        Self(self.0.not())
                    }
                }
                pub const COINIT_APARTMENTTHREADED: COINIT = COINIT(2i32);
                windows_core::imp::define_interface!(
                    IClassFactory,
                    IClassFactory_Vtbl,
                    0x00000001_0000_0000_c000_000000000046
                );
                windows_core::imp::interface_hierarchy!(IClassFactory, windows_core::IUnknown);
                impl IClassFactory {
                    pub unsafe fn CreateInstance<P0, T>(
                        &self,
                        punkouter: P0,
                    ) -> windows_core::Result<T>
                    where
                        P0: windows_core::Param<windows_core::IUnknown>,
                        T: windows_core::Interface,
                    {
                        let mut result__ = core::ptr::null_mut();
                        unsafe {
                            (windows_core::Interface::vtable(self).CreateInstance)(
                                windows_core::Interface::as_raw(self),
                                punkouter.param().abi(),
                                &T::IID,
                                &mut result__,
                            )
                            .and_then(|| windows_core::Type::from_abi(result__))
                        }
                    }
                    pub unsafe fn LockServer(&self, flock: bool) -> windows_core::Result<()> {
                        unsafe {
                            (windows_core::Interface::vtable(self).LockServer)(
                                windows_core::Interface::as_raw(self),
                                flock.into(),
                            )
                            .ok()
                        }
                    }
                }
                #[repr(C)]
                #[doc(hidden)]
                pub struct IClassFactory_Vtbl {
                    pub base__: windows_core::IUnknown_Vtbl,
                    pub CreateInstance: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        *mut core::ffi::c_void,
                        *const windows_core::GUID,
                        *mut *mut core::ffi::c_void,
                    )
                        -> windows_core::HRESULT,
                    pub LockServer: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        windows_core::BOOL,
                    )
                        -> windows_core::HRESULT,
                }
                pub trait IClassFactory_Impl: windows_core::IUnknownImpl {
                    fn CreateInstance(
                        &self,
                        punkouter: windows_core::Ref<windows_core::IUnknown>,
                        riid: *const windows_core::GUID,
                        ppvobject: *mut *mut core::ffi::c_void,
                    ) -> windows_core::Result<()>;
                    fn LockServer(&self, flock: windows_core::BOOL) -> windows_core::Result<()>;
                }
                impl IClassFactory_Vtbl {
                    pub const fn new<Identity: IClassFactory_Impl, const OFFSET: isize>() -> Self {
                        unsafe extern "system" fn CreateInstance<
                            Identity: IClassFactory_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            punkouter: *mut core::ffi::c_void,
                            riid: *const windows_core::GUID,
                            ppvobject: *mut *mut core::ffi::c_void,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                IClassFactory_Impl::CreateInstance(
                                    this,
                                    core::mem::transmute_copy(&punkouter),
                                    core::mem::transmute_copy(&riid),
                                    core::mem::transmute_copy(&ppvobject),
                                )
                                .into()
                            }
                        }
                        unsafe extern "system" fn LockServer<
                            Identity: IClassFactory_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            flock: windows_core::BOOL,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                IClassFactory_Impl::LockServer(
                                    this,
                                    core::mem::transmute_copy(&flock),
                                )
                                .into()
                            }
                        }
                        Self {
                            base__: windows_core::IUnknown_Vtbl::new::<Identity, OFFSET>(),
                            CreateInstance: CreateInstance::<Identity, OFFSET>,
                            LockServer: LockServer::<Identity, OFFSET>,
                        }
                    }
                    pub fn matches(iid: &windows_core::GUID) -> bool {
                        iid == &<IClassFactory as windows_core::Interface>::IID
                    }
                }
                impl windows_core::RuntimeName for IClassFactory {}
            }
            pub mod LibraryLoader {
                #[inline]
                pub unsafe fn GetModuleFileNameW(
                    hmodule: Option<super::super::Foundation::HMODULE>,
                    lpfilename: &mut [u16],
                ) -> u32 {
                    windows_link::link!("kernel32.dll" "system" fn GetModuleFileNameW(hmodule : super::super::Foundation:: HMODULE, lpfilename : windows_core::PWSTR, nsize : u32) -> u32);
                    unsafe {
                        GetModuleFileNameW(
                            hmodule.unwrap_or(core::mem::zeroed()) as _,
                            core::mem::transmute(lpfilename.as_ptr()),
                            lpfilename.len().try_into().unwrap(),
                        )
                    }
                }
                #[inline]
                pub unsafe fn GetProcAddress<P1>(
                    hmodule: super::super::Foundation::HMODULE,
                    lpprocname: P1,
                ) -> super::super::Foundation::FARPROC
                where
                    P1: windows_core::Param<windows_core::PCSTR>,
                {
                    windows_link::link!("kernel32.dll" "system" fn GetProcAddress(hmodule : super::super::Foundation:: HMODULE, lpprocname : windows_core::PCSTR) -> super::super::Foundation:: FARPROC);
                    unsafe { GetProcAddress(hmodule, lpprocname.param().abi()) }
                }
                #[inline]
                pub unsafe fn LoadLibraryW<P0>(
                    lplibfilename: P0,
                ) -> windows_core::Result<super::super::Foundation::HMODULE>
                where
                    P0: windows_core::Param<windows_core::PCWSTR>,
                {
                    windows_link::link!("kernel32.dll" "system" fn LoadLibraryW(lplibfilename : windows_core::PCWSTR) -> super::super::Foundation:: HMODULE);
                    let result__ = unsafe { LoadLibraryW(lplibfilename.param().abi()) };
                    (!result__.is_invalid())
                        .then_some(result__)
                        .ok_or_else(windows_core::Error::from_thread)
                }
            }
        }
        pub mod UI {
            pub mod Shell {
                pub const CPUS_CREDUI: CREDENTIAL_PROVIDER_USAGE_SCENARIO =
                    CREDENTIAL_PROVIDER_USAGE_SCENARIO(4i32);
                pub const CPUS_LOGON: CREDENTIAL_PROVIDER_USAGE_SCENARIO =
                    CREDENTIAL_PROVIDER_USAGE_SCENARIO(1i32);
                #[repr(transparent)]
                #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
                pub struct CREDENTIAL_PROVIDER_USAGE_SCENARIO(pub i32);
                windows_core::imp::define_interface!(
                    ICredentialProvider,
                    ICredentialProvider_Vtbl,
                    0xd27c3481_5a1c_45b2_8aaa_c20ebbe8229e
                );
                windows_core::imp::interface_hierarchy!(
                    ICredentialProvider,
                    windows_core::IUnknown
                );
                impl ICredentialProvider {
                    pub unsafe fn SetUsageScenario(
                        &self,
                        cpus: CREDENTIAL_PROVIDER_USAGE_SCENARIO,
                        dwflags: u32,
                    ) -> windows_core::Result<()> {
                        unsafe {
                            (windows_core::Interface::vtable(self).SetUsageScenario)(
                                windows_core::Interface::as_raw(self),
                                cpus,
                                dwflags,
                            )
                            .ok()
                        }
                    }
                    pub unsafe fn UnAdvise(&self) -> windows_core::Result<()> {
                        unsafe {
                            (windows_core::Interface::vtable(self).UnAdvise)(
                                windows_core::Interface::as_raw(self),
                            )
                            .ok()
                        }
                    }
                    pub unsafe fn GetFieldDescriptorCount(&self) -> windows_core::Result<u32> {
                        unsafe {
                            let mut result__ = core::mem::zeroed();
                            (windows_core::Interface::vtable(self).GetFieldDescriptorCount)(
                                windows_core::Interface::as_raw(self),
                                &mut result__,
                            )
                            .map(|| result__)
                        }
                    }
                    pub unsafe fn GetCredentialCount(
                        &self,
                        pdwcount: *mut u32,
                        pdwdefault: *mut u32,
                        pbautologonwithdefault: *mut windows_core::BOOL,
                    ) -> windows_core::Result<()> {
                        unsafe {
                            (windows_core::Interface::vtable(self).GetCredentialCount)(
                                windows_core::Interface::as_raw(self),
                                pdwcount as _,
                                pdwdefault as _,
                                pbautologonwithdefault as _,
                            )
                            .ok()
                        }
                    }
                }
                #[repr(C)]
                #[doc(hidden)]
                pub struct ICredentialProvider_Vtbl {
                    pub base__: windows_core::IUnknown_Vtbl,
                    pub SetUsageScenario: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        CREDENTIAL_PROVIDER_USAGE_SCENARIO,
                        u32,
                    )
                        -> windows_core::HRESULT,
                    SetSerialization: usize,
                    Advise: usize,
                    pub UnAdvise:
                        unsafe extern "system" fn(*mut core::ffi::c_void) -> windows_core::HRESULT,
                    pub GetFieldDescriptorCount: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        *mut u32,
                    )
                        -> windows_core::HRESULT,
                    GetFieldDescriptorAt: usize,
                    pub GetCredentialCount: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        *mut u32,
                        *mut u32,
                        *mut windows_core::BOOL,
                    )
                        -> windows_core::HRESULT,
                    GetCredentialAt: usize,
                }
                pub trait ICredentialProvider_Impl: windows_core::IUnknownImpl {
                    fn SetUsageScenario(
                        &self,
                        cpus: CREDENTIAL_PROVIDER_USAGE_SCENARIO,
                        dwflags: u32,
                    ) -> windows_core::Result<()>;
                    fn UnAdvise(&self) -> windows_core::Result<()>;
                    fn GetFieldDescriptorCount(&self) -> windows_core::Result<u32>;
                    fn GetCredentialCount(
                        &self,
                        pdwcount: *mut u32,
                        pdwdefault: *mut u32,
                        pbautologonwithdefault: *mut windows_core::BOOL,
                    ) -> windows_core::Result<()>;
                }
                impl ICredentialProvider_Vtbl {
                    pub const fn new<Identity: ICredentialProvider_Impl, const OFFSET: isize>()
                    -> Self {
                        unsafe extern "system" fn SetUsageScenario<
                            Identity: ICredentialProvider_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            cpus: CREDENTIAL_PROVIDER_USAGE_SCENARIO,
                            dwflags: u32,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProvider_Impl::SetUsageScenario(
                                    this,
                                    core::mem::transmute_copy(&cpus),
                                    core::mem::transmute_copy(&dwflags),
                                )
                                .into()
                            }
                        }
                        unsafe extern "system" fn UnAdvise<
                            Identity: ICredentialProvider_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProvider_Impl::UnAdvise(this).into()
                            }
                        }
                        unsafe extern "system" fn GetFieldDescriptorCount<
                            Identity: ICredentialProvider_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            pdwcount: *mut u32,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                match ICredentialProvider_Impl::GetFieldDescriptorCount(this) {
                                    Ok(ok__) => {
                                        pdwcount.write(core::mem::transmute(ok__));
                                        windows_core::HRESULT(0)
                                    }
                                    Err(err) => err.into(),
                                }
                            }
                        }
                        unsafe extern "system" fn GetCredentialCount<
                            Identity: ICredentialProvider_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            pdwcount: *mut u32,
                            pdwdefault: *mut u32,
                            pbautologonwithdefault: *mut windows_core::BOOL,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProvider_Impl::GetCredentialCount(
                                    this,
                                    core::mem::transmute_copy(&pdwcount),
                                    core::mem::transmute_copy(&pdwdefault),
                                    core::mem::transmute_copy(&pbautologonwithdefault),
                                )
                                .into()
                            }
                        }
                        Self {
                            base__: windows_core::IUnknown_Vtbl::new::<Identity, OFFSET>(),
                            SetUsageScenario: SetUsageScenario::<Identity, OFFSET>,
                            SetSerialization: 0,
                            Advise: 0,
                            UnAdvise: UnAdvise::<Identity, OFFSET>,
                            GetFieldDescriptorCount: GetFieldDescriptorCount::<Identity, OFFSET>,
                            GetFieldDescriptorAt: 0,
                            GetCredentialCount: GetCredentialCount::<Identity, OFFSET>,
                            GetCredentialAt: 0,
                        }
                    }
                    pub fn matches(iid: &windows_core::GUID) -> bool {
                        iid == &<ICredentialProvider as windows_core::Interface>::IID
                    }
                }
                impl windows_core::RuntimeName for ICredentialProvider {}
            }
        }
    }
}
