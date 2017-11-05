mod end_of_central_directory_record;
pub use spec::end_of_central_directory_record::*;
mod zip64_end_of_central_directory_locator;
pub use spec::zip64_end_of_central_directory_locator::*;
mod zip64_end_of_central_directory_record;
pub use spec::zip64_end_of_central_directory_record::*;

pub static LOCAL_FILE_HEADER_SIGNATURE: u32 = 0x04034b50;
pub static CENTRAL_DIRECTORY_HEADER_SIGNATURE: u32 = 0x02014b50;
