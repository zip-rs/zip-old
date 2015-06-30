use time::{self, Tm};

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

    let tm = Tm {
        tm_sec: seconds as i32,
        tm_min: minutes as i32,
        tm_hour: hours as i32,
        tm_mday: days as i32,
        tm_mon: months as i32 - 1,
        tm_year: years as i32 + 80,
        ..time::empty_tm()
    };

    // Re-parse the possibly incorrect timestamp to get a correct one.
    // This every value will be in range
    time::at_utc(tm.to_timespec())
}

pub fn tm_to_msdos_time(time: Tm) -> u16
{
    ((time.tm_sec >> 1) | (time.tm_min << 5) | (time.tm_hour << 11)) as u16
}

pub fn tm_to_msdos_date(time: Tm) -> u16
{
    (time.tm_mday | ((time.tm_mon + 1) << 5) | ((time.tm_year - 80) << 9)) as u16
}

#[cfg(test)]
mod dos_tm_test {
    use super::*;
    use time::{self, Tm};

    fn check_date(input: Tm, day: i32, month: i32, year: i32) {
        assert_eq!(input.tm_mday, day);
        assert_eq!(input.tm_mon + 1, month);
        assert_eq!(input.tm_year + 1900, year);
    }

    fn check_time(input: Tm, hour: i32, minute: i32, second: i32) {
        assert_eq!(input.tm_hour, hour);
        assert_eq!(input.tm_min, minute);
        assert_eq!(input.tm_sec, second);
    }

    #[test]
    fn dos_zero() {
        // The 0 date is actually not a correct msdos date, but we
        // will parse it and adjust accordingly.
        let tm = msdos_datetime_to_tm(0, 0);
        check_date(tm, 30, 11, 1979);
        check_time(tm, 0, 0, 0);

        // This is the actual smallest date possible
        let tm = msdos_datetime_to_tm(0, 0b100001);
        check_date(tm, 1, 1, 1980);
        check_time(tm, 0, 0, 0);
    }

    #[test]
    fn dos_today() {
        let tm = msdos_datetime_to_tm(0b01001_100000_10101, 0b0100011_0110_11110);
        check_date(tm, 30, 6, 2015);
        check_time(tm, 9, 32, 42);
    }

    #[test]
    fn zero_dos() {
        let tm = Tm {
            tm_year: 80,
            tm_mon: 0,
            tm_mday: 1,
            tm_hour: 0,
            tm_min: 0,
            tm_sec: 0,
            ..time::empty_tm()
        };
        assert_eq!(tm_to_msdos_date(tm), 0b100001);
        assert_eq!(tm_to_msdos_time(tm), 0);
    }

    #[test]
    fn today_dos() {
        let tm = Tm {
            tm_year: 115,
            tm_mon: 5,
            tm_mday: 30,
            tm_hour: 9,
            tm_min: 32,
            tm_sec: 42,
            ..time::empty_tm()
        };
        assert_eq!(tm_to_msdos_date(tm), 0b0100011_0110_11110);
        assert_eq!(tm_to_msdos_time(tm), 0b01001_100000_10101);
    }
}
