/*
See Zip format spec document: https://pkware.cachefly.net/webdocs/APPNOTE/APPNOTE-6.3.4.TXT
This is just a plan for implementation

[magic "PK"] :                            (0x4b50)
[local file header 1]                     (0x04034b50){...}
[encryption header 1]                     Not supported
[file data 1]                             (raw file data)
[data descriptor 1]                       Not implemented
...
[local file header n]                     (0x04034b50){...}
[encryption header n]                     Not supported
[file data n]                             (raw file data)
[data descriptor n]                       Not implemented

[archive decryption header]               Not implemented
[archive extra data record]               Not implemented

[central directory header 1]              (0x02014b50){...}
...
[central directory header n]              (0x02014b50){...}

[zip64 end of central directory record]   (0x06064b50){...}
[zip64 end of central directory locator]  (0x07064b50){...}
[end of central directory record]         (0x06054b50){...}

====================================================================================================================
[Local file header]
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
====================================================================================================================      
[Encryption header]
====================================================================================================================      
[File data]
====================================================================================================================
[Data descriptor]
      data descriptor signature       4 bytes  [0x08074b50 ] - pay attention - optional signature
      crc-32                          4 bytes
      compressed size                 4 bytes       8 bytes for ZIP64(tm) format
      uncompressed size               4 bytes       8 bytes for ZIP64(tm) format
====================================================================================================================
[Archive decryption header]
====================================================================================================================
[Archive extra data record]
        archive extra data signature    4 bytes  (0x08064b50)
        extra field length              4 bytes
        extra field data                (variable size)
====================================================================================================================
[central directory header]
        central file header signature   4 bytes  (0x02014b50)
        version made by                 2 bytes
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
        file comment length             2 bytes
        disk number start               2 bytes
        internal file attributes        2 bytes
        external file attributes        4 bytes
        relative offset of local header 4 bytes
        file name (variable size)
        extra field (variable size)
        file comment (variable size)
====================================================================================================================
[digital signature]
        header signature                  4 bytes  (0x05054b50)
        size of data                      2 bytes
        signature data                    (variable size)
====================================================================================================================
[Zip64 end of central directory record]
        zip64 end of central dir signature                              4 bytes  (0x06064b50)
        size of zip64 end of central directory record                   8 bytes
        version made by                                                 2 bytes
        version needed to extract                                       2 bytes
        number of this disk                                             4 bytes
        number of the disk with the start of the central directory      4 bytes
        total number of entries in the central directory on this disk   8 bytes
        total number of entries in the central directory                8 bytes
        size of the central directory                                   8 bytes
        offset of central directory with respect of disk number         8 bytes
        zip64 extensible data sector                                    (variable size)
====================================================================================================================
[Zip64 end of central directory locator]
      zip64 end of central dir locator signature                        4 bytes  (0x07064b50)
      number of the disk with the zip64 end of central directory        4 bytes
      relative offset of the zip64 end of central directory record      8 bytes
      total number of disks                                             4 bytes
====================================================================================================================
[End of central directory record]
      end of central dir signature                                      4 bytes  (0x06054b50)
      number of this disk                                               2 bytes
      number of the disk with the start of the central directory        2 bytes
      total number of entries in the central directory on this disk     2 bytes
      total number of entries in the central directory                  2 bytes
      size of the central directory                                     4 bytes
      offset of start of central directory                              4 bytes
      .ZIP file comment length                                          2 bytes
      .ZIP file comment                                                 (variable size)
====================================================================================================================
*/

mod end_of_central_directory_record;
pub use spec::end_of_central_directory_record::*;
mod zip64_end_of_central_directory_locator;
pub use spec::zip64_end_of_central_directory_locator::*;
mod zip64_end_of_central_directory_record;
pub use spec::zip64_end_of_central_directory_record::*;
mod central_directory_header;
pub use spec::central_directory_header::*;

pub static LOCAL_FILE_HEADER_SIGNATURE: u32 = 0x04034b50;
pub static CENTRAL_DIRECTORY_HEADER_SIGNATURE: u32 = 0x02014b50;
