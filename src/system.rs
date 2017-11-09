#[derive(Clone, Copy, Debug, PartialEq)]
pub enum System {
    Dos = 0,
    Unix = 3,
    Unknown,
    #[doc(hidden)]
    __Nonexhaustive,
}

impl System {
    pub fn from_u8(system: u8) -> System {
        use self::System::*;

        match system {
            0 => Dos,
            3 => Unix,
            _ => Unknown,
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
}