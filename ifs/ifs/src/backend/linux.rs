// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The backend over a real directory (the xfs mounted at
//! `/run/qs400/ifs`). Every call works relative to the top's directory
//! descriptor: the parent is opened with `openat2` refusing symbolic
//! links and escapes, then the `*at` call acts on the last name without
//! following it. Extended attributes go through `/proc/self/fd`, which
//! reaches a symbolic link or FIFO opened with `O_PATH` as well as a file.

use std::os::fd::{AsFd, OwnedFd};

use rustix::fs::{
    self as rfs, AtFlags, FileType, Mode, OFlags, RenameFlags, ResolveFlags, Timespec, Timestamps,
    XattrFlags,
};

use super::{BFile, BStat, Backend, DirEntry, Kind, Nanos, StatFs};
use crate::errno::linux::from_linux;
use crate::errno::{Errno, Result};

fn e(err: rustix::io::Errno) -> Errno {
    from_linux(err.raw_os_error())
}

fn kind_of(ft: FileType) -> Option<Kind> {
    Some(match ft {
        FileType::RegularFile => Kind::File,
        FileType::Directory => Kind::Dir,
        FileType::Symlink => Kind::Symlink,
        FileType::Fifo => Kind::Fifo,
        FileType::Socket => Kind::Socket,
        FileType::CharacterDevice => Kind::CharDev,
        _ => return None,
    })
}

fn nanos(sec: i64, nsec: u64) -> Nanos {
    sec.saturating_mul(1_000_000_000)
        .saturating_add(i64::try_from(nsec).unwrap_or(0))
}

#[allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::unnecessary_cast
)]
fn bstat(s: &rfs::Stat) -> Result<BStat> {
    let kind = kind_of(FileType::from_raw_mode(s.st_mode as _)).ok_or(Errno::ENOTSUP)?;
    Ok(BStat {
        kind,
        ino: s.st_ino as u64,
        dev: s.st_dev as u64,
        nlink: s.st_nlink as u32,
        size: s.st_size as u64,
        alloc: (s.st_blocks as u64) * 512,
        blksize: s.st_blksize as u32,
        atime: nanos(s.st_atime as i64, s.st_atime_nsec as u64),
        mtime: nanos(s.st_mtime as i64, s.st_mtime_nsec as u64),
        ctime: nanos(s.st_ctime as i64, s.st_ctime_nsec as u64),
        mode: (s.st_mode as u32) & 0o7777,
        uid: s.st_uid as u32,
        gid: s.st_gid as u32,
    })
}

/// A directory on a Linux file system with `trusted.` extended
/// attributes (the caller is root).
#[derive(Debug)]
pub struct LinuxBackend {
    top: OwnedFd,
}

impl LinuxBackend {
    /// Open the directory at `path` as the top.
    pub fn open(path: &str) -> Result<Self> {
        let top = rfs::open(
            path,
            OFlags::DIRECTORY | OFlags::RDONLY | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(e)?;
        Ok(Self { top })
    }

    /// The directory holding `p`'s last name, opened `O_PATH`, and the
    /// name (`.` for the top itself).
    fn at<'p>(&self, p: &'p str) -> Result<(OwnedFd, &'p str)> {
        let dir = super::parent(p);
        let name = super::last(p);
        let fd = rfs::openat2(
            &self.top,
            if dir.is_empty() { "." } else { dir },
            OFlags::PATH | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
            ResolveFlags::BENEATH | ResolveFlags::NO_SYMLINKS | ResolveFlags::NO_MAGICLINKS,
        )
        .map_err(e)?;
        Ok((fd, if name.is_empty() { "." } else { name }))
    }

    /// The node itself opened `O_PATH`, never following it.
    fn node(&self, p: &str) -> Result<OwnedFd> {
        let (dir, name) = self.at(p)?;
        rfs::openat(
            &dir,
            name,
            OFlags::PATH | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(e)
    }

    fn proc_path(fd: &OwnedFd) -> String {
        use std::os::fd::AsRawFd;
        format!("/proc/self/fd/{}", fd.as_raw_fd())
    }
}

/// An open file.
#[derive(Debug)]
pub struct LinuxFile {
    fd: OwnedFd,
}

impl BFile for LinuxFile {
    fn read_at(&self, buf: &mut [u8], off: u64) -> Result<usize> {
        rustix::io::pread(&self.fd, buf, off).map_err(e)
    }

    fn write_at(&self, buf: &[u8], off: u64) -> Result<usize> {
        rustix::io::pwrite(&self.fd, buf, off).map_err(e)
    }

    fn stat(&self) -> Result<BStat> {
        bstat(&rfs::fstat(&self.fd).map_err(e)?)
    }

    fn set_len(&self, len: u64) -> Result<()> {
        rfs::ftruncate(&self.fd, len).map_err(e)
    }

    fn sync(&self, data_only: bool) -> Result<()> {
        if data_only {
            rfs::fdatasync(&self.fd).map_err(e)
        } else {
            rfs::fsync(&self.fd).map_err(e)
        }
    }
}

fn ts(t: Option<Nanos>) -> Timespec {
    match t {
        Some(n) => Timespec {
            tv_sec: n.div_euclid(1_000_000_000),
            tv_nsec: n.rem_euclid(1_000_000_000),
        },
        None => Timespec {
            tv_sec: 0,
            tv_nsec: rfs::UTIME_OMIT,
        },
    }
}

impl Backend for LinuxBackend {
    type File = LinuxFile;

    fn stat(&self, p: &str) -> Result<BStat> {
        if p.is_empty() {
            return bstat(&rfs::fstat(&self.top).map_err(e)?);
        }
        let (dir, name) = self.at(p)?;
        bstat(&rfs::statat(&dir, name, AtFlags::SYMLINK_NOFOLLOW).map_err(e)?)
    }

    fn readdir(&self, p: &str) -> Result<Vec<DirEntry>> {
        let (dir, name) = self.at(p)?;
        let fd = rfs::openat(
            &dir,
            name,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(e)?;
        let mut d = rfs::Dir::read_from(&fd).map_err(e)?;
        let mut out = Vec::new();
        while let Some(ent) = d.read() {
            let ent = ent.map_err(e)?;
            let raw = ent.file_name().to_bytes();
            if raw == b"." || raw == b".." {
                continue;
            }
            let name = String::from_utf8(raw.to_vec()).map_err(|_| Errno::EILSEQ)?;
            let kind = if let Some(k) = kind_of(ent.file_type()) {
                k
            } else {
                let s = rfs::statat(&fd, name.as_str(), AtFlags::SYMLINK_NOFOLLOW).map_err(e)?;
                bstat(&s)?.kind
            };
            out.push(DirEntry {
                name,
                ino: ent.ino(),
                kind,
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    fn mkdir(&self, p: &str, mode: u32) -> Result<()> {
        let (dir, name) = self.at(p)?;
        rfs::mkdirat(&dir, name, Mode::from_raw_mode(mode)).map_err(e)
    }

    fn rmdir(&self, p: &str) -> Result<()> {
        let (dir, name) = self.at(p)?;
        rfs::unlinkat(&dir, name, AtFlags::REMOVEDIR).map_err(e)
    }

    fn create(&self, p: &str, mode: u32) -> Result<LinuxFile> {
        let (dir, name) = self.at(p)?;
        let fd = rfs::openat(
            &dir,
            name,
            OFlags::CREATE | OFlags::EXCL | OFlags::RDWR | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(mode),
        )
        .map_err(e)?;
        Ok(LinuxFile { fd })
    }

    fn open(&self, p: &str, write: bool) -> Result<LinuxFile> {
        let (dir, name) = self.at(p)?;
        let s = bstat(&rfs::statat(&dir, name, AtFlags::SYMLINK_NOFOLLOW).map_err(e)?)?;
        match s.kind {
            Kind::File => {}
            Kind::Dir => return Err(Errno::EISDIR),
            Kind::Symlink => return Err(Errno::ELOOP),
            _ => return Err(Errno::ENOTSUP),
        }
        let acc = if write { OFlags::RDWR } else { OFlags::RDONLY };
        let fd = rfs::openat(
            &dir,
            name,
            acc | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .map_err(e)?;
        Ok(LinuxFile { fd })
    }

    fn mknod(&self, p: &str, kind: Kind, mode: u32) -> Result<()> {
        let ft = match kind {
            Kind::Fifo => FileType::Fifo,
            Kind::Socket => FileType::Socket,
            _ => return Err(Errno::ENOTSUP),
        };
        let (dir, name) = self.at(p)?;
        rfs::mknodat(&dir, name, ft, Mode::from_raw_mode(mode), 0).map_err(e)
    }

    fn unlink(&self, p: &str) -> Result<()> {
        let (dir, name) = self.at(p)?;
        rfs::unlinkat(&dir, name, AtFlags::empty()).map_err(e)
    }

    fn rename(&self, from: &str, to: &str, replace: bool) -> Result<()> {
        let (fd1, n1) = self.at(from)?;
        let (fd2, n2) = self.at(to)?;
        let flags = if replace {
            RenameFlags::empty()
        } else {
            RenameFlags::NOREPLACE
        };
        rfs::renameat_with(&fd1, n1, &fd2, n2, flags).map_err(e)
    }

    fn link(&self, existing: &str, new: &str) -> Result<()> {
        let (fd1, n1) = self.at(existing)?;
        let (fd2, n2) = self.at(new)?;
        rfs::linkat(&fd1, n1, &fd2, n2, AtFlags::empty()).map_err(e)
    }

    fn symlink(&self, target: &str, p: &str) -> Result<()> {
        let (dir, name) = self.at(p)?;
        rfs::symlinkat(target, &dir, name).map_err(e)
    }

    fn readlink(&self, p: &str) -> Result<String> {
        let (dir, name) = self.at(p)?;
        let s = rfs::readlinkat(&dir, name, Vec::new()).map_err(e)?;
        String::from_utf8(s.into_bytes()).map_err(|_| Errno::EILSEQ)
    }

    fn set_times(&self, p: &str, atime: Option<Nanos>, mtime: Option<Nanos>) -> Result<()> {
        let (dir, name) = self.at(p)?;
        let t = Timestamps {
            last_access: ts(atime),
            last_modification: ts(mtime),
        };
        rfs::utimensat(&dir, name, &t, AtFlags::SYMLINK_NOFOLLOW).map_err(e)
    }

    fn set_mode(&self, p: &str, mode: u32) -> Result<()> {
        if self.stat(p)?.kind == Kind::Symlink {
            return Ok(());
        }
        let (dir, name) = self.at(p)?;
        rfs::chmodat(
            &dir,
            name,
            Mode::from_raw_mode(mode & 0o7777),
            AtFlags::empty(),
        )
        .map_err(e)
    }

    fn set_owner(&self, p: &str, uid: u32, gid: u32) -> Result<()> {
        let (dir, name) = self.at(p)?;
        rfs::chownat(
            &dir,
            name,
            Some(rfs::Uid::from_raw(uid)),
            Some(rfs::Gid::from_raw(gid)),
            AtFlags::SYMLINK_NOFOLLOW,
        )
        .map_err(e)
    }

    fn xattr_get(&self, p: &str, name: &str) -> Result<Option<Vec<u8>>> {
        let fd = self.node(p)?;
        let path = Self::proc_path(&fd);
        let mut buf = vec![0u8; 512];
        loop {
            match rfs::getxattr(path.as_str(), name, &mut buf[..]) {
                Ok(n) => {
                    buf.truncate(n);
                    return Ok(Some(buf));
                }
                Err(rustix::io::Errno::NODATA) => return Ok(None),
                Err(rustix::io::Errno::RANGE) if buf.len() < 1 << 16 => {
                    buf.resize(buf.len() * 4, 0);
                }
                Err(err) => return Err(e(err)),
            }
        }
    }

    fn xattr_set(&self, p: &str, name: &str, value: &[u8]) -> Result<()> {
        let fd = self.node(p)?;
        rfs::setxattr(
            Self::proc_path(&fd).as_str(),
            name,
            value,
            XattrFlags::empty(),
        )
        .map_err(e)
    }

    fn xattr_list(&self, p: &str) -> Result<Vec<String>> {
        let fd = self.node(p)?;
        let path = Self::proc_path(&fd);
        let mut buf = vec![0u8; 1024];
        let n = loop {
            match rfs::listxattr(path.as_str(), &mut buf[..]) {
                Ok(n) => break n,
                Err(rustix::io::Errno::RANGE) if buf.len() < 1 << 16 => {
                    buf.resize(buf.len() * 4, 0);
                }
                Err(err) => return Err(e(err)),
            }
        };
        Ok(buf[..n]
            .split(|&b| b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect())
    }

    fn xattr_remove(&self, p: &str, name: &str) -> Result<()> {
        let fd = self.node(p)?;
        match rfs::removexattr(Self::proc_path(&fd).as_str(), name) {
            Ok(()) | Err(rustix::io::Errno::NODATA) => Ok(()),
            Err(err) => Err(e(err)),
        }
    }

    #[allow(clippy::unnecessary_cast)]
    fn statfs(&self) -> Result<StatFs> {
        let s = rfs::fstatvfs(self.top.as_fd()).map_err(e)?;
        Ok(StatFs {
            block_size: s.f_frsize as u64,
            blocks: s.f_blocks as u64,
            bfree: s.f_bfree as u64,
            bavail: s.f_bavail as u64,
            files: s.f_files as u64,
            ffree: s.f_ffree as u64,
            name_max: s.f_namemax as u64,
            fsid: s.f_fsid as u64,
        })
    }
}
