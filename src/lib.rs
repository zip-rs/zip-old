#![feature(phase)]

#[phase(plugin, link)] extern crate log;
extern crate time;
extern crate flate2;

mod util;
mod spec;
pub mod crc32;
pub mod reader;
pub mod types;
