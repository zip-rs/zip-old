#![no_main]

use std::cell::RefCell;
use libfuzzer_sys::fuzz_target;
use arbitrary::Arbitrary;
use std::io::{Cursor, Read, Seek, Write};

#[derive(Arbitrary,Debug)]
pub struct File {
    pub name: String,
    pub contents: Vec<Vec<u8>>
}

#[derive(Arbitrary,Debug)]
pub enum FileOperation {
    Write {
        file: File,
        options: zip_next::write::FileOptions,
        reopen: bool,
    },
    ShallowCopy {
        base: Box<FileOperation>,
        new_name: String,
        reopen: bool,
    },
    DeepCopy {
        base: Box<FileOperation>,
        new_name: String,
        reopen: bool,
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

    pub fn should_reopen(&self) -> bool {
        match self {
            FileOperation::Write {reopen, ..} => *reopen,
            FileOperation::ShallowCopy {reopen, ..} => *reopen,
            FileOperation::DeepCopy {reopen, ..} => *reopen
        }
    }
}

fn do_operation<T>(writer: &mut RefCell<zip_next::ZipWriter<T>>,
                   operation: &FileOperation) -> Result<(), Box<dyn std::error::Error>>
                   where T: Read + Write + Seek {
    if zip_next::write::validate_name(&operation.get_name()).is_err() {
        return Ok(());
    }
    match operation {
        FileOperation::Write {file, mut options, ..} => {
            if file.contents.iter().map(Vec::len).sum::<usize>() >= u32::MAX as usize {
                options = options.large_file(true);
            }
            writer.borrow_mut().start_file(file.name.to_owned(), options)?;
            for chunk in &file.contents {
                writer.borrow_mut().write_all(chunk.as_slice())?;
            }
        }
        FileOperation::ShallowCopy {base, new_name, .. } => {
            do_operation(writer, base)?;
            writer.borrow_mut().shallow_copy_file(&base.get_name(), new_name)?;
        }
        FileOperation::DeepCopy {base, new_name, .. } => {
            do_operation(writer, base)?;
            writer.borrow_mut().deep_copy_file(&base.get_name(), new_name)?;
        }
    }
    if operation.should_reopen() {
        let new_writer = zip_next::ZipWriter::new_append(writer.borrow_mut().finish().unwrap()).unwrap();
        *writer = new_writer.into();
    }
    Ok(())
}

fuzz_target!(|data: Vec<FileOperation>| {
    let mut writer = RefCell::new(zip_next::ZipWriter::new(Cursor::new(Vec::new())));
    for operation in data {
        let _ = do_operation(&mut writer, &operation);
    }
    let _ = zip_next::ZipArchive::new(writer.borrow_mut().finish().unwrap());
});