use time::{Tm, empty_tm};

pub fn msdos_datetime_to_tm(time: u16, date: u16) -> Tm
{
    let seconds = (time & 0b0000000000011111) << 1;
    let minutes = (time & 0b0000011111100000) >> 5;
    let hours =   (time & 0b1111100000000000) >> 11;
    let days =    (date & 0b0000000000011111) >> 0;
    let months =  (date & 0b0000000111100000) >> 5;
    let years =   (date & 0b1111111000000000) >> 9;

    // Month range: Dos [1..12], Tm [0..11]
    // Year zero: Dos 1980, Tm 1900

    Tm {
        tm_sec: seconds as i32,
        tm_min: minutes as i32,
        tm_hour: hours as i32,
        tm_mday: days as i32,
        tm_mon: months as i32 - 1,
        tm_year: years as i32 + 80,
        ..empty_tm()
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
