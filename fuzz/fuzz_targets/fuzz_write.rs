#![no_main]

use std::cell::RefCell;
use libfuzzer_sys::fuzz_target;
use arbitrary::Arbitrary;
use std::io::{Cursor, Read, Seek, Write};
use std::path::{PathBuf};

#[derive(Arbitrary,Debug)]
pub struct File {
    pub name: String,
    pub contents: Vec<Vec<u8>>
}

#[derive(Arbitrary,Debug)]
pub enum FileOperation {
    WriteNormalFile {
        file: File,
        options: zip_next::write::FileOptions,
        reopen: bool,
    },
    WriteDirectory {
        name: String,
        options: zip_next::write::FileOptions,
        reopen: bool,
    },
    WriteSymlink {
        name: String,
        target: Box<PathBuf>,
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
            FileOperation::WriteNormalFile {file, ..} => &file.name,
            FileOperation::WriteDirectory {name, ..} => name,
            FileOperation::WriteSymlink {name, ..} => name,
            FileOperation::ShallowCopy {new_name, ..} => new_name,
            FileOperation::DeepCopy {new_name, ..} => new_name
        }.to_owned()
    }

    pub fn should_reopen(&self) -> bool {
        match self {
            FileOperation::WriteNormalFile {reopen, ..} => *reopen,
            FileOperation::ShallowCopy {reopen, ..} => *reopen,
            FileOperation::DeepCopy {reopen, ..} => *reopen,
            FileOperation::WriteDirectory {reopen, ..} => *reopen,
            FileOperation::WriteSymlink {reopen, ..} => *reopen,
        }
    }
}

fn do_operation<T>(writer: &mut RefCell<zip_next::ZipWriter<T>>,
                   operation: FileOperation) -> Result<(), Box<dyn std::error::Error>>
                   where T: Read + Write + Seek {
    let should_reopen = operation.should_reopen();
    match operation {
        FileOperation::WriteNormalFile {file, mut options, ..} => {
            if file.contents.iter().map(Vec::len).sum::<usize>() >= u32::MAX as usize {
                options = options.large_file(true);
            }
            writer.borrow_mut().start_file(file.name.to_owned(), options)?;
            for chunk in &file.contents {
                writer.borrow_mut().write_all(chunk.as_slice())?;
            }
        }
        FileOperation::WriteDirectory {name, options, ..} => {
            writer.borrow_mut().add_directory(name, options)?;
        }
        FileOperation::WriteSymlink {name, target, options, ..} => {
            writer.borrow_mut().add_symlink(name, target.to_string_lossy(), options)?;
        }
        FileOperation::ShallowCopy {base, ref new_name, .. } => {
            let base_name = base.get_name();
            do_operation(writer, *base)?;
            writer.borrow_mut().shallow_copy_file(&base_name, new_name)?;
        }
        FileOperation::DeepCopy {base, ref new_name, .. } => {
            let base_name = base.get_name();
            do_operation(writer, *base)?;
            writer.borrow_mut().deep_copy_file(&base_name, new_name)?;
        }
    }
    if should_reopen {
        let new_writer = zip_next::ZipWriter::new_append(writer.borrow_mut().finish().unwrap()).unwrap();
        *writer = new_writer.into();
    }
    Ok(())
}

fuzz_target!(|data: Vec<FileOperation>| {
    let mut writer = RefCell::new(zip_next::ZipWriter::new(Cursor::new(Vec::new())));
    for operation in data {
        let _ = do_operation(&mut writer, operation);
    }
    let _ = zip_next::ZipArchive::new(writer.borrow_mut().finish().unwrap());
});