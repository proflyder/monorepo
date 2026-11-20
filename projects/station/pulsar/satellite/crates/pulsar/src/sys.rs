pub fn write_stderr(msg: &[u8]) {
    // SAFETY: the pointer and length describe a live byte slice.
    let _ = unsafe { libc::write(2, msg.as_ptr().cast(), msg.len()) };
}

pub fn exit_now(status: i32) -> ! {
    // SAFETY: _exit has no preconditions.
    unsafe { libc::_exit(status) }
}
