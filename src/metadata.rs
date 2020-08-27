use std::fmt;

pub struct Name(TextImpl<Vec<u8>, String>);
pub struct Text<'a>(TextImpl<&'a [u8], &'a str>);

impl Text<'_> {
    fn to_str(&self) -> Option<&str> {
        match self.0 {
            TextImpl::BadStr(_) => None,
            TextImpl::Str(s) => Some(s),
            TextImpl::Cp437(b) => todo!("check if {:?} contains UTF-8 compatible data", b),
        }
    }
}

pub struct Fields<'a> {
    extra_fields: &'a [u8],
}
impl<'a> Iterator for Fields<'a> {
    type Item = Result<(u16, &'a [u8]), BadBlock>;
    fn next(&mut self) -> Option<Self::Item> {
        match self.extra_fields {
            [id_low, id_high, len_low, len_high, rest @ ..] => Some({
                let len = u16::from_le_bytes([*len_low, *len_high]) as usize;
                let id = u16::from_le_bytes([*id_low, *id_high]);
                rest.get(..len)
                    .map(|data| {
                        self.extra_fields = &rest[len..];
                        (id, data)
                    })
                    .ok_or(BadBlock(()))
            }),
            [] => None,
            [..] => Some(Err(BadBlock(()))),
        }
    }
}

#[derive(Debug)]
pub struct BadBlock(());
impl fmt::Display for BadBlock {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a data block in this file's metadata was invalid")
    }
}

// TODO: Construct these while parsing, and convert them to the super::Archive's
//       Metadata. `From` might do it?
pub struct Header<'a> {
    made_by: u16,
    datetime: (u16, u16),
    size: u32,
    name: Text<'a>,
    comment: Text<'a>,
    attrs: u32,
    fields: Fields<'a>,
}
pub struct LocalHeader<'a> {
    datetime: (u16, u16),
    name: Text<'a>,
    fields: Fields<'a>,
}

enum TextImpl<Buf: AsRef<[u8]>, Str: AsRef<str>> {
    BadStr(Buf),
    Cp437(Buf),
    // TODO: Do we want a `Cp437Str(Str)`? This'd make `Text::to_str` cheap
    Str(Str),
}
