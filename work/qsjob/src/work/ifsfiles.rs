// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The HTTP server's files (`net/httpd`) through the IFS as this job
//! sees it, with its user's authority. The link is the job's main
//! thread's, so [`IfsFiles`] is a [`Local`]; [`with_files`] runs work
//! that wants [`Files`] on a helper thread while this one answers.

use std::io;

use httpd::bridge::{self, Local};
use httpd::files::{Entry, Files};
use ifs::client::{IfsClient, read_all, replace, write_all};
use ifs::errno::Errno;
use ifs::ops::flags::{O_APPEND, O_CREAT, O_RDONLY, O_WRONLY, S_IFDIR, S_IFMT};
use ifs::ops::{OpenArgs, RenameMode};
use stg::Shared;
use stg::remote::RemoteIfs;

/// The IFS over the job's link.
pub struct IfsFiles {
    c: RemoteIfs,
}

impl IfsFiles {
    /// Over `link`.
    pub fn new(link: &Shared) -> Self {
        Self { c: RemoteIfs::new(link.clone()) }
    }
}

fn err(path: &str, e: Errno) -> io::Error {
    let kind = match e {
        Errno::ENOENT | Errno::ENOTDIR => io::ErrorKind::NotFound,
        Errno::EACCES => io::ErrorKind::PermissionDenied,
        Errno::EEXIST => io::ErrorKind::AlreadyExists,
        _ => io::ErrorKind::Other,
    };
    io::Error::new(kind, format!("{path}: {}", e.text()))
}

fn name_of(path: &str) -> &str {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or("")
}

impl IfsFiles {
    fn put(&self, path: &str, data: &[u8], oflag: u32) -> io::Result<()> {
        let fd = self
            .c
            .open(path, OpenArgs::new(O_WRONLY | O_CREAT | oflag).mode(0o644))
            .map_err(|e| err(path, e))?;
        let r = write_all(&self.c, fd, data).map_err(|e| err(path, e));
        let _ = self.c.close(fd);
        r
    }

    fn entry(&self, path: &str, name: &str) -> io::Result<Entry> {
        let s = self.c.stat(path, true).map_err(|e| err(path, e))?;
        let dir = s.mode & S_IFMT == S_IFDIR;
        Ok(Entry {
            name: name.to_string(),
            dir,
            size: if dir { 0 } else { s.size },
            modified: u64::try_from(s.mtime / 1_000_000_000).unwrap_or(0),
        })
    }
}

impl Local for IfsFiles {
    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        let fd = self.c.open(path, OpenArgs::new(O_RDONLY)).map_err(|e| err(path, e))?;
        let r = read_all(&self.c, fd).map_err(|e| err(path, e));
        let _ = self.c.close(fd);
        r
    }

    fn write_atomic(&self, path: &str, data: &[u8]) -> io::Result<()> {
        // A file another profile owns keeps its owner and authorities.
        replace(&self.c, path, data, 0o644).map_err(|e| err(path, e))
    }

    fn append(&self, path: &str, data: &[u8]) -> io::Result<()> {
        self.put(path, data, O_APPEND)
    }

    fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        self.c.rename(from, to, RenameMode::Unlink).map_err(|e| err(from, e))
    }

    fn mkdir_all(&self, path: &str) -> io::Result<()> {
        let mut cur = String::new();
        for part in path.split('/').filter(|p| !p.is_empty()) {
            cur.push('/');
            cur.push_str(part);
            match self.c.mkdir(&cur, 0o755) {
                Ok(()) | Err(Errno::EEXIST) => {}
                Err(e) => return Err(err(&cur, e)),
            }
        }
        Ok(())
    }

    fn remove(&self, path: &str) -> io::Result<()> {
        self.c.unlink(path).map_err(|e| err(path, e))
    }

    fn remove_tree(&self, path: &str) -> io::Result<()> {
        // A link is removed, not followed; an entry that cannot be
        // looked at is the error, not a directory left non-empty.
        let s = self.c.stat(path, false).map_err(|e| err(path, e))?;
        if s.mode & S_IFMT != S_IFDIR {
            return self.remove(path);
        }
        for d in self.c.read_dir(path).map_err(|e| err(path, e))? {
            if d.name != "." && d.name != ".." {
                Local::remove_tree(self, &format!("{}/{}", path.trim_end_matches('/'), d.name))?;
            }
        }
        self.c.rmdir(path).map_err(|e| err(path, e))
    }

    fn list(&self, dir: &str) -> io::Result<Vec<Entry>> {
        let mut out = Vec::new();
        for d in self.c.read_dir(dir).map_err(|e| err(dir, e))? {
            if d.name == "." || d.name == ".." {
                continue;
            }
            let p = format!("{}/{}", dir.trim_end_matches('/'), d.name);
            if let Ok(e) = self.entry(&p, &d.name) {
                out.push(e);
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    fn stat(&self, path: &str) -> io::Result<Entry> {
        self.entry(path, name_of(path))
    }
}

/// Run `work` with [`Files`] over this job's IFS.
pub fn with_files<T: Send>(link: &Shared, work: impl FnOnce(&dyn Files) -> T + Send) -> T {
    let (remote, pump) = bridge::pair();
    pump.run(&remote, &IfsFiles::new(link), work)
}
