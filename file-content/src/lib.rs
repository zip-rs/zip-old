use memmap2::MmapRaw;
pub struct FileContent {
    mmap: MmapRaw,
}
impl FileContent {
    pub fn from_file(file: &std::fs::File) -> std::io::Result<Self> {
        // SAFETY: not rlly lmao
        let mmap = unsafe { memmap2::Mmap::map(file) }?;
        Ok(Self {
            mmap: mmap.into(),
        })
    }
    pub fn read_byte_at(&self, pos: usize) -> Option<u8> {
        if pos >= self.mmap.len() {
            return None;
        }
        // SAFETY: pos is in bounds BUT file may be truncated.
        Some(unsafe { *self.mmap.as_ptr().add(pos) })
    }
}
pub struct FileCursor<'a> {
    content: &'a FileContent,
    pos: usize,
}
impl<'a> FileCursor<'a> {
    pub fn new(content: &'a FileContent) -> Self {
        Self {
            content,
            pos: 0,
        }
    }
    fn read_u8(&mut self) -> Option<u8> {
        let byte = self.content.read_byte_at(self.pos)?;
        self.pos += 1;
        Some(byte)
    }
}
impl std::io::Read for FileCursor<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut i = 0;
        while i < buf.len() {
            match self.read_u8() {
                Some(byte) => buf[i] = byte,
                None => break,
            }
            i += 1;
        }
        Ok(i)
    }
}
impl std::io::Seek for FileCursor<'_> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        match pos {
            std::io::SeekFrom::Start(pos) => {
                self.pos = pos as usize;
            }
            std::io::SeekFrom::Current(pos) => {
                self.pos = (self.pos as i64 + pos) as usize;
            }
            std::io::SeekFrom::End(pos) => {
                self.pos = (self.content.mmap.len() as i64 + pos) as usize;
            }
        }
        Ok(self.pos as u64)
    }
}