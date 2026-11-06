//! The rtnetlink socket, the only part of the responder that socket2 and tokio do not cover.

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

use tokio::io::Interest;
use tokio::io::unix::AsyncFd;

/// socket(AF_NETLINK, SOCK_RAW | SOCK_NONBLOCK | SOCK_CLOEXEC, NETLINK_ROUTE) bound to the
/// multicast `groups` (RTMGRP_* bits) and registered with the runtime for reading.
pub fn open_netlink(groups: u32) -> io::Result<AsyncFd<OwnedFd>> {
    // SAFETY: plain syscall with constant arguments; the result is checked below.
    let fd = unsafe {
        libc::socket(
            libc::AF_NETLINK,
            libc::SOCK_RAW | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            libc::NETLINK_ROUTE,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `fd` is a fresh descriptor that nothing else owns.
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    // SAFETY: sockaddr_nl is plain data; all-zero is a valid value (nl_pad is private in libc).
    let mut sa: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
    sa.nl_family = libc::AF_NETLINK as libc::sa_family_t;
    sa.nl_groups = groups;
    // SAFETY: `sa` is a sockaddr_nl of the length passed and outlives the call.
    let r = unsafe {
        libc::bind(
            fd.as_raw_fd(),
            (&raw const sa).cast(),
            size_of::<libc::sockaddr_nl>() as libc::socklen_t,
        )
    };
    if r != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the AsyncFd takes the OwnedFd itself, so the descriptor stays open and refers to
    // the same socket until the AsyncFd (and with it the registration) is dropped.
    unsafe { AsyncFd::register_with_interest(fd, Interest::READABLE) }.map_err(io::Error::from)
}

/// recv(2) without flags.
pub fn recv(fd: std::os::fd::BorrowedFd<'_>, buf: &mut [u8]) -> io::Result<usize> {
    // SAFETY: `buf` is valid for writes of `buf.len()` bytes during the call.
    let n = unsafe { libc::recv(fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len(), 0) };
    if n < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(n as usize)
    }
}
