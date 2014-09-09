use time;
use time::Tm;

pub fn msdos_datetime_to_tm(time: u16, date: u16) -> Tm
{
    let seconds = (time & 0b0000000000011111) << 1;
    let minutes = (time & 0b0000011111100000) >> 5;
    let hours =   (time & 0b1111100000000000) >> 11;
    let days =    (date & 0b0000000000011111) >> 0;
    let months =  (date & 0b0000000111100000) >> 5;
    let years =   (date & 0b1111111000000000) >> 9;

    let datetime = format!("{:04u}-{:02u}-{:02u} {:02u}:{:02u}:{:02u}",
                           years as uint + 1980,
                           months,
                           days,
                           hours,
                           minutes,
                           seconds);
    let format = "%Y-%m-%d %H:%M:%S";

    match time::strptime(datetime.as_slice(), format)
    {
        Ok(tm) => tm,
        Err(m) => { debug!("Failed parsing date: {}", m); time::empty_tm() },
    }
}
