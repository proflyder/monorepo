//! Keeps a Windows console window that was opened for this process (double-click in
//! Explorer) until Enter, so the output can be read. From a terminal the console is shared.

#[cfg(windows)]
mod imp {
    use std::io::Write;

    // `GetConsoleProcessList` lives in kernel32; no Windows SDK or extra crate is needed
    // for this one call.
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetConsoleProcessList(list: *mut u32, count: u32) -> u32;
    }

    pub fn pause_if_own_console() {
        let mut pids = [0u32; 2];
        // SAFETY: `pids` is a valid, writable array of `len` u32 values for the duration of
        // the call; GetConsoleProcessList writes at most `count` entries.
        let n = unsafe { GetConsoleProcessList(pids.as_mut_ptr(), pids.len() as u32) };
        if n != 1 {
            return;
        }
        let mut err = std::io::stderr();
        let _ = write!(err, "\nPress Enter to close this window...");
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
    }
}

#[cfg(not(windows))]
mod imp {
    /// Only Windows opens a console window that closes with the process.
    pub fn pause_if_own_console() {}
}

pub use imp::pause_if_own_console;
