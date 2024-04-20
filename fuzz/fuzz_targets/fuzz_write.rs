#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use std::cell::RefCell;
use std::io::{Cursor, Read, Seek, Write};
use std::path::PathBuf;

#[derive(Arbitrary, Clone, Debug)]
pub enum BasicFileOperation {
    WriteNormalFile {
        contents: Vec<Vec<u8>>,
        options: zip::write::FullFileOptions,
    },
    WriteDirectory(zip::write::FullFileOptions),
    WriteSymlinkWithTarget {
        target: PathBuf,
        options: zip::write::FullFileOptions,
    },
    ShallowCopy(Box<FileOperation>),
    DeepCopy(Box<FileOperation>),
}

#[derive(Arbitrary, Clone, Debug)]
pub struct FileOperation {
    basic: BasicFileOperation,
    path: PathBuf,
    reopen: bool,
    // 'abort' flag is separate, to prevent trying to copy an aborted file
}

#[derive(Arbitrary, Clone, Debug)]
pub struct FuzzTestCase {
    comment: Vec<u8>,
    operations: Vec<(FileOperation, bool)>,
    flush_on_finish_file: bool,
}

fn do_operation<T>(
    writer: &mut RefCell<zip::ZipWriter<T>>,
    operation: &FileOperation,
    abort: bool,
    flush_on_finish_file: bool,
) -> Result<(), Box<dyn std::error::Error>>
where
    T: Read + Write + Seek,
{
    writer
        .borrow_mut()
        .set_flush_on_finish_file(flush_on_finish_file);
    let path = &operation.path;
    match &operation.basic {
        BasicFileOperation::WriteNormalFile {
            contents,
            options,
            ..
        } => {
            let uncompressed_size = contents.iter().map(Vec::len).sum::<usize>();
            let mut options = (*options).to_owned();
            if uncompressed_size >= u32::MAX as usize {
                options = options.large_file(true);
            }
            writer.borrow_mut().start_file_from_path(path, options)?;
            for chunk in contents {
                writer.borrow_mut().write_all(chunk.as_slice())?;
            }
        }
        BasicFileOperation::WriteDirectory(options) => {
            writer.borrow_mut().add_directory_from_path(path, options.to_owned())?;
        }
        BasicFileOperation::WriteSymlinkWithTarget { target, options } => {
            writer
                .borrow_mut()
                .add_symlink_from_path(&path, target, options.to_owned())?;
        }
        BasicFileOperation::ShallowCopy(base) => {
            do_operation(writer, &base, false, flush_on_finish_file)?;
            writer.borrow_mut().shallow_copy_file_from_path(&base.path, &path)?;
        }
        BasicFileOperation::DeepCopy(base) => {
            do_operation(writer, &base, false, flush_on_finish_file)?;
            writer.borrow_mut().deep_copy_file_from_path(&base.path, &path)?;
        }
    }
    if abort {
        writer.borrow_mut().abort_file().unwrap();
    }
    if operation.reopen {
        let old_comment = writer.borrow().get_raw_comment().to_owned();
        let new_writer =
            zip::ZipWriter::new_append(writer.borrow_mut().finish().unwrap()).unwrap();
        assert_eq!(&old_comment, new_writer.get_raw_comment());
        *writer = new_writer.into();
    }
    Ok(())
}

fuzz_target!(|test_case: FuzzTestCase| {
    let mut writer = RefCell::new(zip::ZipWriter::new(Cursor::new(Vec::new())));
    writer.borrow_mut().set_raw_comment(test_case.comment);
    for (operation, abort) in test_case.operations {
        let _ = do_operation(
            &mut writer,
            &operation,
            abort,
            test_case.flush_on_finish_file,
        );
    }
    let _ = zip::ZipArchive::new(writer.borrow_mut().finish().unwrap());
});
