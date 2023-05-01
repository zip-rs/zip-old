#![no_main]

use libfuzzer_sys::fuzz_target;
use arbitrary::{Arbitrary};
use std::fmt::Debug;
use std::io::{Cursor, Read, Seek, Write};
use std::iter::{repeat};

#[derive(Arbitrary,Debug)]
pub struct File {
    pub name: String,
    pub contents: Vec<Vec<u8>>
}

const LARGE_FILE_BUF_SIZE: usize = u32::MAX as usize + 1;

#[derive(Arbitrary, Clone, Debug)]
pub struct SparseFilePart {
    pub start: u32,
    pub first_byte: u8,
    pub extra_bytes: Vec<u8>,
    pub repeats: u8
}

#[derive(Arbitrary,Debug)]
pub struct LargeFile {
    pub name: String,
    pub default_pattern_first_byte: u8,
    pub default_pattern_extra_bytes: Vec<u8>,
    pub parts: Vec<SparseFilePart>,
    pub min_extra_length: u16
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
            options = options.large_file(true).force_compression();
            writer.start_file(file.name.to_owned(), options)?;
            let mut default_pattern = Vec::with_capacity(file.default_pattern_extra_bytes.len() + 1);
            default_pattern.push(file.default_pattern_first_byte);
            default_pattern.extend(&file.default_pattern_extra_bytes);
            let mut sparse_file: Vec<u8> =
                repeat(default_pattern.into_iter()).flatten().take(LARGE_FILE_BUF_SIZE + file.min_extra_length as usize)
                    .collect();
            for part in &file.parts {
                let mut bytes = Vec::with_capacity(part.extra_bytes.len() + 1);
                bytes.push(part.first_byte);
                bytes.extend(part.extra_bytes.iter());
                for (index, byte) in repeat(bytes.iter()).take(part.repeats as usize + 1).flatten().enumerate() {
                    sparse_file[part.start as usize + index] = *byte;
                }
            }
            writer.write_all(sparse_file.as_slice())?;
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