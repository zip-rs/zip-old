#![allow(missing_docs)]

use std::{
    cmp, ops, slice,
    sync::atomic::{AtomicU8, AtomicUsize, Ordering},
};

#[derive(Debug, Copy, Clone)]
pub enum Lease<Permit> {
    NoSpace,
    PossiblyTaken,
    Taken(Permit),
}

impl<Permit> Lease<Permit> {
    #[inline]
    pub fn option(self) -> Option<Permit> {
        match self {
            Self::NoSpace => None,
            Self::PossiblyTaken => None,
            Self::Taken(permit) => Some(permit),
        }
    }
}

#[repr(u8)]
#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Default, num_enum::TryFromPrimitive, num_enum::IntoPrimitive,
)]
pub enum PermitState {
    #[default]
    Unleashed = 0,
    TakenOut = 1,
}

///```
/// use zip::channels::*;
///
/// let mut ring = Ring::with_capacity(10);
///
/// assert!(matches![ring.request_read_lease(1), Lease::NoSpace]);
/// {
///   let mut write_lease = ring.request_write_lease(5).option().unwrap();
///   write_lease.copy_from_slice(b"world");
/// }
/// {
///   let read_lease = ring.request_read_lease(5).option().unwrap();
///   assert_eq!(std::str::from_utf8(&*read_lease).unwrap(), "world");
/// }
/// {
///   let mut write_lease = ring.request_write_lease(6).option().unwrap();
///   assert_eq!(5, write_lease.len());
///   write_lease.copy_from_slice(b"hello");
/// }
/// {
///   let read_lease = ring.request_read_lease(4).option().unwrap();
///   assert_eq!(std::str::from_utf8(&*read_lease).unwrap(), "hell");
/// }
/// {
///   let mut write_lease = ring.request_write_lease(2).option().unwrap();
///   write_lease.copy_from_slice(b"k!");
/// }
/// let mut buf = Vec::new();
/// {
///   let read_lease = ring.request_read_lease(3).option().unwrap();
///   assert_eq!(1, read_lease.len());
///   buf.extend_from_slice(&read_lease);
/// }
/// {
///   let read_lease = ring.request_read_lease(3).option().unwrap();
///   assert_eq!(2, read_lease.len());
///   buf.extend_from_slice(&read_lease);
/// }
/// assert_eq!(std::str::from_utf8(&buf).unwrap(), "ok!");
/// assert!(matches![ring.request_read_lease(1), Lease::NoSpace]);
///```
#[derive(Debug)]
pub struct Ring {
    buf: Box<[u8]>,
    write_head: AtomicUsize,
    remaining_inline_write: AtomicUsize,
    write_state: AtomicU8,
    read_head: AtomicUsize,
    remaining_inline_read: AtomicUsize,
    read_state: AtomicU8,
}

impl Ring {
    pub fn with_capacity(capacity: usize) -> Self {
        assert!(capacity > 0);
        Self {
            buf: vec![0u8; capacity].into_boxed_slice(),
            write_head: AtomicUsize::new(0),
            remaining_inline_write: AtomicUsize::new(capacity),
            write_state: AtomicU8::new(PermitState::Unleashed.into()),
            read_head: AtomicUsize::new(0),
            remaining_inline_read: AtomicUsize::new(0),
            read_state: AtomicU8::new(PermitState::Unleashed.into()),
        }
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.buf.len()
    }

    pub(crate) fn return_write_lease(&self, permit: &WritePermit<'_>) {
        debug_assert!(self.write_state.load(Ordering::Relaxed) == PermitState::TakenOut.into());

        let truncated_length = permit.truncated_length();
        if truncated_length > 0 {
            self.remaining_inline_write
                .fetch_add(truncated_length, Ordering::Release);
        }

        let len = permit.len();

        if len > 0 {
            let new_write_head = len + self.write_head.load(Ordering::Acquire);
            let read_head = self.read_head.load(Ordering::Acquire);

            if new_write_head == self.capacity() {
                debug_assert_eq!(0, self.remaining_inline_write.load(Ordering::Acquire));
                self.remaining_inline_write
                    .store(read_head, Ordering::Release);
                self.write_head.store(0, Ordering::Release);
            } else {
                self.write_head.store(new_write_head, Ordering::Release);
            }

            if new_write_head > read_head {
                self.remaining_inline_read.fetch_add(len, Ordering::Release);
            }
        }

        self.write_state
            .store(PermitState::Unleashed.into(), Ordering::Release);
    }

    pub fn request_write_lease(&self, requested_length: usize) -> Lease<WritePermit<'_>> {
        assert!(requested_length > 0);
        if self.remaining_inline_write.load(Ordering::Relaxed) == 0 {
            return Lease::NoSpace;
        }

        if let Err(_) = self.write_state.compare_exchange_weak(
            PermitState::Unleashed.into(),
            PermitState::TakenOut.into(),
            Ordering::AcqRel,
            Ordering::Relaxed,
        ) {
            return Lease::PossiblyTaken;
        }

        let remaining_inline_write = self.remaining_inline_write.load(Ordering::Acquire);
        if remaining_inline_write == 0 {
            self.write_state
                .store(PermitState::Unleashed.into(), Ordering::Release);
            return Lease::NoSpace;
        }

        let limited_length = cmp::min(remaining_inline_write, requested_length);
        debug_assert!(limited_length > 0);
        self.remaining_inline_write
            .fetch_sub(limited_length, Ordering::Release);

        let prev_write_head = self.write_head.load(Ordering::Acquire);

        let buf: &mut [u8] = unsafe {
            let buf: *const u8 = self.buf.as_ptr();
            let start = buf.add(prev_write_head) as *mut u8;
            slice::from_raw_parts_mut(start, limited_length)
        };
        Lease::Taken(WritePermit::view(buf, self))
    }

    pub(crate) fn return_read_lease(&self, permit: &ReadPermit<'_>) {
        debug_assert!(self.read_state.load(Ordering::Relaxed) == PermitState::TakenOut.into());

        let truncated_length = permit.truncated_length();
        if truncated_length > 0 {
            self.remaining_inline_read
                .fetch_add(truncated_length, Ordering::Release);
        }

        let len = permit.len();

        if len > 0 {
            let new_read_head = len + self.read_head.load(Ordering::Acquire);
            let write_head = self.write_head.load(Ordering::Acquire);

            if new_read_head == self.capacity() {
                debug_assert_eq!(0, self.remaining_inline_read.load(Ordering::Acquire));
                self.remaining_inline_read
                    .store(write_head, Ordering::Release);
                self.read_head.store(0, Ordering::Release);
            } else {
                self.read_head.store(new_read_head, Ordering::Release);
            }

            if new_read_head > write_head {
                self.remaining_inline_write
                    .fetch_add(len, Ordering::Release);
            }
        }

        self.read_state
            .store(PermitState::Unleashed.into(), Ordering::Release);
    }

    pub fn request_read_lease(&self, requested_length: usize) -> Lease<ReadPermit<'_>> {
        assert!(requested_length > 0);
        if self.remaining_inline_read.load(Ordering::Relaxed) == 0 {
            return Lease::NoSpace;
        }

        if let Err(_) = self.read_state.compare_exchange_weak(
            PermitState::Unleashed.into(),
            PermitState::TakenOut.into(),
            Ordering::AcqRel,
            Ordering::Relaxed,
        ) {
            return Lease::PossiblyTaken;
        }

        let remaining_inline_read = self.remaining_inline_read.load(Ordering::Acquire);
        if remaining_inline_read == 0 {
            self.read_state
                .store(PermitState::Unleashed.into(), Ordering::Release);
            return Lease::NoSpace;
        }

        let limited_length = cmp::min(remaining_inline_read, requested_length);
        debug_assert!(limited_length > 0);
        self.remaining_inline_read
            .fetch_sub(limited_length, Ordering::Release);

        let prev_read_head = self.read_head.load(Ordering::Acquire);

        Lease::Taken(ReadPermit::view(
            &self.buf[prev_read_head..(prev_read_head + limited_length)],
            self,
        ))
    }
}

///```
/// use zip::channels::*;
///
/// let msg = "hello world";
/// let mut ring = Ring::with_capacity(30);
///
/// let mut buf = Vec::new();
/// {
///   let mut write_lease = ring.request_write_lease(5).option().unwrap();
///   write_lease.copy_from_slice(&msg.as_bytes()[..5]);
///   write_lease.truncate(4);
///   assert_eq!(4, write_lease.len());
/// }
/// {
///   let mut read_lease = ring.request_read_lease(5).option().unwrap();
///   assert_eq!(4, read_lease.len());
///   buf.extend_from_slice(read_lease.truncate(1));
///   assert_eq!(1, buf.len());
///   assert_eq!(1, read_lease.len());
/// }
/// {
///   let mut write_lease = ring.request_write_lease(msg.len() - 4).option().unwrap();
///   write_lease.copy_from_slice(&msg.as_bytes()[4..]);
/// }
/// {
///   let read_lease = ring.request_read_lease(msg.len() - 1).option().unwrap();
///   assert_eq!(read_lease.len(), msg.len() - 1);
///   buf.extend_from_slice(&read_lease);
/// }
/// assert_eq!(msg, std::str::from_utf8(&buf).unwrap());
///```
pub trait TruncateLength {
    fn truncated_length(&self) -> usize;
    fn truncate(&mut self, len: usize) -> &mut Self;
}

#[derive(Debug)]
pub struct ReadPermit<'a> {
    view: &'a [u8],
    parent: &'a Ring,
    original_length: usize,
}

impl<'a> ReadPermit<'a> {
    pub(crate) fn view(view: &'a [u8], parent: &'a Ring) -> Self {
        let original_length = view.len();
        Self {
            view,
            parent,
            original_length,
        }
    }
}

impl<'a> TruncateLength for ReadPermit<'a> {
    #[inline]
    fn truncated_length(&self) -> usize {
        self.original_length - self.len()
    }

    #[inline]
    fn truncate(&mut self, len: usize) -> &mut Self {
        assert!(len <= self.len());
        self.view = &self.view[..len];
        self
    }
}

impl<'a> ops::Drop for ReadPermit<'a> {
    fn drop(&mut self) {
        self.parent.return_read_lease(self);
    }
}

impl<'a> AsRef<[u8]> for ReadPermit<'a> {
    fn as_ref(&self) -> &[u8] {
        &self.view
    }
}

impl<'a> ops::Deref for ReadPermit<'a> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.view
    }
}

#[derive(Debug)]
pub struct WritePermit<'a> {
    view: &'a mut [u8],
    parent: &'a Ring,
    original_length: usize,
}

impl<'a> WritePermit<'a> {
    pub(crate) fn view(view: &'a mut [u8], parent: &'a Ring) -> Self {
        let original_length = view.len();
        Self {
            view,
            parent,
            original_length,
        }
    }
}

impl<'a> TruncateLength for WritePermit<'a> {
    #[inline]
    fn truncated_length(&self) -> usize {
        self.original_length - self.len()
    }

    #[inline]
    fn truncate(&mut self, len: usize) -> &mut Self {
        assert!(len <= self.len());
        self.view = unsafe { slice::from_raw_parts_mut(self.view.as_ptr() as *mut u8, len) };
        self
    }
}

impl<'a> ops::Drop for WritePermit<'a> {
    fn drop(&mut self) {
        self.parent.return_write_lease(self);
    }
}

impl<'a> AsRef<[u8]> for WritePermit<'a> {
    fn as_ref(&self) -> &[u8] {
        &self.view
    }
}

impl<'a> AsMut<[u8]> for WritePermit<'a> {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.view
    }
}

impl<'a> ops::Deref for WritePermit<'a> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.view
    }
}

impl<'a> ops::DerefMut for WritePermit<'a> {
    fn deref_mut(&mut self) -> &mut [u8] {
        &mut self.view
    }
}

pub mod futurized {
    use super::*;

    use std::{
        future::Future,
        mem,
        pin::Pin,
        task::{Context, Poll, Waker},
    };

    ///```
    /// # fn main() { tokio_test::block_on(async {
    /// use zip::channels::{*, futurized::*};
    /// use futures_util::future::poll_fn;
    /// use tokio::task;
    /// use std::pin::Pin;
    ///
    /// let ring = Ring::with_capacity(20);
    /// let mut ring = RingFuturized::wrap_ring(&ring);
    /// {
    ///   let mut write_lease = poll_fn(|cx| Pin::new(&mut ring).poll_write(cx, 5)).await;
    ///   write_lease.copy_from_slice(b"hello");
    /// }
    /// {
    ///   let read_lease = poll_fn(|cx| Pin::new(&mut ring).poll_read(cx, 5)).await;
    ///   assert_eq!("hello", std::str::from_utf8(&read_lease).unwrap());
    /// }
    /// # })}
    ///```
    pub struct RingFuturized<'a> {
        buf: &'a Ring,
        read_wakers: Vec<Waker>,
        write_wakers: Vec<Waker>,
    }

    pub struct ReadPermitFuturized<'a> {
        buf: ReadPermit<'a>,
        write_wakers: Vec<Waker>,
    }

    impl<'a> ReadPermitFuturized<'a> {
        pub fn with_wakers(buf: ReadPermit<'a>, write_wakers: Vec<Waker>) -> Self {
            Self { buf, write_wakers }
        }
    }

    impl<'a> ops::Drop for ReadPermitFuturized<'a> {
        fn drop(&mut self) {
            for waker in mem::take(&mut self.write_wakers).into_iter() {
                waker.wake();
            }
        }
    }

    impl<'a> AsRef<ReadPermit<'a>> for ReadPermitFuturized<'a> {
        fn as_ref(&self) -> &ReadPermit<'a> {
            &self.buf
        }
    }

    impl<'a> AsMut<ReadPermit<'a>> for ReadPermitFuturized<'a> {
        fn as_mut(&mut self) -> &mut ReadPermit<'a> {
            &mut self.buf
        }
    }

    impl<'a> ops::Deref for ReadPermitFuturized<'a> {
        type Target = ReadPermit<'a>;

        fn deref(&self) -> &ReadPermit<'a> {
            &self.buf
        }
    }

    impl<'a> ops::DerefMut for ReadPermitFuturized<'a> {
        fn deref_mut(&mut self) -> &mut ReadPermit<'a> {
            &mut self.buf
        }
    }

    pub struct WritePermitFuturized<'a> {
        buf: WritePermit<'a>,
        read_wakers: Vec<Waker>,
    }

    impl<'a> WritePermitFuturized<'a> {
        pub fn with_wakers(buf: WritePermit<'a>, read_wakers: Vec<Waker>) -> Self {
            Self { buf, read_wakers }
        }
    }

    impl<'a> ops::Drop for WritePermitFuturized<'a> {
        fn drop(&mut self) {
            for waker in mem::take(&mut self.read_wakers).into_iter() {
                waker.wake();
            }
        }
    }

    impl<'a> AsRef<WritePermit<'a>> for WritePermitFuturized<'a> {
        fn as_ref(&self) -> &WritePermit<'a> {
            &self.buf
        }
    }

    impl<'a> AsMut<WritePermit<'a>> for WritePermitFuturized<'a> {
        fn as_mut(&mut self) -> &mut WritePermit<'a> {
            &mut self.buf
        }
    }

    impl<'a> ops::Deref for WritePermitFuturized<'a> {
        type Target = WritePermit<'a>;

        fn deref(&self) -> &WritePermit<'a> {
            &self.buf
        }
    }

    impl<'a> ops::DerefMut for WritePermitFuturized<'a> {
        fn deref_mut(&mut self) -> &mut WritePermit<'a> {
            &mut self.buf
        }
    }

    impl<'a> RingFuturized<'a> {
        pub fn wrap_ring(buf: &'a Ring) -> Self {
            Self {
                buf,
                read_wakers: Vec::new(),
                write_wakers: Vec::new(),
            }
        }

        fn get_read_wakers(self: Pin<&mut Self>) -> &mut Vec<Waker> {
            unsafe { &mut self.get_unchecked_mut().read_wakers }
        }

        fn get_write_wakers(self: Pin<&mut Self>) -> &mut Vec<Waker> {
            unsafe { &mut self.get_unchecked_mut().write_wakers }
        }

        pub fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            requested_length: usize,
        ) -> Poll<ReadPermitFuturized<'a>> {
            match self.buf.request_read_lease(requested_length) {
                Lease::NoSpace | Lease::PossiblyTaken => {
                    self.get_read_wakers().push(cx.waker().clone());
                    Poll::Pending
                }
                Lease::Taken(permit) => Poll::Ready(ReadPermitFuturized::with_wakers(
                    permit,
                    mem::take(self.get_write_wakers()),
                )),
            }
        }

        pub fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            requested_length: usize,
        ) -> Poll<WritePermitFuturized<'a>> {
            match self.buf.request_write_lease(requested_length) {
                Lease::NoSpace | Lease::PossiblyTaken => {
                    self.get_write_wakers().push(cx.waker().clone());
                    Poll::Pending
                }
                Lease::Taken(permit) => Poll::Ready(WritePermitFuturized::with_wakers(
                    permit,
                    mem::take(self.get_read_wakers()),
                )),
            }
        }
    }
}

/* impl std::io::Read for RingBuffer { */
/*     fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> { */
/*         debug_assert!(!buf.is_empty()); */
/*         /\* TODO: is this sufficient to make underflow unambiguous? *\/ */
/*         static_assertions::const_assert!(N < (usize::MAX >> 1)); */

/*         let requested_length: usize = cmp::min(N, buf.len()); */
/*         self.remaining */
/*     } */
/* } */
