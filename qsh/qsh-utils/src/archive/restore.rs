// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Making files from members: replacing what is there (unless kept or
//! newer), parents made as needed, modes, owners and times restored as
//! asked, directories' modes and times set after their contents.

use qsh::sys::stream::write_all;
use qsh::sys::{Errno, FileType, SysError, UtilCtx};

use super::member::{Kind, Member};
use crate::common::io::open_write;

/// What to restore.
#[derive(Clone, Copy, Debug, Default)]
pub struct Restore {
    /// Never replace an existing file (`-k`).
    pub keep: bool,
    /// Replace only an older file (`-u`).
    pub newer_only: bool,
    /// Keep every mode bit, set-ID bits included, without the umask.
    pub mode: bool,
    /// Restore owner and group.
    pub owner: bool,
    /// Restore modification times.
    pub mtime: bool,
}

/// What became of a member.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Done {
    /// Made.
    Made,
    /// Left alone (`-k`, `-u`).
    Skipped,
}

/// A restore in progress: directories wait for their final settings.
pub struct Restorer {
    opts: Restore,
    dirs: Vec<(String, Member)>,
}

fn parent(p: &str) -> String {
    match p.trim_end_matches('/').rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(i) => p[..i].to_string(),
    }
}

/// Make a directory and its missing parents.
pub fn mkdirs(cx: &UtilCtx<'_>, abs: &str) -> Result<(), SysError> {
    if cx.sys.stat(abs).is_ok() {
        return Ok(());
    }
    let up = parent(abs);
    if up != abs {
        mkdirs(cx, &up)?;
    }
    match cx.sys.mkdir(abs, 0o777 & !cx.umask) {
        Err(e) if e.errno == Errno::EEXIST => Ok(()),
        r => r,
    }
}

impl Restorer {
    /// A restore with these options.
    pub fn new(opts: Restore) -> Self {
        Self {
            opts,
            dirs: Vec::new(),
        }
    }

    fn perms(&self, cx: &UtilCtx<'_>, m: &Member) -> u32 {
        if self.opts.mode {
            m.mode & 0o7777
        } else {
            m.mode & 0o777 & !cx.umask
        }
    }

    fn finish_one(&self, cx: &UtilCtx<'_>, abs: &str, m: &Member) -> Result<(), SysError> {
        if m.kind == Kind::Symlink {
            return Ok(());
        }
        if self.opts.owner {
            let uid = cx.sys.user_id(&m.uname).unwrap_or(m.uid);
            let gid = cx.sys.group_id(&m.gname).unwrap_or(m.gid);
            cx.sys.chown(abs, Some(uid), Some(gid))?;
        }
        cx.sys.chmod(abs, self.perms(cx, m))?;
        if self.opts.mtime {
            cx.sys.utimes(abs, m.mtime)?;
        }
        Ok(())
    }

    /// Make one member at `abs`; `link_abs` is where a hard link's
    /// target was made.
    pub fn put(
        &mut self,
        cx: &UtilCtx<'_>,
        m: &Member,
        abs: &str,
        link_abs: &str,
    ) -> Result<Done, SysError> {
        if let Ok(old) = cx.sys.lstat(abs) {
            if self.opts.keep {
                return Ok(Done::Skipped);
            }
            if self.opts.newer_only && old.mtime >= m.mtime {
                return Ok(Done::Skipped);
            }
            if old.ftype == FileType::Dir {
                if m.kind == Kind::Dir {
                    self.dirs.push((abs.to_string(), m.clone()));
                    return Ok(Done::Made);
                }
                cx.sys.rmdir(abs)?;
            } else {
                cx.sys.unlink(abs)?;
            }
        }
        mkdirs(cx, &parent(abs))?;
        match m.kind {
            Kind::File => {
                write_file(cx, abs, &m.data)?;
            }
            Kind::Dir => {
                cx.sys.mkdir(abs, 0o700)?;
                self.dirs.push((abs.to_string(), m.clone()));
                return Ok(Done::Made);
            }
            Kind::Symlink => cx.sys.symlink(&m.link, abs)?,
            Kind::Hardlink => {
                // A cpio link carries the data again: write it through
                // the link and settle the shared file's attributes.
                match cx.sys.link(link_abs, abs) {
                    Ok(()) if m.data.is_empty() => return Ok(Done::Made),
                    Ok(()) => write_file(cx, abs, &m.data)?,
                    Err(_) if !m.data.is_empty() => write_file(cx, abs, &m.data)?,
                    Err(e) => return Err(e),
                }
            }
            Kind::Fifo => cx.sys.mkfifo(abs, self.perms(cx, m))?,
            Kind::Char | Kind::Block => return Err(Errno::EPERM.into()),
        }
        self.finish_one(cx, abs, m)?;
        Ok(Done::Made)
    }

    /// Set the waiting directories' modes and times, deepest first.
    pub fn finish(&mut self, cx: &UtilCtx<'_>) -> Vec<(String, SysError)> {
        let mut errs = Vec::new();
        let mut dirs = std::mem::take(&mut self.dirs);
        dirs.sort_by_key(|d| std::cmp::Reverse(d.0.len()));
        for (abs, m) in dirs {
            if let Err(e) = self.finish_one(cx, &abs, &m) {
                errs.push((abs, e));
            }
        }
        errs
    }
}

/// Write a whole file, replacing it.
pub fn write_file(cx: &UtilCtx<'_>, abs: &str, data: &[u8]) -> Result<(), SysError> {
    let fd = open_write(cx, abs, false)?;
    write_all(&*fd, data).map_err(SysError::from)
}
