use crate::{cvt, try_libc};

use cfg_if::cfg_if;
use libc;
use once_cell::sync::Lazy;

use std::{
    io, mem,
    os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd, RawFd},
    pin::Pin,
    ptr,
};
