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

/* pub enum CopyFileRangeRequest { */

/* } */

/* fn copy_file_range(src: RawFd, dst: RawFd) */

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
struct FileFd(RawFd);

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn check_copy_file_range() {
        assert!(matches!(
            *HAS_COPY_FILE_RANGE,
            SyscallAvailability::Available
        ));
    }
}
