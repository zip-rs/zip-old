use crate::tokio::{
    os::SharedSubset,
    read::{Shared, SharedData},
};

use cfg_if::cfg_if;
use libc;
use once_cell::sync::Lazy;

use std::{
    ffi::c_void,
    io, mem,
    os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd},
    pin::Pin,
    ptr,
    task::{Context, Poll},
};

const MAX_LEN: usize = <libc::ssize_t>::MAX as usize;

macro_rules! cvt {
    ($e:expr) => {{
        let ret = $e;
        if ret == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(ret)
        }
    }};
}

macro_rules! try_libc {
    ($e: expr) => {{
        let ret = $e;
        if ret == -1 {
            return Err(io::Error::last_os_error());
        }
        ret
    }};
}

enum SyscallAvailability {
    Available,
    FailedProbe(io::Error),
    NotOnThisPlatform,
}

/// Invalid file descriptor.
///
/// Valid file descriptors are guaranteed to be positive numbers (see `open()` manpage)
/// while negative values are used to indicate errors.
/// Thus -1 will never be overlap with a valid open file.
const INVALID_FD: RawFd = -1;

fn invalid_copy_file_range() -> io::Error {
    let ret = unsafe {
        libc::copy_file_range(
            INVALID_FD,
            ptr::null_mut(),
            INVALID_FD,
            ptr::null_mut(),
            1,
            0,
        )
    };
    assert_eq!(-1, ret);
    io::Error::last_os_error()
}

static HAS_COPY_FILE_RANGE: Lazy<SyscallAvailability> = Lazy::new(|| {
    cfg_if! {
        if #[cfg(target_os = "linux")] {
            match invalid_copy_file_range().raw_os_error().unwrap() {
                libc::EBADF => SyscallAvailability::Available,
                errno => SyscallAvailability::FailedProbe(io::Error::from_raw_os_error(errno)),
            }
        } else {
            SyscallAvailability::NotOnThisPlatform
        }
    }
});

struct RawArgs {
    fd: libc::c_int,
    off: *mut libc::off64_t,
}

trait CopyFileRangeHandle {
    fn as_args(self: Pin<&mut Self>) -> RawArgs;
}

pub struct MutateInnerOffset(RawFd);

impl CopyFileRangeHandle for MutateInnerOffset {
    fn as_args(self: Pin<&mut Self>) -> RawArgs {
        RawArgs {
            fd: self.0,
            off: ptr::null_mut(),
        }
    }
}

pub struct FromGivenOffset(RawFd, i64);

impl CopyFileRangeHandle for FromGivenOffset {
    fn as_args(self: Pin<&mut Self>) -> RawArgs {
        let Self(fd, ref mut off) = self.get_mut();
        RawArgs { fd: *fd, off }
    }
}

fn copy_file_range_raw(
    src: Pin<&mut impl CopyFileRangeHandle>,
    dst: Pin<&mut impl CopyFileRangeHandle>,
    len: usize,
) -> io::Result<usize> {
    let RawArgs {
        fd: fd_in,
        off: off_in,
    } = src.as_args();
    let RawArgs {
        fd: fd_out,
        off: off_out,
    } = dst.as_args();

    const FUTURE_FLAGS: libc::c_uint = 0;
    let written: libc::ssize_t =
        cvt!(unsafe { libc::copy_file_range(fd_in, off_in, fd_out, off_out, len, FUTURE_FLAGS) })?;
    assert!(written >= 0);
    Ok(written as usize)
}

fn check_regular_file(fd: RawFd) -> io::Result<()> {
    let mut stat = mem::MaybeUninit::<libc::stat>::uninit();

    try_libc!(unsafe { libc::fstat(fd, stat.as_mut_ptr()) });

    let stat = unsafe { stat.assume_init() };
    if (stat.st_mode & libc::S_IFMT) == libc::S_IFREG {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            "Fd is not a regular file",
        ))
    }
}

fn get_status_flags(fd: RawFd) -> io::Result<libc::c_int> {
    Ok(try_libc!(unsafe { libc::fcntl(fd, libc::F_GETFL) }))
}

// needs impl AsRawFd for RawFd (^v1.48)
#[derive(Debug)]
// Optimize size of `Option<PipeFd>` by manually specifing the range.
// Shamelessly taken from [`io-lifetimes::OwnedFd`](https://github.com/sunfishcode/io-lifetimes/blob/8669b5a9fc1d0604d1105f6e39c77fa633ac9c71/src/types.rs#L99).
#[cfg_attr(rustc_attrs, rustc_layout_scalar_valid_range_start(0))]
// libstd/os/raw/mod.rs me that every libstd-supported platform has a
// 32-bit c_int.
//
// Below is -2, in two's complement, but that only works out
// because c_int is 32 bits.
#[cfg_attr(rustc_attrs, rustc_layout_scalar_valid_range_end(0xFF_FF_FF_FE))]
pub struct FileFd(RawFd);

pub enum Role {
    Readable,
    Writable,
}

impl Role {
    fn allowed_modes(&self) -> &'static [libc::c_int] {
        static READABLE: &'static [libc::c_int] = &[libc::O_RDONLY, libc::O_RDWR];
        static WRITABLE: &'static [libc::c_int] = &[libc::O_WRONLY, libc::O_RDWR];
        match self {
            Self::Readable => READABLE,
            Self::Writable => WRITABLE,
        }
    }

    fn check_append(&self, flags: libc::c_int) -> io::Result<()> {
        if let Self::Writable = self {
            if (flags & libc::O_APPEND) != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Writable Fd was set for append!",
                ));
            }
        }
        Ok(())
    }

    fn errmsg(&self) -> &'static str {
        static READABLE: &'static str = "Fd is not readable!";
        static WRITABLE: &'static str = "Fd is not writable!";
        match self {
            Self::Readable => READABLE,
            Self::Writable => WRITABLE,
        }
    }

    pub(crate) fn validate_flags(&self, flags: libc::c_int) -> io::Result<()> {
        let access_mode = flags & libc::O_ACCMODE;

        if !self.allowed_modes().contains(&access_mode) {
            return Err(io::Error::new(io::ErrorKind::Other, self.errmsg()));
        }
        self.check_append(flags)?;

        Ok(())
    }
}

impl FileFd {
    pub fn from_raw_fd_checked(fd: RawFd, role: Role) -> io::Result<Self> {
        check_regular_file(fd)?;

        let status_flags = get_status_flags(fd)?;
        role.validate_flags(status_flags)?;

        Ok(Self(fd))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use std::fs;

    #[test]
    fn check_copy_file_range() {
        assert!(matches!(
            *HAS_COPY_FILE_RANGE,
            SyscallAvailability::Available
        ));
    }

    #[test]
    fn check_readable_writable_file() {
        let f = tempfile::tempfile().unwrap();
        let fd: RawFd = f.as_raw_fd();

        FileFd::from_raw_fd_checked(fd, Role::Readable).unwrap();
        FileFd::from_raw_fd_checked(fd, Role::Writable).unwrap();
    }

    #[test]
    fn check_only_writable() {
        let td = tempfile::tempdir().unwrap();
        let f = fs::OpenOptions::new()
            .create_new(true)
            .read(false)
            .write(true)
            .open(td.path().join("asdf.txt"))
            .unwrap();
        let fd: RawFd = f.as_raw_fd();

        FileFd::from_raw_fd_checked(fd, Role::Writable).unwrap();
        assert!(FileFd::from_raw_fd_checked(fd, Role::Readable).is_err());
    }

    #[test]
    fn check_only_readable() {
        let td = tempfile::tempdir().unwrap();
        let p = td.path().join("asdf.txt");
        fs::write(&p, b"wow!").unwrap();

        let f = fs::OpenOptions::new()
            .read(true)
            .write(false)
            .open(&p)
            .unwrap();
        let fd: RawFd = f.as_raw_fd();

        FileFd::from_raw_fd_checked(fd, Role::Readable).unwrap();
        assert!(FileFd::from_raw_fd_checked(fd, Role::Writable).is_err());
    }

    #[test]
    fn check_no_append() {
        let td = tempfile::tempdir().unwrap();

        let f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .write(true)
            .open(td.path().join("asdf.txt"))
            .unwrap();
        let fd: RawFd = f.as_raw_fd();

        assert!(FileFd::from_raw_fd_checked(fd, Role::Writable).is_err());
        assert!(FileFd::from_raw_fd_checked(fd, Role::Readable).is_err());
    }
}
