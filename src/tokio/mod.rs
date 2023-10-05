#![allow(missing_docs)]

pub mod buf_reader;
pub mod buf_writer;
pub mod combinators;
pub mod extraction;
pub mod read;
pub mod crc32;
pub mod stream_impls;
/* pub mod write; */

use std::pin::Pin;

pub trait WrappedPin<S> {
    fn unwrap_inner_pin(self) -> Pin<Box<S>>;
}

pub mod utils {
    use std::{cell::UnsafeCell, mem::ManuallyDrop};

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
}
