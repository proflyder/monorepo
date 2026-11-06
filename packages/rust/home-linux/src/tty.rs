//! A serial port (tty) of the station: opened read-write, never as the controlling terminal,
//! configured raw 8N1 at 115200 without flow control, and handed to the runtime for reads.
//!
//! The flasher needs the same descriptor as a blocking port with `poll` before every read, so
//! [`Tty`] itself stays runtime-free; [`AsyncTty`] is its reactor-registered form.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use rustix::termios::{self, OptionalActions, QueueSelector, SpecialCodeIndex, speed};
use tokio::io::unix::AsyncFd;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// An open serial port.
#[derive(Debug)]
pub struct Tty(File);

impl Tty {
    /// Opens `path` read-write, never as the controlling terminal (std adds `O_CLOEXEC`):
    /// non-blocking for a link that reads through the runtime, blocking for a flasher that polls
    /// before every read.
    pub fn open(path: &str, nonblocking: bool) -> io::Result<Self> {
        let flags = libc::O_NOCTTY | if nonblocking { libc::O_NONBLOCK } else { 0 };
        OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(flags)
            .open(path)
            .map(Self)
    }

    /// `ENOTTY` unless this is a terminal: never write menu keys or an image into a regular file.
    pub fn ensure_tty(&self) -> io::Result<()> {
        if termios::isatty(&self.0) {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(
                rustix::io::Errno::NOTTY.raw_os_error(),
            ))
        }
    }

    /// Raw 8N1 at 115200 without flow control. `vmin_zero`: VMIN = VTIME = 0, so a read returns
    /// whatever is there (the link); otherwise cfmakeraw's VMIN = 1 (the flasher polls first).
    pub fn configure_raw_115200(&self, vmin_zero: bool) -> io::Result<()> {
        // A failing tcgetattr (not a terminal) is passed through: the caller decides whether
        // that is fatal (the link) or not (the flasher).
        let mut t = termios::tcgetattr(&self.0).map_err(io::Error::from)?;
        t.make_raw();
        t.set_input_speed(speed::B115200).map_err(io::Error::from)?;
        t.set_output_speed(speed::B115200)
            .map_err(io::Error::from)?;
        t.control_modes |= termios::ControlModes::CLOCAL | termios::ControlModes::CREAD;
        t.control_modes &= !(termios::ControlModes::CSTOPB | termios::ControlModes::CRTSCTS);
        if vmin_zero {
            t.special_codes[SpecialCodeIndex::VMIN] = 0;
            t.special_codes[SpecialCodeIndex::VTIME] = 0;
        }
        termios::tcsetattr(&self.0, OptionalActions::Now, &t).map_err(io::Error::from)
    }

    /// Drops whatever the port has buffered in both directions.
    pub fn flush_io(&self) {
        let _ = termios::tcflush(&self.0, QueueSelector::IOFlush);
    }

    /// Waits up to `ms` for input. false on timeout and on any poll error, EINTR included.
    pub fn wait_readable(&self, ms: i32) -> bool {
        let timeout = rustix::event::Timespec {
            tv_sec: i64::from(ms / 1000),
            tv_nsec: i64::from(ms % 1000) * 1_000_000,
        };
        let mut fds = [rustix::event::PollFd::new(
            &self.0,
            rustix::event::PollFlags::IN,
        )];
        matches!(rustix::event::poll(&mut fds, Some(&timeout)), Ok(n) if n > 0)
    }

    /// Builds a `Tty` from an already open descriptor (the pty master in [`crate::testing`]).
    #[cfg(feature = "testing")]
    pub(crate) fn from_file(file: File) -> Self {
        Self(file)
    }

    /// Hands the port to the reactor for read readiness (writes never wait for it).
    pub fn into_async(self) -> io::Result<AsyncTty> {
        let fd = self.0;
        // SAFETY: `File` owns its descriptor, so it stays open and the same for the AsyncFd's
        // whole lifetime, and as_raw_fd always returns it.
        let fd = unsafe {
            AsyncFd::register_with_interest(
                fd,
                tokio::io::Interest::READABLE | tokio::io::Interest::WRITABLE,
            )
        }?;
        Ok(AsyncTty(fd))
    }
}

impl Read for Tty {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl Write for Tty {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

// Shared references, as with `File`: a test may hold the pty master in two closures.
impl Read for &Tty {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&self.0).read(buf)
    }
}

impl Write for &Tty {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&self.0).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&self.0).flush()
    }
}

/// A [`Tty`] registered with the tokio reactor. Reads wait for readiness; a non-blocking read
/// that returns no bytes is "nothing now" (VMIN = VTIME = 0), not the end of input, so the
/// descriptor's read-closed flag alone reports a hangup.
#[derive(Debug)]
pub struct AsyncTty(AsyncFd<File>);

impl AsyncTty {
    pub fn get_ref(&self) -> &File {
        self.0.get_ref()
    }
}

impl AsyncRead for AsyncTty {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let me = self.get_mut();
        loop {
            let mut guard = ready!(me.0.poll_read_ready(cx))?;
            let dst = buf.initialize_unfilled();
            let r = guard.get_inner().read(dst);
            match r {
                Ok(n) if n > 0 => {
                    buf.advance(n);
                    return Poll::Ready(Ok(()));
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                // With VMIN = VTIME = 0 an empty tty reads as 0 bytes (n_tty), not EAGAIN: both
                // just mean "nothing now". Only READ_CLOSED (final: a hangup would stay
                // "readable" for good) makes them the end of input.
                Ok(_) => {
                    if guard.ready().is_read_closed() {
                        return Poll::Ready(Ok(()));
                    }
                    guard.clear_ready();
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    if guard.ready().is_read_closed() {
                        return Poll::Ready(Ok(()));
                    }
                    guard.clear_ready();
                }
                Err(e) => return Poll::Ready(Err(e)),
            }
        }
    }
}

impl AsyncWrite for AsyncTty {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let me = self.get_mut();
        loop {
            let mut guard = ready!(me.0.poll_write_ready(cx))?;
            match guard.get_inner().write(buf) {
                Ok(n) => return Poll::Ready(Ok(n)),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                // The kernel buffer is full: wait for the reactor, then retry. The write
                // interest is registered in into_async().
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => guard.clear_ready(),
                Err(e) => return Poll::Ready(Err(e)),
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
