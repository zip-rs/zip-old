use time;
use time::Tm;
use std::cell::RefMut;

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

pub struct RefMutReader<'a, R:'a>
{
    inner: RefMut<'a, R>,
}

impl<'a, R: Reader> RefMutReader<'a, R>
{
    pub fn new(inner: RefMut<'a, R>) -> RefMutReader<'a, R>
    {
        RefMutReader { inner: inner, }
    }
}

impl<'a, R: Reader> Reader for RefMutReader<'a, R>
{
    fn read(&mut self, buf: &mut [u8]) -> ::std::old_io::IoResult<usize>
    {
        self.inner.read(buf)
    }
}
