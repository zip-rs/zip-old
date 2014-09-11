use time;

#[deriving(FromPrimitive, Clone)]
pub enum CompressionMethod
{
    Stored = 0,
    Shrunk = 1,
    Reduced1 = 2,
    Reduced2 = 3,
    Reduced3 = 4,
    Reduced4 = 5,
    Imploded = 6,
    Deflated = 8,
    Deflate64 = 9,
    PkwareImploding = 10,
    Bzip2 = 12,
    LZMA = 14,
    IBMTerse = 18,
    LZ77 = 19,
    WavPack = 97,
    PPMdI1 = 98,
    Unknown = 100000,
}


pub struct ZipFile
{
    pub encrypted: bool,
    pub compression_method: CompressionMethod,
    pub last_modified_time: time::Tm,
    pub crc32: u32,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub file_name: Vec<u8>,
    pub file_comment: Vec<u8>,
    pub header_start: u64,
    pub data_start: u64,
}

impl ZipFile
{
    pub fn file_name_string(&self) -> String
    {
        String::from_utf8_lossy(self.file_name.as_slice()).into_string()
    }

    pub fn file_comment_string(&self) -> String
    {
        String::from_utf8_lossy(self.file_comment.as_slice()).into_string()
    }
}
