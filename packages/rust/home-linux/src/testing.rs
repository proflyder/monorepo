use std::ffi::CStr;
use std::io;
use std::os::fd::{FromRawFd, OwnedFd};

use crate::Tty;

pub fn open_pty() -> io::Result<(Tty, String)> {
    // SAFETY: plain call with valid flags; the result is checked.
    let fd = unsafe { libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `fd` was just opened and is owned by nothing else.
    let master = std::fs::File::from(unsafe { OwnedFd::from_raw_fd(fd) });
    // SAFETY: `fd` is an open pty master (kept open by `master`).
    if unsafe { libc::grantpt(fd) } != 0 || unsafe { libc::unlockpt(fd) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let mut name = [0 as libc::c_char; 128];
    // SAFETY: the buffer is writable for the length passed.
    if unsafe { libc::ptsname_r(fd, name.as_mut_ptr(), name.len()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: ptsname_r succeeded, so the buffer holds a NUL-terminated path.
    let name = unsafe { CStr::from_ptr(name.as_ptr()) };
    Ok((Tty::from_file(master), name.to_string_lossy().into_owned()))
}
