#![no_main]

use std::cell::RefCell;
use libfuzzer_sys::fuzz_target;
use arbitrary::Arbitrary;
use std::io::{Cursor, Read, Seek, Write};
use std::path::{PathBuf};

#[derive(Arbitrary,Debug)]
pub enum BasicFileOperation {
    WriteNormalFile {
        contents: Vec<Vec<u8>>,
        options: zip_next::write::FileOptions,
    },
    WriteDirectory(zip_next::write::FileOptions),
    WriteSymlinkWithTarget {
        target: Box<PathBuf>,
        options: zip_next::write::FileOptions,
    },
    ShallowCopy(Box<FileOperation>),
    DeepCopy(Box<FileOperation>),
}

#[derive(Arbitrary,Debug)]
pub struct FileOperation {
    basic: BasicFileOperation,
    name: String,
    reopen: bool,
}

impl FileOperation {
    fn referenceable_name(&self) -> &str {
        if let WriteDirectory(_) = self.basic {
            self.name + "/"
        } else {
            self.name
        }
    }
}

fn do_operation<T>(writer: &mut RefCell<zip_next::ZipWriter<T>>,
                   operation: FileOperation) -> Result<(), Box<dyn std::error::Error>>
                   where T: Read + Write + Seek {
    let name = operation.name;
    match operation.basic {
        BasicFileOperation::WriteNormalFile {contents, mut options, ..} => {
            if file.contents.iter().map(Vec::len).sum::<usize>() >= u32::MAX as usize {
                options = options.large_file(true);
            }
            writer.borrow_mut().start_file(name, options)?;
            for chunk in contents {
                writer.borrow_mut().write_all(chunk.as_slice())?;
            }
        }
        BasicFileOperation::WriteDirectory(options) => {
            writer.borrow_mut().add_directory(name, options)?;
        }
        BasicFileOperation::WriteSymlinkWithTarget {target, options} => {
            writer.borrow_mut().add_symlink(name, target.to_string_lossy(), options)?;
        }
        BasicFileOperation::ShallowCopy(base) => {
            let base_name = base.referenceable_name().to_owned();
            do_operation(writer, *base)?;
            writer.borrow_mut().shallow_copy_file(&base_name, &name)?;
        }
        BasicFileOperation::DeepCopy(base) => {
            let base_name = base.referenceable_name().to_owned();
            do_operation(writer, *base)?;
            writer.borrow_mut().deep_copy_file(&base_name, &name)?;
        }
    }
    if operation.reopen {
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