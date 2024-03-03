use std::marker::PhantomData;

use super::{CentralHeaderVersion, ExtraFieldVersion, LocalHeaderVersion};

pub struct ExtendedTimestamp<V: ExtraFieldVersion> {
    flags: u8,
    mod_time: u32,
    ac_time: Option<u32>,
    cr_time: Option<u32>,
    _version: PhantomData<V>,
}

impl<V> ExtendedTimestamp<V> where V: ExtraFieldVersion {
    pub fn flags(&self) -> u8 {
        self.flags
    }

    pub fn mod_time(&self) -> u32 {
        self.mod_time
    }
}

impl ExtendedTimestamp<CentralHeaderVersion> {
    pub fn new_central(flags: u8, mod_time: u32) -> Self {
        Self {
            flags,
            mod_time,
            ac_time: None,
            cr_time: None,
            _version: PhantomData
        }
    }
}


impl ExtendedTimestamp<LocalHeaderVersion> {
    pub fn new_local(flags: u8, mod_time: u32, ac_time: u32, cr_time: u32) -> Self {
        Self {
            flags,
            mod_time,
            ac_time: Some(ac_time),
            cr_time: Some(cr_time),
            _version: PhantomData
        }
    }

    pub fn ac_time(&self) -> u32 {
        self.ac_time.unwrap()
    }


    pub fn cr_time(&self) -> u32 {
        self.cr_time.unwrap()
    }
}
