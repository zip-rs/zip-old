// TODO: move this module to deflate64 crate

use deflate64::InflaterManaged;
use std::io;
use std::io::{BufRead, BufReader, Read};

const IN_BUFFER_MAX_SIZE: usize = 8192;

pub(crate) struct BufferedDeflate64Decoder<R> {
    inner: R,
    inflator: Box<InflaterManaged>,
}

impl<R: Read> BufferedDeflate64Decoder<BufReader<R>> {
    pub(crate) fn new(inner: R) -> Self {
        Self {
            inner: BufReader::new(inner),
            inflator: Box::new(InflaterManaged::new()),
        }
    }
}

impl<R> BufferedDeflate64Decoder<R> {
    pub(crate) fn into_inner(self) -> R {
        self.inner
    }
}

struct State {
    inflater: InflaterManaged,
    in_buffer_size: u16,
    in_buffer: [u8; IN_BUFFER_MAX_SIZE],
}

impl<R: BufRead> Read for BufferedDeflate64Decoder<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            let input = self.inner.fill_buf()?;
            let eof = input.is_empty();

            let result = self.inflator.inflate(input, buf);

            self.inner.consume(result.bytes_consumed);

            if result.data_error {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid deflate64",
                ));
            }

            if result.bytes_written == 0 && !eof && !self.inflator.finished() {
                // if we haven't ready any data and we haven't hit EOF yet,
                // ask again. We must not return 0 in such case
                continue;
            }

            Ok(result.bytes_written)
        }
    }
}
