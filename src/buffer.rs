use std::borrow::{Borrow, Cow};
use std::io::Read;
use std::ops::Deref;
use std::hash::{Hash, Hasher};

use bytes::{Buf, Bytes};
use string;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct StrBuf(string::String<Bytes>);

impl StrBuf {
    pub(crate) fn from_str(s: &str) -> Self {
        StrBuf(string::String::from_str(s))
    }

    pub(crate) fn from_utf8(bytes: ByteBuf) -> Result<Self, ::std::str::Utf8Error> {
        Ok(StrBuf(string::TryFrom::try_from(bytes.0)?))
    }

    pub(crate) fn from_utf8_lossy(bytes: ByteBuf) -> Self {
        match String::from_utf8_lossy(bytes.as_ref()) {
            Cow::Owned(s) => s.into(),
            Cow::Borrowed(s) => {
                // SAFETY: We know that `bytes` only contains valid utf-8,
                //         since the `from_utf8_lossy` operation returned the
                //         input verbatim.
                debug_assert_eq!(s.len(), bytes.len());
                StrBuf(unsafe { string::String::from_utf8_unchecked(bytes.clone().0) })
            }
        }
    }
}

impl From<String> for StrBuf {
    fn from(s: String) -> Self {
        let bytes = s.into_bytes().into();
        // SAFETY: We know that `bytes` only contains valid utf-8,
        //         since the underlying data comes from the input string.
        StrBuf(unsafe { string::String::from_utf8_unchecked(bytes) })
    }
}

impl Hash for StrBuf {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Because of the impl Borrow<str> for StrBuf, we need to make sure that the Hash
        // implementations behave identically between str and StrBuf.
        //
        // Quoting the documentation for the Borrow trait:
        //
        // > Further, when providing implementations for additional traits, it needs to be
        // > considered whether they should behave identical to those of the underlying type as a
        // > consequence of acting as a representation of that underlying type.
        // > Generic code typically uses Borrow<T> when it relies on the identical behavior of
        // > these additional trait implementations.
        // > These traits will likely appear as additional trait bounds.
        //
        // Without this, it would be impossible to look up an entry from the names_map by &str,
        // since the str and StrBuf would evaluate to different hashes, even if they represent the
        // same sequence of characters.
        str::hash(&*self, state)
    }
}

impl Borrow<str> for StrBuf {
    fn borrow(&self) -> &str {
        self.0.borrow()
    }
}

impl Deref for StrBuf {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ByteBuf(Bytes);

impl ByteBuf {
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn split_to(&mut self, at: usize) -> ByteBuf {
        ByteBuf(self.0.split_to(at))
    }
}

impl Buf for ByteBuf {
    fn remaining(&self) -> usize {
        self.0.len()
    }

    fn bytes(&self) -> &[u8] {
        self.0.as_ref()
    }

    fn advance(&mut self, cnt: usize) {
        self.0.advance(cnt)
    }
}

impl AsRef<[u8]> for ByteBuf {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl Read for ByteBuf {
    fn read(&mut self, buf: &mut [u8]) -> ::std::io::Result<usize> {
        self.reader().read(buf)
    }
}

impl From<Vec<u8>> for ByteBuf {
    fn from(vec: Vec<u8>) -> Self {
        ByteBuf(vec.into())
    }
}

impl From<Bytes> for ByteBuf {
    fn from(bytes: Bytes) -> Self {
        ByteBuf(bytes)
    }
}
