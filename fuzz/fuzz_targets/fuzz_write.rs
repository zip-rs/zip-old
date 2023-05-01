#![no_main]

use libfuzzer_sys::fuzz_target;
use arbitrary::{Arbitrary};
use std::fmt::Debug;
use std::io::{Cursor, Read, Seek, Write};
use std::iter::{repeat, Flatten, Repeat, Take};

#[derive(Arbitrary,Debug)]
pub struct File {
    pub name: String,
    pub contents: Vec<Vec<u8>>
}

const LARGE_FILE_BUF_SIZE: usize = u32::MAX as usize + 1;

#[derive(Arbitrary, Clone, Debug)]
pub enum RepeatedBytes {
    Once {
        min_bytes: [u8; 1024],
        extra_bytes: Vec<u8>
    },
    U8Times {
        bytes: Vec<u8>,
        repeats: u8,
    },
    U16Times {
        bytes: Vec<u8>,
        repeats: u16,
    }
}

impl IntoIterator for RepeatedBytes {
    type Item = u8;
    type IntoIter = Flatten<Take<Repeat<Vec<u8>>>>;
    fn into_iter(self) -> Self::IntoIter {
        match self {
            RepeatedBytes::Once {min_bytes, extra_bytes} => {
                let mut bytes = min_bytes.to_vec();
                bytes.extend(extra_bytes);
                repeat(bytes).take(1)
            },
            RepeatedBytes::U8Times {bytes, repeats} => {
                repeat(bytes).take(repeats as usize + 2)
            },
            RepeatedBytes::U16Times {bytes, repeats} => {
                repeat(bytes).take(repeats as usize + u8::MAX as usize + 2)
            }
        }.flatten()
    }
}

#[derive(Arbitrary,Debug)]
pub struct LargeFile {
    pub name: String,
    pub large_contents: Vec<Vec<RepeatedBytes>>,
    pub extra_contents: Vec<Vec<u8>>
}

#[derive(Arbitrary,Debug)]
pub enum FileOperation {
    Write {
        file: File,
        options: zip_next::write::FileOptions
    },
    WriteLarge {
        file: LargeFile,
        options: zip_next::write::FileOptions
    },
    ShallowCopy {
        base: Box<FileOperation>,
        new_name: String
    },
    DeepCopy {
        base: Box<FileOperation>,
        new_name: String
    }
}

impl FileOperation {
    pub fn get_name(&self) -> String {
        match self {
            FileOperation::Write {file, ..} => &file.name,
            FileOperation::WriteLarge {file, ..} => &file.name,
            FileOperation::ShallowCopy {new_name, ..} => new_name,
            FileOperation::DeepCopy {new_name, ..} => new_name
        }.to_owned()
    }
}

fn do_operation<T>(writer: &mut zip_next::ZipWriter<T>,
                   operation: &FileOperation) -> Result<(), Box<dyn std::error::Error>>
                   where T: Read + Write + Seek {
    match operation {
        FileOperation::Write {file, mut options} => {
            if file.contents.iter().map(Vec::len).sum::<usize>() >= u32::MAX as usize {
                options = options.large_file(true);
            }
            writer.start_file(file.name.to_owned(), options)?;
            for chunk in &file.contents {
                writer.write_all(chunk.as_slice())?;
            }
        }
        FileOperation::WriteLarge {file, mut options} => {
            options = options.large_file(true);
            writer.start_file(file.name.to_owned(), options)?;
            let mut written: usize = 0;
            while written < LARGE_FILE_BUF_SIZE {
                for chunk in &file.large_contents {
                    let chunk: Vec<u8> = chunk.to_owned().into_iter()
                        .flat_map(RepeatedBytes::into_iter)
                        .collect();
                    written += chunk.len();
                    writer.write_all(chunk.as_slice())?;
                }
            }
            for chunk in &file.extra_contents {
                writer.write_all(chunk.as_slice())?;
            }
        }
        FileOperation::ShallowCopy {base, new_name} => {
            do_operation(writer, base)?;
            writer.shallow_copy_file(&base.get_name(), new_name)?;
        }
        FileOperation::DeepCopy {base, new_name} => {
            do_operation(writer, base)?;
            writer.deep_copy_file(&base.get_name(), new_name)?;
        }
    }
    Ok(())
}

fuzz_target!(|data: Vec<FileOperation>| {
    let mut writer = zip_next::ZipWriter::new(Cursor::new(Vec::new()));
    for operation in data {
        let _ = do_operation(&mut writer, &operation);
    }
    let _ = zip_next::ZipArchive::new(writer.finish().unwrap());
});