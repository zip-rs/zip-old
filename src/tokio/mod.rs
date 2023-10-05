#![allow(missing_docs)]

pub mod buf_reader;
pub mod buf_writer;
pub mod combinators;
pub mod crc32;
pub mod extraction;
pub mod read;
pub mod stream_impls;
pub mod write;

use std::pin::Pin;

pub trait WrappedPin<S> {
    fn unwrap_inner_pin(self) -> Pin<Box<S>>;
}

pub mod utils {
    use std::{
        cell::UnsafeCell,
        mem::{self, ManuallyDrop, MaybeUninit},
        ptr,
    };

    /* nightly-only UnsafeCell::from_mut() */
    pub(crate) fn unsafe_cell_from_mut<T: ?Sized>(value: &mut T) -> &mut UnsafeCell<T> {
        unsafe { &mut *(value as *mut T as *mut UnsafeCell<T>) }
    }

    pub(crate) fn map_take_manual_drop<T, U, F: FnOnce(T) -> U>(
        slot: &mut ManuallyDrop<T>,
        f: F,
    ) -> ManuallyDrop<U> {
        let taken = unsafe { ManuallyDrop::take(slot) };
        ManuallyDrop::new(f(taken))
    }

    pub(crate) fn leak_box_address<T>(b: &mut Box<T>) -> *mut T {
        let mut other = MaybeUninit::<Box<T>>::uninit();
        /* `other` IS UNINIT! */
        unsafe {
            ptr::swap(b, other.as_mut_ptr());
        }
        /* `b` IS UNINIT! */
        let ret = Box::into_raw(unsafe { other.assume_init() });
        unsafe {
            let new = Box::from_raw(ret);
            ptr::write(b, new);
        }
        /* `b` IS NOW SAFE! */
        ret
    }

    /// Convert a `&mut T` into a `T` temporarily by temporarily swapping its contents with
    /// a [`MaybeUninit`].
    ///
    ///```
    /// use zip::tokio::utils::map_swap_uninit;
    ///
    /// let mut x = 5;
    /// map_swap_uninit(&mut x, |x| x + 3);
    /// assert_eq!(x, 8);
    ///```
    pub fn map_swap_uninit<T, F: FnOnce(T) -> T>(slot: &mut T, f: F) {
        let mut other = MaybeUninit::<T>::uninit();
        unsafe {
            ptr::swap(slot, other.as_mut_ptr());
        }
        /* `slot` IS NOW UNINIT!!!! */
        let mut wrapped = f(unsafe { other.assume_init() });
        unsafe {
            ptr::swap(slot, &mut wrapped);
        }
        /* `wrapped` IS NOW UNINIT!!!! */
        mem::forget(wrapped);
    }

    #[cfg(test)]
    mod test {
        use super::*;

        #[test]
        fn test_leak_box_address() {
            let mut b = Box::new(3);
            let p = leak_box_address(&mut b);
            *b = 4;
            assert_eq!(4, *Box::leak(unsafe { Box::from_raw(p) }));
            assert_eq!(4, *b);
        }

        #[test]
        fn test_map_swap_uninit_primitive() {
            let mut x = 5;
            map_swap_uninit(&mut x, |x| x + 3);
            assert_eq!(x, 8);
        }

        #[test]
        fn test_map_swap_uninit_heap_alloc() {
            let mut x: Vec<u8> = (0..12).map(|x| x * 3).collect();
            map_swap_uninit(&mut x, |mut x| {
                x.push(192);
                x.insert(0, 123);
                x
            });
            assert_eq!(
                x,
                vec![123, 0, 3, 6, 9, 12, 15, 18, 21, 24, 27, 30, 33, 192]
            );
        }
    }
}
