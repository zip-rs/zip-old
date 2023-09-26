#![allow(missing_docs)]

use std::{
    cmp, mem, ops, slice,
    sync::{
        atomic::{AtomicU8, AtomicUsize, Ordering},
        Arc,
    },
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
/// use std::sync::Arc;
///
/// let ring = Arc::new(Ring::with_capacity(10));
///
/// assert!(matches![ring.clone().request_read_lease(1), Lease::NoSpace]);
/// {
///   let mut write_lease = ring.clone().request_write_lease(5).option().unwrap();
///   write_lease.copy_from_slice(b"world");
/// }
/// {
///   let read_lease = ring.clone().request_read_lease(5).option().unwrap();
///   assert_eq!(std::str::from_utf8(&*read_lease).unwrap(), "world");
/// }
/// {
///   let mut write_lease = ring.clone().request_write_lease(6).option().unwrap();
///   assert_eq!(5, write_lease.len());
///   write_lease.copy_from_slice(b"hello");
/// }
/// {
///   let read_lease = ring.clone().request_read_lease(4).option().unwrap();
///   assert_eq!(std::str::from_utf8(&*read_lease).unwrap(), "hell");
/// }
/// {
///   let mut write_lease = ring.clone().request_write_lease(2).option().unwrap();
///   write_lease.copy_from_slice(b"k!");
/// }
/// let mut buf = Vec::new();
/// {
///   let read_lease = ring.clone().request_read_lease(3).option().unwrap();
///   assert_eq!(1, read_lease.len());
///   buf.extend_from_slice(&read_lease);
/// }
/// {
///   let read_lease = ring.clone().request_read_lease(3).option().unwrap();
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

    pub(crate) fn return_write_lease(&self, permit: &WritePermit) {
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

    pub fn request_write_lease(self: Arc<Self>, requested_length: usize) -> Lease<WritePermit> {
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

        let buf: &'static mut [u8] = unsafe {
            let buf: *const u8 = self.buf.as_ptr();
            let start = buf.add(prev_write_head) as *mut u8;
            slice::from_raw_parts_mut(start, limited_length)
        };
        Lease::Taken(unsafe { WritePermit::view(buf, self) })
    }

    pub(crate) fn return_read_lease(&self, permit: &ReadPermit) {
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

    pub fn request_read_lease(self: Arc<Self>, requested_length: usize) -> Lease<ReadPermit> {
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

        let buf: &'static [u8] = unsafe {
            let buf: *const u8 = self.buf.as_ptr();
            let start = buf.add(prev_read_head);
            slice::from_raw_parts(start, limited_length)
        };
        Lease::Taken(unsafe { ReadPermit::view(buf, self) })
    }
}

///```
/// use zip::channels::*;
/// use std::sync::Arc;
///
/// let msg = "hello world";
/// let ring = Arc::new(Ring::with_capacity(30));
///
/// let mut buf = Vec::new();
/// {
///   let mut write_lease = ring.clone().request_write_lease(5).option().unwrap();
///   write_lease.copy_from_slice(&msg.as_bytes()[..5]);
///   write_lease.truncate(4);
///   assert_eq!(4, write_lease.len());
/// }
/// {
///   let mut read_lease = ring.clone().request_read_lease(5).option().unwrap();
///   assert_eq!(4, read_lease.len());
///   buf.extend_from_slice(read_lease.truncate(1));
///   assert_eq!(1, buf.len());
///   assert_eq!(1, read_lease.len());
/// }
/// {
///   let mut write_lease = ring.clone().request_write_lease(msg.len() - 4).option().unwrap();
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
pub struct ReadPermit {
    view: &'static [u8],
    parent: Arc<Ring>,
    original_length: usize,
}

impl ReadPermit {
    pub(crate) unsafe fn view<'a>(view: &'a [u8], parent: Arc<Ring>) -> Self {
        let original_length = view.len();
        Self {
            view: mem::transmute(view),
            parent,
            original_length,
        }
    }
}

impl TruncateLength for ReadPermit {
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

impl ops::Drop for ReadPermit {
    fn drop(&mut self) {
        self.parent.return_read_lease(self);
    }
}

impl AsRef<[u8]> for ReadPermit {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        &self.view
    }
}

impl ops::Deref for ReadPermit {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &[u8] {
        &self.view
    }
}

#[derive(Debug)]
pub struct WritePermit {
    view: &'static mut [u8],
    parent: Arc<Ring>,
    original_length: usize,
}

impl WritePermit {
    pub(crate) unsafe fn view<'a>(view: &'a mut [u8], parent: Arc<Ring>) -> Self {
        let original_length = view.len();
        Self {
            view: mem::transmute(view),
            parent,
            original_length,
        }
    }
}

impl TruncateLength for WritePermit {
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

impl ops::Drop for WritePermit {
    fn drop(&mut self) {
        self.parent.return_write_lease(self);
    }
}

impl AsRef<[u8]> for WritePermit {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        &self.view
    }
}

impl AsMut<[u8]> for WritePermit {
    #[inline]
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.view
    }
}

impl ops::Deref for WritePermit {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &[u8] {
        &self.view
    }
}

impl ops::DerefMut for WritePermit {
    #[inline]
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
        sync::Arc,
        task::{Context, Poll, Waker},
    };

    ///```
    /// # fn main() { tokio_test::block_on(async {
    /// use zip::channels::{*, futurized::*};
    /// use futures_util::future::poll_fn;
    /// use tokio::task;
    /// use std::{cell::UnsafeCell, pin::Pin};
    ///
    /// let ring = Ring::with_capacity(20);
    /// let ring = UnsafeCell::new(RingFuturized::wrap_ring(ring));
    /// let read_lease = poll_fn(|cx| unsafe { &mut *ring.get() }.poll_read(cx, 5));
    /// {
    ///   let mut write_lease = poll_fn(|cx| {
    ///     unsafe { &mut *ring.get() }.poll_write(cx, 20)
    ///   }).await;
    ///   write_lease.truncate(5).copy_from_slice(b"hello");
    /// }
    /// {
    ///   let read_lease = read_lease.await;
    ///   assert_eq!("hello", std::str::from_utf8(&read_lease).unwrap());
    /// }
    /// # })}
    ///```
    pub struct RingFuturized {
        buf: Arc<Ring>,
        read_wakers: Vec<Waker>,
        write_wakers: Vec<Waker>,
    }

    pub struct ReadPermitFuturized {
        buf: ReadPermit,
        write_wakers: Vec<Waker>,
    }

    impl ReadPermitFuturized {
        pub fn with_wakers(buf: ReadPermit, write_wakers: Vec<Waker>) -> Self {
            Self { buf, write_wakers }
        }
    }

    impl ops::Drop for ReadPermitFuturized {
        fn drop(&mut self) {
            for waker in mem::take(&mut self.write_wakers).into_iter() {
                waker.wake();
            }
        }
    }

    impl AsRef<ReadPermit> for ReadPermitFuturized {
        #[inline]
        fn as_ref(&self) -> &ReadPermit {
            &self.buf
        }
    }

    impl AsMut<ReadPermit> for ReadPermitFuturized {
        #[inline]
        fn as_mut(&mut self) -> &mut ReadPermit {
            &mut self.buf
        }
    }

    impl ops::Deref for ReadPermitFuturized {
        type Target = ReadPermit;

        #[inline]
        fn deref(&self) -> &ReadPermit {
            &self.buf
        }
    }

    impl ops::DerefMut for ReadPermitFuturized {
        #[inline]
        fn deref_mut(&mut self) -> &mut ReadPermit {
            &mut self.buf
        }
    }

    pub struct WritePermitFuturized {
        buf: WritePermit,
        read_wakers: Vec<Waker>,
    }

    impl WritePermitFuturized {
        pub fn with_wakers(buf: WritePermit, read_wakers: Vec<Waker>) -> Self {
            Self { buf, read_wakers }
        }
    }

    impl ops::Drop for WritePermitFuturized {
        fn drop(&mut self) {
            for waker in mem::take(&mut self.read_wakers).into_iter() {
                waker.wake();
            }
        }
    }

    impl AsRef<WritePermit> for WritePermitFuturized {
        #[inline]
        fn as_ref(&self) -> &WritePermit {
            &self.buf
        }
    }

    impl AsMut<WritePermit> for WritePermitFuturized {
        #[inline]
        fn as_mut(&mut self) -> &mut WritePermit {
            &mut self.buf
        }
    }

    impl ops::Deref for WritePermitFuturized {
        type Target = WritePermit;

        #[inline]
        fn deref(&self) -> &WritePermit {
            &self.buf
        }
    }

    impl ops::DerefMut for WritePermitFuturized {
        #[inline]
        fn deref_mut(&mut self) -> &mut WritePermit {
            &mut self.buf
        }
    }

    impl RingFuturized {
        pub fn wrap_ring(buf: Ring) -> Self {
            Self {
                buf: Arc::new(buf),
                read_wakers: Vec::new(),
                write_wakers: Vec::new(),
            }
        }

        pub fn poll_read(
            &mut self,
            cx: &mut Context<'_>,
            requested_length: usize,
        ) -> Poll<ReadPermitFuturized> {
            match self.buf.clone().request_read_lease(requested_length) {
                Lease::NoSpace | Lease::PossiblyTaken => {
                    self.read_wakers.push(cx.waker().clone());
                    Poll::Pending
                }
                Lease::Taken(permit) => Poll::Ready(ReadPermitFuturized::with_wakers(
                    permit,
                    mem::take(&mut self.write_wakers),
                )),
            }
        }

        pub fn poll_write(
            &mut self,
            cx: &mut Context<'_>,
            requested_length: usize,
        ) -> Poll<WritePermitFuturized> {
            match self.buf.clone().request_write_lease(requested_length) {
                Lease::NoSpace | Lease::PossiblyTaken => {
                    self.write_wakers.push(cx.waker().clone());
                    Poll::Pending
                }
                Lease::Taken(permit) => Poll::Ready(WritePermitFuturized::with_wakers(
                    permit,
                    mem::take(&mut self.read_wakers),
                )),
            }
        }
    }

    impl ops::Drop for RingFuturized {
        fn drop(&mut self) {
            for waker in mem::take(&mut self.read_wakers).into_iter() {
                waker.wake();
            }
            for waker in mem::take(&mut self.write_wakers).into_iter() {
                waker.wake();
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
