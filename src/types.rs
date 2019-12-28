//! Types that specify what is contained in a ZIP.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum System
{
    Dos = 0,
    Unix = 3,
    Unknown,
    #[doc(hidden)]
    __Nonexhaustive,
}

impl System {
    pub fn from_u8(system: u8) -> System
    {
        use self::System::*;

        match system {
            0 => Dos,
            3 => Unix,
            _ => Unknown,
        }
    }
}

/// A DateTime field to be used for storing timestamps in a zip file
///
/// This structure does bounds checking to ensure the date is able to be stored in a zip file.
///
/// When constructed manually from a date and time, it will also check if the input is sensible
/// (e.g. months are from [1, 12]), but when read from a zip some parts may be out of their normal
/// bounds (e.g. month 0, or hour 31).
#[derive(Debug, Clone, Copy)]
pub struct DateTime {
    year: u16,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
}

impl ::std::default::Default for DateTime {
    /// Constructs an 'default' datetime of 1980-01-01 00:00:00
    fn default() -> DateTime {
        DateTime {
            year: 1980,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
        }
    }
}

impl DateTime {
    /// Converts an msdos (u16, u16) pair to a DateTime object
    pub fn from_msdos(datepart: u16, timepart: u16) -> DateTime {
        let seconds = (timepart & 0b0000000000011111) << 1;
        let minutes = (timepart & 0b0000011111100000) >> 5;
        let hours =   (timepart & 0b1111100000000000) >> 11;
        let days =    (datepart & 0b0000000000011111) >> 0;
        let months =  (datepart & 0b0000000111100000) >> 5;
        let years =   (datepart & 0b1111111000000000) >> 9;

        DateTime {
            year: (years + 1980) as u16,
            month: months as u8,
            day: days as u8,
            hour: hours as u8,
            minute: minutes as u8,
            second: seconds as u8,
        }
    }

    /// Constructs a DateTime from a specific date and time
    ///
    /// The bounds are:
    /// * year: [1980, 2107]
    /// * month: [1, 12]
    /// * day: [1, 31]
    /// * hour: [0, 23]
    /// * minute: [0, 59]
    /// * second: [0, 60]
    pub fn from_date_and_time(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> Result<DateTime, ()> {
        if year >= 1980 && year <= 2107
            && month >= 1 && month <= 12
            && day >= 1 && day <= 31
            && hour <= 23
            && minute <= 59
            && second <= 60
        {
            Ok(DateTime {
                year: year,
                month: month,
                day: day,
                hour: hour,
                minute: minute,
                second: second,
            })
        }
        else {
            Err(())
        }
    }

    #[cfg(feature = "time")]
    /// Converts a ::time::PrimitiveDateTime object to a DateTime
    ///
    /// Returns `Err` when this object is out of bounds
    pub fn from_time(tm: ::time::PrimitiveDateTime) -> Result<DateTime, ()> {
        if tm.year() >= 80 && tm.year() <= 207
            && tm.month() >= 1 && tm.month() <= 31
            && tm.day() >= 1 && tm.day() <= 31
            && tm.hour() <= 23
            && tm.minute() <= 59
            && tm.second() <= 60
        {
            Ok(DateTime {
                year: (tm.year()) as u16,
                month: (tm.month()) as u8,
                day: tm.day() as u8,
                hour: tm.hour() as u8,
                minute: tm.minute() as u8,
                second: tm.second() as u8,
            })
        }
        else {
            Err(())
        }
    }

    /// Gets the time portion of this datetime in the msdos representation
    pub fn timepart(&self) -> u16 {
        ((self.second as u16) >> 1) | ((self.minute as u16) << 5) | ((self.hour as u16) << 11)
    }

    /// Gets the date portion of this datetime in the msdos representation
    pub fn datepart(&self) -> u16 {
        (self.day as u16) | ((self.month as u16) << 5) | ((self.year - 1980) << 9)
    }

    #[cfg(feature = "time")]
    /// Converts the datetime to a PrimitiveDateTime structure
    ///
    /// The fields `tm_wday`, `tm_yday`, `tm_utcoff` and `tm_nsec` are set to their defaults.
    pub fn to_time(&self) -> Result<::time::PrimitiveDateTime, ()> {
        Ok(::time::PrimitiveDateTime::new(
            ::time::Date::try_from_ymd(self.year as i32, self.month as u8, self.day).map_err(|_| ())?,
            ::time::Time::try_from_hms(self.hour, self.minute, self.second).map_err(|_| ())?
        ))
    }

    /// Get the year. There is no epoch, i.e. 2018 will be returned as 2018.
    pub fn year(&self) -> u16 {
        self.year
    }

    /// Get the month, where 1 = january and 12 = december
    pub fn month(&self) -> u8 {
        self.month
    }

    /// Get the day
    pub fn day(&self) -> u8 {
        self.day
    }

    /// Get the hour
    pub fn hour(&self) -> u8 {
        self.hour
    }

    /// Get the minute
    pub fn minute(&self) -> u8 {
        self.minute
    }

    /// Get the second
    pub fn second(&self) -> u8 {
        self.second
    }
}

pub const DEFAULT_VERSION: u8 = 46;

/// Structure representing a ZIP file.
#[derive(Debug, Clone)]
pub struct ZipFileData
{
    /// Compatibility of the file attribute information
    pub system: System,
    /// Specification version
    pub version_made_by: u8,
    /// True if the file is encrypted.
    pub encrypted: bool,
    /// Compression method used to store the file
    pub compression_method: crate::compression::CompressionMethod,
    /// Last modified time. This will only have a 2 second precision.
    pub last_modified_time: DateTime,
    /// CRC32 checksum
    pub crc32: u32,
    /// Size of the file in the ZIP
    pub compressed_size: u64,
    /// Size of the file when extracted
    pub uncompressed_size: u64,
    /// Name of the file
    pub file_name: String,
    /// Raw file name. To be used when file_name was incorrectly decoded.
    pub file_name_raw: Vec<u8>,
    /// File comment
    pub file_comment: String,
    /// Specifies where the local header of the file starts
    pub header_start: u64,
    /// Specifies where the compressed data of the file starts
    pub data_start: u64,
    /// External file attributes
    pub external_attributes: u32,
}

impl ZipFileData {
    pub fn file_name_sanitized(&self) -> ::std::path::PathBuf {
        let no_null_filename = match self.file_name.find('\0') {
            Some(index) => &self.file_name[0..index],
            None => &self.file_name,
        }.to_string();

        // zip files can contain both / and \ as separators regardless of the OS
        // and as we want to return a sanitized PathBuf that only supports the
        // OS separator let's convert incompatible separators to compatible ones
        let separator = ::std::path::MAIN_SEPARATOR;
        let opposite_separator = match separator {
            '/' => '\\',
            '\\' | _ => '/',
        };
        let filename =
            no_null_filename.replace(&opposite_separator.to_string(), &separator.to_string());

        ::std::path::Path::new(&filename)
            .components()
            .filter(|component| match *component {
                ::std::path::Component::Normal(..) => true,
                _ => false,
            })
            .fold(::std::path::PathBuf::new(), |mut path, ref cur| {
                path.push(cur.as_os_str());
                path
            })
    }

    pub fn version_needed(&self) -> u16 {
        match self.compression_method {
            #[cfg(feature = "bzip2")]
            crate::compression::CompressionMethod::Bzip2 => 46,
            _ => 20,
        }
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn system() {
        use super::System;
        assert_eq!(System::Dos as u16, 0u16);
        assert_eq!(System::Unix as u16, 3u16);
        assert_eq!(System::from_u8(0), System::Dos);
        assert_eq!(System::from_u8(3), System::Unix);
    }

    #[test]
    fn sanitize() {
        use super::*;
        let file_name = "/path/../../../../etc/./passwd\0/etc/shadow".to_string();
        let data = ZipFileData {
            system: System::Dos,
            version_made_by: 0,
            encrypted: false,
            compression_method: crate::compression::CompressionMethod::Stored,
            last_modified_time: DateTime::default(),
            crc32: 0,
            compressed_size: 0,
            uncompressed_size: 0,
            file_name: file_name.clone(),
            file_name_raw: file_name.into_bytes(),
            file_comment: String::new(),
            header_start: 0,
            data_start: 0,
            external_attributes: 0,
        };
        assert_eq!(data.file_name_sanitized(), ::std::path::PathBuf::from("path/etc/passwd"));
    }

    #[test]
    fn datetime_default() {
        use super::DateTime;
        let dt = DateTime::default();
        assert_eq!(dt.timepart(), 0);
        assert_eq!(dt.datepart(), 0b0000000_0001_00001);
    }

    #[test]
    fn datetime_max() {
        use super::DateTime;
        let dt = DateTime::from_date_and_time(2107, 12, 31, 23, 59, 60).unwrap();
        assert_eq!(dt.timepart(), 0b10111_111011_11110);
        assert_eq!(dt.datepart(), 0b1111111_1100_11111);
    }

    #[test]
    fn datetime_bounds() {
        use super::DateTime;

        assert!(DateTime::from_date_and_time(2000, 1, 1, 23, 59, 60).is_ok());
        assert!(DateTime::from_date_and_time(2000, 1, 1, 24, 0, 0).is_err());
        assert!(DateTime::from_date_and_time(2000, 1, 1, 0, 60, 0).is_err());
        assert!(DateTime::from_date_and_time(2000, 1, 1, 0, 0, 61).is_err());

        assert!(DateTime::from_date_and_time(2107, 12, 31, 0, 0, 0).is_ok());
        assert!(DateTime::from_date_and_time(1980, 1, 1, 0, 0, 0).is_ok());
        assert!(DateTime::from_date_and_time(1979, 1, 1, 0, 0, 0).is_err());
        assert!(DateTime::from_date_and_time(1980, 0, 1, 0, 0, 0).is_err());
        assert!(DateTime::from_date_and_time(1980, 1, 0, 0, 0, 0).is_err());
        assert!(DateTime::from_date_and_time(2108, 12, 31, 0, 0, 0).is_err());
        assert!(DateTime::from_date_and_time(2107, 13, 31, 0, 0, 0).is_err());
        assert!(DateTime::from_date_and_time(2107, 12, 32, 0, 0, 0).is_err());
    }

    #[test]
    fn time_conversion() {
        use super::DateTime;
        let dt = DateTime::from_msdos(0x4D71, 0x54CF);
        assert_eq!(dt.year(), 2018);
        assert_eq!(dt.month(), 11);
        assert_eq!(dt.day(), 17);
        assert_eq!(dt.hour(), 10);
        assert_eq!(dt.minute(), 38);
        assert_eq!(dt.second(), 30);

        #[cfg(feature = "time")]
        assert_eq!(format!("{}", dt.to_time().unwrap().format("%Y-%m-%dT%H:%M:%SZ")), "2018-11-17T10:38:30Z");
    }

    #[test]
    fn time_out_of_bounds() {
        use super::DateTime;
        let dt = DateTime::from_msdos(0xFFFF, 0xFFFF);
        assert_eq!(dt.year(), 2107);
        assert_eq!(dt.month(), 15);
        assert_eq!(dt.day(), 31);
        assert_eq!(dt.hour(), 31);
        assert_eq!(dt.minute(), 63);
        assert_eq!(dt.second(), 62);

        #[cfg(feature = "time")]
        assert!(dt.to_time().is_err());

        let dt = DateTime::from_msdos(0x0000, 0x0000);
        assert_eq!(dt.year(), 1980);
        assert_eq!(dt.month(), 0);
        assert_eq!(dt.day(), 0);
        assert_eq!(dt.hour(), 0);
        assert_eq!(dt.minute(), 0);
        assert_eq!(dt.second(), 0);

        #[cfg(feature = "time")]
        assert!(dt.to_time().is_err());
    }
}
