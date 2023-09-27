#![allow(missing_docs)]

use indexmap::IndexSet;

use std::{
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    str,
};

#[derive(Debug, Clone)]
pub struct CompletedPaths {
    seen: IndexSet<PathBuf>,
}

impl CompletedPaths {
    pub fn new() -> Self {
        Self {
            seen: IndexSet::new(),
        }
    }

    #[inline]
    pub fn contains(&self, path: &Path) -> bool {
        self.seen.contains(Self::normalize_trailing_slashes(path))
    }

    #[inline]
    pub fn is_dir(path: &Path) -> bool {
        Self::path_str(path).ends_with('/')
    }

    #[inline]
    pub(crate) fn path_str(path: &Path) -> &str {
        debug_assert!(path.to_str().is_some());
        unsafe { str::from_utf8_unchecked(path.as_os_str().as_bytes()) }
    }

    #[inline]
    pub fn normalize_trailing_slashes(path: &Path) -> &Path {
        Path::new(Self::path_str(path).trim_end_matches('/'))
    }

    pub fn containing_dirs<'a>(
        path: &'a (impl AsRef<Path> + ?Sized),
    ) -> impl Iterator<Item = &'a Path> {
        let is_dir = Self::is_dir(path.as_ref());
        path.as_ref()
            .ancestors()
            .inspect(|p| {
                if p == &Path::new("/") {
                    unreachable!("did not expect absolute paths")
                }
            })
            .filter_map(move |p| {
                if &p == &path.as_ref() {
                    if is_dir {
                        Some(p)
                    } else {
                        None
                    }
                } else if p == Path::new("") {
                    None
                } else {
                    Some(p)
                }
            })
            .map(Self::normalize_trailing_slashes)
    }

    pub fn new_containing_dirs_needed<'a>(
        &self,
        path: &'a (impl AsRef<Path> + ?Sized),
    ) -> Vec<&'a Path> {
        let mut ret: Vec<_> = Self::containing_dirs(path)
            /* Assuming we are given ancestors in order from child to parent. */
            .take_while(|p| !self.contains(p))
            .collect();
        /* Get dirs in order from parent to child. */
        ret.reverse();
        ret
    }

    pub fn confirm_dir(&mut self, dir: &Path) {
        let dir = Self::normalize_trailing_slashes(dir);
        if !self.seen.contains(dir) {
            self.seen.insert(dir.to_path_buf());
        }
    }
}
