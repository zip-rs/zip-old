use std::io;
use std::io::prelude::*;
use result::{ZipError, ZipResult};
use podio::{LittleEndian, ReadPodExt};

/*
Local file header
      local file header signature     4 bytes  (0x04034b50)
      version needed to extract       2 bytes
      general purpose bit flag        2 bytes
      compression method              2 bytes
      last mod file time              2 bytes
      last mod file date              2 bytes
      crc-32                          4 bytes
      compressed size                 4 bytes
      uncompressed size               4 bytes
      file name length                2 bytes
      extra field length              2 bytes
      file name (variable size)
      extra field (variable size)
*/

#[derive(Debug)]
#[allow(non_camel_case_types)]
pub struct LOCAL_FILE_HEADER {
    pub version_to_extract: u16,
    pub general_purpose_flag: u16,
    pub compression_method: u16,
    pub last_mod_time: u16,
    pub last_mod_date: u16,
    pub crc32: u32,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub file_name_length: u16,
    pub extra_field_length: u16,
    pub file_name: Vec<u8>,
    pub extra_field: Vec<u8>,
}
impl LOCAL_FILE_HEADER {
    pub fn signature() -> u32 {
        0x04034b50
    }

    pub fn name() -> &'static str {
        "[Local file header]"
    }

    pub fn load<T>(reader: &mut T) -> ZipResult<Self>
    where
        T: Read + Seek,
    {
        if Self::signature() != reader.read_u32::<LittleEndian>()? {
            return Err(ZipError::SectionNotFound(Self::name()));
        }
        let mut header = LOCAL_FILE_HEADER {
            version_to_extract: reader.read_u16::<LittleEndian>()?,
            general_purpose_flag: reader.read_u16::<LittleEndian>()?,
            compression_method: reader.read_u16::<LittleEndian>()?,
            last_mod_time: reader.read_u16::<LittleEndian>()?,
            last_mod_date: reader.read_u16::<LittleEndian>()?,
            crc32: reader.read_u32::<LittleEndian>()?,
            compressed_size: reader.read_u32::<LittleEndian>()? as u64,
            uncompressed_size: reader.read_u32::<LittleEndian>()? as u64,
            file_name_length: reader.read_u16::<LittleEndian>()?,
            extra_field_length: reader.read_u16::<LittleEndian>()?,
            file_name: Vec::new(),
            extra_field: Vec::new(),
        };

        header.file_name = ReadPodExt::read_exact(reader, header.file_name_length as usize)?;

        let mut rest_of_bytes = header.extra_field_length as i64;
        while rest_of_bytes > 0 {
            let extra_field = EXTRA_FIELD::load(reader)?;
            rest_of_bytes = rest_of_bytes - 4 - extra_field.data_size as i64;
            if 0x0001 == extra_field.header_id {
                if 0xFFFFFFFF == header.uncompressed_size {
                    header.uncompressed_size = reader.read_u64::<LittleEndian>()?;
                }
                if 0xFFFFFFFF == header.compressed_size {
                    header.compressed_size = reader.read_u64::<LittleEndian>()?;
                }
            } else {
                reader.seek(
                    io::SeekFrom::Current(extra_field.data_size as i64),
                )?;
            }
        }
        if rest_of_bytes < 0 {
            return Err(ZipError::InvalidArchive(
                "The [Local file header] contains wrong [extensible data fields]",
            ));
        }
        return Ok(header);
    }
}

/*
Extensible data fields
--------------------------
   The following structure MUST be used for all programs storing data in this field:

       header1+data1 + header2+data2 . . .

   Each header should consist of:

       Header ID - 2 bytes
       Data Size - 2 bytes

   Note: all fields stored in Intel low-byte/high-byte order.

   The Header ID field indicates the type of data that is in the following data block.

   Header IDs of 0 thru 31 are reserved for use by PKWARE.
   The remaining IDs can be used by third party vendors for proprietary usage.

   The current Header ID mappings defined by PKWARE are:

      0x0001        Zip64 extended information extra field
      0x0007        AV Info
      0x0008        Reserved for extended language encoding data (PFS)
                    (see APPENDIX D)
      0x0009        OS/2
      0x000a        NTFS 
      0x000c        OpenVMS
      0x000d        UNIX
      0x000e        Reserved for file stream and fork descriptors
      0x000f        Patch Descriptor
      0x0014        PKCS#7 Store for X.509 Certificates
      0x0015        X.509 Certificate ID and Signature for 
                    individual file
      0x0016        X.509 Certificate ID for Central Directory
      0x0017        Strong Encryption Header
      0x0018        Record Management Controls
      0x0019        PKCS#7 Encryption Recipient Certificate List
      0x0065        IBM S/390 (Z390), AS/400 (I400) attributes 
                    - uncompressed
      0x0066        Reserved for IBM S/390 (Z390), AS/400 (I400) 
                    attributes - compressed
      0x4690        POSZIP 4690 (reserved) 

Zip64 Extended Information Extra Field (0x0001):
      The following is the layout of the zip64 extended information "extra" block. If one of the size or
      offset fields in the Local or Central directory record is too small to hold the required data,a Zip64 
      extended information record is created.
      The order of the fields in the zip64 extended information record is fixed, but the fields MUST only 
      appear if the corresponding Local or Central directory record field is set to 0xFFFF or 0xFFFFFFFF.

      Note: all fields stored in Intel low-byte/high-byte order.

        Value      Size       Description
        -----      ----       -----------
(ZIP64) 0x0001     2 bytes    Tag for this "extra" block type
        Size       2 bytes    Size of this "extra" block
        Original 
        Size       8 bytes    Original uncompressed file size
        Compressed
        Size       8 bytes    Size of compressed data
        Relative Header
        Offset     8 bytes    Offset of local header record
        Disk Start
        Number     4 bytes    Number of the disk on which
                              this file starts 

      This entry in the Local header MUST include BOTH original and compressed file size fields. If encrypting the 
      central directory and bit 13 of the general purpose bit flag is set indicating masking, the value stored in the
      Local Header for the original file size will be zero.      
*/

#[derive(Debug)]
#[allow(non_camel_case_types)]
struct EXTRA_FIELD {
    pub header_id: u16,
    pub data_size: u16,
}
impl EXTRA_FIELD {
    pub fn load<T: Read>(reader: &mut T) -> ZipResult<Self> {
        Ok(EXTRA_FIELD {
            header_id: reader.read_u16::<LittleEndian>()?,
            data_size: reader.read_u16::<LittleEndian>()?,
        })
    }
}
