#![allow(missing_docs)]

use indexmap::IndexSet;

use std::path::{Path, PathBuf};

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
    pub fn contains(&self, path: impl AsRef<Path>) -> bool {
        self.seen.contains(path.as_ref())
    }

    pub fn containing_dirs<'a>(
        path: &'a (impl AsRef<Path> + ?Sized),
    ) -> impl Iterator<Item = &'a Path> {
        let is_dir = path
            .as_ref()
            .to_str()
            .expect("paths should be valid unicode")
            .ends_with('/');
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
    }

    pub fn new_containing_dirs_needed<'a>(
        &self,
        path: &'a (impl AsRef<Path> + ?Sized),
    ) -> Vec<PathBuf> {
        let mut ret: Vec<_> = Self::containing_dirs(path)
            /* Assuming we are given ancestors in order from child to parent. */
            .take_while(|p| !self.contains(p))
            .map(|p| p.to_path_buf())
            .collect();
        /* Get dirs in order from parent to child. */
        ret.reverse();
        ret
    }

    pub fn write_dirs(&mut self, paths: Vec<PathBuf>) {
        self.seen.extend(paths);
    }
}
