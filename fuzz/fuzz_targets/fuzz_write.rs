#![no_main]

use libfuzzer_sys::fuzz_target;
use arbitrary::Arbitrary;
use std::io::{Cursor, Read, Seek, Write};

#[derive(Arbitrary,Debug)]
pub struct ExtraData {
    pub header_id: u16,
    pub data: Vec<u8>
}

#[derive(Arbitrary,Debug)]
pub struct File {
    pub name: String,
    pub contents: Vec<Vec<u8>>,
    pub local_extra_data: Vec<ExtraData>,
    pub central_extra_data: Vec<ExtraData>
}

#[derive(Arbitrary,Debug)]
pub enum FileOperation {
    Write {
        file: File,
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

fuzz_target!(|data: Vec<(FileOperation, bool)>| {
    let mut writer = zip_next::ZipWriter::new(Cursor::new(Vec::new()));
    for (operation, close_and_reopen) in data {
        let _ = do_operation(&mut writer, &operation);
        if close_and_reopen {
            writer = zip_next::ZipWriter::new_append(writer.finish().unwrap()).unwrap();
        }
    }
    let _ = zip_next::ZipArchive::new(writer.finish().unwrap());
});