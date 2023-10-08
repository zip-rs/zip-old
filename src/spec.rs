use crate::result::{ZipError, ZipResult};

use byteorder::{ByteOrder, LittleEndian, ReadBytesExt, WriteBytesExt};
use tokio::io::{self, AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

use std::io::prelude::*;
use std::io::Seek;
use std::pin::Pin;
use std::{cmp, io::IoSlice, mem, slice};

pub const LOCAL_FILE_HEADER_SIGNATURE: u32 = 0x04034b50;
pub const CENTRAL_DIRECTORY_HEADER_SIGNATURE: u32 = 0x02014b50;
const CENTRAL_DIRECTORY_END_SIGNATURE: u32 = 0x06054b50;
pub const ZIP64_CENTRAL_DIRECTORY_END_SIGNATURE: u32 = 0x06064b50;
const ZIP64_CENTRAL_DIRECTORY_END_LOCATOR_SIGNATURE: u32 = 0x07064b50;

pub const ZIP64_BYTES_THR: u64 = u32::MAX as u64;
pub const ZIP64_ENTRY_THR: usize = u16::MAX as usize;

pub struct CentralDirectoryEnd {
    pub disk_number: u16,
    pub disk_with_central_directory: u16,
    pub number_of_files_on_this_disk: u16,
    pub number_of_files: u16,
    pub central_directory_size: u32,
    pub central_directory_offset: u32,
    pub zip_file_comment: Vec<u8>,
}

#[repr(packed)]
struct CentralDirectoryEndBuffer {
    pub magic: u32,
    pub disk_number: u16,
    pub disk_with_central_directory: u16,
    pub number_of_files_on_this_disk: u16,
    pub number_of_files: u16,
    pub central_directory_size: u32,
    pub central_directory_offset: u32,
    pub zip_file_comment_length: u16,
}

impl CentralDirectoryEndBuffer {
    #[inline]
    pub fn extract(mut info: [u8; mem::size_of::<Self>()]) -> Self {
        let start: *mut u8 = info.as_mut_ptr();

        LittleEndian::from_slice_u16(unsafe { slice::from_raw_parts_mut(start as *mut u16, 11) });

        unsafe { mem::transmute(info) }
    }

    #[inline]
    pub fn writable_block(self) -> [u8; mem::size_of::<Self>()] {
        let mut buf: [u8; mem::size_of::<Self>()] = unsafe { mem::transmute(self) };

        LittleEndian::from_slice_u16(unsafe {
            slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u16, 11)
        });

        buf
    }
}

impl CentralDirectoryEnd {
    // Per spec 4.4.1.4 - a CentralDirectoryEnd field might be insufficient to hold the
    // required data. In this case the file SHOULD contain a ZIP64 format record
    // and the field of this record will be set to -1
    #[inline]
    pub(crate) fn record_too_small(&self) -> bool {
        self.disk_number == 0xFFFF
            || self.disk_with_central_directory == 0xFFFF
            || self.number_of_files_on_this_disk == 0xFFFF
            || self.number_of_files == 0xFFFF
            || self.central_directory_size == 0xFFFFFFFF
            || self.central_directory_offset == 0xFFFFFFFF
    }

    pub fn parse<T: Read>(reader: &mut T) -> ZipResult<CentralDirectoryEnd> {
        let magic = reader.read_u32::<LittleEndian>()?;
        if magic != CENTRAL_DIRECTORY_END_SIGNATURE {
            return Err(ZipError::InvalidArchive("Invalid digital signature header"));
        }
        let disk_number = reader.read_u16::<LittleEndian>()?;
        let disk_with_central_directory = reader.read_u16::<LittleEndian>()?;
        let number_of_files_on_this_disk = reader.read_u16::<LittleEndian>()?;
        let number_of_files = reader.read_u16::<LittleEndian>()?;
        let central_directory_size = reader.read_u32::<LittleEndian>()?;
        let central_directory_offset = reader.read_u32::<LittleEndian>()?;
        let zip_file_comment_length = reader.read_u16::<LittleEndian>()? as usize;
        let mut zip_file_comment = vec![0; zip_file_comment_length];
        reader.read_exact(&mut zip_file_comment)?;

        Ok(CentralDirectoryEnd {
            disk_number,
            disk_with_central_directory,
            number_of_files_on_this_disk,
            number_of_files,
            central_directory_size,
            central_directory_offset,
            zip_file_comment,
        })
    }

    pub async fn parse_async<T: io::AsyncRead>(mut reader: Pin<&mut T>) -> ZipResult<Self> {
        static_assertions::assert_eq_size!([u8; 22], CentralDirectoryEndBuffer);
        let mut info = [0u8; 22];
        reader.read_exact(&mut info[..]).await?;

        let CentralDirectoryEndBuffer {
            magic,
            disk_number,
            disk_with_central_directory,
            number_of_files_on_this_disk,
            number_of_files,
            central_directory_size,
            central_directory_offset,
            zip_file_comment_length,
        } = CentralDirectoryEndBuffer::extract(info);

        if magic != CENTRAL_DIRECTORY_END_SIGNATURE {
            return Err(ZipError::InvalidArchive("Invalid digital signature header"));
        }

        let mut zip_file_comment = vec![0u8; zip_file_comment_length.into()];
        reader.read_exact(&mut zip_file_comment).await?;

        Ok(CentralDirectoryEnd {
            disk_number,
            disk_with_central_directory,
            number_of_files_on_this_disk,
            number_of_files,
            central_directory_size,
            central_directory_offset,
            zip_file_comment,
        })
    }

    const HEADER_SIZE: u64 = 22;
    const BYTES_BETWEEN_MAGIC_AND_COMMENT_SIZE: u64 = Self::HEADER_SIZE - 6;

    pub const SEARCH_BUFFER_SIZE: u64 = 10 * Self::HEADER_SIZE;

    pub async fn find_and_parse_async<T: io::AsyncRead + io::AsyncSeek>(
        mut reader: Pin<&mut T>,
    ) -> ZipResult<(CentralDirectoryEnd, u64)> {
        let file_length = reader.seek(io::SeekFrom::End(0)).await?;

        if file_length < Self::HEADER_SIZE {
            return Err(ZipError::InvalidArchive("Invalid zip header"));
        }

        let search_lower_bound =
            file_length.saturating_sub(Self::HEADER_SIZE + ::std::u16::MAX as u64);
        dbg!(search_lower_bound);

        dbg!(Self::SEARCH_BUFFER_SIZE);
        let mut buf = [0u8; Self::SEARCH_BUFFER_SIZE as usize];
        let sig: [u8; 4] = CENTRAL_DIRECTORY_END_SIGNATURE.to_le_bytes();
        dbg!(sig);

        let mut leftmost_frontier = file_length;
        while leftmost_frontier > search_lower_bound {
            dbg!(leftmost_frontier);
            let remaining = leftmost_frontier - search_lower_bound;
            dbg!(remaining);
            let cur_len = cmp::min(remaining, Self::SEARCH_BUFFER_SIZE);
            dbg!(cur_len);
            let cur_buf: &mut [u8] = &mut buf[..cur_len as usize];

            reader
                .seek(io::SeekFrom::Current(-(cur_len as i64)))
                .await?;
            reader.read_exact(cur_buf).await?;

            /* dbg!(&cur_buf); */

            if let Some(index_within_buffer) = memchr2::memmem::rfind(&cur_buf, &sig[..]) {
                dbg!(index_within_buffer);
                let central_directory_end =
                    leftmost_frontier - cur_len + index_within_buffer as u64;
                dbg!(central_directory_end);

                reader
                    .seek(io::SeekFrom::Start(central_directory_end))
                    .await?;

                return CentralDirectoryEnd::parse_async(reader)
                    .await
                    .map(|cde| (cde, central_directory_end));
            } else {
                leftmost_frontier -= cur_len;
            }
        }
        Err(ZipError::InvalidArchive(
            "Could not find central directory end",
        ))
    }

    pub fn find_and_parse<T: Read + Seek>(reader: &mut T) -> ZipResult<(CentralDirectoryEnd, u64)> {
        let file_length = reader.seek(io::SeekFrom::End(0))?;

        let search_upper_bound =
            file_length.saturating_sub(Self::HEADER_SIZE + ::std::u16::MAX as u64);

        if file_length < Self::HEADER_SIZE {
            return Err(ZipError::InvalidArchive("Invalid zip header"));
        }

        let mut pos = file_length - Self::HEADER_SIZE;
        while pos >= search_upper_bound {
            reader.seek(io::SeekFrom::Start(pos))?;
            if reader.read_u32::<LittleEndian>()? == CENTRAL_DIRECTORY_END_SIGNATURE {
                reader.seek(io::SeekFrom::Current(
                    Self::BYTES_BETWEEN_MAGIC_AND_COMMENT_SIZE as i64,
                ))?;
                let cde_start_pos = reader.seek(io::SeekFrom::Start(pos))?;
                return CentralDirectoryEnd::parse(reader).map(|cde| (cde, cde_start_pos));
            }
            pos = match pos.checked_sub(1) {
                Some(p) => p,
                None => break,
            };
        }
        Err(ZipError::InvalidArchive(
            "Could not find central directory end",
        ))
    }

    pub fn write<T: Write>(&self, writer: &mut T) -> ZipResult<()> {
        writer.write_u32::<LittleEndian>(CENTRAL_DIRECTORY_END_SIGNATURE)?;
        writer.write_u16::<LittleEndian>(self.disk_number)?;
        writer.write_u16::<LittleEndian>(self.disk_with_central_directory)?;
        writer.write_u16::<LittleEndian>(self.number_of_files_on_this_disk)?;
        writer.write_u16::<LittleEndian>(self.number_of_files)?;
        writer.write_u32::<LittleEndian>(self.central_directory_size)?;
        writer.write_u32::<LittleEndian>(self.central_directory_offset)?;
        writer.write_u16::<LittleEndian>(self.zip_file_comment.len() as u16)?;
        writer.write_all(&self.zip_file_comment)?;
        Ok(())
    }

    pub async fn write_async<T: io::AsyncWrite>(&self, mut writer: Pin<&mut T>) -> ZipResult<()> {
        let block = CentralDirectoryEndBuffer {
            magic: CENTRAL_DIRECTORY_END_SIGNATURE,
            disk_number: self.disk_number,
            disk_with_central_directory: self.disk_with_central_directory,
            number_of_files_on_this_disk: self.number_of_files_on_this_disk,
            number_of_files: self.number_of_files,
            central_directory_size: self.central_directory_size,
            central_directory_offset: self.central_directory_offset,
            zip_file_comment_length: self.zip_file_comment.len() as u16,
        }
        .writable_block();

        if writer.is_write_vectored() {
            /* TODO: zero-copy!! */
            let block = IoSlice::new(&block);
            let comment = IoSlice::new(&self.zip_file_comment);
            writer.write_vectored(&[block, comment]).await?;
        } else {
            /* If no special vector write support, just perform two separate writes. */
            writer.write_all(&block).await?;
            writer.write_all(&self.zip_file_comment).await?;
        }

        Ok(())
    }
}

pub struct Zip64CentralDirectoryEndLocator {
    pub disk_with_central_directory: u32,
    pub end_of_central_directory_offset: u64,
    pub number_of_disks: u32,
}

#[repr(packed)]
struct Zip64CentralDirectoryEndLocatorBuffer {
    pub magic: u32,
    pub disk_with_central_directory: u32,
    pub end_of_central_directory_offset: u64,
    pub number_of_disks: u32,
}

impl Zip64CentralDirectoryEndLocatorBuffer {
    #[inline]
    pub fn extract(mut info: [u8; mem::size_of::<Self>()]) -> Self {
        let start: *mut u8 = info.as_mut_ptr();

        LittleEndian::from_slice_u32(unsafe { slice::from_raw_parts_mut(start as *mut u32, 5) });

        unsafe { mem::transmute(info) }
    }

    #[inline]
    pub fn writable_block(self) -> [u8; mem::size_of::<Self>()] {
        let mut buf: [u8; mem::size_of::<Self>()] = unsafe { mem::transmute(self) };

        LittleEndian::from_slice_u32(unsafe {
            slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u32, 5)
        });

        buf
    }
}

impl Zip64CentralDirectoryEndLocator {
    pub fn parse<T: Read>(reader: &mut T) -> ZipResult<Zip64CentralDirectoryEndLocator> {
        let magic = reader.read_u32::<LittleEndian>()?;
        if magic != ZIP64_CENTRAL_DIRECTORY_END_LOCATOR_SIGNATURE {
            return Err(ZipError::InvalidArchive(
                "Invalid zip64 locator digital signature header",
            ));
        }
        let disk_with_central_directory = reader.read_u32::<LittleEndian>()?;
        let end_of_central_directory_offset = reader.read_u64::<LittleEndian>()?;
        let number_of_disks = reader.read_u32::<LittleEndian>()?;

        Ok(Zip64CentralDirectoryEndLocator {
            disk_with_central_directory,
            end_of_central_directory_offset,
            number_of_disks,
        })
    }

    pub async fn parse_async<T: io::AsyncRead>(mut reader: Pin<&mut T>) -> ZipResult<Self> {
        static_assertions::assert_eq_size!([u8; 20], Zip64CentralDirectoryEndLocatorBuffer);
        let mut info = [0u8; 20];
        reader.read_exact(&mut info[..]).await?;

        let Zip64CentralDirectoryEndLocatorBuffer {
            magic,
            disk_with_central_directory,
            end_of_central_directory_offset,
            number_of_disks,
        } = Zip64CentralDirectoryEndLocatorBuffer::extract(info);

        if magic != ZIP64_CENTRAL_DIRECTORY_END_LOCATOR_SIGNATURE {
            return Err(ZipError::InvalidArchive(
                "Invalid zip64 locator digital signature header",
            ));
        }

        Ok(Zip64CentralDirectoryEndLocator {
            disk_with_central_directory,
            end_of_central_directory_offset,
            number_of_disks,
        })
    }

    pub fn write<T: Write>(&self, writer: &mut T) -> ZipResult<()> {
        writer.write_u32::<LittleEndian>(ZIP64_CENTRAL_DIRECTORY_END_LOCATOR_SIGNATURE)?;
        writer.write_u32::<LittleEndian>(self.disk_with_central_directory)?;
        writer.write_u64::<LittleEndian>(self.end_of_central_directory_offset)?;
        writer.write_u32::<LittleEndian>(self.number_of_disks)?;
        Ok(())
    }

    pub async fn write_async<T: io::AsyncWrite>(&self, mut writer: Pin<&mut T>) -> ZipResult<()> {
        let block = Zip64CentralDirectoryEndLocatorBuffer {
            magic: ZIP64_CENTRAL_DIRECTORY_END_LOCATOR_SIGNATURE,
            disk_with_central_directory: self.disk_with_central_directory,
            end_of_central_directory_offset: self.end_of_central_directory_offset,
            number_of_disks: self.number_of_disks,
        }
        .writable_block();

        if writer.is_write_vectored() {
            /* TODO: zero-copy?? */
            let block = IoSlice::new(&block);
            writer.write_vectored(&[block]).await?;
        } else {
            /* If no special vector write support, just perform a normal write. */
            writer.write_all(&block).await?;
        }

        Ok(())
    }
}

pub struct Zip64CentralDirectoryEnd {
    pub version_made_by: u16,
    pub version_needed_to_extract: u16,
    pub disk_number: u32,
    pub disk_with_central_directory: u32,
    pub number_of_files_on_this_disk: u64,
    pub number_of_files: u64,
    pub central_directory_size: u64,
    pub central_directory_offset: u64,
    //pub extensible_data_sector: Vec<u8>, <-- We don't do anything with this at the moment.
}

#[repr(packed)]
struct Zip64CentralDirectoryEndBuffer {
    pub magic: u32,
    pub record_size: u64, /* this should always be 44! */
    pub version_made_by: u16,
    pub version_needed_to_extract: u16,
    pub disk_number: u32,
    pub disk_with_central_directory: u32,
    pub number_of_files_on_this_disk: u64,
    pub number_of_files: u64,
    pub central_directory_size: u64,
    pub central_directory_offset: u64,
}

impl Zip64CentralDirectoryEndBuffer {
    #[inline]
    pub fn extract(mut info: [u8; mem::size_of::<Self>()]) -> Self {
        let start: *mut u8 = info.as_mut_ptr();

        LittleEndian::from_slice_u32(unsafe { slice::from_raw_parts_mut(start as *mut u32, 14) });

        unsafe { mem::transmute(info) }
    }

    #[inline]
    pub fn writable_block(self) -> [u8; mem::size_of::<Self>()] {
        let mut buf: [u8; mem::size_of::<Self>()] = unsafe { mem::transmute(self) };

        LittleEndian::from_slice_u32(unsafe {
            slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u32, 14)
        });

        buf
    }
}

impl Zip64CentralDirectoryEnd {
    pub fn find_and_parse<T: Read + Seek>(
        reader: &mut T,
        nominal_offset: u64,
        search_upper_bound: u64,
    ) -> ZipResult<(Zip64CentralDirectoryEnd, u64)> {
        let mut pos = nominal_offset;

        while pos <= search_upper_bound {
            reader.seek(io::SeekFrom::Start(pos))?;

            if reader.read_u32::<LittleEndian>()? == ZIP64_CENTRAL_DIRECTORY_END_SIGNATURE {
                let archive_offset = pos - nominal_offset;

                let _record_size = reader.read_u64::<LittleEndian>()?;
                // We would use this value if we did anything with the "zip64 extensible data sector".

                let version_made_by = reader.read_u16::<LittleEndian>()?;
                let version_needed_to_extract = reader.read_u16::<LittleEndian>()?;
                let disk_number = reader.read_u32::<LittleEndian>()?;
                let disk_with_central_directory = reader.read_u32::<LittleEndian>()?;
                let number_of_files_on_this_disk = reader.read_u64::<LittleEndian>()?;
                let number_of_files = reader.read_u64::<LittleEndian>()?;
                let central_directory_size = reader.read_u64::<LittleEndian>()?;
                let central_directory_offset = reader.read_u64::<LittleEndian>()?;

                return Ok((
                    Zip64CentralDirectoryEnd {
                        version_made_by,
                        version_needed_to_extract,
                        disk_number,
                        disk_with_central_directory,
                        number_of_files_on_this_disk,
                        number_of_files,
                        central_directory_size,
                        central_directory_offset,
                    },
                    archive_offset,
                ));
            }

            pos += 1;
        }

        Err(ZipError::InvalidArchive(
            "Could not find ZIP64 central directory end",
        ))
    }

    pub async fn parse_async<T: io::AsyncRead>(mut reader: Pin<&mut T>) -> ZipResult<Self> {
        static_assertions::assert_eq_size!([u8; 56], Zip64CentralDirectoryEndBuffer);
        let mut info = [0u8; 56];
        reader.read_exact(&mut info[..]).await?;

        let Zip64CentralDirectoryEndBuffer {
            magic,
            record_size,
            version_made_by,
            version_needed_to_extract,
            disk_number,
            disk_with_central_directory,
            number_of_files_on_this_disk,
            number_of_files,
            central_directory_size,
            central_directory_offset,
        } = Zip64CentralDirectoryEndBuffer::extract(info);

        assert_eq!(record_size, 44);

        if magic != ZIP64_CENTRAL_DIRECTORY_END_SIGNATURE {
            return Err(ZipError::InvalidArchive("Invalid digital signature header"));
        }

        Ok(Zip64CentralDirectoryEnd {
            version_made_by,
            version_needed_to_extract,
            disk_number,
            disk_with_central_directory,
            number_of_files_on_this_disk,
            number_of_files,
            central_directory_size,
            central_directory_offset,
        })
    }

    const ZIP64_SEARCH_BUFFER_SIZE: u64 = 2 * CentralDirectoryEnd::SEARCH_BUFFER_SIZE;

    pub async fn find_and_parse_async<T: io::AsyncRead + io::AsyncSeek>(
        mut reader: Pin<&mut T>,
        nominal_offset: u64,
        search_upper_bound: u64,
    ) -> ZipResult<(Self, u64)> {
        let mut rightmost_frontier = reader.seek(io::SeekFrom::Start(nominal_offset)).await?;

        let mut buf = [0u8; Self::ZIP64_SEARCH_BUFFER_SIZE as usize];

        let sig: [u8; 4] = ZIP64_CENTRAL_DIRECTORY_END_SIGNATURE.to_le_bytes();

        while rightmost_frontier <= search_upper_bound {
            let remaining = search_upper_bound - rightmost_frontier;
            let cur_len = cmp::min(remaining, Self::ZIP64_SEARCH_BUFFER_SIZE);
            let cur_buf: &mut [u8] = &mut buf[..cur_len as usize];

            reader.read_exact(cur_buf).await?;

            if let Some(index_within_buffer) = memchr2::memmem::find(&cur_buf, &sig[..]) {
                let zip64_central_directory_end = rightmost_frontier + index_within_buffer as u64;

                reader
                    .seek(io::SeekFrom::Start(zip64_central_directory_end))
                    .await?;

                let archive_offset = zip64_central_directory_end - nominal_offset;
                return Zip64CentralDirectoryEnd::parse_async(reader)
                    .await
                    .map(|cde| (cde, archive_offset));
            } else {
                rightmost_frontier += cur_len;
            }
        }

        Err(ZipError::InvalidArchive(
            "Could not find ZIP64 central directory end",
        ))
    }

    pub fn write<T: Write>(&self, writer: &mut T) -> ZipResult<()> {
        writer.write_u32::<LittleEndian>(ZIP64_CENTRAL_DIRECTORY_END_SIGNATURE)?;
        writer.write_u64::<LittleEndian>(44)?; // record size
        writer.write_u16::<LittleEndian>(self.version_made_by)?;
        writer.write_u16::<LittleEndian>(self.version_needed_to_extract)?;
        writer.write_u32::<LittleEndian>(self.disk_number)?;
        writer.write_u32::<LittleEndian>(self.disk_with_central_directory)?;
        writer.write_u64::<LittleEndian>(self.number_of_files_on_this_disk)?;
        writer.write_u64::<LittleEndian>(self.number_of_files)?;
        writer.write_u64::<LittleEndian>(self.central_directory_size)?;
        writer.write_u64::<LittleEndian>(self.central_directory_offset)?;
        Ok(())
    }

    pub async fn write_async<T: io::AsyncWrite>(&self, mut writer: Pin<&mut T>) -> ZipResult<()> {
        let block = Zip64CentralDirectoryEndBuffer {
            magic: ZIP64_CENTRAL_DIRECTORY_END_SIGNATURE,
            record_size: 44,
            version_made_by: self.version_made_by,
            version_needed_to_extract: self.version_needed_to_extract,
            disk_number: self.disk_number,
            disk_with_central_directory: self.disk_with_central_directory,
            number_of_files_on_this_disk: self.number_of_files_on_this_disk,
            number_of_files: self.number_of_files,
            central_directory_size: self.central_directory_size,
            central_directory_offset: self.central_directory_offset,
        }
        .writable_block();

        if writer.is_write_vectored() {
            /* TODO: zero-copy?? */
            let block = IoSlice::new(&block);
            writer.write_vectored(&[block]).await?;
        } else {
            /* If no special vector write support, just perform a normal write. */
            writer.write_all(&block).await?;
        }

        Ok(())
    }
}

#[repr(packed)]
pub struct LocalHeaderBuffer {
    // local file header signature
    pub magic: u32,
    // version needed to extract
    pub version_needed_to_extract: u16,
    // general purpose bit flag
    pub flag: u16,
    // Compression method
    pub compression_method: u16,
    // last mod file time and last mod file date
    pub last_modified_time_timepart: u16,
    pub last_modified_time_datepart: u16,
    // crc-32
    pub crc32: u32,
    // compressed size and uncompressed size
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    // file name length
    pub file_name_length: u16,
    // extra field length
    pub extra_field_length: u16,
}

impl LocalHeaderBuffer {
    #[inline]
    pub fn extract(mut info: [u8; mem::size_of::<Self>()]) -> Self {
        let start: *mut u8 = info.as_mut_ptr();

        LittleEndian::from_slice_u16(unsafe { slice::from_raw_parts_mut(start as *mut u16, 15) });

        unsafe { mem::transmute(info) }
    }

    #[inline]
    pub fn writable_block(self) -> [u8; mem::size_of::<Self>()] {
        let mut buf: [u8; mem::size_of::<Self>()] = unsafe { mem::transmute(self) };

        LittleEndian::from_slice_u16(unsafe {
            slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u16, 15)
        });

        buf
    }
}

#[repr(packed)]
pub struct CentralDirectoryHeaderBuffer {
    pub magic: u32,                       /* 2 */
    pub version_made_by: u16,             /* 1 */
    pub version_needed: u16,              /* 1 */
    pub flag: u16,                        /* 1 */
    pub compression_method: u16,          /* 1 */
    pub last_modified_time_timepart: u16, /* 1 */
    pub last_modified_time_datepart: u16, /* 1 */
    pub crc32: u32,                       /* 2 */
    pub compressed_size: u32,             /* 2 */
    pub uncompressed_size: u32,           /* 2 */
    pub file_name_length: u16,            /* 1 */
    pub extra_field_length: u16,          /* 1 */
    pub file_comment_length: u16,         /* 1 */
    pub disk_number_start: u16,           /* 1 */
    pub internal_attributes: u16,         /* 1 */
    pub external_attributes: u32,         /* 2 */
    pub header_start: u32,                /* 2 */
}

impl CentralDirectoryHeaderBuffer {
    #[inline]
    pub fn extract(mut info: [u8; mem::size_of::<Self>()]) -> Self {
        let start: *mut u8 = info.as_mut_ptr();

        LittleEndian::from_slice_u16(unsafe { slice::from_raw_parts_mut(start as *mut u16, 22) });

        unsafe { mem::transmute(info) }
    }

    #[inline]
    pub fn writable_block(self) -> [u8; mem::size_of::<Self>()] {
        let mut buf: [u8; mem::size_of::<Self>()] = unsafe { mem::transmute(self) };

        LittleEndian::from_slice_u16(unsafe {
            slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u16, 22)
        });

        buf
    }
}
