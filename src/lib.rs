#![feature(phase)]

#[phase(plugin, link)] extern crate log;
extern crate time;
extern crate flatestream;

mod util;
pub mod spec;
pub mod crc32;
