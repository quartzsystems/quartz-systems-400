// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Walking a directory tree in name order, with a visit before and
//! after each directory, pruning, symbolic-link policy and loop
//! detection.

use qsh::sys::{FileType, Stat, Sys, SysError};

/// One path met on the walk.
#[derive(Clone, Debug)]
pub struct Entry {
    /// The path as built from the root operand.
    pub path: String,
    /// Its status (`lstat`, or `stat` when the link was followed).
    pub st: Stat,
    /// Depth below the root (0 is the root).
    pub depth: usize,
}

/// What the callback sees.
pub enum Visit<'a> {
    /// A path, before its contents.
    Pre(&'a Entry),
    /// A directory after its contents.
    Post(&'a Entry),
    /// A path that could not be read.
    Fail(&'a str, SysError),
}

/// What the callback asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    /// Go on.
    Continue,
    /// Do not enter this directory.
    Prune,
    /// End the walk.
    Stop,
}

/// How to walk.
#[derive(Clone, Copy, Debug, Default)]
pub struct Walker {
    /// Follow a symbolic link given as the root (`-H`).
    pub follow_root: bool,
    /// Follow every symbolic link (`-L`).
    pub follow_all: bool,
    /// Stay on the root's file system (`-x`).
    pub one_fs: bool,
}

impl Walker {
    /// Walk from `root`; `false` when the callback stopped it.
    pub fn walk(
        self,
        sys: &dyn Sys,
        root: &str,
        f: &mut dyn FnMut(Visit<'_>) -> Control,
    ) -> bool {
        let follow = self.follow_all || self.follow_root;
        let st = match Self::stat(sys, root, follow) {
            Ok(st) => st,
            Err(e) => {
                return f(Visit::Fail(root, e)) != Control::Stop;
            }
        };
        let mut stack = Vec::new();
        self.descend(
            sys,
            &Entry {
                path: root.to_string(),
                st,
                depth: 0,
            },
            st.dev,
            &mut stack,
            f,
        )
    }

    fn stat(sys: &dyn Sys, path: &str, follow: bool) -> Result<Stat, SysError> {
        let st = sys.lstat(path)?;
        if st.ftype == FileType::Symlink && follow {
            return sys.stat(path).or(Ok(st));
        }
        Ok(st)
    }

    fn descend(
        self,
        sys: &dyn Sys,
        e: &Entry,
        dev: u64,
        stack: &mut Vec<(u64, u64)>,
        f: &mut dyn FnMut(Visit<'_>) -> Control,
    ) -> bool {
        match f(Visit::Pre(e)) {
            Control::Stop => return false,
            Control::Prune => return true,
            Control::Continue => {}
        }
        if e.st.ftype != FileType::Dir {
            return true;
        }
        if self.one_fs && e.st.dev != dev {
            return true;
        }
        let key = (e.st.dev, e.st.ino);
        if stack.contains(&key) {
            return f(Visit::Fail(&e.path, SysError::new(qsh::sys::Errno::ELOOP)))
                != Control::Stop;
        }
        let mut names = match sys.read_dir(&e.path) {
            Ok(n) => n,
            Err(err) => {
                return f(Visit::Fail(&e.path, err)) != Control::Stop;
            }
        };
        names.sort();
        stack.push(key);
        for name in names {
            let path = if e.path.ends_with('/') {
                format!("{}{name}", e.path)
            } else {
                format!("{}/{name}", e.path)
            };
            let st = match Self::stat(sys, &path, self.follow_all) {
                Ok(st) => st,
                Err(err) => {
                    if f(Visit::Fail(&path, err)) == Control::Stop {
                        stack.pop();
                        return false;
                    }
                    continue;
                }
            };
            let child = Entry {
                path,
                st,
                depth: e.depth + 1,
            };
            if !self.descend(sys, &child, dev, stack, f) {
                stack.pop();
                return false;
            }
        }
        stack.pop();
        f(Visit::Post(e)) != Control::Stop
    }
}
