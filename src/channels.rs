#![allow(missing_docs)]

/* use std::{ */
/*     cmp, mem, ops, */
/*     sync::{ */
/*         atomic::{AtomicUsize, Ordering}, */
/*         Arc, */
/*     }, */
/*     task::{Context, Poll}, */
/* }; */

pub mod sync {
    use std::{cell, cmp, ops};

    #[derive(Debug, Copy, Clone)]
    pub enum Lease<Permit> {
        NoSpace,
        Found(Permit),
    }

    impl<Permit> Lease<Permit> {
        #[inline]
        pub fn option(self) -> Option<Permit> {
            match self {
                Self::NoSpace => None,
                Self::Found(permit) => Some(permit),
            }
        }
    }

    ///```
    /// use zip::channels::sync::*;
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
        write_head: usize,
        remaining_inline_write: usize,
        read_head: usize,
        remaining_inline_read: usize,
    }

    impl Ring {
        pub fn with_capacity(capacity: usize) -> Self {
            assert!(capacity > 0);
            Self {
                buf: vec![0u8; capacity].into_boxed_slice(),
                write_head: 0,
                remaining_inline_write: capacity,
                read_head: 0,
                remaining_inline_read: 0,
            }
        }

        #[inline]
        pub fn capacity(&self) -> usize {
            self.buf.len()
        }

        pub(crate) fn return_write_lease(&mut self, permit: &WritePermit<'_>) {
            let len = permit.len();

            self.write_head += len;

            if self.write_head > self.read_head {
                self.remaining_inline_read += len;
            }
            if self.write_head == self.capacity() {
                debug_assert_eq!(0, self.remaining_inline_write);
                self.remaining_inline_write = self.read_head;
                self.write_head = 0;
            }
        }

        pub fn request_write_lease(&mut self, requested_length: usize) -> Lease<WritePermit<'_>> {
            assert!(requested_length > 0);
            if self.remaining_inline_write == 0 {
                return Lease::NoSpace;
            }

            let prev_write_head = self.write_head;

            let limited_length = cmp::min(self.remaining_inline_write, requested_length);
            debug_assert!(limited_length > 0);
            self.remaining_inline_write -= limited_length;

            let final_write_head = self.write_head + limited_length;

            let s = cell::UnsafeCell::new(self);
            Lease::Found(WritePermit::view(
                &mut unsafe { &mut *s.get() }.buf[prev_write_head..final_write_head],
                unsafe { *s.get() },
            ))
        }

        pub(crate) fn return_read_lease(&mut self, permit: &ReadPermit<'_>) {
            let len = permit.len();

            self.read_head += len;

            if self.read_head > self.write_head {
                self.remaining_inline_write += len;
            }
            if self.read_head == self.capacity() {
                debug_assert_eq!(0, self.remaining_inline_read);
                self.remaining_inline_read = self.write_head;
                self.read_head = 0;
            }
        }

        pub fn request_read_lease(&mut self, requested_length: usize) -> Lease<ReadPermit<'_>> {
            assert!(requested_length > 0);
            if self.remaining_inline_read == 0 {
                return Lease::NoSpace;
            }

            let prev_read_head = self.read_head;

            let limited_length = cmp::min(self.remaining_inline_read, requested_length);
            debug_assert!(limited_length > 0);
            self.remaining_inline_read -= limited_length;

            let final_read_head = self.read_head + limited_length;

            let s = cell::UnsafeCell::new(self);
            Lease::Found(ReadPermit::view(
                &unsafe { &*s.get() }.buf[prev_read_head..final_read_head],
                unsafe { *s.get() },
            ))
        }
    }

    #[derive(Debug)]
    pub struct ReadPermit<'a> {
        /* TODO: if we have any remaining bytes, return those immediately instead of wrapping around
         * (?) ideally this would be easier to implement and avoid tricky logic (and let us
         * masquerade as just a &[u8]). */
        view: &'a [u8],
        parent: &'a mut Ring,
    }

    impl<'a> ReadPermit<'a> {
        pub(crate) fn view(view: &'a [u8], parent: &'a mut Ring) -> Self {
            Self { view, parent }
        }
    }

    impl<'a> ops::Drop for ReadPermit<'a> {
        fn drop(&mut self) {
            let s = cell::UnsafeCell::new(self);
            unsafe { &mut *s.get() }
                .parent
                .return_read_lease(unsafe { &*s.get() });
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
        parent: &'a mut Ring,
    }

    impl<'a> WritePermit<'a> {
        pub(crate) fn view(view: &'a mut [u8], parent: &'a mut Ring) -> Self {
            Self { view, parent }
        }
    }

    impl<'a> ops::Drop for WritePermit<'a> {
        fn drop(&mut self) {
            let s = cell::UnsafeCell::new(self);
            unsafe { &mut *s.get() }
                .parent
                .return_write_lease(unsafe { &*s.get() });
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
}

/* #[derive(Debug)] */
/* pub struct RingBuffer { */
/*     buf: Box<[u8]>, */
/*     write_head: usize, */
/*     read_head: usize, */
/*     /\* read_head: AtomicUsize, *\/ */
/*     /\* write_head: AtomicUsize, *\/ */
/* } */

/* impl RingBuffer { */
/*     #[inline] */
/*     pub fn len(&self) -> usize { */
/*         self.buf.len() */
/*     } */

/*     pub fn with_capacity(length: usize) -> Self { */
/*         assert!(length > 0); */
/*         Self { */
/*             buf: vec![0u8; length].into_boxed_slice(), */
/*             remaining: AtomicUsize::new(0), */
/*         } */
/*     } */

/*     pub fn lease(&self, requested_length: usize) -> Poll<BufferPermit<'_>> { */
/*         /\* let remaining: usize = self.write_head.fetch_max() *\/ */

/*         let num_found: usize = self */
/*             .remaining */
/*             .fetch_min(requested_length, Ordering::Relaxed); */
/*         if num_found == 0 { */
/*             Poll::Pending */
/*         } else { */
/*             Poll::Ready(BufferPermit::owned_range(&self.buf[..], &self)) */
/*         } */
/*     } */
/* } */

/* impl std::io::Read for RingBuffer { */
/*     fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> { */
/*         debug_assert!(!buf.is_empty()); */
/*         /\* TODO: is this sufficient to make underflow unambiguous? *\/ */
/*         static_assertions::const_assert!(N < (usize::MAX >> 1)); */

/*         let requested_length: usize = cmp::min(N, buf.len()); */
/*         self.remaining */
/*     } */
/* } */
