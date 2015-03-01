use time;
use time::Tm;
use std::io;
use std::io::prelude::*;

pub fn msdos_datetime_to_tm(time: u16, date: u16) -> Tm
{
    let seconds = (time & 0b0000000000011111) << 1;
    let minutes = (time & 0b0000011111100000) >> 5;
    let hours =   (time & 0b1111100000000000) >> 11;
    let days =    (date & 0b0000000000011111) >> 0;
    let months =  (date & 0b0000000111100000) >> 5;
    let years =   (date & 0b1111111000000000) >> 9;

    let datetime = format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                           years as u32 + 1980,
                           months,
                           days,
                           hours,
                           minutes,
                           seconds);
    let format = "%Y-%m-%d %H:%M:%S";

    match time::strptime(&*datetime, format)
    {
        Ok(tm) => tm,
        Err(m) => {
            let _ = write!(&mut ::std::old_io::stdio::stderr(), "Failed parsing date: {}", m);
            time::empty_tm()
        },
    }
}

pub fn tm_to_msdos_time(time: Tm) -> u16
{
    ((time.tm_sec >> 1) | (time.tm_min << 5) | (time.tm_hour << 11)) as u16
}

pub fn tm_to_msdos_date(time: Tm) -> u16
{
    (time.tm_mday | ((time.tm_mon + 1) << 5) | ((time.tm_year - 80) << 9)) as u16
}

/// Additional integer write methods for a io::Read
pub trait WriteIntExt {
    /// Write a u32 in little-endian mode
    fn write_le_u32(&mut self, u32) -> io::Result<()>;
    /// Write a u16 in little-endian mode
    fn write_le_u16(&mut self, u16) -> io::Result<()>;
}

impl<W: Write> WriteIntExt for W {
    fn write_le_u32(&mut self, val: u32) -> io::Result<()> {
        let mut buf = [0u8; 4];
        let v = val;
        buf[0] = ((v >>  0) & 0xFF) as u8;
        buf[1] = ((v >>  8) & 0xFF) as u8;
        buf[2] = ((v >> 16) & 0xFF) as u8;
        buf[3] = ((v >> 24) & 0xFF) as u8;
        self.write_all(&buf)
    }

    fn write_le_u16(&mut self, val: u16) -> io::Result<()> {
        let mut buf = [0u8; 2];
        let v = val;
        buf[0] = ((v >>  0) & 0xFF) as u8;
        buf[1] = ((v >>  8) & 0xFF) as u8;
        self.write_all(&buf)
    }
}
/// Additional integer write methods for a io::Read
pub trait ReadIntExt {
    /// Read a u32 in little-endian mode
    fn read_le_u32(&mut self) -> io::Result<u32>;
    /// Read a u16 in little-endian mode
    fn read_le_u16(&mut self) -> io::Result<u16>;
    /// Read exactly n bytes
    fn read_exact(&mut self, usize) -> io::Result<Vec<u8>>;
}

fn fill_exact<R: Read>(reader: &mut R, buf: &mut [u8]) -> io::Result<()> {
    let mut idx = 0;
    while idx < buf.len() {
        match reader.read(&mut buf[idx..]) {
            Err(v) => return Err(v),
            Ok(0) => return Err(io::Error::new(io::ErrorKind::ResourceUnavailable, "Could not fill the buffer", None)),
            Ok(i) => idx += i,
        }
    }
    Ok(())
}

impl<R: Read> ReadIntExt for R {
    fn read_le_u32(&mut self) -> io::Result<u32> {
        let mut buf = [0u8; 4];
        try!(fill_exact(self, &mut buf));

        Ok(
            buf[0] as u32
            | ((buf[1] as u32) << 8)
            | ((buf[2] as u32) << 16)
            | ((buf[3] as u32) << 24)
        )
    }

    fn read_le_u16(&mut self) -> io::Result<u16> {
        let mut buf = [0u8; 2];
        try!(fill_exact(self, &mut buf));

        Ok(
            buf[0] as u16
            | ((buf[1] as u16) << 8)
        )
    }

    fn read_exact(&mut self, n: usize) -> io::Result<Vec<u8>> {
        let mut res = vec![0u8; n];
        try!(fill_exact(self, &mut res));
        Ok(res)
    }
}
