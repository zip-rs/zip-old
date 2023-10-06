/* use crate::tokio::{ */
/*     os::SharedSubset, */
/*     read::{Shared, SharedData}, */
/* }; */

use cfg_if::cfg_if;
use displaydoc::Display;
use libc;
use once_cell::sync::Lazy;

use std::{
    ffi::c_void,
    io, mem,
    os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd, RawFd},
    pin::Pin,
    ptr,
    task::{Context, Poll},
};

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

#[allow(dead_code)]
pub enum SyscallAvailability {
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

pub static HAS_COPY_FILE_RANGE: Lazy<SyscallAvailability> = Lazy::new(|| {
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

pub struct RawArgs {
    fd: libc::c_int,
    off: *mut libc::off64_t,
}

impl RawArgs {
    pub fn as_option<'a>(self) -> Option<&'a mut libc::off64_t> {
        ptr::NonNull::new(self.off).map(|ref mut off| unsafe { off.as_mut() })
    }
}

pub trait CopyFileRangeHandle {
    fn role(&self) -> Role;
    fn as_args(self: Pin<&mut Self>) -> RawArgs;
}

pub struct MutateInnerOffset {
    role: Role,
    owned_fd: OwnedFd,
}

impl MutateInnerOffset {
    pub fn new(f: impl IntoRawFd, role: Role) -> io::Result<Self> {
        let raw_fd = validate_raw_fd(f.into_raw_fd(), role)?;
        let owned_fd = unsafe { OwnedFd::from_raw_fd(raw_fd) };
        Ok(Self { role, owned_fd })
    }

    pub fn into_owned(self) -> OwnedFd {
        self.owned_fd
    }
}

impl AsRawFd for MutateInnerOffset {
    fn as_raw_fd(&self) -> RawFd {
        self.owned_fd.as_raw_fd()
    }
}

impl IntoRawFd for MutateInnerOffset {
    fn into_raw_fd(self) -> RawFd {
        self.into_owned().into_raw_fd()
    }
}

impl CopyFileRangeHandle for MutateInnerOffset {
    fn role(&self) -> Role {
        self.role
    }
    fn as_args(self: Pin<&mut Self>) -> RawArgs {
        RawArgs {
            fd: self.owned_fd.as_raw_fd(),
            off: ptr::null_mut(),
        }
    }
}

pub struct FromGivenOffset {
    fd: RawFd,
    pub offset: i64,
    role: Role,
}

impl FromGivenOffset {
    pub fn new(f: &impl AsRawFd, role: Role, init: u32) -> io::Result<Self> {
        let raw_fd = f.as_raw_fd();
        let fd = validate_raw_fd(raw_fd, role)?;
        Ok(Self {
            fd,
            role,
            offset: init as i64,
        })
    }
}

impl AsRawFd for FromGivenOffset {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl CopyFileRangeHandle for FromGivenOffset {
    fn role(&self) -> Role {
        self.role
    }
    fn as_args(self: Pin<&mut Self>) -> RawArgs {
        let Self {
            fd, ref mut offset, ..
        } = self.get_mut();
        RawArgs {
            fd: fd.as_raw_fd(),
            off: offset,
        }
    }
}

pub fn copy_file_range_raw(
    src: Pin<&mut impl CopyFileRangeHandle>,
    dst: Pin<&mut impl CopyFileRangeHandle>,
    len: usize,
) -> io::Result<usize> {
    assert_eq!(src.role(), Role::Readable);
    let RawArgs {
        fd: fd_in,
        off: off_in,
    } = src.as_args();
    assert_eq!(dst.role(), Role::Writable);
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

#[derive(Copy, Clone, Debug, Display, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum Role {
    /// fd has the read capability
    Readable,
    /// fd has the write capability
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

    pub(crate) fn interest(&self) -> &'static tokio::io::Interest {
        use tokio::io::Interest;
        static READABLE: Interest = Interest::READABLE.add(Interest::ERROR);
        static WRITABLE: Interest = Interest::WRITABLE.add(Interest::ERROR);
        match self {
            Self::Readable => &READABLE,
            Self::Writable => &WRITABLE,
        }
    }
}

fn validate_raw_fd(fd: RawFd, role: Role) -> io::Result<RawFd> {
    check_regular_file(fd)?;

    let status_flags = get_status_flags(fd)?;
    role.validate_flags(status_flags)?;

    Ok(fd)
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

        validate_raw_fd(fd, Role::Readable).unwrap();
        validate_raw_fd(fd, Role::Writable).unwrap();
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

        validate_raw_fd(fd, Role::Writable).unwrap();
        assert!(validate_raw_fd(fd, Role::Readable).is_err());
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

        validate_raw_fd(fd, Role::Readable).unwrap();
        assert!(validate_raw_fd(fd, Role::Writable).is_err());
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

        assert!(validate_raw_fd(fd, Role::Writable).is_err());
        assert!(validate_raw_fd(fd, Role::Readable).is_err());
    }

    #[test]
    fn read_ref_into_write_owned() {
        use io::{Read, Seek};

        let td = tempfile::tempdir().unwrap();
        let p = td.path().join("asdf.txt");
        fs::write(&p, b"wow!").unwrap();

        let in_file = fs::File::open(&p).unwrap();
        let mut src = FromGivenOffset::new(&in_file, Role::Readable, 0).unwrap();

        let p2 = td.path().join("asdf2.txt");
        let out_file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            /* Need this to read the output file contents at the end! */
            .read(true)
            .open(&p2)
            .unwrap();
        let mut dst = MutateInnerOffset::new(out_file, Role::Writable).unwrap();

        /* Explicit offset begins at 0. */
        assert_eq!(0, src.offset);

        /* 4 bytes were written. */
        assert_eq!(
            4,
            /* NB: 5 bytes were requested! */
            copy_file_range_raw(Pin::new(&mut src), Pin::new(&mut dst), 5).unwrap()
        );
        assert_eq!(4, src.offset);

        let mut dst: fs::File = dst.into_owned().into();
        assert_eq!(4, dst.stream_position().unwrap());
        dst.rewind().unwrap();
        let mut s = String::new();
        dst.read_to_string(&mut s).unwrap();
        assert_eq!(&s, "wow!");
    }
}

pub mod async_fd {
    use super::{CopyFileRangeHandle, Role};

    use tokio::io::{self, unix::AsyncFd};
    use tokio_pipe::{PipeRead, PipeWrite};

    use std::{
        os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd, RawFd},
        pin::Pin,
        task::{ready, Context, Poll},
    };

    pub struct SpliceArgs<'a, T: AsRawFd> {
        pub fd: &'a mut AsyncFd<T>,
        pub off: Option<&'a mut libc::off64_t>,
    }

    pub trait SpliceHandle {
        fn role(&self) -> Role;
        type FD: AsRawFd;
        fn as_args(self: Pin<&mut Self>) -> SpliceArgs<'_, Self::FD>;
    }

    pub struct MutateAsync {
        fd: AsyncFd<super::MutateInnerOffset>,
    }

    impl SpliceHandle for MutateAsync {
        fn role(&self) -> Role {
            self.fd.get_ref().role()
        }
        type FD = super::MutateInnerOffset;
        fn as_args(self: Pin<&mut Self>) -> SpliceArgs<'_, Self::FD> {
            let s = self.get_mut();

            let raw_args = Pin::new(s.fd.get_mut()).as_args();
            SpliceArgs {
                fd: &mut s.fd,
                off: raw_args.as_option(),
            }
        }
    }

    impl MutateAsync {
        pub fn new(inner: super::MutateInnerOffset) -> io::Result<Self> {
            let role = inner.role();
            Ok(Self {
                fd: AsyncFd::with_interest(inner, *role.interest())?,
            })
        }

        pub fn into_copy_file_range_handle(self) -> super::MutateInnerOffset {
            self.fd.into_inner()
        }
    }

    impl AsRawFd for MutateAsync {
        fn as_raw_fd(&self) -> RawFd {
            self.fd.as_raw_fd()
        }
    }

    impl IntoRawFd for MutateAsync {
        fn into_raw_fd(self) -> RawFd {
            self.into_copy_file_range_handle().into_raw_fd()
        }
    }

    pub struct GivenOffsetAsync {
        fd: AsyncFd<super::FromGivenOffset>,
    }

    impl SpliceHandle for GivenOffsetAsync {
        fn role(&self) -> Role {
            self.fd.get_ref().role()
        }
        type FD = super::FromGivenOffset;
        fn as_args(self: Pin<&mut Self>) -> SpliceArgs<'_, Self::FD> {
            let s = self.get_mut();

            let raw_args = Pin::new(s.fd.get_mut()).as_args();
            SpliceArgs {
                fd: &mut s.fd,
                off: raw_args.as_option(),
            }
        }
    }

    impl GivenOffsetAsync {
        pub fn new(inner: super::FromGivenOffset) -> io::Result<Self> {
            let role = inner.role();
            Ok(Self {
                fd: AsyncFd::with_interest(inner, *role.interest())?,
            })
        }

        pub fn into_copy_file_range_handle(self) -> super::FromGivenOffset {
            self.fd.into_inner()
        }
    }

    impl AsRawFd for GivenOffsetAsync {
        fn as_raw_fd(&self) -> RawFd {
            self.fd.as_raw_fd()
        }
    }

    pub async fn splice_from_pipe(
        src: Pin<&mut PipeRead>,
        dst: Pin<&mut impl SpliceHandle>,
        len: usize,
        has_more_data: bool,
    ) -> io::Result<usize> {
        assert_eq!(dst.role(), Role::Writable);
        let SpliceArgs { fd, off } = dst.as_args();
        src.get_mut().splice_to(fd, off, len, has_more_data).await
    }

    pub async fn splice_into_pipe(
        src: Pin<&mut impl SpliceHandle>,
        dst: Pin<&mut PipeWrite>,
        len: usize,
    ) -> io::Result<usize> {
        assert_eq!(src.role(), Role::Readable);
        let SpliceArgs { fd, off } = src.as_args();
        dst.get_mut().splice_from(fd, off, len).await
    }
}
