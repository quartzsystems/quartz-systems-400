// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the IFS says, as the library wants it: errors, times, attributes
//! and file IDs.

use ifs::Errno;
use ifs::backend::Nanos;
use ifs::client::IfsClient;
use ifs::ops::{Attr, AttrId, Stat};
use smb_server::{FileInfo, SmbError};

/// `FILE_ATTRIBUTE_HIDDEN`.
pub const HIDDEN: u32 = 0x2;
/// `FILE_ATTRIBUTE_SYSTEM`.
pub const SYSTEM: u32 = 0x4;
/// `FILE_ATTRIBUTE_ARCHIVE`.
pub const ARCHIVE: u32 = 0x20;
/// `FILE_ATTRIBUTE_READONLY`.
pub const READONLY: u32 = 0x1;

/// 100 ns ticks from 1601 to 1970.
const EPOCH_DIFF: i64 = 116_444_736_000_000_000;

/// An IFS error as the library's.
#[must_use]
pub fn smb_error(e: Errno) -> SmbError {
    match e {
        Errno::ENOENT => SmbError::NotFound,
        Errno::EACCES | Errno::EPERM | Errno::EROFS | Errno::EROOBJ => SmbError::AccessDenied,
        Errno::EEXIST => SmbError::Exists,
        Errno::ENOTEMPTY => SmbError::NotEmpty,
        Errno::EISDIR => SmbError::IsDirectory,
        Errno::ENOTDIR => SmbError::NotADirectory,
        Errno::EBUSY | Errno::ETXTBSY | Errno::EAGAIN | Errno::ELOCKED => SmbError::Sharing,
        Errno::ENOSPC | Errno::EFBIG => SmbError::DiskFull,
        Errno::EINVAL => SmbError::InvalidParameter,
        Errno::ENAMETOOLONG | Errno::EBADNAME | Errno::EILSEQ => SmbError::NameInvalid,
        Errno::ENOTSUP | Errno::ENOSYS => SmbError::NotSupported,
        Errno::EXDEV => SmbError::NotSameDevice,
        other => SmbError::Io(std::io::Error::other(format!("{other:?}"))),
    }
}

/// IFS nanoseconds since 1970 as a FILETIME.
#[must_use]
pub fn filetime(ns: Nanos) -> u64 {
    u64::try_from(ns.div_euclid(100).saturating_add(EPOCH_DIFF)).unwrap_or(0)
}

/// A FILETIME as IFS nanoseconds.
#[must_use]
pub fn nanos(ft: u64) -> Nanos {
    (i64::try_from(ft).unwrap_or(i64::MAX) - EPOCH_DIFF).saturating_mul(100)
}

/// The IFS attributes a [`FileInfo`] needs.
pub const WANTED: &[AttrId] = &[
    AttrId::PcReadOnly,
    AttrId::PcHidden,
    AttrId::PcSystem,
    AttrId::PcArchive,
    AttrId::FileId,
];

/// The file ID the library reports: the IFS file ID's file number and
/// generation (never 0; the root's number is kept).
#[must_use]
pub fn file_index(fs: u32, ino: u64, generation: u32) -> u64 {
    let _ = fs;
    let v = (u64::from(generation) << 48) ^ ino;
    if v == 0 { 1 } else { v }
}

/// A [`FileInfo`] named `name` from `st` and the attributes `attrs`.
#[must_use]
pub fn file_info(name: &str, st: &Stat, attrs: &[Attr]) -> FileInfo {
    let mut info = FileInfo::new(name);
    info.is_directory = st.mode & ifs::ops::flags::S_IFMT == ifs::ops::flags::S_IFDIR;
    info.end_of_file = st.size;
    info.allocation_size = st.allocsize.max(st.size);
    info.creation_time = filetime(st.crtime);
    info.last_access_time = filetime(st.atime);
    info.last_write_time = filetime(st.mtime);
    info.change_time = filetime(st.ctime);
    info.unix_mode = Some(st.mode & 0o177_777);
    info.file_index = file_index(st.dev, st.ino, st.ino_gen);
    for a in attrs {
        match *a {
            Attr::PcReadOnly(v) => info.read_only = v && !info.is_directory,
            Attr::PcHidden(true) => info.file_attributes |= HIDDEN,
            Attr::PcSystem(true) => info.file_attributes |= SYSTEM,
            Attr::PcArchive(true) => info.file_attributes |= ARCHIVE,
            Attr::FileId(id) => info.file_index = file_index(id.fs, id.ino, id.generation),
            _ => {}
        }
    }
    info
}

/// Stats `path` (not following a last symbolic link) with its attributes;
/// attributes the object type lacks are left out.
///
/// # Errors
///
/// The stat's.
pub fn stat_path(c: &dyn IfsClient, path: &str) -> ifs::Result<(Stat, Vec<Attr>)> {
    let st = c.stat(path, true)?;
    let attrs = c.get_attrs(path, WANTED, true).unwrap_or_default();
    Ok((st, attrs))
}

/// The attribute changes that make the IFS's match `attributes`.
#[must_use]
pub fn attr_changes(attributes: u32, directory: bool) -> Vec<Attr> {
    let mut v = vec![
        Attr::PcHidden(attributes & HIDDEN != 0),
        Attr::PcSystem(attributes & SYSTEM != 0),
        Attr::PcArchive(attributes & ARCHIVE != 0),
    ];
    if !directory {
        v.push(Attr::PcReadOnly(attributes & READONLY != 0));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_round_trip() {
        assert_eq!(filetime(0), 116_444_736_000_000_000);
        let ns = 1_700_000_000_123_456_700;
        assert_eq!(nanos(filetime(ns)), ns);
    }

    #[test]
    fn errors_map() {
        assert!(matches!(smb_error(Errno::ENOENT), SmbError::NotFound));
        assert!(matches!(smb_error(Errno::EBUSY), SmbError::Sharing));
        assert!(matches!(smb_error(Errno::EACCES), SmbError::AccessDenied));
    }
}
