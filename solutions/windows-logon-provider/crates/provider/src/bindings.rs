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
            #[inline]
            pub unsafe fn CloseHandle(hobject: HANDLE) -> windows_core::Result<()> {
                windows_link::link!("kernel32.dll" "system" fn CloseHandle(hobject : HANDLE) -> windows_core::BOOL);
                unsafe { CloseHandle(hobject).ok() }
            }
            #[inline]
            pub unsafe fn FreeLibrary(hlibmodule: HMODULE) -> windows_core::Result<()> {
                windows_link::link!("kernel32.dll" "system" fn FreeLibrary(hlibmodule : HMODULE) -> windows_core::BOOL);
                unsafe { FreeLibrary(hlibmodule).ok() }
            }
            #[inline]
            pub unsafe fn LocalFree(hmem: Option<HLOCAL>) -> HLOCAL {
                windows_link::link!("kernel32.dll" "system" fn LocalFree(hmem : HLOCAL) -> HLOCAL);
                unsafe { LocalFree(hmem.unwrap_or(core::mem::zeroed()) as _) }
            }
            pub const CLASS_E_CLASSNOTAVAILABLE: windows_core::HRESULT =
                windows_core::HRESULT(0x80040111_u32 as _);
            pub const CLASS_E_NOAGGREGATION: windows_core::HRESULT =
                windows_core::HRESULT(0x80040110_u32 as _);
            pub const ERROR_FILE_NOT_FOUND: WIN32_ERROR = WIN32_ERROR(2u32);
            pub const ERROR_INSUFFICIENT_BUFFER: WIN32_ERROR = WIN32_ERROR(122u32);
            pub const ERROR_SUCCESS: WIN32_ERROR = WIN32_ERROR(0u32);
            pub const E_FAIL: windows_core::HRESULT = windows_core::HRESULT(0x80004005_u32 as _);
            pub const E_INVALIDARG: windows_core::HRESULT =
                windows_core::HRESULT(0x80070057_u32 as _);
            pub const E_NOTIMPL: windows_core::HRESULT = windows_core::HRESULT(0x80004001_u32 as _);
            pub const E_OUTOFMEMORY: windows_core::HRESULT =
                windows_core::HRESULT(0x8007000E_u32 as _);
            pub const E_UNEXPECTED: windows_core::HRESULT =
                windows_core::HRESULT(0x8000FFFF_u32 as _);
            #[repr(C)]
            #[derive(Clone, Copy, Debug, Default, PartialEq)]
            pub struct FILETIME {
                pub dwLowDateTime: u32,
                pub dwHighDateTime: u32,
            }
            #[repr(transparent)]
            #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
            pub struct GENERIC_ACCESS_RIGHTS(pub u32);
            impl GENERIC_ACCESS_RIGHTS {
                pub const fn contains(&self, other: Self) -> bool {
                    self.0 & other.0 == other.0
                }
            }
            impl core::ops::BitOr for GENERIC_ACCESS_RIGHTS {
                type Output = Self;
                fn bitor(self, other: Self) -> Self {
                    Self(self.0 | other.0)
                }
            }
            impl core::ops::BitAnd for GENERIC_ACCESS_RIGHTS {
                type Output = Self;
                fn bitand(self, other: Self) -> Self {
                    Self(self.0 & other.0)
                }
            }
            impl core::ops::BitOrAssign for GENERIC_ACCESS_RIGHTS {
                fn bitor_assign(&mut self, other: Self) {
                    self.0.bitor_assign(other.0)
                }
            }
            impl core::ops::BitAndAssign for GENERIC_ACCESS_RIGHTS {
                fn bitand_assign(&mut self, other: Self) {
                    self.0.bitand_assign(other.0)
                }
            }
            impl core::ops::Not for GENERIC_ACCESS_RIGHTS {
                type Output = Self;
                fn not(self) -> Self {
                    Self(self.0.not())
                }
            }
            pub const GENERIC_READ: GENERIC_ACCESS_RIGHTS = GENERIC_ACCESS_RIGHTS(2147483648u32);
            #[repr(transparent)]
            #[derive(Clone, Copy, Debug, PartialEq, Eq)]
            pub struct HANDLE(pub *mut core::ffi::c_void);
            impl HANDLE {
                pub fn is_invalid(&self) -> bool {
                    self.0 == -1 as _ || self.0 == 0 as _
                }
            }
            impl windows_core::Free for HANDLE {
                #[inline]
                unsafe fn free(&mut self) {
                    if !self.is_invalid() {
                        windows_link::link!("kernel32.dll" "system" fn CloseHandle(hobject : *mut core::ffi::c_void) -> i32);
                        unsafe {
                            CloseHandle(self.0);
                        }
                    }
                }
            }
            impl Default for HANDLE {
                fn default() -> Self {
                    unsafe { core::mem::zeroed() }
                }
            }
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
            pub struct HLOCAL(pub *mut core::ffi::c_void);
            impl HLOCAL {
                pub fn is_invalid(&self) -> bool {
                    self.0 == -1 as _ || self.0 == 0 as _
                }
            }
            impl windows_core::Free for HLOCAL {
                #[inline]
                unsafe fn free(&mut self) {
                    if !self.is_invalid() {
                        windows_link::link!("kernel32.dll" "system" fn LocalFree(hmem : *mut core::ffi::c_void) -> *mut core::ffi::c_void);
                        unsafe {
                            LocalFree(self.0);
                        }
                    }
                }
            }
            impl Default for HLOCAL {
                fn default() -> Self {
                    unsafe { core::mem::zeroed() }
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
            #[repr(transparent)]
            #[derive(Clone, Copy, Debug, PartialEq, Eq)]
            pub struct HWND(pub *mut core::ffi::c_void);
            impl HWND {
                pub fn is_invalid(&self) -> bool {
                    self.0.is_null()
                }
            }
            impl Default for HWND {
                fn default() -> Self {
                    unsafe { core::mem::zeroed() }
                }
            }
            impl windows_core::imp::CanInto<HANDLE> for HWND {}
            impl From<HWND> for HANDLE {
                fn from(value: HWND) -> Self {
                    Self(value.0)
                }
            }
            #[repr(C)]
            #[derive(Clone, Copy, Debug, Default, PartialEq)]
            pub struct LUID {
                pub LowPart: u32,
                pub HighPart: i32,
            }
            #[must_use]
            #[repr(transparent)]
            #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
            pub struct NTSTATUS(pub i32);
            #[repr(C)]
            #[derive(Clone, Copy, Debug, Default, PartialEq)]
            pub struct SYSTEMTIME {
                pub wYear: u16,
                pub wMonth: u16,
                pub wDayOfWeek: u16,
                pub wDay: u16,
                pub wHour: u16,
                pub wMinute: u16,
                pub wSecond: u16,
                pub wMilliseconds: u16,
            }
            pub const S_FALSE: windows_core::HRESULT = windows_core::HRESULT(0x1_u32 as _);
            pub const S_OK: windows_core::HRESULT = windows_core::HRESULT(0x0_u32 as _);
            #[must_use]
            #[repr(transparent)]
            #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
            pub struct WIN32_ERROR(pub u32);
        }
        pub mod Globalization {
            #[inline]
            pub unsafe fn MultiByteToWideChar(
                codepage: u32,
                dwflags: MULTI_BYTE_TO_WIDE_CHAR_FLAGS,
                lpmultibytestr: &[u8],
                lpwidecharstr: Option<&mut [u16]>,
            ) -> i32 {
                windows_link::link!("kernel32.dll" "system" fn MultiByteToWideChar(codepage : u32, dwflags : MULTI_BYTE_TO_WIDE_CHAR_FLAGS, lpmultibytestr : windows_core::PCSTR, cbmultibyte : i32, lpwidecharstr : windows_core::PWSTR, cchwidechar : i32) -> i32);
                unsafe {
                    MultiByteToWideChar(
                        codepage,
                        dwflags,
                        core::mem::transmute(lpmultibytestr.as_ptr()),
                        lpmultibytestr.len().try_into().unwrap(),
                        core::mem::transmute(
                            lpwidecharstr
                                .as_deref()
                                .map_or(core::ptr::null(), |slice| slice.as_ptr()),
                        ),
                        lpwidecharstr
                            .as_deref()
                            .map_or(0, |slice| slice.len().try_into().unwrap()),
                    )
                }
            }
            #[inline]
            pub unsafe fn WideCharToMultiByte<P6>(
                codepage: u32,
                dwflags: u32,
                lpwidecharstr: &[u16],
                lpmultibytestr: Option<&mut [u8]>,
                lpdefaultchar: P6,
                lpuseddefaultchar: Option<*mut windows_core::BOOL>,
            ) -> i32
            where
                P6: windows_core::Param<windows_core::PCSTR>,
            {
                windows_link::link!("kernel32.dll" "system" fn WideCharToMultiByte(codepage : u32, dwflags : u32, lpwidecharstr : windows_core::PCWSTR, cchwidechar : i32, lpmultibytestr : windows_core::PSTR, cbmultibyte : i32, lpdefaultchar : windows_core::PCSTR, lpuseddefaultchar : *mut windows_core::BOOL) -> i32);
                unsafe {
                    WideCharToMultiByte(
                        codepage,
                        dwflags,
                        core::mem::transmute(lpwidecharstr.as_ptr()),
                        lpwidecharstr.len().try_into().unwrap(),
                        core::mem::transmute(
                            lpmultibytestr
                                .as_deref()
                                .map_or(core::ptr::null(), |slice| slice.as_ptr()),
                        ),
                        lpmultibytestr
                            .as_deref()
                            .map_or(0, |slice| slice.len().try_into().unwrap()),
                        lpdefaultchar.param().abi(),
                        lpuseddefaultchar.unwrap_or(core::mem::zeroed()) as _,
                    )
                }
            }
            pub const CP_UTF8: u32 = 65001u32;
            #[repr(transparent)]
            #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
            pub struct MULTI_BYTE_TO_WIDE_CHAR_FLAGS(pub u32);
            impl MULTI_BYTE_TO_WIDE_CHAR_FLAGS {
                pub const fn contains(&self, other: Self) -> bool {
                    self.0 & other.0 == other.0
                }
            }
            impl core::ops::BitOr for MULTI_BYTE_TO_WIDE_CHAR_FLAGS {
                type Output = Self;
                fn bitor(self, other: Self) -> Self {
                    Self(self.0 | other.0)
                }
            }
            impl core::ops::BitAnd for MULTI_BYTE_TO_WIDE_CHAR_FLAGS {
                type Output = Self;
                fn bitand(self, other: Self) -> Self {
                    Self(self.0 & other.0)
                }
            }
            impl core::ops::BitOrAssign for MULTI_BYTE_TO_WIDE_CHAR_FLAGS {
                fn bitor_assign(&mut self, other: Self) {
                    self.0.bitor_assign(other.0)
                }
            }
            impl core::ops::BitAndAssign for MULTI_BYTE_TO_WIDE_CHAR_FLAGS {
                fn bitand_assign(&mut self, other: Self) {
                    self.0.bitand_assign(other.0)
                }
            }
            impl core::ops::Not for MULTI_BYTE_TO_WIDE_CHAR_FLAGS {
                type Output = Self;
                fn not(self) -> Self {
                    Self(self.0.not())
                }
            }
        }
        pub mod Graphics {
            pub mod Gdi {
                #[repr(transparent)]
                #[derive(Clone, Copy, Debug, PartialEq, Eq)]
                pub struct HBITMAP(pub *mut core::ffi::c_void);
                impl HBITMAP {
                    pub fn is_invalid(&self) -> bool {
                        self.0 == -1 as _ || self.0 == 0 as _
                    }
                }
                impl windows_core::Free for HBITMAP {
                    #[inline]
                    unsafe fn free(&mut self) {
                        if !self.is_invalid() {
                            windows_link::link!("gdi32.dll" "system" fn DeleteObject(ho : *mut core::ffi::c_void) -> i32);
                            unsafe {
                                DeleteObject(self.0);
                            }
                        }
                    }
                }
                impl Default for HBITMAP {
                    fn default() -> Self {
                        unsafe { core::mem::zeroed() }
                    }
                }
                impl windows_core::imp::CanInto<HGDIOBJ> for HBITMAP {}
                impl From<HBITMAP> for HGDIOBJ {
                    fn from(value: HBITMAP) -> Self {
                        Self(value.0)
                    }
                }
                #[repr(transparent)]
                #[derive(Clone, Copy, Debug, PartialEq, Eq)]
                pub struct HGDIOBJ(pub *mut core::ffi::c_void);
                impl HGDIOBJ {
                    pub fn is_invalid(&self) -> bool {
                        self.0 == -1 as _ || self.0 == 0 as _
                    }
                }
                impl windows_core::Free for HGDIOBJ {
                    #[inline]
                    unsafe fn free(&mut self) {
                        if !self.is_invalid() {
                            windows_link::link!("gdi32.dll" "system" fn DeleteObject(ho : *mut core::ffi::c_void) -> i32);
                            unsafe {
                                DeleteObject(self.0);
                            }
                        }
                    }
                }
                impl Default for HGDIOBJ {
                    fn default() -> Self {
                        unsafe { core::mem::zeroed() }
                    }
                }
            }
        }
        pub mod Networking {
            pub mod WinHttp {
                #[inline]
                pub unsafe fn WinHttpCloseHandle(
                    hinternet: *mut core::ffi::c_void,
                ) -> windows_core::Result<()> {
                    windows_link::link!("winhttp.dll" "system" fn WinHttpCloseHandle(hinternet : *mut core::ffi::c_void) -> windows_core::BOOL);
                    unsafe { WinHttpCloseHandle(hinternet as _).ok() }
                }
                #[inline]
                pub unsafe fn WinHttpConnect<P1>(
                    hsession: *mut core::ffi::c_void,
                    pswzservername: P1,
                    nserverport: u16,
                    dwreserved: u32,
                ) -> *mut core::ffi::c_void
                where
                    P1: windows_core::Param<windows_core::PCWSTR>,
                {
                    windows_link::link!("winhttp.dll" "system" fn WinHttpConnect(hsession : *mut core::ffi::c_void, pswzservername : windows_core::PCWSTR, nserverport : u16, dwreserved : u32) -> *mut core::ffi::c_void);
                    unsafe {
                        WinHttpConnect(
                            hsession as _,
                            pswzservername.param().abi(),
                            nserverport,
                            dwreserved,
                        )
                    }
                }
                #[inline]
                pub unsafe fn WinHttpOpen<P0, P2, P3>(
                    pszagentw: P0,
                    dwaccesstype: WINHTTP_ACCESS_TYPE,
                    pszproxyw: P2,
                    pszproxybypassw: P3,
                    dwflags: u32,
                ) -> *mut core::ffi::c_void
                where
                    P0: windows_core::Param<windows_core::PCWSTR>,
                    P2: windows_core::Param<windows_core::PCWSTR>,
                    P3: windows_core::Param<windows_core::PCWSTR>,
                {
                    windows_link::link!("winhttp.dll" "system" fn WinHttpOpen(pszagentw : windows_core::PCWSTR, dwaccesstype : WINHTTP_ACCESS_TYPE, pszproxyw : windows_core::PCWSTR, pszproxybypassw : windows_core::PCWSTR, dwflags : u32) -> *mut core::ffi::c_void);
                    unsafe {
                        WinHttpOpen(
                            pszagentw.param().abi(),
                            dwaccesstype,
                            pszproxyw.param().abi(),
                            pszproxybypassw.param().abi(),
                            dwflags,
                        )
                    }
                }
                #[inline]
                pub unsafe fn WinHttpOpenRequest<P1, P2, P3, P4>(
                    hconnect: *mut core::ffi::c_void,
                    pwszverb: P1,
                    pwszobjectname: P2,
                    pwszversion: P3,
                    pwszreferrer: P4,
                    ppwszaccepttypes: *const windows_core::PCWSTR,
                    dwflags: WINHTTP_OPEN_REQUEST_FLAGS,
                ) -> *mut core::ffi::c_void
                where
                    P1: windows_core::Param<windows_core::PCWSTR>,
                    P2: windows_core::Param<windows_core::PCWSTR>,
                    P3: windows_core::Param<windows_core::PCWSTR>,
                    P4: windows_core::Param<windows_core::PCWSTR>,
                {
                    windows_link::link!("winhttp.dll" "system" fn WinHttpOpenRequest(hconnect : *mut core::ffi::c_void, pwszverb : windows_core::PCWSTR, pwszobjectname : windows_core::PCWSTR, pwszversion : windows_core::PCWSTR, pwszreferrer : windows_core::PCWSTR, ppwszaccepttypes : *const windows_core::PCWSTR, dwflags : WINHTTP_OPEN_REQUEST_FLAGS) -> *mut core::ffi::c_void);
                    unsafe {
                        WinHttpOpenRequest(
                            hconnect as _,
                            pwszverb.param().abi(),
                            pwszobjectname.param().abi(),
                            pwszversion.param().abi(),
                            pwszreferrer.param().abi(),
                            ppwszaccepttypes,
                            dwflags,
                        )
                    }
                }
                #[inline]
                pub unsafe fn WinHttpQueryDataAvailable(
                    hrequest: *mut core::ffi::c_void,
                    lpdwnumberofbytesavailable: *mut u32,
                ) -> windows_core::Result<()> {
                    windows_link::link!("winhttp.dll" "system" fn WinHttpQueryDataAvailable(hrequest : *mut core::ffi::c_void, lpdwnumberofbytesavailable : *mut u32) -> windows_core::BOOL);
                    unsafe {
                        WinHttpQueryDataAvailable(hrequest as _, lpdwnumberofbytesavailable as _)
                            .ok()
                    }
                }
                #[inline]
                pub unsafe fn WinHttpQueryHeaders<P2>(
                    hrequest: *mut core::ffi::c_void,
                    dwinfolevel: u32,
                    pwszname: P2,
                    lpbuffer: Option<*mut core::ffi::c_void>,
                    lpdwbufferlength: *mut u32,
                    lpdwindex: *mut u32,
                ) -> windows_core::Result<()>
                where
                    P2: windows_core::Param<windows_core::PCWSTR>,
                {
                    windows_link::link!("winhttp.dll" "system" fn WinHttpQueryHeaders(hrequest : *mut core::ffi::c_void, dwinfolevel : u32, pwszname : windows_core::PCWSTR, lpbuffer : *mut core::ffi::c_void, lpdwbufferlength : *mut u32, lpdwindex : *mut u32) -> windows_core::BOOL);
                    unsafe {
                        WinHttpQueryHeaders(
                            hrequest as _,
                            dwinfolevel,
                            pwszname.param().abi(),
                            lpbuffer.unwrap_or(core::mem::zeroed()) as _,
                            lpdwbufferlength as _,
                            lpdwindex as _,
                        )
                        .ok()
                    }
                }
                #[inline]
                pub unsafe fn WinHttpReadData(
                    hrequest: *mut core::ffi::c_void,
                    lpbuffer: *mut core::ffi::c_void,
                    dwnumberofbytestoread: u32,
                    lpdwnumberofbytesread: *mut u32,
                ) -> windows_core::Result<()> {
                    windows_link::link!("winhttp.dll" "system" fn WinHttpReadData(hrequest : *mut core::ffi::c_void, lpbuffer : *mut core::ffi::c_void, dwnumberofbytestoread : u32, lpdwnumberofbytesread : *mut u32) -> windows_core::BOOL);
                    unsafe {
                        WinHttpReadData(
                            hrequest as _,
                            lpbuffer as _,
                            dwnumberofbytestoread,
                            lpdwnumberofbytesread as _,
                        )
                        .ok()
                    }
                }
                #[inline]
                pub unsafe fn WinHttpReceiveResponse(
                    hrequest: *mut core::ffi::c_void,
                    lpreserved: *mut core::ffi::c_void,
                ) -> windows_core::Result<()> {
                    windows_link::link!("winhttp.dll" "system" fn WinHttpReceiveResponse(hrequest : *mut core::ffi::c_void, lpreserved : *mut core::ffi::c_void) -> windows_core::BOOL);
                    unsafe { WinHttpReceiveResponse(hrequest as _, lpreserved as _).ok() }
                }
                #[inline]
                pub unsafe fn WinHttpSendRequest(
                    hrequest: *mut core::ffi::c_void,
                    lpszheaders: Option<&[u16]>,
                    lpoptional: Option<*const core::ffi::c_void>,
                    dwoptionallength: u32,
                    dwtotallength: u32,
                    dwcontext: usize,
                ) -> windows_core::Result<()> {
                    windows_link::link!("winhttp.dll" "system" fn WinHttpSendRequest(hrequest : *mut core::ffi::c_void, lpszheaders : windows_core::PCWSTR, dwheaderslength : u32, lpoptional : *const core::ffi::c_void, dwoptionallength : u32, dwtotallength : u32, dwcontext : usize) -> windows_core::BOOL);
                    unsafe {
                        WinHttpSendRequest(
                            hrequest as _,
                            core::mem::transmute(
                                lpszheaders
                                    .as_deref()
                                    .map_or(core::ptr::null(), |slice| slice.as_ptr()),
                            ),
                            lpszheaders
                                .as_deref()
                                .map_or(0, |slice| slice.len().try_into().unwrap()),
                            lpoptional.unwrap_or(core::mem::zeroed()) as _,
                            dwoptionallength,
                            dwtotallength,
                            dwcontext,
                        )
                        .ok()
                    }
                }
                #[inline]
                pub unsafe fn WinHttpSetOption(
                    hinternet: Option<*const core::ffi::c_void>,
                    dwoption: u32,
                    lpbuffer: Option<&[u8]>,
                ) -> windows_core::Result<()> {
                    windows_link::link!("winhttp.dll" "system" fn WinHttpSetOption(hinternet : *const core::ffi::c_void, dwoption : u32, lpbuffer : *const core::ffi::c_void, dwbufferlength : u32) -> windows_core::BOOL);
                    unsafe {
                        WinHttpSetOption(
                            hinternet.unwrap_or(core::mem::zeroed()) as _,
                            dwoption,
                            core::mem::transmute(
                                lpbuffer
                                    .as_deref()
                                    .map_or(core::ptr::null(), |slice| slice.as_ptr()),
                            ),
                            lpbuffer
                                .as_deref()
                                .map_or(0, |slice| slice.len().try_into().unwrap()),
                        )
                        .ok()
                    }
                }
                #[inline]
                pub unsafe fn WinHttpSetTimeouts(
                    hinternet: *mut core::ffi::c_void,
                    nresolvetimeout: i32,
                    nconnecttimeout: i32,
                    nsendtimeout: i32,
                    nreceivetimeout: i32,
                ) -> windows_core::Result<()> {
                    windows_link::link!("winhttp.dll" "system" fn WinHttpSetTimeouts(hinternet : *mut core::ffi::c_void, nresolvetimeout : i32, nconnecttimeout : i32, nsendtimeout : i32, nreceivetimeout : i32) -> windows_core::BOOL);
                    unsafe {
                        WinHttpSetTimeouts(
                            hinternet as _,
                            nresolvetimeout,
                            nconnecttimeout,
                            nsendtimeout,
                            nreceivetimeout,
                        )
                        .ok()
                    }
                }
                #[repr(transparent)]
                #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
                pub struct WINHTTP_ACCESS_TYPE(pub u32);
                pub const WINHTTP_ACCESS_TYPE_NO_PROXY: WINHTTP_ACCESS_TYPE =
                    WINHTTP_ACCESS_TYPE(1u32);
                pub const WINHTTP_DISABLE_COOKIES: u32 = 1u32;
                pub const WINHTTP_DISABLE_REDIRECTS: u32 = 2u32;
                pub const WINHTTP_FLAG_SECURE: WINHTTP_OPEN_REQUEST_FLAGS =
                    WINHTTP_OPEN_REQUEST_FLAGS(8388608u32);
                #[repr(transparent)]
                #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
                pub struct WINHTTP_OPEN_REQUEST_FLAGS(pub u32);
                impl WINHTTP_OPEN_REQUEST_FLAGS {
                    pub const fn contains(&self, other: Self) -> bool {
                        self.0 & other.0 == other.0
                    }
                }
                impl core::ops::BitOr for WINHTTP_OPEN_REQUEST_FLAGS {
                    type Output = Self;
                    fn bitor(self, other: Self) -> Self {
                        Self(self.0 | other.0)
                    }
                }
                impl core::ops::BitAnd for WINHTTP_OPEN_REQUEST_FLAGS {
                    type Output = Self;
                    fn bitand(self, other: Self) -> Self {
                        Self(self.0 & other.0)
                    }
                }
                impl core::ops::BitOrAssign for WINHTTP_OPEN_REQUEST_FLAGS {
                    fn bitor_assign(&mut self, other: Self) {
                        self.0.bitor_assign(other.0)
                    }
                }
                impl core::ops::BitAndAssign for WINHTTP_OPEN_REQUEST_FLAGS {
                    fn bitand_assign(&mut self, other: Self) {
                        self.0.bitand_assign(other.0)
                    }
                }
                impl core::ops::Not for WINHTTP_OPEN_REQUEST_FLAGS {
                    type Output = Self;
                    fn not(self) -> Self {
                        Self(self.0.not())
                    }
                }
                pub const WINHTTP_OPTION_DISABLE_FEATURE: u32 = 63u32;
                pub const WINHTTP_OPTION_REDIRECT_POLICY: u32 = 88u32;
                pub const WINHTTP_OPTION_REDIRECT_POLICY_NEVER: u32 = 0u32;
                pub const WINHTTP_QUERY_FLAG_NUMBER: u32 = 536870912u32;
                pub const WINHTTP_QUERY_STATUS_CODE: u32 = 19u32;
            }
        }
        pub mod Security {
            #[inline]
            pub unsafe fn LookupAccountNameW<P0, P1>(
                lpsystemname: P0,
                lpaccountname: P1,
                sid: Option<PSID>,
                cbsid: *mut u32,
                referenceddomainname: Option<windows_core::PWSTR>,
                cchreferenceddomainname: *mut u32,
                peuse: *mut SID_NAME_USE,
            ) -> windows_core::Result<()>
            where
                P0: windows_core::Param<windows_core::PCWSTR>,
                P1: windows_core::Param<windows_core::PCWSTR>,
            {
                windows_link::link!("advapi32.dll" "system" fn LookupAccountNameW(lpsystemname : windows_core::PCWSTR, lpaccountname : windows_core::PCWSTR, sid : PSID, cbsid : *mut u32, referenceddomainname : windows_core::PWSTR, cchreferenceddomainname : *mut u32, peuse : *mut SID_NAME_USE) -> windows_core::BOOL);
                unsafe {
                    LookupAccountNameW(
                        lpsystemname.param().abi(),
                        lpaccountname.param().abi(),
                        sid.unwrap_or(core::mem::zeroed()) as _,
                        cbsid as _,
                        referenceddomainname.unwrap_or(core::mem::zeroed()) as _,
                        cchreferenceddomainname as _,
                        peuse as _,
                    )
                    .ok()
                }
            }
            #[repr(transparent)]
            #[derive(Clone, Copy, Debug, PartialEq, Eq)]
            pub struct PSID(pub *mut core::ffi::c_void);
            impl PSID {
                pub fn is_invalid(&self) -> bool {
                    self.0.is_null()
                }
            }
            impl Default for PSID {
                fn default() -> Self {
                    unsafe { core::mem::zeroed() }
                }
            }
            #[repr(C)]
            #[derive(Clone, Copy, Debug, PartialEq)]
            pub struct SECURITY_ATTRIBUTES {
                pub nLength: u32,
                pub lpSecurityDescriptor: *mut core::ffi::c_void,
                pub bInheritHandle: windows_core::BOOL,
            }
            impl Default for SECURITY_ATTRIBUTES {
                fn default() -> Self {
                    unsafe { core::mem::zeroed() }
                }
            }
            pub const SECURITY_MAX_SID_SIZE: u32 = 68u32;
            #[repr(transparent)]
            #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
            pub struct SID_NAME_USE(pub i32);
            pub mod Authentication {
                pub mod Identity {
                    #[inline]
                    pub unsafe fn LsaConnectUntrusted(
                        lsahandle: *mut super::super::super::Foundation::HANDLE,
                    ) -> super::super::super::Foundation::NTSTATUS {
                        windows_link::link!("secur32.dll" "system" fn LsaConnectUntrusted(lsahandle : *mut super::super::super::Foundation:: HANDLE) -> super::super::super::Foundation:: NTSTATUS);
                        unsafe { LsaConnectUntrusted(lsahandle as _) }
                    }
                    #[inline]
                    pub unsafe fn LsaDeregisterLogonProcess(
                        lsahandle: super::super::super::Foundation::HANDLE,
                    ) -> super::super::super::Foundation::NTSTATUS {
                        windows_link::link!("secur32.dll" "system" fn LsaDeregisterLogonProcess(lsahandle : super::super::super::Foundation:: HANDLE) -> super::super::super::Foundation:: NTSTATUS);
                        unsafe { LsaDeregisterLogonProcess(lsahandle) }
                    }
                    #[inline]
                    pub unsafe fn LsaLookupAuthenticationPackage(
                        lsahandle: super::super::super::Foundation::HANDLE,
                        packagename: *const LSA_STRING,
                        authenticationpackage: *mut u32,
                    ) -> super::super::super::Foundation::NTSTATUS {
                        windows_link::link!("secur32.dll" "system" fn LsaLookupAuthenticationPackage(lsahandle : super::super::super::Foundation:: HANDLE, packagename : *const LSA_STRING, authenticationpackage : *mut u32) -> super::super::super::Foundation:: NTSTATUS);
                        unsafe {
                            LsaLookupAuthenticationPackage(
                                lsahandle,
                                packagename,
                                authenticationpackage as _,
                            )
                        }
                    }
                    #[repr(C)]
                    #[derive(Clone, Copy, Debug, Default, PartialEq)]
                    pub struct KERB_INTERACTIVE_LOGON {
                        pub MessageType: KERB_LOGON_SUBMIT_TYPE,
                        pub LogonDomainName: LSA_UNICODE_STRING,
                        pub UserName: LSA_UNICODE_STRING,
                        pub Password: LSA_UNICODE_STRING,
                    }
                    #[repr(C)]
                    #[derive(Clone, Copy, Debug, Default, PartialEq)]
                    pub struct KERB_INTERACTIVE_UNLOCK_LOGON {
                        pub Logon: KERB_INTERACTIVE_LOGON,
                        pub LogonId: super::super::super::Foundation::LUID,
                    }
                    #[repr(transparent)]
                    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
                    pub struct KERB_LOGON_SUBMIT_TYPE(pub i32);
                    #[repr(C)]
                    #[derive(Clone, Copy, Debug, Default, PartialEq)]
                    pub struct LSA_STRING {
                        pub Length: u16,
                        pub MaximumLength: u16,
                        pub Buffer: windows_core::PSTR,
                    }
                    #[repr(C)]
                    #[derive(Clone, Copy, Debug, Default, PartialEq)]
                    pub struct LSA_UNICODE_STRING {
                        pub Length: u16,
                        pub MaximumLength: u16,
                        pub Buffer: windows_core::PWSTR,
                    }
                }
            }
            pub mod Authorization {
                #[inline]
                pub unsafe fn ConvertSidToStringSidW(
                    sid: super::PSID,
                    stringsid: *mut windows_core::PWSTR,
                ) -> windows_core::Result<()> {
                    windows_link::link!("advapi32.dll" "system" fn ConvertSidToStringSidW(sid : super:: PSID, stringsid : *mut windows_core::PWSTR) -> windows_core::BOOL);
                    unsafe { ConvertSidToStringSidW(sid, stringsid as _).ok() }
                }
            }
            pub mod Credentials {
                #[inline]
                pub unsafe fn CredIsProtectedW<P0>(
                    pszprotectedcredentials: P0,
                    pprotectiontype: *mut CRED_PROTECTION_TYPE,
                ) -> windows_core::Result<()>
                where
                    P0: windows_core::Param<windows_core::PCWSTR>,
                {
                    windows_link::link!("advapi32.dll" "system" fn CredIsProtectedW(pszprotectedcredentials : windows_core::PCWSTR, pprotectiontype : *mut CRED_PROTECTION_TYPE) -> windows_core::BOOL);
                    unsafe {
                        CredIsProtectedW(
                            pszprotectedcredentials.param().abi(),
                            pprotectiontype as _,
                        )
                        .ok()
                    }
                }
                #[inline]
                pub unsafe fn CredProtectW(
                    fasself: bool,
                    pszcredentials: &[u16],
                    pszprotectedcredentials: windows_core::PWSTR,
                    pcchmaxchars: *mut u32,
                    protectiontype: Option<*mut CRED_PROTECTION_TYPE>,
                ) -> windows_core::Result<()> {
                    windows_link::link!("advapi32.dll" "system" fn CredProtectW(fasself : windows_core::BOOL, pszcredentials : windows_core::PCWSTR, cchcredentials : u32, pszprotectedcredentials : windows_core::PWSTR, pcchmaxchars : *mut u32, protectiontype : *mut CRED_PROTECTION_TYPE) -> windows_core::BOOL);
                    unsafe {
                        CredProtectW(
                            fasself.into(),
                            core::mem::transmute(pszcredentials.as_ptr()),
                            pszcredentials.len().try_into().unwrap(),
                            core::mem::transmute(pszprotectedcredentials),
                            pcchmaxchars as _,
                            protectiontype.unwrap_or(core::mem::zeroed()) as _,
                        )
                        .ok()
                    }
                }
                #[repr(transparent)]
                #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
                pub struct CRED_PROTECTION_TYPE(pub i32);
                pub const CredUnprotected: CRED_PROTECTION_TYPE = CRED_PROTECTION_TYPE(0i32);
            }
            pub mod Cryptography {
                #[inline]
                pub unsafe fn CryptUnprotectData(
                    pdatain: *const CRYPT_INTEGER_BLOB,
                    ppszdatadescr: Option<*mut windows_core::PWSTR>,
                    poptionalentropy: Option<*const CRYPT_INTEGER_BLOB>,
                    pvreserved: Option<*const core::ffi::c_void>,
                    ppromptstruct: Option<*const CRYPTPROTECT_PROMPTSTRUCT>,
                    dwflags: u32,
                    pdataout: *mut CRYPT_INTEGER_BLOB,
                ) -> windows_core::Result<()> {
                    windows_link::link!("crypt32.dll" "system" fn CryptUnprotectData(pdatain : *const CRYPT_INTEGER_BLOB, ppszdatadescr : *mut windows_core::PWSTR, poptionalentropy : *const CRYPT_INTEGER_BLOB, pvreserved : *const core::ffi::c_void, ppromptstruct : *const CRYPTPROTECT_PROMPTSTRUCT, dwflags : u32, pdataout : *mut CRYPT_INTEGER_BLOB) -> windows_core::BOOL);
                    unsafe {
                        CryptUnprotectData(
                            pdatain,
                            ppszdatadescr.unwrap_or(core::mem::zeroed()) as _,
                            poptionalentropy.unwrap_or(core::mem::zeroed()) as _,
                            pvreserved.unwrap_or(core::mem::zeroed()) as _,
                            ppromptstruct.unwrap_or(core::mem::zeroed()) as _,
                            dwflags,
                            pdataout as _,
                        )
                        .ok()
                    }
                }
                #[repr(C)]
                #[derive(Clone, Copy, Debug, Default, PartialEq)]
                pub struct CRYPTPROTECT_PROMPTSTRUCT {
                    pub cbSize: u32,
                    pub dwPromptFlags: u32,
                    pub hwndApp: super::super::Foundation::HWND,
                    pub szPrompt: windows_core::PCWSTR,
                }
                #[repr(C)]
                #[derive(Clone, Copy, Debug, PartialEq)]
                pub struct CRYPT_INTEGER_BLOB {
                    pub cbData: u32,
                    pub pbData: *mut u8,
                }
                impl Default for CRYPT_INTEGER_BLOB {
                    fn default() -> Self {
                        unsafe { core::mem::zeroed() }
                    }
                }
            }
        }
        pub mod Storage {
            pub mod FileSystem {
                #[inline]
                pub unsafe fn CreateDirectoryW<P0>(
                    lppathname: P0,
                    lpsecurityattributes: Option<
                        *const super::super::Security::SECURITY_ATTRIBUTES,
                    >,
                ) -> windows_core::Result<()>
                where
                    P0: windows_core::Param<windows_core::PCWSTR>,
                {
                    windows_link::link!("kernel32.dll" "system" fn CreateDirectoryW(lppathname : windows_core::PCWSTR, lpsecurityattributes : *const super::super::Security:: SECURITY_ATTRIBUTES) -> windows_core::BOOL);
                    unsafe {
                        CreateDirectoryW(
                            lppathname.param().abi(),
                            lpsecurityattributes.unwrap_or(core::mem::zeroed()) as _,
                        )
                        .ok()
                    }
                }
                #[inline]
                pub unsafe fn CreateFileW<P0>(
                    lpfilename: P0,
                    dwdesiredaccess: u32,
                    dwsharemode: FILE_SHARE_MODE,
                    lpsecurityattributes: Option<
                        *const super::super::Security::SECURITY_ATTRIBUTES,
                    >,
                    dwcreationdisposition: FILE_CREATION_DISPOSITION,
                    dwflagsandattributes: FILE_FLAGS_AND_ATTRIBUTES,
                    htemplatefile: Option<super::super::Foundation::HANDLE>,
                ) -> windows_core::Result<super::super::Foundation::HANDLE>
                where
                    P0: windows_core::Param<windows_core::PCWSTR>,
                {
                    windows_link::link!("kernel32.dll" "system" fn CreateFileW(lpfilename : windows_core::PCWSTR, dwdesiredaccess : u32, dwsharemode : FILE_SHARE_MODE, lpsecurityattributes : *const super::super::Security:: SECURITY_ATTRIBUTES, dwcreationdisposition : FILE_CREATION_DISPOSITION, dwflagsandattributes : FILE_FLAGS_AND_ATTRIBUTES, htemplatefile : super::super::Foundation:: HANDLE) -> super::super::Foundation:: HANDLE);
                    let result__ = unsafe {
                        CreateFileW(
                            lpfilename.param().abi(),
                            dwdesiredaccess,
                            dwsharemode,
                            lpsecurityattributes.unwrap_or(core::mem::zeroed()) as _,
                            dwcreationdisposition,
                            dwflagsandattributes,
                            htemplatefile.unwrap_or(core::mem::zeroed()) as _,
                        )
                    };
                    (!result__.is_invalid())
                        .then_some(result__)
                        .ok_or_else(windows_core::Error::from_thread)
                }
                #[inline]
                pub unsafe fn DeleteFileW<P0>(lpfilename: P0) -> windows_core::Result<()>
                where
                    P0: windows_core::Param<windows_core::PCWSTR>,
                {
                    windows_link::link!("kernel32.dll" "system" fn DeleteFileW(lpfilename : windows_core::PCWSTR) -> windows_core::BOOL);
                    unsafe { DeleteFileW(lpfilename.param().abi()).ok() }
                }
                #[inline]
                pub unsafe fn GetFileAttributesExW<P0>(
                    lpfilename: P0,
                    finfolevelid: GET_FILEEX_INFO_LEVELS,
                    lpfileinformation: *mut core::ffi::c_void,
                ) -> windows_core::Result<()>
                where
                    P0: windows_core::Param<windows_core::PCWSTR>,
                {
                    windows_link::link!("kernel32.dll" "system" fn GetFileAttributesExW(lpfilename : windows_core::PCWSTR, finfolevelid : GET_FILEEX_INFO_LEVELS, lpfileinformation : *mut core::ffi::c_void) -> windows_core::BOOL);
                    unsafe {
                        GetFileAttributesExW(
                            lpfilename.param().abi(),
                            finfolevelid,
                            lpfileinformation as _,
                        )
                        .ok()
                    }
                }
                #[inline]
                pub unsafe fn GetFileSizeEx(
                    hfile: super::super::Foundation::HANDLE,
                    lpfilesize: *mut i64,
                ) -> windows_core::Result<()> {
                    windows_link::link!("kernel32.dll" "system" fn GetFileSizeEx(hfile : super::super::Foundation:: HANDLE, lpfilesize : *mut i64) -> windows_core::BOOL);
                    unsafe { GetFileSizeEx(hfile, lpfilesize as _).ok() }
                }
                #[inline]
                pub unsafe fn MoveFileExW<P0, P1>(
                    lpexistingfilename: P0,
                    lpnewfilename: P1,
                    dwflags: MOVE_FILE_FLAGS,
                ) -> windows_core::Result<()>
                where
                    P0: windows_core::Param<windows_core::PCWSTR>,
                    P1: windows_core::Param<windows_core::PCWSTR>,
                {
                    windows_link::link!("kernel32.dll" "system" fn MoveFileExW(lpexistingfilename : windows_core::PCWSTR, lpnewfilename : windows_core::PCWSTR, dwflags : MOVE_FILE_FLAGS) -> windows_core::BOOL);
                    unsafe {
                        MoveFileExW(
                            lpexistingfilename.param().abi(),
                            lpnewfilename.param().abi(),
                            dwflags,
                        )
                        .ok()
                    }
                }
                #[inline]
                pub unsafe fn ReadFile(
                    hfile: super::super::Foundation::HANDLE,
                    lpbuffer: Option<&mut [u8]>,
                    lpnumberofbytesread: Option<*mut u32>,
                    lpoverlapped: Option<*mut super::super::System::IO::OVERLAPPED>,
                ) -> windows_core::Result<()> {
                    windows_link::link!("kernel32.dll" "system" fn ReadFile(hfile : super::super::Foundation:: HANDLE, lpbuffer : *mut u8, nnumberofbytestoread : u32, lpnumberofbytesread : *mut u32, lpoverlapped : *mut super::super::System::IO:: OVERLAPPED) -> windows_core::BOOL);
                    unsafe {
                        ReadFile(
                            hfile,
                            core::mem::transmute(
                                lpbuffer
                                    .as_deref()
                                    .map_or(core::ptr::null(), |slice| slice.as_ptr()),
                            ),
                            lpbuffer
                                .as_deref()
                                .map_or(0, |slice| slice.len().try_into().unwrap()),
                            lpnumberofbytesread.unwrap_or(core::mem::zeroed()) as _,
                            lpoverlapped.unwrap_or(core::mem::zeroed()) as _,
                        )
                        .ok()
                    }
                }
                #[inline]
                pub unsafe fn WriteFile(
                    hfile: super::super::Foundation::HANDLE,
                    lpbuffer: Option<&[u8]>,
                    lpnumberofbyteswritten: Option<*mut u32>,
                    lpoverlapped: Option<*mut super::super::System::IO::OVERLAPPED>,
                ) -> windows_core::Result<()> {
                    windows_link::link!("kernel32.dll" "system" fn WriteFile(hfile : super::super::Foundation:: HANDLE, lpbuffer : *const u8, nnumberofbytestowrite : u32, lpnumberofbyteswritten : *mut u32, lpoverlapped : *mut super::super::System::IO:: OVERLAPPED) -> windows_core::BOOL);
                    unsafe {
                        WriteFile(
                            hfile,
                            core::mem::transmute(
                                lpbuffer
                                    .as_deref()
                                    .map_or(core::ptr::null(), |slice| slice.as_ptr()),
                            ),
                            lpbuffer
                                .as_deref()
                                .map_or(0, |slice| slice.len().try_into().unwrap()),
                            lpnumberofbyteswritten.unwrap_or(core::mem::zeroed()) as _,
                            lpoverlapped.unwrap_or(core::mem::zeroed()) as _,
                        )
                        .ok()
                    }
                }
                #[repr(transparent)]
                #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
                pub struct FILE_ACCESS_RIGHTS(pub u32);
                impl FILE_ACCESS_RIGHTS {
                    pub const fn contains(&self, other: Self) -> bool {
                        self.0 & other.0 == other.0
                    }
                }
                impl core::ops::BitOr for FILE_ACCESS_RIGHTS {
                    type Output = Self;
                    fn bitor(self, other: Self) -> Self {
                        Self(self.0 | other.0)
                    }
                }
                impl core::ops::BitAnd for FILE_ACCESS_RIGHTS {
                    type Output = Self;
                    fn bitand(self, other: Self) -> Self {
                        Self(self.0 & other.0)
                    }
                }
                impl core::ops::BitOrAssign for FILE_ACCESS_RIGHTS {
                    fn bitor_assign(&mut self, other: Self) {
                        self.0.bitor_assign(other.0)
                    }
                }
                impl core::ops::BitAndAssign for FILE_ACCESS_RIGHTS {
                    fn bitand_assign(&mut self, other: Self) {
                        self.0.bitand_assign(other.0)
                    }
                }
                impl core::ops::Not for FILE_ACCESS_RIGHTS {
                    type Output = Self;
                    fn not(self) -> Self {
                        Self(self.0.not())
                    }
                }
                pub const FILE_APPEND_DATA: FILE_ACCESS_RIGHTS = FILE_ACCESS_RIGHTS(4u32);
                pub const FILE_ATTRIBUTE_NORMAL: FILE_FLAGS_AND_ATTRIBUTES =
                    FILE_FLAGS_AND_ATTRIBUTES(128u32);
                #[repr(transparent)]
                #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
                pub struct FILE_CREATION_DISPOSITION(pub u32);
                #[repr(transparent)]
                #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
                pub struct FILE_FLAGS_AND_ATTRIBUTES(pub u32);
                impl FILE_FLAGS_AND_ATTRIBUTES {
                    pub const fn contains(&self, other: Self) -> bool {
                        self.0 & other.0 == other.0
                    }
                }
                impl core::ops::BitOr for FILE_FLAGS_AND_ATTRIBUTES {
                    type Output = Self;
                    fn bitor(self, other: Self) -> Self {
                        Self(self.0 | other.0)
                    }
                }
                impl core::ops::BitAnd for FILE_FLAGS_AND_ATTRIBUTES {
                    type Output = Self;
                    fn bitand(self, other: Self) -> Self {
                        Self(self.0 & other.0)
                    }
                }
                impl core::ops::BitOrAssign for FILE_FLAGS_AND_ATTRIBUTES {
                    fn bitor_assign(&mut self, other: Self) {
                        self.0.bitor_assign(other.0)
                    }
                }
                impl core::ops::BitAndAssign for FILE_FLAGS_AND_ATTRIBUTES {
                    fn bitand_assign(&mut self, other: Self) {
                        self.0.bitand_assign(other.0)
                    }
                }
                impl core::ops::Not for FILE_FLAGS_AND_ATTRIBUTES {
                    type Output = Self;
                    fn not(self) -> Self {
                        Self(self.0.not())
                    }
                }
                #[repr(transparent)]
                #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
                pub struct FILE_SHARE_MODE(pub u32);
                impl FILE_SHARE_MODE {
                    pub const fn contains(&self, other: Self) -> bool {
                        self.0 & other.0 == other.0
                    }
                }
                impl core::ops::BitOr for FILE_SHARE_MODE {
                    type Output = Self;
                    fn bitor(self, other: Self) -> Self {
                        Self(self.0 | other.0)
                    }
                }
                impl core::ops::BitAnd for FILE_SHARE_MODE {
                    type Output = Self;
                    fn bitand(self, other: Self) -> Self {
                        Self(self.0 & other.0)
                    }
                }
                impl core::ops::BitOrAssign for FILE_SHARE_MODE {
                    fn bitor_assign(&mut self, other: Self) {
                        self.0.bitor_assign(other.0)
                    }
                }
                impl core::ops::BitAndAssign for FILE_SHARE_MODE {
                    fn bitand_assign(&mut self, other: Self) {
                        self.0.bitand_assign(other.0)
                    }
                }
                impl core::ops::Not for FILE_SHARE_MODE {
                    type Output = Self;
                    fn not(self) -> Self {
                        Self(self.0.not())
                    }
                }
                pub const FILE_SHARE_READ: FILE_SHARE_MODE = FILE_SHARE_MODE(1u32);
                pub const FILE_SHARE_WRITE: FILE_SHARE_MODE = FILE_SHARE_MODE(2u32);
                #[repr(transparent)]
                #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
                pub struct GET_FILEEX_INFO_LEVELS(pub i32);
                pub const GetFileExInfoStandard: GET_FILEEX_INFO_LEVELS =
                    GET_FILEEX_INFO_LEVELS(0i32);
                pub const MOVEFILE_REPLACE_EXISTING: MOVE_FILE_FLAGS = MOVE_FILE_FLAGS(1u32);
                #[repr(transparent)]
                #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
                pub struct MOVE_FILE_FLAGS(pub u32);
                impl MOVE_FILE_FLAGS {
                    pub const fn contains(&self, other: Self) -> bool {
                        self.0 & other.0 == other.0
                    }
                }
                impl core::ops::BitOr for MOVE_FILE_FLAGS {
                    type Output = Self;
                    fn bitor(self, other: Self) -> Self {
                        Self(self.0 | other.0)
                    }
                }
                impl core::ops::BitAnd for MOVE_FILE_FLAGS {
                    type Output = Self;
                    fn bitand(self, other: Self) -> Self {
                        Self(self.0 & other.0)
                    }
                }
                impl core::ops::BitOrAssign for MOVE_FILE_FLAGS {
                    fn bitor_assign(&mut self, other: Self) {
                        self.0.bitor_assign(other.0)
                    }
                }
                impl core::ops::BitAndAssign for MOVE_FILE_FLAGS {
                    fn bitand_assign(&mut self, other: Self) {
                        self.0.bitand_assign(other.0)
                    }
                }
                impl core::ops::Not for MOVE_FILE_FLAGS {
                    type Output = Self;
                    fn not(self) -> Self {
                        Self(self.0.not())
                    }
                }
                pub const OPEN_ALWAYS: FILE_CREATION_DISPOSITION = FILE_CREATION_DISPOSITION(4u32);
                pub const OPEN_EXISTING: FILE_CREATION_DISPOSITION =
                    FILE_CREATION_DISPOSITION(3u32);
                #[repr(C)]
                #[derive(Clone, Copy, Debug, Default, PartialEq)]
                pub struct WIN32_FILE_ATTRIBUTE_DATA {
                    pub dwFileAttributes: u32,
                    pub ftCreationTime: super::super::Foundation::FILETIME,
                    pub ftLastAccessTime: super::super::Foundation::FILETIME,
                    pub ftLastWriteTime: super::super::Foundation::FILETIME,
                    pub nFileSizeHigh: u32,
                    pub nFileSizeLow: u32,
                }
            }
        }
        pub mod System {
            pub mod Com {
                #[inline]
                pub unsafe fn CoTaskMemAlloc(cb: usize) -> *mut core::ffi::c_void {
                    windows_link::link!("combase.dll" "system" fn CoTaskMemAlloc(cb : usize) -> *mut core::ffi::c_void);
                    unsafe { CoTaskMemAlloc(cb) }
                }
                #[inline]
                pub unsafe fn CoTaskMemFree(pv: Option<*const core::ffi::c_void>) {
                    windows_link::link!("combase.dll" "system" fn CoTaskMemFree(pv : *const core::ffi::c_void));
                    unsafe { CoTaskMemFree(pv.unwrap_or(core::mem::zeroed()) as _) }
                }
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
            pub mod IO {
                #[repr(C)]
                #[derive(Clone, Copy)]
                pub struct OVERLAPPED {
                    pub Internal: usize,
                    pub InternalHigh: usize,
                    pub Anonymous: OVERLAPPED_0,
                    pub hEvent: super::super::Foundation::HANDLE,
                }
                impl Default for OVERLAPPED {
                    fn default() -> Self {
                        unsafe { core::mem::zeroed() }
                    }
                }
                #[repr(C)]
                #[derive(Clone, Copy)]
                pub union OVERLAPPED_0 {
                    pub Anonymous: OVERLAPPED_0_0,
                    pub Pointer: *mut core::ffi::c_void,
                }
                impl Default for OVERLAPPED_0 {
                    fn default() -> Self {
                        unsafe { core::mem::zeroed() }
                    }
                }
                #[repr(C)]
                #[derive(Clone, Copy, Debug, Default, PartialEq)]
                pub struct OVERLAPPED_0_0 {
                    pub Offset: u32,
                    pub OffsetHigh: u32,
                }
            }
            pub mod LibraryLoader {
                #[inline]
                pub unsafe fn FreeLibraryAndExitThread(
                    hlibmodule: super::super::Foundation::HMODULE,
                    dwexitcode: u32,
                ) -> ! {
                    windows_link::link!("kernel32.dll" "system" fn FreeLibraryAndExitThread(hlibmodule : super::super::Foundation:: HMODULE, dwexitcode : u32) -> !);
                    unsafe { FreeLibraryAndExitThread(hlibmodule, dwexitcode) }
                }
                #[inline]
                pub unsafe fn GetModuleHandleExW<P1>(
                    dwflags: u32,
                    lpmodulename: P1,
                    phmodule: *mut super::super::Foundation::HMODULE,
                ) -> windows_core::Result<()>
                where
                    P1: windows_core::Param<windows_core::PCWSTR>,
                {
                    windows_link::link!("kernel32.dll" "system" fn GetModuleHandleExW(dwflags : u32, lpmodulename : windows_core::PCWSTR, phmodule : *mut super::super::Foundation:: HMODULE) -> windows_core::BOOL);
                    unsafe {
                        GetModuleHandleExW(dwflags, lpmodulename.param().abi(), phmodule as _).ok()
                    }
                }
                pub const GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS: u32 = 4u32;
            }
            pub mod Registry {
                #[inline]
                pub unsafe fn RegCloseKey(hkey: HKEY) -> super::super::Foundation::WIN32_ERROR {
                    windows_link::link!("advapi32.dll" "system" fn RegCloseKey(hkey : HKEY) -> super::super::Foundation:: WIN32_ERROR);
                    unsafe { RegCloseKey(hkey) }
                }
                #[inline]
                pub unsafe fn RegGetValueW<P1, P2>(
                    hkey: HKEY,
                    lpsubkey: P1,
                    lpvalue: P2,
                    dwflags: REG_ROUTINE_FLAGS,
                    pdwtype: Option<*mut REG_VALUE_TYPE>,
                    pvdata: Option<*mut core::ffi::c_void>,
                    pcbdata: Option<*mut u32>,
                ) -> super::super::Foundation::WIN32_ERROR
                where
                    P1: windows_core::Param<windows_core::PCWSTR>,
                    P2: windows_core::Param<windows_core::PCWSTR>,
                {
                    windows_link::link!("advapi32.dll" "system" fn RegGetValueW(hkey : HKEY, lpsubkey : windows_core::PCWSTR, lpvalue : windows_core::PCWSTR, dwflags : REG_ROUTINE_FLAGS, pdwtype : *mut REG_VALUE_TYPE, pvdata : *mut core::ffi::c_void, pcbdata : *mut u32) -> super::super::Foundation:: WIN32_ERROR);
                    unsafe {
                        RegGetValueW(
                            hkey,
                            lpsubkey.param().abi(),
                            lpvalue.param().abi(),
                            dwflags,
                            pdwtype.unwrap_or(core::mem::zeroed()) as _,
                            pvdata.unwrap_or(core::mem::zeroed()) as _,
                            pcbdata.unwrap_or(core::mem::zeroed()) as _,
                        )
                    }
                }
                #[inline]
                pub unsafe fn RegOpenKeyExW<P1>(
                    hkey: HKEY,
                    lpsubkey: P1,
                    uloptions: Option<u32>,
                    samdesired: REG_SAM_FLAGS,
                    phkresult: *mut HKEY,
                ) -> super::super::Foundation::WIN32_ERROR
                where
                    P1: windows_core::Param<windows_core::PCWSTR>,
                {
                    windows_link::link!("advapi32.dll" "system" fn RegOpenKeyExW(hkey : HKEY, lpsubkey : windows_core::PCWSTR, uloptions : u32, samdesired : REG_SAM_FLAGS, phkresult : *mut HKEY) -> super::super::Foundation:: WIN32_ERROR);
                    unsafe {
                        RegOpenKeyExW(
                            hkey,
                            lpsubkey.param().abi(),
                            uloptions.unwrap_or(core::mem::zeroed()) as _,
                            samdesired,
                            phkresult as _,
                        )
                    }
                }
                #[inline]
                pub unsafe fn RegSetValueExW<P1>(
                    hkey: HKEY,
                    lpvaluename: P1,
                    reserved: Option<u32>,
                    dwtype: REG_VALUE_TYPE,
                    lpdata: Option<&[u8]>,
                ) -> super::super::Foundation::WIN32_ERROR
                where
                    P1: windows_core::Param<windows_core::PCWSTR>,
                {
                    windows_link::link!("advapi32.dll" "system" fn RegSetValueExW(hkey : HKEY, lpvaluename : windows_core::PCWSTR, reserved : u32, dwtype : REG_VALUE_TYPE, lpdata : *const u8, cbdata : u32) -> super::super::Foundation:: WIN32_ERROR);
                    unsafe {
                        RegSetValueExW(
                            hkey,
                            lpvaluename.param().abi(),
                            reserved.unwrap_or(core::mem::zeroed()) as _,
                            dwtype,
                            core::mem::transmute(
                                lpdata
                                    .as_deref()
                                    .map_or(core::ptr::null(), |slice| slice.as_ptr()),
                            ),
                            lpdata
                                .as_deref()
                                .map_or(0, |slice| slice.len().try_into().unwrap()),
                        )
                    }
                }
                #[repr(transparent)]
                #[derive(Clone, Copy, Debug, PartialEq, Eq)]
                pub struct HKEY(pub *mut core::ffi::c_void);
                impl HKEY {
                    pub fn is_invalid(&self) -> bool {
                        self.0 == -1 as _ || self.0 == 0 as _
                    }
                }
                impl windows_core::Free for HKEY {
                    #[inline]
                    unsafe fn free(&mut self) {
                        if !self.is_invalid() {
                            windows_link::link!("advapi32.dll" "system" fn RegCloseKey(hkey : *mut core::ffi::c_void) -> u32);
                            unsafe {
                                RegCloseKey(self.0);
                            }
                        }
                    }
                }
                impl Default for HKEY {
                    fn default() -> Self {
                        unsafe { core::mem::zeroed() }
                    }
                }
                pub const HKEY_LOCAL_MACHINE: HKEY = HKEY(-2147483646i32 as _);
                pub const KEY_READ: REG_SAM_FLAGS = REG_SAM_FLAGS(131097u32);
                pub const KEY_SET_VALUE: REG_SAM_FLAGS = REG_SAM_FLAGS(2u32);
                pub const KEY_WOW64_64KEY: REG_SAM_FLAGS = REG_SAM_FLAGS(256u32);
                pub const REG_DWORD: REG_VALUE_TYPE = REG_VALUE_TYPE(4u32);
                #[repr(transparent)]
                #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
                pub struct REG_ROUTINE_FLAGS(pub u32);
                impl REG_ROUTINE_FLAGS {
                    pub const fn contains(&self, other: Self) -> bool {
                        self.0 & other.0 == other.0
                    }
                }
                impl core::ops::BitOr for REG_ROUTINE_FLAGS {
                    type Output = Self;
                    fn bitor(self, other: Self) -> Self {
                        Self(self.0 | other.0)
                    }
                }
                impl core::ops::BitAnd for REG_ROUTINE_FLAGS {
                    type Output = Self;
                    fn bitand(self, other: Self) -> Self {
                        Self(self.0 & other.0)
                    }
                }
                impl core::ops::BitOrAssign for REG_ROUTINE_FLAGS {
                    fn bitor_assign(&mut self, other: Self) {
                        self.0.bitor_assign(other.0)
                    }
                }
                impl core::ops::BitAndAssign for REG_ROUTINE_FLAGS {
                    fn bitand_assign(&mut self, other: Self) {
                        self.0.bitand_assign(other.0)
                    }
                }
                impl core::ops::Not for REG_ROUTINE_FLAGS {
                    type Output = Self;
                    fn not(self) -> Self {
                        Self(self.0.not())
                    }
                }
                #[repr(transparent)]
                #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
                pub struct REG_SAM_FLAGS(pub u32);
                impl REG_SAM_FLAGS {
                    pub const fn contains(&self, other: Self) -> bool {
                        self.0 & other.0 == other.0
                    }
                }
                impl core::ops::BitOr for REG_SAM_FLAGS {
                    type Output = Self;
                    fn bitor(self, other: Self) -> Self {
                        Self(self.0 | other.0)
                    }
                }
                impl core::ops::BitAnd for REG_SAM_FLAGS {
                    type Output = Self;
                    fn bitand(self, other: Self) -> Self {
                        Self(self.0 & other.0)
                    }
                }
                impl core::ops::BitOrAssign for REG_SAM_FLAGS {
                    fn bitor_assign(&mut self, other: Self) {
                        self.0.bitor_assign(other.0)
                    }
                }
                impl core::ops::BitAndAssign for REG_SAM_FLAGS {
                    fn bitand_assign(&mut self, other: Self) {
                        self.0.bitand_assign(other.0)
                    }
                }
                impl core::ops::Not for REG_SAM_FLAGS {
                    type Output = Self;
                    fn not(self) -> Self {
                        Self(self.0.not())
                    }
                }
                #[repr(transparent)]
                #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
                pub struct REG_VALUE_TYPE(pub u32);
                pub const RRF_RT_REG_DWORD: REG_ROUTINE_FLAGS = REG_ROUTINE_FLAGS(16u32);
                pub const RRF_RT_REG_SZ: REG_ROUTINE_FLAGS = REG_ROUTINE_FLAGS(2u32);
                pub const RRF_SUBKEY_WOW6464KEY: REG_ROUTINE_FLAGS = REG_ROUTINE_FLAGS(65536u32);
            }
            pub mod SystemInformation {
                #[inline]
                pub unsafe fn GetSystemTime() -> super::super::Foundation::SYSTEMTIME {
                    windows_link::link!("kernel32.dll" "system" fn GetSystemTime(lpsystemtime : *mut super::super::Foundation:: SYSTEMTIME));
                    unsafe {
                        let mut result__ = core::mem::zeroed();
                        GetSystemTime(&mut result__);
                        result__
                    }
                }
                #[inline]
                pub unsafe fn GetTickCount64() -> u64 {
                    windows_link::link!("kernel32.dll" "system" fn GetTickCount64() -> u64);
                    unsafe { GetTickCount64() }
                }
            }
            pub mod Threading {
                #[inline]
                pub unsafe fn CreateThread(
                    lpthreadattributes: Option<*const super::super::Security::SECURITY_ATTRIBUTES>,
                    dwstacksize: usize,
                    lpstartaddress: LPTHREAD_START_ROUTINE,
                    lpparameter: Option<*const core::ffi::c_void>,
                    dwcreationflags: THREAD_CREATION_FLAGS,
                    lpthreadid: Option<*mut u32>,
                ) -> windows_core::Result<super::super::Foundation::HANDLE> {
                    windows_link::link!("kernel32.dll" "system" fn CreateThread(lpthreadattributes : *const super::super::Security:: SECURITY_ATTRIBUTES, dwstacksize : usize, lpstartaddress : LPTHREAD_START_ROUTINE, lpparameter : *const core::ffi::c_void, dwcreationflags : THREAD_CREATION_FLAGS, lpthreadid : *mut u32) -> super::super::Foundation:: HANDLE);
                    let result__ = unsafe {
                        CreateThread(
                            lpthreadattributes.unwrap_or(core::mem::zeroed()) as _,
                            dwstacksize,
                            lpstartaddress,
                            lpparameter.unwrap_or(core::mem::zeroed()) as _,
                            dwcreationflags,
                            lpthreadid.unwrap_or(core::mem::zeroed()) as _,
                        )
                    };
                    (!result__.is_invalid())
                        .then_some(result__)
                        .ok_or_else(windows_core::Error::from_thread)
                }
                #[inline]
                pub unsafe fn GetCurrentProcessId() -> u32 {
                    windows_link::link!("kernel32.dll" "system" fn GetCurrentProcessId() -> u32);
                    unsafe { GetCurrentProcessId() }
                }
                #[inline]
                pub unsafe fn GetCurrentThreadId() -> u32 {
                    windows_link::link!("kernel32.dll" "system" fn GetCurrentThreadId() -> u32);
                    unsafe { GetCurrentThreadId() }
                }
                pub type LPTHREAD_START_ROUTINE = Option<
                    unsafe extern "system" fn(lpthreadparameter: *mut core::ffi::c_void) -> u32,
                >;
                #[repr(transparent)]
                #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
                pub struct THREAD_CREATION_FLAGS(pub u32);
                impl THREAD_CREATION_FLAGS {
                    pub const fn contains(&self, other: Self) -> bool {
                        self.0 & other.0 == other.0
                    }
                }
                impl core::ops::BitOr for THREAD_CREATION_FLAGS {
                    type Output = Self;
                    fn bitor(self, other: Self) -> Self {
                        Self(self.0 | other.0)
                    }
                }
                impl core::ops::BitAnd for THREAD_CREATION_FLAGS {
                    type Output = Self;
                    fn bitand(self, other: Self) -> Self {
                        Self(self.0 & other.0)
                    }
                }
                impl core::ops::BitOrAssign for THREAD_CREATION_FLAGS {
                    fn bitor_assign(&mut self, other: Self) {
                        self.0.bitor_assign(other.0)
                    }
                }
                impl core::ops::BitAndAssign for THREAD_CREATION_FLAGS {
                    fn bitand_assign(&mut self, other: Self) {
                        self.0.bitand_assign(other.0)
                    }
                }
                impl core::ops::Not for THREAD_CREATION_FLAGS {
                    type Output = Self;
                    fn not(self) -> Self {
                        Self(self.0.not())
                    }
                }
            }
            pub mod WindowsProgramming {
                #[inline]
                pub unsafe fn GetComputerNameW(
                    lpbuffer: Option<windows_core::PWSTR>,
                    nsize: *mut u32,
                ) -> windows_core::Result<()> {
                    windows_link::link!("kernel32.dll" "system" fn GetComputerNameW(lpbuffer : windows_core::PWSTR, nsize : *mut u32) -> windows_core::BOOL);
                    unsafe {
                        GetComputerNameW(lpbuffer.unwrap_or(core::mem::zeroed()) as _, nsize as _)
                            .ok()
                    }
                }
                pub const MAX_COMPUTERNAME_LENGTH: u32 = 15u32;
            }
        }
        pub mod UI {
            pub mod Shell {
                #[inline]
                pub unsafe fn SHStrDupW<P0>(psz: P0) -> windows_core::Result<windows_core::PWSTR>
                where
                    P0: windows_core::Param<windows_core::PCWSTR>,
                {
                    windows_link::link!("shlwapi.dll" "system" fn SHStrDupW(psz : windows_core::PCWSTR, ppwsz : *mut windows_core::PWSTR) -> windows_core::HRESULT);
                    unsafe {
                        let mut result__ = core::mem::zeroed();
                        SHStrDupW(psz.param().abi(), &mut result__).map(|| result__)
                    }
                }
                pub const CPFIS_NONE: CREDENTIAL_PROVIDER_FIELD_INTERACTIVE_STATE =
                    CREDENTIAL_PROVIDER_FIELD_INTERACTIVE_STATE(0i32);
                pub const CPFS_DISPLAY_IN_BOTH: CREDENTIAL_PROVIDER_FIELD_STATE =
                    CREDENTIAL_PROVIDER_FIELD_STATE(3i32);
                pub const CPFS_DISPLAY_IN_SELECTED_TILE: CREDENTIAL_PROVIDER_FIELD_STATE =
                    CREDENTIAL_PROVIDER_FIELD_STATE(1i32);
                pub const CPFT_LARGE_TEXT: CREDENTIAL_PROVIDER_FIELD_TYPE =
                    CREDENTIAL_PROVIDER_FIELD_TYPE(1i32);
                pub const CPFT_SMALL_TEXT: CREDENTIAL_PROVIDER_FIELD_TYPE =
                    CREDENTIAL_PROVIDER_FIELD_TYPE(2i32);
                pub const CPGSR_NO_CREDENTIAL_NOT_FINISHED:
                    CREDENTIAL_PROVIDER_GET_SERIALIZATION_RESPONSE =
                    CREDENTIAL_PROVIDER_GET_SERIALIZATION_RESPONSE(0i32);
                pub const CPGSR_RETURN_CREDENTIAL_FINISHED:
                    CREDENTIAL_PROVIDER_GET_SERIALIZATION_RESPONSE =
                    CREDENTIAL_PROVIDER_GET_SERIALIZATION_RESPONSE(2i32);
                pub const CPSI_NONE: CREDENTIAL_PROVIDER_STATUS_ICON =
                    CREDENTIAL_PROVIDER_STATUS_ICON(0i32);
                pub const CPUS_CREDUI: CREDENTIAL_PROVIDER_USAGE_SCENARIO =
                    CREDENTIAL_PROVIDER_USAGE_SCENARIO(4i32);
                pub const CPUS_LOGON: CREDENTIAL_PROVIDER_USAGE_SCENARIO =
                    CREDENTIAL_PROVIDER_USAGE_SCENARIO(1i32);
                pub const CPUS_UNLOCK_WORKSTATION: CREDENTIAL_PROVIDER_USAGE_SCENARIO =
                    CREDENTIAL_PROVIDER_USAGE_SCENARIO(2i32);
                #[repr(C)]
                #[derive(Clone, Copy, Debug, PartialEq)]
                pub struct CREDENTIAL_PROVIDER_CREDENTIAL_SERIALIZATION {
                    pub ulAuthenticationPackage: u32,
                    pub clsidCredentialProvider: windows_core::GUID,
                    pub cbSerialization: u32,
                    pub rgbSerialization: *mut u8,
                }
                impl Default for CREDENTIAL_PROVIDER_CREDENTIAL_SERIALIZATION {
                    fn default() -> Self {
                        unsafe { core::mem::zeroed() }
                    }
                }
                #[repr(C)]
                #[derive(Clone, Copy, Debug, Default, PartialEq)]
                pub struct CREDENTIAL_PROVIDER_FIELD_DESCRIPTOR {
                    pub dwFieldID: u32,
                    pub cpft: CREDENTIAL_PROVIDER_FIELD_TYPE,
                    pub pszLabel: windows_core::PWSTR,
                    pub guidFieldType: windows_core::GUID,
                }
                #[repr(transparent)]
                #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
                pub struct CREDENTIAL_PROVIDER_FIELD_INTERACTIVE_STATE(pub i32);
                #[repr(transparent)]
                #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
                pub struct CREDENTIAL_PROVIDER_FIELD_STATE(pub i32);
                #[repr(transparent)]
                #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
                pub struct CREDENTIAL_PROVIDER_FIELD_TYPE(pub i32);
                #[repr(transparent)]
                #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
                pub struct CREDENTIAL_PROVIDER_GET_SERIALIZATION_RESPONSE(pub i32);
                pub const CREDENTIAL_PROVIDER_NO_DEFAULT: u32 = 4294967295u32;
                #[repr(transparent)]
                #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
                pub struct CREDENTIAL_PROVIDER_STATUS_ICON(pub i32);
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
                    pub unsafe fn SetSerialization(
                        &self,
                        pcpcs: *const CREDENTIAL_PROVIDER_CREDENTIAL_SERIALIZATION,
                    ) -> windows_core::Result<()> {
                        unsafe {
                            (windows_core::Interface::vtable(self).SetSerialization)(
                                windows_core::Interface::as_raw(self),
                                pcpcs,
                            )
                            .ok()
                        }
                    }
                    pub unsafe fn Advise<P0>(
                        &self,
                        pcpe: P0,
                        upadvisecontext: usize,
                    ) -> windows_core::Result<()>
                    where
                        P0: windows_core::Param<ICredentialProviderEvents>,
                    {
                        unsafe {
                            (windows_core::Interface::vtable(self).Advise)(
                                windows_core::Interface::as_raw(self),
                                pcpe.param().abi(),
                                upadvisecontext,
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
                    pub unsafe fn GetFieldDescriptorAt(
                        &self,
                        dwindex: u32,
                    ) -> windows_core::Result<*mut CREDENTIAL_PROVIDER_FIELD_DESCRIPTOR>
                    {
                        unsafe {
                            let mut result__ = core::mem::zeroed();
                            (windows_core::Interface::vtable(self).GetFieldDescriptorAt)(
                                windows_core::Interface::as_raw(self),
                                dwindex,
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
                    pub unsafe fn GetCredentialAt(
                        &self,
                        dwindex: u32,
                    ) -> windows_core::Result<ICredentialProviderCredential> {
                        unsafe {
                            let mut result__ = core::mem::zeroed();
                            (windows_core::Interface::vtable(self).GetCredentialAt)(
                                windows_core::Interface::as_raw(self),
                                dwindex,
                                &mut result__,
                            )
                            .and_then(|| windows_core::Type::from_abi(result__))
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
                    pub SetSerialization: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        *const CREDENTIAL_PROVIDER_CREDENTIAL_SERIALIZATION,
                    )
                        -> windows_core::HRESULT,
                    pub Advise: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        *mut core::ffi::c_void,
                        usize,
                    )
                        -> windows_core::HRESULT,
                    pub UnAdvise:
                        unsafe extern "system" fn(*mut core::ffi::c_void) -> windows_core::HRESULT,
                    pub GetFieldDescriptorCount: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        *mut u32,
                    )
                        -> windows_core::HRESULT,
                    pub GetFieldDescriptorAt: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        u32,
                        *mut *mut CREDENTIAL_PROVIDER_FIELD_DESCRIPTOR,
                    )
                        -> windows_core::HRESULT,
                    pub GetCredentialCount: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        *mut u32,
                        *mut u32,
                        *mut windows_core::BOOL,
                    )
                        -> windows_core::HRESULT,
                    pub GetCredentialAt: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        u32,
                        *mut *mut core::ffi::c_void,
                    )
                        -> windows_core::HRESULT,
                }
                pub trait ICredentialProvider_Impl: windows_core::IUnknownImpl {
                    fn SetUsageScenario(
                        &self,
                        cpus: CREDENTIAL_PROVIDER_USAGE_SCENARIO,
                        dwflags: u32,
                    ) -> windows_core::Result<()>;
                    fn SetSerialization(
                        &self,
                        pcpcs: *const CREDENTIAL_PROVIDER_CREDENTIAL_SERIALIZATION,
                    ) -> windows_core::Result<()>;
                    fn Advise(
                        &self,
                        pcpe: windows_core::Ref<ICredentialProviderEvents>,
                        upadvisecontext: usize,
                    ) -> windows_core::Result<()>;
                    fn UnAdvise(&self) -> windows_core::Result<()>;
                    fn GetFieldDescriptorCount(&self) -> windows_core::Result<u32>;
                    fn GetFieldDescriptorAt(
                        &self,
                        dwindex: u32,
                    ) -> windows_core::Result<*mut CREDENTIAL_PROVIDER_FIELD_DESCRIPTOR>;
                    fn GetCredentialCount(
                        &self,
                        pdwcount: *mut u32,
                        pdwdefault: *mut u32,
                        pbautologonwithdefault: *mut windows_core::BOOL,
                    ) -> windows_core::Result<()>;
                    fn GetCredentialAt(
                        &self,
                        dwindex: u32,
                    ) -> windows_core::Result<ICredentialProviderCredential>;
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
                        unsafe extern "system" fn SetSerialization<
                            Identity: ICredentialProvider_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            pcpcs: *const CREDENTIAL_PROVIDER_CREDENTIAL_SERIALIZATION,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProvider_Impl::SetSerialization(
                                    this,
                                    core::mem::transmute_copy(&pcpcs),
                                )
                                .into()
                            }
                        }
                        unsafe extern "system" fn Advise<
                            Identity: ICredentialProvider_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            pcpe: *mut core::ffi::c_void,
                            upadvisecontext: usize,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProvider_Impl::Advise(
                                    this,
                                    core::mem::transmute_copy(&pcpe),
                                    core::mem::transmute_copy(&upadvisecontext),
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
                        unsafe extern "system" fn GetFieldDescriptorAt<
                            Identity: ICredentialProvider_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            dwindex: u32,
                            ppcpfd: *mut *mut CREDENTIAL_PROVIDER_FIELD_DESCRIPTOR,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                match ICredentialProvider_Impl::GetFieldDescriptorAt(
                                    this,
                                    core::mem::transmute_copy(&dwindex),
                                ) {
                                    Ok(ok__) => {
                                        ppcpfd.write(core::mem::transmute(ok__));
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
                        unsafe extern "system" fn GetCredentialAt<
                            Identity: ICredentialProvider_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            dwindex: u32,
                            ppcpc: *mut *mut core::ffi::c_void,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                match ICredentialProvider_Impl::GetCredentialAt(
                                    this,
                                    core::mem::transmute_copy(&dwindex),
                                ) {
                                    Ok(ok__) => {
                                        ppcpc.write(core::mem::transmute(ok__));
                                        windows_core::HRESULT(0)
                                    }
                                    Err(err) => err.into(),
                                }
                            }
                        }
                        Self {
                            base__: windows_core::IUnknown_Vtbl::new::<Identity, OFFSET>(),
                            SetUsageScenario: SetUsageScenario::<Identity, OFFSET>,
                            SetSerialization: SetSerialization::<Identity, OFFSET>,
                            Advise: Advise::<Identity, OFFSET>,
                            UnAdvise: UnAdvise::<Identity, OFFSET>,
                            GetFieldDescriptorCount: GetFieldDescriptorCount::<Identity, OFFSET>,
                            GetFieldDescriptorAt: GetFieldDescriptorAt::<Identity, OFFSET>,
                            GetCredentialCount: GetCredentialCount::<Identity, OFFSET>,
                            GetCredentialAt: GetCredentialAt::<Identity, OFFSET>,
                        }
                    }
                    pub fn matches(iid: &windows_core::GUID) -> bool {
                        iid == &<ICredentialProvider as windows_core::Interface>::IID
                    }
                }
                impl windows_core::RuntimeName for ICredentialProvider {}
                windows_core::imp::define_interface!(
                    ICredentialProviderCredential,
                    ICredentialProviderCredential_Vtbl,
                    0x63913a93_40c1_481a_818d_4072ff8c70cc
                );
                windows_core::imp::interface_hierarchy!(
                    ICredentialProviderCredential,
                    windows_core::IUnknown
                );
                impl ICredentialProviderCredential {
                    pub unsafe fn Advise<P0>(&self, pcpce: P0) -> windows_core::Result<()>
                    where
                        P0: windows_core::Param<ICredentialProviderCredentialEvents>,
                    {
                        unsafe {
                            (windows_core::Interface::vtable(self).Advise)(
                                windows_core::Interface::as_raw(self),
                                pcpce.param().abi(),
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
                    pub unsafe fn SetSelected(&self) -> windows_core::Result<windows_core::BOOL> {
                        unsafe {
                            let mut result__ = core::mem::zeroed();
                            (windows_core::Interface::vtable(self).SetSelected)(
                                windows_core::Interface::as_raw(self),
                                &mut result__,
                            )
                            .map(|| result__)
                        }
                    }
                    pub unsafe fn SetDeselected(&self) -> windows_core::Result<()> {
                        unsafe {
                            (windows_core::Interface::vtable(self).SetDeselected)(
                                windows_core::Interface::as_raw(self),
                            )
                            .ok()
                        }
                    }
                    pub unsafe fn GetFieldState(
                        &self,
                        dwfieldid: u32,
                        pcpfs: *mut CREDENTIAL_PROVIDER_FIELD_STATE,
                        pcpfis: *mut CREDENTIAL_PROVIDER_FIELD_INTERACTIVE_STATE,
                    ) -> windows_core::Result<()> {
                        unsafe {
                            (windows_core::Interface::vtable(self).GetFieldState)(
                                windows_core::Interface::as_raw(self),
                                dwfieldid,
                                pcpfs as _,
                                pcpfis as _,
                            )
                            .ok()
                        }
                    }
                    pub unsafe fn GetStringValue(
                        &self,
                        dwfieldid: u32,
                    ) -> windows_core::Result<windows_core::PWSTR> {
                        unsafe {
                            let mut result__ = core::mem::zeroed();
                            (windows_core::Interface::vtable(self).GetStringValue)(
                                windows_core::Interface::as_raw(self),
                                dwfieldid,
                                &mut result__,
                            )
                            .map(|| result__)
                        }
                    }
                    pub unsafe fn GetBitmapValue(
                        &self,
                        dwfieldid: u32,
                    ) -> windows_core::Result<super::super::Graphics::Gdi::HBITMAP>
                    {
                        unsafe {
                            let mut result__ = core::mem::zeroed();
                            (windows_core::Interface::vtable(self).GetBitmapValue)(
                                windows_core::Interface::as_raw(self),
                                dwfieldid,
                                &mut result__,
                            )
                            .map(|| result__)
                        }
                    }
                    pub unsafe fn GetCheckboxValue(
                        &self,
                        dwfieldid: u32,
                        pbchecked: *mut windows_core::BOOL,
                        ppszlabel: *mut windows_core::PWSTR,
                    ) -> windows_core::Result<()> {
                        unsafe {
                            (windows_core::Interface::vtable(self).GetCheckboxValue)(
                                windows_core::Interface::as_raw(self),
                                dwfieldid,
                                pbchecked as _,
                                ppszlabel as _,
                            )
                            .ok()
                        }
                    }
                    pub unsafe fn GetSubmitButtonValue(
                        &self,
                        dwfieldid: u32,
                    ) -> windows_core::Result<u32> {
                        unsafe {
                            let mut result__ = core::mem::zeroed();
                            (windows_core::Interface::vtable(self).GetSubmitButtonValue)(
                                windows_core::Interface::as_raw(self),
                                dwfieldid,
                                &mut result__,
                            )
                            .map(|| result__)
                        }
                    }
                    pub unsafe fn GetComboBoxValueCount(
                        &self,
                        dwfieldid: u32,
                        pcitems: *mut u32,
                        pdwselecteditem: *mut u32,
                    ) -> windows_core::Result<()> {
                        unsafe {
                            (windows_core::Interface::vtable(self).GetComboBoxValueCount)(
                                windows_core::Interface::as_raw(self),
                                dwfieldid,
                                pcitems as _,
                                pdwselecteditem as _,
                            )
                            .ok()
                        }
                    }
                    pub unsafe fn GetComboBoxValueAt(
                        &self,
                        dwfieldid: u32,
                        dwitem: u32,
                    ) -> windows_core::Result<windows_core::PWSTR> {
                        unsafe {
                            let mut result__ = core::mem::zeroed();
                            (windows_core::Interface::vtable(self).GetComboBoxValueAt)(
                                windows_core::Interface::as_raw(self),
                                dwfieldid,
                                dwitem,
                                &mut result__,
                            )
                            .map(|| result__)
                        }
                    }
                    pub unsafe fn SetStringValue<P1>(
                        &self,
                        dwfieldid: u32,
                        psz: P1,
                    ) -> windows_core::Result<()>
                    where
                        P1: windows_core::Param<windows_core::PCWSTR>,
                    {
                        unsafe {
                            (windows_core::Interface::vtable(self).SetStringValue)(
                                windows_core::Interface::as_raw(self),
                                dwfieldid,
                                psz.param().abi(),
                            )
                            .ok()
                        }
                    }
                    pub unsafe fn SetCheckboxValue(
                        &self,
                        dwfieldid: u32,
                        bchecked: bool,
                    ) -> windows_core::Result<()> {
                        unsafe {
                            (windows_core::Interface::vtable(self).SetCheckboxValue)(
                                windows_core::Interface::as_raw(self),
                                dwfieldid,
                                bchecked.into(),
                            )
                            .ok()
                        }
                    }
                    pub unsafe fn SetComboBoxSelectedValue(
                        &self,
                        dwfieldid: u32,
                        dwselecteditem: u32,
                    ) -> windows_core::Result<()> {
                        unsafe {
                            (windows_core::Interface::vtable(self).SetComboBoxSelectedValue)(
                                windows_core::Interface::as_raw(self),
                                dwfieldid,
                                dwselecteditem,
                            )
                            .ok()
                        }
                    }
                    pub unsafe fn CommandLinkClicked(
                        &self,
                        dwfieldid: u32,
                    ) -> windows_core::Result<()> {
                        unsafe {
                            (windows_core::Interface::vtable(self).CommandLinkClicked)(
                                windows_core::Interface::as_raw(self),
                                dwfieldid,
                            )
                            .ok()
                        }
                    }
                    pub unsafe fn GetSerialization(
                        &self,
                        pcpgsr: *mut CREDENTIAL_PROVIDER_GET_SERIALIZATION_RESPONSE,
                        pcpcs: *mut CREDENTIAL_PROVIDER_CREDENTIAL_SERIALIZATION,
                        ppszoptionalstatustext: *mut windows_core::PWSTR,
                        pcpsioptionalstatusicon: *mut CREDENTIAL_PROVIDER_STATUS_ICON,
                    ) -> windows_core::Result<()> {
                        unsafe {
                            (windows_core::Interface::vtable(self).GetSerialization)(
                                windows_core::Interface::as_raw(self),
                                pcpgsr as _,
                                pcpcs as _,
                                ppszoptionalstatustext as _,
                                pcpsioptionalstatusicon as _,
                            )
                            .ok()
                        }
                    }
                    pub unsafe fn ReportResult(
                        &self,
                        ntsstatus: super::super::Foundation::NTSTATUS,
                        ntssubstatus: super::super::Foundation::NTSTATUS,
                        ppszoptionalstatustext: *mut windows_core::PWSTR,
                        pcpsioptionalstatusicon: *mut CREDENTIAL_PROVIDER_STATUS_ICON,
                    ) -> windows_core::Result<()> {
                        unsafe {
                            (windows_core::Interface::vtable(self).ReportResult)(
                                windows_core::Interface::as_raw(self),
                                ntsstatus,
                                ntssubstatus,
                                ppszoptionalstatustext as _,
                                pcpsioptionalstatusicon as _,
                            )
                            .ok()
                        }
                    }
                }
                #[repr(C)]
                #[doc(hidden)]
                pub struct ICredentialProviderCredential_Vtbl {
                    pub base__: windows_core::IUnknown_Vtbl,
                    pub Advise: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        *mut core::ffi::c_void,
                    )
                        -> windows_core::HRESULT,
                    pub UnAdvise:
                        unsafe extern "system" fn(*mut core::ffi::c_void) -> windows_core::HRESULT,
                    pub SetSelected: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        *mut windows_core::BOOL,
                    )
                        -> windows_core::HRESULT,
                    pub SetDeselected:
                        unsafe extern "system" fn(*mut core::ffi::c_void) -> windows_core::HRESULT,
                    pub GetFieldState: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        u32,
                        *mut CREDENTIAL_PROVIDER_FIELD_STATE,
                        *mut CREDENTIAL_PROVIDER_FIELD_INTERACTIVE_STATE,
                    )
                        -> windows_core::HRESULT,
                    pub GetStringValue: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        u32,
                        *mut windows_core::PWSTR,
                    )
                        -> windows_core::HRESULT,
                    pub GetBitmapValue: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        u32,
                        *mut super::super::Graphics::Gdi::HBITMAP,
                    )
                        -> windows_core::HRESULT,
                    pub GetCheckboxValue: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        u32,
                        *mut windows_core::BOOL,
                        *mut windows_core::PWSTR,
                    )
                        -> windows_core::HRESULT,
                    pub GetSubmitButtonValue: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        u32,
                        *mut u32,
                    )
                        -> windows_core::HRESULT,
                    pub GetComboBoxValueCount: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        u32,
                        *mut u32,
                        *mut u32,
                    )
                        -> windows_core::HRESULT,
                    pub GetComboBoxValueAt: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        u32,
                        u32,
                        *mut windows_core::PWSTR,
                    )
                        -> windows_core::HRESULT,
                    pub SetStringValue: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        u32,
                        windows_core::PCWSTR,
                    )
                        -> windows_core::HRESULT,
                    pub SetCheckboxValue: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        u32,
                        windows_core::BOOL,
                    )
                        -> windows_core::HRESULT,
                    pub SetComboBoxSelectedValue:
                        unsafe extern "system" fn(
                            *mut core::ffi::c_void,
                            u32,
                            u32,
                        ) -> windows_core::HRESULT,
                    pub CommandLinkClicked: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        u32,
                    )
                        -> windows_core::HRESULT,
                    pub GetSerialization: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        *mut CREDENTIAL_PROVIDER_GET_SERIALIZATION_RESPONSE,
                        *mut CREDENTIAL_PROVIDER_CREDENTIAL_SERIALIZATION,
                        *mut windows_core::PWSTR,
                        *mut CREDENTIAL_PROVIDER_STATUS_ICON,
                    )
                        -> windows_core::HRESULT,
                    pub ReportResult: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        super::super::Foundation::NTSTATUS,
                        super::super::Foundation::NTSTATUS,
                        *mut windows_core::PWSTR,
                        *mut CREDENTIAL_PROVIDER_STATUS_ICON,
                    )
                        -> windows_core::HRESULT,
                }
                pub trait ICredentialProviderCredential_Impl: windows_core::IUnknownImpl {
                    fn Advise(
                        &self,
                        pcpce: windows_core::Ref<ICredentialProviderCredentialEvents>,
                    ) -> windows_core::Result<()>;
                    fn UnAdvise(&self) -> windows_core::Result<()>;
                    fn SetSelected(&self) -> windows_core::Result<windows_core::BOOL>;
                    fn SetDeselected(&self) -> windows_core::Result<()>;
                    fn GetFieldState(
                        &self,
                        dwfieldid: u32,
                        pcpfs: *mut CREDENTIAL_PROVIDER_FIELD_STATE,
                        pcpfis: *mut CREDENTIAL_PROVIDER_FIELD_INTERACTIVE_STATE,
                    ) -> windows_core::Result<()>;
                    fn GetStringValue(
                        &self,
                        dwfieldid: u32,
                    ) -> windows_core::Result<windows_core::PWSTR>;
                    fn GetBitmapValue(
                        &self,
                        dwfieldid: u32,
                    ) -> windows_core::Result<super::super::Graphics::Gdi::HBITMAP>;
                    fn GetCheckboxValue(
                        &self,
                        dwfieldid: u32,
                        pbchecked: *mut windows_core::BOOL,
                        ppszlabel: *mut windows_core::PWSTR,
                    ) -> windows_core::Result<()>;
                    fn GetSubmitButtonValue(&self, dwfieldid: u32) -> windows_core::Result<u32>;
                    fn GetComboBoxValueCount(
                        &self,
                        dwfieldid: u32,
                        pcitems: *mut u32,
                        pdwselecteditem: *mut u32,
                    ) -> windows_core::Result<()>;
                    fn GetComboBoxValueAt(
                        &self,
                        dwfieldid: u32,
                        dwitem: u32,
                    ) -> windows_core::Result<windows_core::PWSTR>;
                    fn SetStringValue(
                        &self,
                        dwfieldid: u32,
                        psz: &windows_core::PCWSTR,
                    ) -> windows_core::Result<()>;
                    fn SetCheckboxValue(
                        &self,
                        dwfieldid: u32,
                        bchecked: windows_core::BOOL,
                    ) -> windows_core::Result<()>;
                    fn SetComboBoxSelectedValue(
                        &self,
                        dwfieldid: u32,
                        dwselecteditem: u32,
                    ) -> windows_core::Result<()>;
                    fn CommandLinkClicked(&self, dwfieldid: u32) -> windows_core::Result<()>;
                    fn GetSerialization(
                        &self,
                        pcpgsr: *mut CREDENTIAL_PROVIDER_GET_SERIALIZATION_RESPONSE,
                        pcpcs: *mut CREDENTIAL_PROVIDER_CREDENTIAL_SERIALIZATION,
                        ppszoptionalstatustext: *mut windows_core::PWSTR,
                        pcpsioptionalstatusicon: *mut CREDENTIAL_PROVIDER_STATUS_ICON,
                    ) -> windows_core::Result<()>;
                    fn ReportResult(
                        &self,
                        ntsstatus: super::super::Foundation::NTSTATUS,
                        ntssubstatus: super::super::Foundation::NTSTATUS,
                        ppszoptionalstatustext: *mut windows_core::PWSTR,
                        pcpsioptionalstatusicon: *mut CREDENTIAL_PROVIDER_STATUS_ICON,
                    ) -> windows_core::Result<()>;
                }
                impl ICredentialProviderCredential_Vtbl {
                    pub const fn new<
                        Identity: ICredentialProviderCredential_Impl,
                        const OFFSET: isize,
                    >() -> Self {
                        unsafe extern "system" fn Advise<
                            Identity: ICredentialProviderCredential_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            pcpce: *mut core::ffi::c_void,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProviderCredential_Impl::Advise(
                                    this,
                                    core::mem::transmute_copy(&pcpce),
                                )
                                .into()
                            }
                        }
                        unsafe extern "system" fn UnAdvise<
                            Identity: ICredentialProviderCredential_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProviderCredential_Impl::UnAdvise(this).into()
                            }
                        }
                        unsafe extern "system" fn SetSelected<
                            Identity: ICredentialProviderCredential_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            pbautologon: *mut windows_core::BOOL,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                match ICredentialProviderCredential_Impl::SetSelected(this) {
                                    Ok(ok__) => {
                                        pbautologon.write(core::mem::transmute(ok__));
                                        windows_core::HRESULT(0)
                                    }
                                    Err(err) => err.into(),
                                }
                            }
                        }
                        unsafe extern "system" fn SetDeselected<
                            Identity: ICredentialProviderCredential_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProviderCredential_Impl::SetDeselected(this).into()
                            }
                        }
                        unsafe extern "system" fn GetFieldState<
                            Identity: ICredentialProviderCredential_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            dwfieldid: u32,
                            pcpfs: *mut CREDENTIAL_PROVIDER_FIELD_STATE,
                            pcpfis: *mut CREDENTIAL_PROVIDER_FIELD_INTERACTIVE_STATE,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProviderCredential_Impl::GetFieldState(
                                    this,
                                    core::mem::transmute_copy(&dwfieldid),
                                    core::mem::transmute_copy(&pcpfs),
                                    core::mem::transmute_copy(&pcpfis),
                                )
                                .into()
                            }
                        }
                        unsafe extern "system" fn GetStringValue<
                            Identity: ICredentialProviderCredential_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            dwfieldid: u32,
                            ppsz: *mut windows_core::PWSTR,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                match ICredentialProviderCredential_Impl::GetStringValue(
                                    this,
                                    core::mem::transmute_copy(&dwfieldid),
                                ) {
                                    Ok(ok__) => {
                                        ppsz.write(core::mem::transmute(ok__));
                                        windows_core::HRESULT(0)
                                    }
                                    Err(err) => err.into(),
                                }
                            }
                        }
                        unsafe extern "system" fn GetBitmapValue<
                            Identity: ICredentialProviderCredential_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            dwfieldid: u32,
                            phbmp: *mut super::super::Graphics::Gdi::HBITMAP,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                match ICredentialProviderCredential_Impl::GetBitmapValue(
                                    this,
                                    core::mem::transmute_copy(&dwfieldid),
                                ) {
                                    Ok(ok__) => {
                                        phbmp.write(core::mem::transmute(ok__));
                                        windows_core::HRESULT(0)
                                    }
                                    Err(err) => err.into(),
                                }
                            }
                        }
                        unsafe extern "system" fn GetCheckboxValue<
                            Identity: ICredentialProviderCredential_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            dwfieldid: u32,
                            pbchecked: *mut windows_core::BOOL,
                            ppszlabel: *mut windows_core::PWSTR,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProviderCredential_Impl::GetCheckboxValue(
                                    this,
                                    core::mem::transmute_copy(&dwfieldid),
                                    core::mem::transmute_copy(&pbchecked),
                                    core::mem::transmute_copy(&ppszlabel),
                                )
                                .into()
                            }
                        }
                        unsafe extern "system" fn GetSubmitButtonValue<
                            Identity: ICredentialProviderCredential_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            dwfieldid: u32,
                            pdwadjacentto: *mut u32,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                match ICredentialProviderCredential_Impl::GetSubmitButtonValue(
                                    this,
                                    core::mem::transmute_copy(&dwfieldid),
                                ) {
                                    Ok(ok__) => {
                                        pdwadjacentto.write(core::mem::transmute(ok__));
                                        windows_core::HRESULT(0)
                                    }
                                    Err(err) => err.into(),
                                }
                            }
                        }
                        unsafe extern "system" fn GetComboBoxValueCount<
                            Identity: ICredentialProviderCredential_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            dwfieldid: u32,
                            pcitems: *mut u32,
                            pdwselecteditem: *mut u32,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProviderCredential_Impl::GetComboBoxValueCount(
                                    this,
                                    core::mem::transmute_copy(&dwfieldid),
                                    core::mem::transmute_copy(&pcitems),
                                    core::mem::transmute_copy(&pdwselecteditem),
                                )
                                .into()
                            }
                        }
                        unsafe extern "system" fn GetComboBoxValueAt<
                            Identity: ICredentialProviderCredential_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            dwfieldid: u32,
                            dwitem: u32,
                            ppszitem: *mut windows_core::PWSTR,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                match ICredentialProviderCredential_Impl::GetComboBoxValueAt(
                                    this,
                                    core::mem::transmute_copy(&dwfieldid),
                                    core::mem::transmute_copy(&dwitem),
                                ) {
                                    Ok(ok__) => {
                                        ppszitem.write(core::mem::transmute(ok__));
                                        windows_core::HRESULT(0)
                                    }
                                    Err(err) => err.into(),
                                }
                            }
                        }
                        unsafe extern "system" fn SetStringValue<
                            Identity: ICredentialProviderCredential_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            dwfieldid: u32,
                            psz: windows_core::PCWSTR,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProviderCredential_Impl::SetStringValue(
                                    this,
                                    core::mem::transmute_copy(&dwfieldid),
                                    core::mem::transmute(&psz),
                                )
                                .into()
                            }
                        }
                        unsafe extern "system" fn SetCheckboxValue<
                            Identity: ICredentialProviderCredential_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            dwfieldid: u32,
                            bchecked: windows_core::BOOL,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProviderCredential_Impl::SetCheckboxValue(
                                    this,
                                    core::mem::transmute_copy(&dwfieldid),
                                    core::mem::transmute_copy(&bchecked),
                                )
                                .into()
                            }
                        }
                        unsafe extern "system" fn SetComboBoxSelectedValue<
                            Identity: ICredentialProviderCredential_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            dwfieldid: u32,
                            dwselecteditem: u32,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProviderCredential_Impl::SetComboBoxSelectedValue(
                                    this,
                                    core::mem::transmute_copy(&dwfieldid),
                                    core::mem::transmute_copy(&dwselecteditem),
                                )
                                .into()
                            }
                        }
                        unsafe extern "system" fn CommandLinkClicked<
                            Identity: ICredentialProviderCredential_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            dwfieldid: u32,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProviderCredential_Impl::CommandLinkClicked(
                                    this,
                                    core::mem::transmute_copy(&dwfieldid),
                                )
                                .into()
                            }
                        }
                        unsafe extern "system" fn GetSerialization<
                            Identity: ICredentialProviderCredential_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            pcpgsr: *mut CREDENTIAL_PROVIDER_GET_SERIALIZATION_RESPONSE,
                            pcpcs: *mut CREDENTIAL_PROVIDER_CREDENTIAL_SERIALIZATION,
                            ppszoptionalstatustext: *mut windows_core::PWSTR,
                            pcpsioptionalstatusicon: *mut CREDENTIAL_PROVIDER_STATUS_ICON,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProviderCredential_Impl::GetSerialization(
                                    this,
                                    core::mem::transmute_copy(&pcpgsr),
                                    core::mem::transmute_copy(&pcpcs),
                                    core::mem::transmute_copy(&ppszoptionalstatustext),
                                    core::mem::transmute_copy(&pcpsioptionalstatusicon),
                                )
                                .into()
                            }
                        }
                        unsafe extern "system" fn ReportResult<
                            Identity: ICredentialProviderCredential_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            ntsstatus: super::super::Foundation::NTSTATUS,
                            ntssubstatus: super::super::Foundation::NTSTATUS,
                            ppszoptionalstatustext: *mut windows_core::PWSTR,
                            pcpsioptionalstatusicon: *mut CREDENTIAL_PROVIDER_STATUS_ICON,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProviderCredential_Impl::ReportResult(
                                    this,
                                    core::mem::transmute_copy(&ntsstatus),
                                    core::mem::transmute_copy(&ntssubstatus),
                                    core::mem::transmute_copy(&ppszoptionalstatustext),
                                    core::mem::transmute_copy(&pcpsioptionalstatusicon),
                                )
                                .into()
                            }
                        }
                        Self {
                            base__: windows_core::IUnknown_Vtbl::new::<Identity, OFFSET>(),
                            Advise: Advise::<Identity, OFFSET>,
                            UnAdvise: UnAdvise::<Identity, OFFSET>,
                            SetSelected: SetSelected::<Identity, OFFSET>,
                            SetDeselected: SetDeselected::<Identity, OFFSET>,
                            GetFieldState: GetFieldState::<Identity, OFFSET>,
                            GetStringValue: GetStringValue::<Identity, OFFSET>,
                            GetBitmapValue: GetBitmapValue::<Identity, OFFSET>,
                            GetCheckboxValue: GetCheckboxValue::<Identity, OFFSET>,
                            GetSubmitButtonValue: GetSubmitButtonValue::<Identity, OFFSET>,
                            GetComboBoxValueCount: GetComboBoxValueCount::<Identity, OFFSET>,
                            GetComboBoxValueAt: GetComboBoxValueAt::<Identity, OFFSET>,
                            SetStringValue: SetStringValue::<Identity, OFFSET>,
                            SetCheckboxValue: SetCheckboxValue::<Identity, OFFSET>,
                            SetComboBoxSelectedValue: SetComboBoxSelectedValue::<Identity, OFFSET>,
                            CommandLinkClicked: CommandLinkClicked::<Identity, OFFSET>,
                            GetSerialization: GetSerialization::<Identity, OFFSET>,
                            ReportResult: ReportResult::<Identity, OFFSET>,
                        }
                    }
                    pub fn matches(iid: &windows_core::GUID) -> bool {
                        iid == &<ICredentialProviderCredential as windows_core::Interface>::IID
                    }
                }
                impl windows_core::RuntimeName for ICredentialProviderCredential {}
                windows_core::imp::define_interface!(
                    ICredentialProviderCredential2,
                    ICredentialProviderCredential2_Vtbl,
                    0xfd672c54_40ea_4d6e_9b49_cfb1a7507bd7
                );
                impl core::ops::Deref for ICredentialProviderCredential2 {
                    type Target = ICredentialProviderCredential;
                    fn deref(&self) -> &Self::Target {
                        unsafe { core::mem::transmute(self) }
                    }
                }
                windows_core::imp::interface_hierarchy!(
                    ICredentialProviderCredential2,
                    windows_core::IUnknown,
                    ICredentialProviderCredential
                );
                impl ICredentialProviderCredential2 {
                    pub unsafe fn GetUserSid(&self) -> windows_core::Result<windows_core::PWSTR> {
                        unsafe {
                            let mut result__ = core::mem::zeroed();
                            (windows_core::Interface::vtable(self).GetUserSid)(
                                windows_core::Interface::as_raw(self),
                                &mut result__,
                            )
                            .map(|| result__)
                        }
                    }
                }
                #[repr(C)]
                #[doc(hidden)]
                pub struct ICredentialProviderCredential2_Vtbl {
                    pub base__: ICredentialProviderCredential_Vtbl,
                    pub GetUserSid: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        *mut windows_core::PWSTR,
                    )
                        -> windows_core::HRESULT,
                }
                pub trait ICredentialProviderCredential2_Impl:
                    ICredentialProviderCredential_Impl
                {
                    fn GetUserSid(&self) -> windows_core::Result<windows_core::PWSTR>;
                }
                impl ICredentialProviderCredential2_Vtbl {
                    pub const fn new<
                        Identity: ICredentialProviderCredential2_Impl,
                        const OFFSET: isize,
                    >() -> Self {
                        unsafe extern "system" fn GetUserSid<
                            Identity: ICredentialProviderCredential2_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            sid: *mut windows_core::PWSTR,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                match ICredentialProviderCredential2_Impl::GetUserSid(this) {
                                    Ok(ok__) => {
                                        sid.write(core::mem::transmute(ok__));
                                        windows_core::HRESULT(0)
                                    }
                                    Err(err) => err.into(),
                                }
                            }
                        }
                        Self {
                            base__: ICredentialProviderCredential_Vtbl::new::<Identity, OFFSET>(),
                            GetUserSid: GetUserSid::<Identity, OFFSET>,
                        }
                    }
                    pub fn matches(iid: &windows_core::GUID) -> bool {
                        iid == &<ICredentialProviderCredential2 as windows_core::Interface>::IID
                            || iid
                                == &<ICredentialProviderCredential as windows_core::Interface>::IID
                    }
                }
                impl windows_core::RuntimeName for ICredentialProviderCredential2 {}
                windows_core::imp::define_interface!(
                    ICredentialProviderCredentialEvents,
                    ICredentialProviderCredentialEvents_Vtbl,
                    0xfa6fa76b_66b7_4b11_95f1_86171118e816
                );
                windows_core::imp::interface_hierarchy!(
                    ICredentialProviderCredentialEvents,
                    windows_core::IUnknown
                );
                impl ICredentialProviderCredentialEvents {
                    pub unsafe fn SetFieldState<P0>(
                        &self,
                        pcpc: P0,
                        dwfieldid: u32,
                        cpfs: CREDENTIAL_PROVIDER_FIELD_STATE,
                    ) -> windows_core::Result<()>
                    where
                        P0: windows_core::Param<ICredentialProviderCredential>,
                    {
                        unsafe {
                            (windows_core::Interface::vtable(self).SetFieldState)(
                                windows_core::Interface::as_raw(self),
                                pcpc.param().abi(),
                                dwfieldid,
                                cpfs,
                            )
                            .ok()
                        }
                    }
                    pub unsafe fn SetFieldInteractiveState<P0>(
                        &self,
                        pcpc: P0,
                        dwfieldid: u32,
                        cpfis: CREDENTIAL_PROVIDER_FIELD_INTERACTIVE_STATE,
                    ) -> windows_core::Result<()>
                    where
                        P0: windows_core::Param<ICredentialProviderCredential>,
                    {
                        unsafe {
                            (windows_core::Interface::vtable(self).SetFieldInteractiveState)(
                                windows_core::Interface::as_raw(self),
                                pcpc.param().abi(),
                                dwfieldid,
                                cpfis,
                            )
                            .ok()
                        }
                    }
                    pub unsafe fn SetFieldString<P0, P2>(
                        &self,
                        pcpc: P0,
                        dwfieldid: u32,
                        psz: P2,
                    ) -> windows_core::Result<()>
                    where
                        P0: windows_core::Param<ICredentialProviderCredential>,
                        P2: windows_core::Param<windows_core::PCWSTR>,
                    {
                        unsafe {
                            (windows_core::Interface::vtable(self).SetFieldString)(
                                windows_core::Interface::as_raw(self),
                                pcpc.param().abi(),
                                dwfieldid,
                                psz.param().abi(),
                            )
                            .ok()
                        }
                    }
                    pub unsafe fn SetFieldCheckbox<P0, P3>(
                        &self,
                        pcpc: P0,
                        dwfieldid: u32,
                        bchecked: bool,
                        pszlabel: P3,
                    ) -> windows_core::Result<()>
                    where
                        P0: windows_core::Param<ICredentialProviderCredential>,
                        P3: windows_core::Param<windows_core::PCWSTR>,
                    {
                        unsafe {
                            (windows_core::Interface::vtable(self).SetFieldCheckbox)(
                                windows_core::Interface::as_raw(self),
                                pcpc.param().abi(),
                                dwfieldid,
                                bchecked.into(),
                                pszlabel.param().abi(),
                            )
                            .ok()
                        }
                    }
                    pub unsafe fn SetFieldBitmap<P0>(
                        &self,
                        pcpc: P0,
                        dwfieldid: u32,
                        hbmp: super::super::Graphics::Gdi::HBITMAP,
                    ) -> windows_core::Result<()>
                    where
                        P0: windows_core::Param<ICredentialProviderCredential>,
                    {
                        unsafe {
                            (windows_core::Interface::vtable(self).SetFieldBitmap)(
                                windows_core::Interface::as_raw(self),
                                pcpc.param().abi(),
                                dwfieldid,
                                hbmp,
                            )
                            .ok()
                        }
                    }
                    pub unsafe fn SetFieldComboBoxSelectedItem<P0>(
                        &self,
                        pcpc: P0,
                        dwfieldid: u32,
                        dwselecteditem: u32,
                    ) -> windows_core::Result<()>
                    where
                        P0: windows_core::Param<ICredentialProviderCredential>,
                    {
                        unsafe {
                            (windows_core::Interface::vtable(self).SetFieldComboBoxSelectedItem)(
                                windows_core::Interface::as_raw(self),
                                pcpc.param().abi(),
                                dwfieldid,
                                dwselecteditem,
                            )
                            .ok()
                        }
                    }
                    pub unsafe fn DeleteFieldComboBoxItem<P0>(
                        &self,
                        pcpc: P0,
                        dwfieldid: u32,
                        dwitem: u32,
                    ) -> windows_core::Result<()>
                    where
                        P0: windows_core::Param<ICredentialProviderCredential>,
                    {
                        unsafe {
                            (windows_core::Interface::vtable(self).DeleteFieldComboBoxItem)(
                                windows_core::Interface::as_raw(self),
                                pcpc.param().abi(),
                                dwfieldid,
                                dwitem,
                            )
                            .ok()
                        }
                    }
                    pub unsafe fn AppendFieldComboBoxItem<P0, P2>(
                        &self,
                        pcpc: P0,
                        dwfieldid: u32,
                        pszitem: P2,
                    ) -> windows_core::Result<()>
                    where
                        P0: windows_core::Param<ICredentialProviderCredential>,
                        P2: windows_core::Param<windows_core::PCWSTR>,
                    {
                        unsafe {
                            (windows_core::Interface::vtable(self).AppendFieldComboBoxItem)(
                                windows_core::Interface::as_raw(self),
                                pcpc.param().abi(),
                                dwfieldid,
                                pszitem.param().abi(),
                            )
                            .ok()
                        }
                    }
                    pub unsafe fn SetFieldSubmitButton<P0>(
                        &self,
                        pcpc: P0,
                        dwfieldid: u32,
                        dwadjacentto: u32,
                    ) -> windows_core::Result<()>
                    where
                        P0: windows_core::Param<ICredentialProviderCredential>,
                    {
                        unsafe {
                            (windows_core::Interface::vtable(self).SetFieldSubmitButton)(
                                windows_core::Interface::as_raw(self),
                                pcpc.param().abi(),
                                dwfieldid,
                                dwadjacentto,
                            )
                            .ok()
                        }
                    }
                    pub unsafe fn OnCreatingWindow(
                        &self,
                    ) -> windows_core::Result<super::super::Foundation::HWND> {
                        unsafe {
                            let mut result__ = core::mem::zeroed();
                            (windows_core::Interface::vtable(self).OnCreatingWindow)(
                                windows_core::Interface::as_raw(self),
                                &mut result__,
                            )
                            .map(|| result__)
                        }
                    }
                }
                #[repr(C)]
                #[doc(hidden)]
                pub struct ICredentialProviderCredentialEvents_Vtbl {
                    pub base__: windows_core::IUnknown_Vtbl,
                    pub SetFieldState: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        *mut core::ffi::c_void,
                        u32,
                        CREDENTIAL_PROVIDER_FIELD_STATE,
                    )
                        -> windows_core::HRESULT,
                    pub SetFieldInteractiveState:
                        unsafe extern "system" fn(
                            *mut core::ffi::c_void,
                            *mut core::ffi::c_void,
                            u32,
                            CREDENTIAL_PROVIDER_FIELD_INTERACTIVE_STATE,
                        ) -> windows_core::HRESULT,
                    pub SetFieldString: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        *mut core::ffi::c_void,
                        u32,
                        windows_core::PCWSTR,
                    )
                        -> windows_core::HRESULT,
                    pub SetFieldCheckbox: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        *mut core::ffi::c_void,
                        u32,
                        windows_core::BOOL,
                        windows_core::PCWSTR,
                    )
                        -> windows_core::HRESULT,
                    pub SetFieldBitmap: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        *mut core::ffi::c_void,
                        u32,
                        super::super::Graphics::Gdi::HBITMAP,
                    )
                        -> windows_core::HRESULT,
                    pub SetFieldComboBoxSelectedItem:
                        unsafe extern "system" fn(
                            *mut core::ffi::c_void,
                            *mut core::ffi::c_void,
                            u32,
                            u32,
                        ) -> windows_core::HRESULT,
                    pub DeleteFieldComboBoxItem: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        *mut core::ffi::c_void,
                        u32,
                        u32,
                    )
                        -> windows_core::HRESULT,
                    pub AppendFieldComboBoxItem: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        *mut core::ffi::c_void,
                        u32,
                        windows_core::PCWSTR,
                    )
                        -> windows_core::HRESULT,
                    pub SetFieldSubmitButton: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        *mut core::ffi::c_void,
                        u32,
                        u32,
                    )
                        -> windows_core::HRESULT,
                    pub OnCreatingWindow: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        *mut super::super::Foundation::HWND,
                    )
                        -> windows_core::HRESULT,
                }
                pub trait ICredentialProviderCredentialEvents_Impl:
                    windows_core::IUnknownImpl
                {
                    fn SetFieldState(
                        &self,
                        pcpc: windows_core::Ref<ICredentialProviderCredential>,
                        dwfieldid: u32,
                        cpfs: CREDENTIAL_PROVIDER_FIELD_STATE,
                    ) -> windows_core::Result<()>;
                    fn SetFieldInteractiveState(
                        &self,
                        pcpc: windows_core::Ref<ICredentialProviderCredential>,
                        dwfieldid: u32,
                        cpfis: CREDENTIAL_PROVIDER_FIELD_INTERACTIVE_STATE,
                    ) -> windows_core::Result<()>;
                    fn SetFieldString(
                        &self,
                        pcpc: windows_core::Ref<ICredentialProviderCredential>,
                        dwfieldid: u32,
                        psz: &windows_core::PCWSTR,
                    ) -> windows_core::Result<()>;
                    fn SetFieldCheckbox(
                        &self,
                        pcpc: windows_core::Ref<ICredentialProviderCredential>,
                        dwfieldid: u32,
                        bchecked: windows_core::BOOL,
                        pszlabel: &windows_core::PCWSTR,
                    ) -> windows_core::Result<()>;
                    fn SetFieldBitmap(
                        &self,
                        pcpc: windows_core::Ref<ICredentialProviderCredential>,
                        dwfieldid: u32,
                        hbmp: super::super::Graphics::Gdi::HBITMAP,
                    ) -> windows_core::Result<()>;
                    fn SetFieldComboBoxSelectedItem(
                        &self,
                        pcpc: windows_core::Ref<ICredentialProviderCredential>,
                        dwfieldid: u32,
                        dwselecteditem: u32,
                    ) -> windows_core::Result<()>;
                    fn DeleteFieldComboBoxItem(
                        &self,
                        pcpc: windows_core::Ref<ICredentialProviderCredential>,
                        dwfieldid: u32,
                        dwitem: u32,
                    ) -> windows_core::Result<()>;
                    fn AppendFieldComboBoxItem(
                        &self,
                        pcpc: windows_core::Ref<ICredentialProviderCredential>,
                        dwfieldid: u32,
                        pszitem: &windows_core::PCWSTR,
                    ) -> windows_core::Result<()>;
                    fn SetFieldSubmitButton(
                        &self,
                        pcpc: windows_core::Ref<ICredentialProviderCredential>,
                        dwfieldid: u32,
                        dwadjacentto: u32,
                    ) -> windows_core::Result<()>;
                    fn OnCreatingWindow(
                        &self,
                    ) -> windows_core::Result<super::super::Foundation::HWND>;
                }
                impl ICredentialProviderCredentialEvents_Vtbl {
                    pub const fn new<
                        Identity: ICredentialProviderCredentialEvents_Impl,
                        const OFFSET: isize,
                    >() -> Self {
                        unsafe extern "system" fn SetFieldState<
                            Identity: ICredentialProviderCredentialEvents_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            pcpc: *mut core::ffi::c_void,
                            dwfieldid: u32,
                            cpfs: CREDENTIAL_PROVIDER_FIELD_STATE,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProviderCredentialEvents_Impl::SetFieldState(
                                    this,
                                    core::mem::transmute_copy(&pcpc),
                                    core::mem::transmute_copy(&dwfieldid),
                                    core::mem::transmute_copy(&cpfs),
                                )
                                .into()
                            }
                        }
                        unsafe extern "system" fn SetFieldInteractiveState<
                            Identity: ICredentialProviderCredentialEvents_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            pcpc: *mut core::ffi::c_void,
                            dwfieldid: u32,
                            cpfis: CREDENTIAL_PROVIDER_FIELD_INTERACTIVE_STATE,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProviderCredentialEvents_Impl::SetFieldInteractiveState(
                                    this,
                                    core::mem::transmute_copy(&pcpc),
                                    core::mem::transmute_copy(&dwfieldid),
                                    core::mem::transmute_copy(&cpfis),
                                )
                                .into()
                            }
                        }
                        unsafe extern "system" fn SetFieldString<
                            Identity: ICredentialProviderCredentialEvents_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            pcpc: *mut core::ffi::c_void,
                            dwfieldid: u32,
                            psz: windows_core::PCWSTR,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProviderCredentialEvents_Impl::SetFieldString(
                                    this,
                                    core::mem::transmute_copy(&pcpc),
                                    core::mem::transmute_copy(&dwfieldid),
                                    core::mem::transmute(&psz),
                                )
                                .into()
                            }
                        }
                        unsafe extern "system" fn SetFieldCheckbox<
                            Identity: ICredentialProviderCredentialEvents_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            pcpc: *mut core::ffi::c_void,
                            dwfieldid: u32,
                            bchecked: windows_core::BOOL,
                            pszlabel: windows_core::PCWSTR,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProviderCredentialEvents_Impl::SetFieldCheckbox(
                                    this,
                                    core::mem::transmute_copy(&pcpc),
                                    core::mem::transmute_copy(&dwfieldid),
                                    core::mem::transmute_copy(&bchecked),
                                    core::mem::transmute(&pszlabel),
                                )
                                .into()
                            }
                        }
                        unsafe extern "system" fn SetFieldBitmap<
                            Identity: ICredentialProviderCredentialEvents_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            pcpc: *mut core::ffi::c_void,
                            dwfieldid: u32,
                            hbmp: super::super::Graphics::Gdi::HBITMAP,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProviderCredentialEvents_Impl::SetFieldBitmap(
                                    this,
                                    core::mem::transmute_copy(&pcpc),
                                    core::mem::transmute_copy(&dwfieldid),
                                    core::mem::transmute_copy(&hbmp),
                                )
                                .into()
                            }
                        }
                        unsafe extern "system" fn SetFieldComboBoxSelectedItem<
                            Identity: ICredentialProviderCredentialEvents_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            pcpc: *mut core::ffi::c_void,
                            dwfieldid: u32,
                            dwselecteditem: u32,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProviderCredentialEvents_Impl:: SetFieldComboBoxSelectedItem ( this , core::mem::transmute_copy ( & pcpc ) , core::mem::transmute_copy ( & dwfieldid ) , core::mem::transmute_copy ( & dwselecteditem ) , ) . into ( )
                            }
                        }
                        unsafe extern "system" fn DeleteFieldComboBoxItem<
                            Identity: ICredentialProviderCredentialEvents_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            pcpc: *mut core::ffi::c_void,
                            dwfieldid: u32,
                            dwitem: u32,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProviderCredentialEvents_Impl::DeleteFieldComboBoxItem(
                                    this,
                                    core::mem::transmute_copy(&pcpc),
                                    core::mem::transmute_copy(&dwfieldid),
                                    core::mem::transmute_copy(&dwitem),
                                )
                                .into()
                            }
                        }
                        unsafe extern "system" fn AppendFieldComboBoxItem<
                            Identity: ICredentialProviderCredentialEvents_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            pcpc: *mut core::ffi::c_void,
                            dwfieldid: u32,
                            pszitem: windows_core::PCWSTR,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProviderCredentialEvents_Impl::AppendFieldComboBoxItem(
                                    this,
                                    core::mem::transmute_copy(&pcpc),
                                    core::mem::transmute_copy(&dwfieldid),
                                    core::mem::transmute(&pszitem),
                                )
                                .into()
                            }
                        }
                        unsafe extern "system" fn SetFieldSubmitButton<
                            Identity: ICredentialProviderCredentialEvents_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            pcpc: *mut core::ffi::c_void,
                            dwfieldid: u32,
                            dwadjacentto: u32,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProviderCredentialEvents_Impl::SetFieldSubmitButton(
                                    this,
                                    core::mem::transmute_copy(&pcpc),
                                    core::mem::transmute_copy(&dwfieldid),
                                    core::mem::transmute_copy(&dwadjacentto),
                                )
                                .into()
                            }
                        }
                        unsafe extern "system" fn OnCreatingWindow<
                            Identity: ICredentialProviderCredentialEvents_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            phwndowner: *mut super::super::Foundation::HWND,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                match ICredentialProviderCredentialEvents_Impl::OnCreatingWindow(
                                    this,
                                ) {
                                    Ok(ok__) => {
                                        phwndowner.write(core::mem::transmute(ok__));
                                        windows_core::HRESULT(0)
                                    }
                                    Err(err) => err.into(),
                                }
                            }
                        }
                        Self {
                            base__: windows_core::IUnknown_Vtbl::new::<Identity, OFFSET>(),
                            SetFieldState: SetFieldState::<Identity, OFFSET>,
                            SetFieldInteractiveState: SetFieldInteractiveState::<Identity, OFFSET>,
                            SetFieldString: SetFieldString::<Identity, OFFSET>,
                            SetFieldCheckbox: SetFieldCheckbox::<Identity, OFFSET>,
                            SetFieldBitmap: SetFieldBitmap::<Identity, OFFSET>,
                            SetFieldComboBoxSelectedItem: SetFieldComboBoxSelectedItem::<
                                Identity,
                                OFFSET,
                            >,
                            DeleteFieldComboBoxItem: DeleteFieldComboBoxItem::<Identity, OFFSET>,
                            AppendFieldComboBoxItem: AppendFieldComboBoxItem::<Identity, OFFSET>,
                            SetFieldSubmitButton: SetFieldSubmitButton::<Identity, OFFSET>,
                            OnCreatingWindow: OnCreatingWindow::<Identity, OFFSET>,
                        }
                    }
                    pub fn matches(iid: &windows_core::GUID) -> bool {
                        iid == & < ICredentialProviderCredentialEvents as windows_core::Interface >::IID
                    }
                }
                impl windows_core::RuntimeName for ICredentialProviderCredentialEvents {}
                windows_core::imp::define_interface!(
                    ICredentialProviderEvents,
                    ICredentialProviderEvents_Vtbl,
                    0x34201e5a_a787_41a3_a5a4_bd6dcf2a854e
                );
                windows_core::imp::interface_hierarchy!(
                    ICredentialProviderEvents,
                    windows_core::IUnknown
                );
                impl ICredentialProviderEvents {
                    pub unsafe fn CredentialsChanged(
                        &self,
                        upadvisecontext: usize,
                    ) -> windows_core::Result<()> {
                        unsafe {
                            (windows_core::Interface::vtable(self).CredentialsChanged)(
                                windows_core::Interface::as_raw(self),
                                upadvisecontext,
                            )
                            .ok()
                        }
                    }
                }
                #[repr(C)]
                #[doc(hidden)]
                pub struct ICredentialProviderEvents_Vtbl {
                    pub base__: windows_core::IUnknown_Vtbl,
                    pub CredentialsChanged: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        usize,
                    )
                        -> windows_core::HRESULT,
                }
                pub trait ICredentialProviderEvents_Impl: windows_core::IUnknownImpl {
                    fn CredentialsChanged(
                        &self,
                        upadvisecontext: usize,
                    ) -> windows_core::Result<()>;
                }
                impl ICredentialProviderEvents_Vtbl {
                    pub const fn new<
                        Identity: ICredentialProviderEvents_Impl,
                        const OFFSET: isize,
                    >() -> Self {
                        unsafe extern "system" fn CredentialsChanged<
                            Identity: ICredentialProviderEvents_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            upadvisecontext: usize,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProviderEvents_Impl::CredentialsChanged(
                                    this,
                                    core::mem::transmute_copy(&upadvisecontext),
                                )
                                .into()
                            }
                        }
                        Self {
                            base__: windows_core::IUnknown_Vtbl::new::<Identity, OFFSET>(),
                            CredentialsChanged: CredentialsChanged::<Identity, OFFSET>,
                        }
                    }
                    pub fn matches(iid: &windows_core::GUID) -> bool {
                        iid == &<ICredentialProviderEvents as windows_core::Interface>::IID
                    }
                }
                impl windows_core::RuntimeName for ICredentialProviderEvents {}
                windows_core::imp::define_interface!(
                    ICredentialProviderSetUserArray,
                    ICredentialProviderSetUserArray_Vtbl,
                    0x095c1484_1c0c_4388_9c6d_500e61bf84bd
                );
                windows_core::imp::interface_hierarchy!(
                    ICredentialProviderSetUserArray,
                    windows_core::IUnknown
                );
                impl ICredentialProviderSetUserArray {
                    pub unsafe fn SetUserArray<P0>(&self, users: P0) -> windows_core::Result<()>
                    where
                        P0: windows_core::Param<ICredentialProviderUserArray>,
                    {
                        unsafe {
                            (windows_core::Interface::vtable(self).SetUserArray)(
                                windows_core::Interface::as_raw(self),
                                users.param().abi(),
                            )
                            .ok()
                        }
                    }
                }
                #[repr(C)]
                #[doc(hidden)]
                pub struct ICredentialProviderSetUserArray_Vtbl {
                    pub base__: windows_core::IUnknown_Vtbl,
                    pub SetUserArray: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        *mut core::ffi::c_void,
                    )
                        -> windows_core::HRESULT,
                }
                pub trait ICredentialProviderSetUserArray_Impl: windows_core::IUnknownImpl {
                    fn SetUserArray(
                        &self,
                        users: windows_core::Ref<ICredentialProviderUserArray>,
                    ) -> windows_core::Result<()>;
                }
                impl ICredentialProviderSetUserArray_Vtbl {
                    pub const fn new<
                        Identity: ICredentialProviderSetUserArray_Impl,
                        const OFFSET: isize,
                    >() -> Self {
                        unsafe extern "system" fn SetUserArray<
                            Identity: ICredentialProviderSetUserArray_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            users: *mut core::ffi::c_void,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProviderSetUserArray_Impl::SetUserArray(
                                    this,
                                    core::mem::transmute_copy(&users),
                                )
                                .into()
                            }
                        }
                        Self {
                            base__: windows_core::IUnknown_Vtbl::new::<Identity, OFFSET>(),
                            SetUserArray: SetUserArray::<Identity, OFFSET>,
                        }
                    }
                    pub fn matches(iid: &windows_core::GUID) -> bool {
                        iid == &<ICredentialProviderSetUserArray as windows_core::Interface>::IID
                    }
                }
                impl windows_core::RuntimeName for ICredentialProviderSetUserArray {}
                windows_core::imp::define_interface!(
                    ICredentialProviderUserArray,
                    ICredentialProviderUserArray_Vtbl,
                    0x90c119ae_0f18_4520_a1f1_114366a40fe8
                );
                windows_core::imp::interface_hierarchy!(
                    ICredentialProviderUserArray,
                    windows_core::IUnknown
                );
                impl ICredentialProviderUserArray {
                    pub unsafe fn SetProviderFilter(
                        &self,
                        guidprovidertofilterto: *const windows_core::GUID,
                    ) -> windows_core::Result<()> {
                        unsafe {
                            (windows_core::Interface::vtable(self).SetProviderFilter)(
                                windows_core::Interface::as_raw(self),
                                guidprovidertofilterto,
                            )
                            .ok()
                        }
                    }
                    pub unsafe fn GetCount(&self) -> windows_core::Result<u32> {
                        unsafe {
                            let mut result__ = core::mem::zeroed();
                            (windows_core::Interface::vtable(self).GetCount)(
                                windows_core::Interface::as_raw(self),
                                &mut result__,
                            )
                            .map(|| result__)
                        }
                    }
                }
                #[repr(C)]
                #[doc(hidden)]
                pub struct ICredentialProviderUserArray_Vtbl {
                    pub base__: windows_core::IUnknown_Vtbl,
                    pub SetProviderFilter: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        *const windows_core::GUID,
                    )
                        -> windows_core::HRESULT,
                    GetAccountOptions: usize,
                    pub GetCount: unsafe extern "system" fn(
                        *mut core::ffi::c_void,
                        *mut u32,
                    )
                        -> windows_core::HRESULT,
                    GetAt: usize,
                }
                pub trait ICredentialProviderUserArray_Impl: windows_core::IUnknownImpl {
                    fn SetProviderFilter(
                        &self,
                        guidprovidertofilterto: *const windows_core::GUID,
                    ) -> windows_core::Result<()>;
                    fn GetCount(&self) -> windows_core::Result<u32>;
                }
                impl ICredentialProviderUserArray_Vtbl {
                    pub const fn new<
                        Identity: ICredentialProviderUserArray_Impl,
                        const OFFSET: isize,
                    >() -> Self {
                        unsafe extern "system" fn SetProviderFilter<
                            Identity: ICredentialProviderUserArray_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            guidprovidertofilterto: *const windows_core::GUID,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                ICredentialProviderUserArray_Impl::SetProviderFilter(
                                    this,
                                    core::mem::transmute_copy(&guidprovidertofilterto),
                                )
                                .into()
                            }
                        }
                        unsafe extern "system" fn GetCount<
                            Identity: ICredentialProviderUserArray_Impl,
                            const OFFSET: isize,
                        >(
                            this: *mut core::ffi::c_void,
                            usercount: *mut u32,
                        ) -> windows_core::HRESULT {
                            unsafe {
                                let this: &Identity = &*((this as *const *const ()).offset(OFFSET)
                                    as *const Identity);
                                match ICredentialProviderUserArray_Impl::GetCount(this) {
                                    Ok(ok__) => {
                                        usercount.write(core::mem::transmute(ok__));
                                        windows_core::HRESULT(0)
                                    }
                                    Err(err) => err.into(),
                                }
                            }
                        }
                        Self {
                            base__: windows_core::IUnknown_Vtbl::new::<Identity, OFFSET>(),
                            SetProviderFilter: SetProviderFilter::<Identity, OFFSET>,
                            GetAccountOptions: 0,
                            GetCount: GetCount::<Identity, OFFSET>,
                            GetAt: 0,
                        }
                    }
                    pub fn matches(iid: &windows_core::GUID) -> bool {
                        iid == &<ICredentialProviderUserArray as windows_core::Interface>::IID
                    }
                }
                impl windows_core::RuntimeName for ICredentialProviderUserArray {}
            }
        }
    }
}
