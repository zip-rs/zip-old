#![no_main]

use libfuzzer_sys::fuzz_target;
use arbitrary::Arbitrary;
use std::fmt::{Debug, Formatter};
use std::io::{Cursor, Read, Seek, Write};

#[derive(Arbitrary,Debug)]
pub struct File {
    pub name: String,
    pub contents: Vec<Vec<u8>>
}

#[derive(Arbitrary)]
pub struct LargeFile {
    pub name: String,
    pub large_contents: [u8; u32::MAX as usize + 1],
    pub extra_contents: Vec<Vec<u8>>
}

impl Debug for LargeFile {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LargeFile")
            .field("name", &self.name)
            .field("extra_contents", &self.extra_contents)
            .finish()
    }
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
            writer.write_all(&file.large_contents)?;
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