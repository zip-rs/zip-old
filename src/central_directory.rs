use std::io::{Read, Seek};

use result::ZipResult;
use spec::END_OF_CENTRAL_DIRECTORY_RECORD;
use spec::ZIP64_END_OF_CENTRAL_DIRECTORY_LOCATOR;
use spec::ZIP64_END_OF_CENTRAL_DIRECTORY_RECORD;

pub struct CentralDirectory {
    pub disk_number: u32,
    pub disk_with_central_directory: u32,
    pub number_of_files_on_this_disk: u64,
    pub number_of_files: u64,
    pub size: u64,
    pub offset: u64,
}
impl CentralDirectory {
    pub fn new() -> Self {
        CentralDirectory {
            disk_number: 0,
            disk_with_central_directory: 0,
            number_of_files_on_this_disk: 0,
            number_of_files: 0,
            size: 0,
            offset: 0,
        }
    }

    pub fn load<T: Read + Seek>(reader: &mut T) -> ZipResult<CentralDirectory> {
        let mut directory = CentralDirectory::new();
        let (zip32, pos) = END_OF_CENTRAL_DIRECTORY_RECORD::load(reader)?;
        if let Some(loc) = ZIP64_END_OF_CENTRAL_DIRECTORY_LOCATOR::load(reader, &pos).ok() {
            let pos = loc.central_directory_offset;
            let zip64 = ZIP64_END_OF_CENTRAL_DIRECTORY_RECORD::load(reader, &pos)?;
            directory.disk_number = zip64.disk_number;
            directory.disk_with_central_directory = zip64.disk_with_central_directory;
            directory.number_of_files_on_this_disk = zip64.number_of_files_on_this_disk;
            directory.number_of_files = zip64.number_of_files;
            directory.size = zip64.central_directory_size;
            directory.offset = zip64.central_directory_offset;
        } else {
            directory.disk_number = zip32.disk_number as u32;
            directory.disk_with_central_directory = zip32.disk_with_central_directory as u32;
            directory.number_of_files_on_this_disk = zip32.number_of_files_on_this_disk as u64;
            directory.number_of_files = zip32.number_of_files as u64;
            directory.size = zip32.central_directory_size as u64;
            directory.offset = zip32.central_directory_offset as u64;
        }
        Ok(directory)
    }
}
