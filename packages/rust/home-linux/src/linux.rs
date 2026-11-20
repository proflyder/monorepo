use std::fs::File;
use std::io;
use std::os::fd::{AsFd as _, AsRawFd, RawFd};
use std::os::unix::fs::OpenOptionsExt as _;

use rustix::ioctl::{Getter, Opcode, opcode};

#[repr(C)]
#[derive(Clone, Copy)]
struct FbVarScreeninfo([u32; 40]);

const _: () = assert!(size_of::<FbVarScreeninfo>() == 160);

const FBIOGET_VSCREENINFO: Opcode = 0x4600;
const FBIOPAN_DISPLAY: Opcode = 0x4606;

struct PanDisplay<'a>(&'a mut FbVarScreeninfo);

// SAFETY: the opcode is the kernel's _IOW('F', 6, fb_var_screeninfo) and the argument is exactly one writable fb_var_screeninfo; PanDisplay is only built for an open framebuffer node.
unsafe impl rustix::ioctl::Ioctl for PanDisplay<'_> {
    type Output = ();

    const IS_MUTATING: bool = true;

    fn opcode(&self) -> Opcode {
        FBIOPAN_DISPLAY
    }

    fn as_ptr(&mut self) -> *mut core::ffi::c_void {
        std::ptr::from_mut(self.0).cast()
    }

    unsafe fn output_from_ptr(
        _: rustix::ioctl::IoctlOutput,
        _: *mut core::ffi::c_void,
    ) -> Result<(), rustix::io::Errno> {
        Ok(())
    }
}

pub struct Framebuffer {
    file: File,
    var: FbVarScreeninfo,
}

impl Framebuffer {
    pub fn open(path: impl AsRef<std::path::Path>) -> io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?;
        // SAFETY: `file` is a framebuffer device (Framebuffer is only created here, for an fb node), for which this request writes one fb_var_screeninfo; Getter hands a value of the kernel's size. A failure keeps a zeroed screen info, as in the C++ daemon.
        let var = unsafe {
            rustix::ioctl::ioctl(&file, Getter::<FBIOGET_VSCREENINFO, FbVarScreeninfo>::new())
        }
        .unwrap_or(FbVarScreeninfo([0; 40]));
        Ok(Self { file, var })
    }

    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        rustix::io::pwrite(&self.file, buf, 0).map_err(io::Error::from)
    }

    pub fn pan(&mut self) {
        // SAFETY: as in open(): a framebuffer device, which reads (and may update) the fb_var_screeninfo `PanDisplay` points to.
        let _ = unsafe { rustix::ioctl::ioctl(&self.file, PanDisplay(&mut self.var)) };
    }
}

#[repr(C)]
struct I2cMsg {
    addr: u16,
    flags: u16,
    len: u16,
    buf: *mut u8,
}

#[repr(C)]
struct I2cRdwrIoctlData {
    msgs: *mut I2cMsg,
    nmsgs: u32,
}

const I2C_RDWR: libc::Ioctl = 0x0707;

pub struct I2c(File);

impl I2c {
    pub fn open(path: &str) -> io::Result<Self> {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_CLOEXEC)
            .open(path)
            .map(Self)
    }

    pub fn write_register(&self, addr: u16, reg: u8, data: &[u8]) -> io::Result<()> {
        if data.len() > 31 {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        let mut buf = [0u8; 32];
        buf[0] = reg;
        buf[1..=data.len()].copy_from_slice(data);
        let mut msg = I2cMsg {
            addr,
            flags: 0,
            len: u16::try_from(data.len() + 1).map_err(|_| io::ErrorKind::InvalidInput)?,
            buf: buf.as_mut_ptr(),
        };
        let mut xfer = I2cRdwrIoctlData {
            msgs: &mut msg,
            nmsgs: 1,
        };
        // SAFETY: `self.0` is an i2c-dev node (I2c is only created here), for which I2C_RDWR reads one i2c_rdwr_ioctl_data and the i2c_msg it points to; `buf` stays alive and writable for the duration of the ioctl.
        let rc =
            unsafe { libc::ioctl(self.0.as_raw_fd(), I2C_RDWR, std::ptr::from_mut(&mut xfer)) };
        if rc < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RawInputEvent {
    pub sec: libc::c_ulong,
    pub usec: libc::c_ulong,
    pub type_: u16,
    pub code: u16,
    pub value: i32,
}

#[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
const _: () = {
    assert!(size_of::<RawInputEvent>() == 24);
    assert!(core::mem::offset_of!(RawInputEvent, type_) == 16);
    assert!(core::mem::offset_of!(RawInputEvent, value) == 20);
};
#[cfg(target_arch = "arm")]
const _: () = {
    assert!(size_of::<RawInputEvent>() == 16);
    assert!(core::mem::offset_of!(RawInputEvent, type_) == 8);
    assert!(core::mem::offset_of!(RawInputEvent, value) == 12);
};

pub const EMPTY_EVENT: RawInputEvent = RawInputEvent {
    sec: 0,
    usec: 0,
    type_: 0,
    code: 0,
    value: 0,
};

pub type KeyBits = [u8; 0x300 / 8];
pub type SwBits = [u8; 8];

const EVIOCGKEY: Opcode = opcode::read::<KeyBits>(b'E', 0x18);
const EVIOCGSW: Opcode = opcode::read::<SwBits>(b'E', 0x1b);

pub struct Evdev(File);

impl AsRawFd for Evdev {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

impl Evdev {
    pub fn open(path: &str) -> io::Result<Self> {
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)
            .map(Self)
    }

    pub fn into_async(self) -> io::Result<tokio::io::unix::AsyncFd<Self>> {
        // SAFETY: the AsyncFd owns the file and never replaces it, so its descriptor stays open and refers to the same file description until the AsyncFd is dropped.
        Ok(unsafe {
            tokio::io::unix::AsyncFd::register_with_interest(self, tokio::io::Interest::READABLE)
        }?)
    }

    pub fn read_events(&self, buf: &mut [RawInputEvent]) -> io::Result<usize> {
        // SAFETY: RawInputEvent is plain data, so viewing the same writable allocation as bytes is valid; the kernel may write any bit pattern into the event fields.
        let bytes = unsafe {
            std::slice::from_raw_parts_mut(buf.as_mut_ptr().cast::<u8>(), size_of_val(buf))
        };
        let n = rustix::io::read(self.0.as_fd(), bytes).map_err(io::Error::from)?;
        Ok(n / size_of::<RawInputEvent>())
    }

    pub fn key_state(&self) -> io::Result<KeyBits> {
        // SAFETY: `self` is an evdev node (Evdev is only created by open()), and EVIOCGKEY is the kernel's _IOR('E', 0x18, [u8; 96]); Getter supplies an array of that size.
        unsafe { rustix::ioctl::ioctl(&self.0, Getter::<EVIOCGKEY, KeyBits>::new()) }
            .map_err(io::Error::from)
    }

    pub fn switch_state(&self) -> io::Result<SwBits> {
        // SAFETY: see key_state(); EVIOCGSW copies up to one long (8 bytes here).
        unsafe { rustix::ioctl::ioctl(&self.0, Getter::<EVIOCGSW, SwBits>::new()) }
            .map_err(io::Error::from)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WallTime {
    pub hour: i32,
    pub min: i32,
    pub first_half: bool,
}

pub fn wall_time() -> WallTime {
    let ts = rustix::time::clock_gettime(rustix::time::ClockId::Realtime);
    let seconds = ts.tv_sec as libc::time_t;
    let mut tm = zeroed_tm();
    // SAFETY: both pointers refer to live locals; localtime_r is the thread-safe variant and always returns its `tm` argument on Linux.
    if unsafe { libc::localtime_r(&seconds, &mut tm) }.is_null() {
        tm = zeroed_tm();
    }
    WallTime {
        hour: tm.tm_hour,
        min: tm.tm_min,
        first_half: ts.tv_nsec < 500_000_000,
    }
}

fn zeroed_tm() -> libc::tm {
    // SAFETY: an all-zero `tm` is a valid value (integers and a nullable `tm_zone` pointer).
    unsafe { std::mem::zeroed() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evdev_ioctl_numbers_match_the_kernel_macros() {
        assert_eq!(EVIOCGKEY as u64, 0x8060_4518);
        assert_eq!(EVIOCGSW as u64, 0x8008_451b);
    }

    #[test]
    fn wall_time_is_a_plausible_local_time() {
        let w = wall_time();
        assert!((0..24).contains(&w.hour), "{w:?}");
        assert!((0..60).contains(&w.min), "{w:?}");
    }
}
