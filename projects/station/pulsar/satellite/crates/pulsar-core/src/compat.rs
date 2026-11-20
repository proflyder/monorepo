#[cfg(all(target_arch = "arm", target_os = "linux"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getrandom(buf: *mut std::ffi::c_void, buflen: usize, flags: u32) -> isize {
    // SAFETY: the caller (libc / Rust std) owns a writable buffer of `buflen` bytes; the syscall has the same contract as glibc's getrandom.
    unsafe { libc::syscall(libc::SYS_getrandom, buf, buflen, flags) as isize }
}
