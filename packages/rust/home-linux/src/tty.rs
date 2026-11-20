use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use rustix::termios::{self, OptionalActions, QueueSelector, SpecialCodeIndex, speed};
use tokio::io::unix::AsyncFd;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

#[derive(Debug)]
pub struct Tty(File);

impl Tty {
    pub fn open(path: &str, nonblocking: bool) -> io::Result<Self> {
        let flags = libc::O_NOCTTY | if nonblocking { libc::O_NONBLOCK } else { 0 };
        OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(flags)
            .open(path)
            .map(Self)
    }

    pub fn ensure_tty(&self) -> io::Result<()> {
        if termios::isatty(&self.0) {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(
                rustix::io::Errno::NOTTY.raw_os_error(),
            ))
        }
    }

    pub fn configure_raw_115200(&self, vmin_zero: bool) -> io::Result<()> {
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

    pub fn flush_io(&self) {
        let _ = termios::tcflush(&self.0, QueueSelector::IOFlush);
    }

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

    #[cfg(feature = "testing")]
    pub(crate) fn from_file(file: File) -> Self {
        Self(file)
    }

    pub fn into_async(self) -> io::Result<AsyncTty> {
        let fd = self.0;
        // SAFETY: `File` owns its descriptor, so it stays open and the same for the AsyncFd's whole lifetime, and as_raw_fd always returns it.
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
