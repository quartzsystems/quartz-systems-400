// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Files through the job's IFS client: paths, attributes, directories
//! and [`IfsFile`], a stream over one of the service's descriptors.

use std::any::Any;
use std::io;

use ifs::ops::flags::{
    O_APPEND, O_CREAT, O_EXCL, O_RDONLY, O_RDWR, O_TRUNC, O_WRONLY, S_IFBLK, S_IFCHR, S_IFDIR,
    S_IFIFO, S_IFLNK, S_IFMT, S_IFREG, S_IFSOCK,
};
use ifs::ops::OpenArgs;
use qsh::sys::{Errno, FileType, FsInfo, OpenFlags, Stat, Stream, SysError};

use crate::broker::Broker;

/// The shell's error for the IFS's.
pub fn err(e: ifs::Errno) -> SysError {
    SysError::new(Errno(e.code()))
}

/// The IFS's `oflag` for the shell's flags.
pub fn oflag(f: OpenFlags) -> u32 {
    let mut o = match (f.read, f.write) {
        (true, true) => O_RDWR,
        (false, true) => O_WRONLY,
        _ => O_RDONLY,
    };
    if f.create {
        o |= O_CREAT;
    }
    if f.truncate {
        o |= O_TRUNC;
    }
    if f.append {
        o |= O_APPEND;
    }
    if f.excl {
        o |= O_EXCL;
    }
    o
}

/// The shell's attributes from the IFS's.
pub fn stat_of(s: &ifs::ops::Stat) -> Stat {
    let ftype = match s.mode & S_IFMT {
        S_IFDIR => FileType::Dir,
        S_IFREG => FileType::File,
        S_IFLNK => FileType::Symlink,
        S_IFIFO => FileType::Fifo,
        S_IFCHR => FileType::Char,
        S_IFBLK => FileType::Block,
        S_IFSOCK => FileType::Socket,
        _ => FileType::Other,
    };
    let secs = s.mtime.div_euclid(1_000_000_000);
    Stat {
        ftype,
        mode: s.mode & 0o7777,
        size: s.size,
        uid: s.uid,
        gid: s.gid,
        mtime: secs,
        ino: s.ino,
        dev: u64::from(s.dev),
        nlink: s.nlink,
        blocks: s.allocsize.div_ceil(512),
    }
}

/// `df`'s view of a file system.
pub fn fsinfo_of(v: &ifs::ops::StatVfs, path: &str) -> FsInfo {
    let bs = if v.frsize == 0 { v.bsize.max(512) } else { v.frsize };
    FsInfo {
        name: if v.name.is_empty() {
            "/dev/QASP01".to_string()
        } else {
            v.name.clone()
        },
        mounted_on: path.to_string(),
        block_size: bs,
        blocks: v.blocks,
        free: v.bfree,
        avail: v.bavail,
    }
}

/// `open(path)` as `OpenArgs`.
pub fn open_args(f: OpenFlags, mode: u32) -> OpenArgs {
    OpenArgs::new(oflag(f)).mode(mode)
}

/// A stream over a descriptor of the job's IFS; closed when the last
/// shell descriptor for it goes.
pub struct IfsFile {
    fd: i32,
    broker: Broker,
}

impl IfsFile {
    /// Over descriptor `fd`.
    pub fn new(fd: i32, broker: Broker) -> Self {
        Self { fd, broker }
    }
}

fn io_err(e: SysError) -> io::Error {
    e.into()
}

impl Stream for IfsFile {
    fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        let fd = self.fd;
        let max = buf.len().min(ifs::client::CHUNK);
        let got = self
            .broker
            .call(move |s| s.ifs().read(fd, max))
            .map_err(io_err)?
            .map_err(|e| io_err(err(e)))?;
        let n = got.len().min(buf.len());
        buf[..n].copy_from_slice(&got[..n]);
        Ok(n)
    }

    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        let fd = self.fd;
        let data = buf[..buf.len().min(ifs::client::CHUNK)].to_vec();
        self.broker
            .call(move |s| s.ifs().write(fd, &data))
            .map_err(io_err)?
            .map_err(|e| io_err(err(e)))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl Drop for IfsFile {
    fn drop(&mut self) {
        let fd = self.fd;
        self.broker.post(move |s| {
            let _ = s.ifs().close(fd);
        });
    }
}

/// The CCSID members are read and written in.
pub const MEMBER_CCSID: u32 = 819;

/// Whether a path names a `/QSYS.LIB` member.
#[allow(clippy::case_sensitive_file_extension_comparisons)] // upper-cased first
pub fn is_member(path: &str) -> bool {
    let up = path.to_ascii_uppercase();
    up.starts_with("/QSYS.LIB/") && up.ends_with(".MBR")
}

/// A member read as lines: each record's trailing blanks dropped.
pub struct Trimmed {
    file: IfsFile,
    state: std::sync::Mutex<(Vec<u8>, bool)>,
}

impl Trimmed {
    /// Over an open member.
    pub fn new(file: IfsFile) -> Self {
        Self {
            file,
            state: std::sync::Mutex::new((Vec::new(), false)),
        }
    }
}

impl Stream for Trimmed {
    fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        let mut st = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !st.1 {
            let mut all = Vec::new();
            let mut chunk = vec![0u8; 64 * 1024];
            loop {
                let n = self.file.read(&mut chunk)?;
                if n == 0 {
                    break;
                }
                all.extend_from_slice(&chunk[..n]);
            }
            let mut out = Vec::with_capacity(all.len());
            for line in all.split_inclusive(|b| *b == b'\n') {
                let (body, nl) = match line.split_last() {
                    Some((b'\n', rest)) => (rest, true),
                    _ => (line, false),
                };
                let end = body.iter().rposition(|b| *b != b' ').map_or(0, |i| i + 1);
                out.extend_from_slice(&body[..end]);
                if nl {
                    out.push(b'\n');
                }
            }
            *st = (out, true);
        }
        let n = st.0.len().min(buf.len());
        buf[..n].copy_from_slice(&st.0[..n]);
        st.0.drain(..n);
        Ok(n)
    }

    fn write(&self, _buf: &[u8]) -> io::Result<usize> {
        Err(io::Error::from(SysError::new(Errno::EBADF)))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// `realpath`: every symbolic link resolved, `.` and `..` removed.
pub fn realpath(
    lstat: &dyn Fn(&str) -> Result<Stat, SysError>,
    readlink: &dyn Fn(&str) -> Result<String, SysError>,
    path: &str,
) -> Result<String, SysError> {
    let mut todo: Vec<String> = path
        .split('/')
        .filter(|c| !c.is_empty())
        .rev()
        .map(str::to_string)
        .collect();
    let mut done: Vec<String> = Vec::new();
    let mut links = 0;
    while let Some(c) = todo.pop() {
        match c.as_str() {
            "." => continue,
            ".." => {
                done.pop();
                continue;
            }
            _ => {}
        }
        let at = format!("/{}", {
            let mut v = done.clone();
            v.push(c.clone());
            v.join("/")
        });
        let st = lstat(&at)?;
        if st.ftype == FileType::Symlink {
            links += 1;
            if links > 32 {
                return Err(Errno::ELOOP.into());
            }
            let target = readlink(&at)?;
            if target.starts_with('/') {
                done.clear();
            }
            for t in target.split('/').filter(|t| !t.is_empty()).rev() {
                todo.push(t.to_string());
            }
        } else {
            if !todo.is_empty() && st.ftype != FileType::Dir {
                return Err(Errno::ENOTDIR.into());
            }
            done.push(c);
        }
    }
    Ok(format!("/{}", done.join("/")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_map_to_the_systems() {
        let f = OpenFlags {
            write: true,
            create: true,
            truncate: true,
            ..OpenFlags::default()
        };
        assert_eq!(oflag(f), O_WRONLY | O_CREAT | O_TRUNC);
        assert_eq!(oflag(OpenFlags::default()), O_RDONLY);
    }

    #[test]
    fn realpath_follows_links() {
        let lstat = |p: &str| -> Result<Stat, SysError> {
            let ftype = match p {
                "/a" | "/a/b" | "/c" => FileType::Dir,
                "/a/l" => FileType::Symlink,
                _ => return Err(Errno::ENOENT.into()),
            };
            Ok(Stat {
                ftype,
                mode: 0o755,
                size: 0,
                uid: 0,
                gid: 0,
                mtime: 0,
                ino: 0,
                dev: 0,
                nlink: 1,
                blocks: 0,
            })
        };
        let readlink = |_: &str| -> Result<String, SysError> { Ok("../c".into()) };
        assert_eq!(realpath(&lstat, &readlink, "/a/./b/../l").unwrap(), "/c");
        assert!(realpath(&lstat, &readlink, "/nope").is_err());
    }
}
