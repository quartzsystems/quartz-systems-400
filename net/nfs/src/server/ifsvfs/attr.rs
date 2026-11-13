// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! IBM's `stat` as the protocols' attributes: the type from the mode's
//! type bits (a `/QSYS.LIB` object that is neither directory nor file
//! is a regular file), the permission bits, the export and file system
//! as `fsid`, the times.

use ifs::ops::Stat;
use ifs::ops::flags as f;

use crate::server::vfs::{Attr, FileType, Req, Time, WccAttr};

/// The type of a `st_mode`.
pub(crate) fn ftype_of(mode: u32) -> FileType {
    match mode & f::S_IFMT {
        f::S_IFDIR => FileType::Dir,
        f::S_IFLNK => FileType::Lnk,
        f::S_IFIFO => FileType::Fifo,
        f::S_IFSOCK => FileType::Sock,
        f::S_IFCHR => FileType::Chr,
        f::S_IFBLK => FileType::Blk,
        _ => FileType::Reg,
    }
}

/// The attributes of `st` in `r`'s export.
pub(crate) fn attr_of(r: &Req, st: &Stat) -> Attr {
    Attr {
        ftype: ftype_of(st.mode),
        mode: st.mode & 0o7777,
        nlink: st.nlink,
        uid: st.uid,
        gid: st.gid,
        size: st.size,
        used: st.allocsize,
        rdev: (
            u32::try_from(st.rdev >> 32).unwrap_or(0),
            u32::try_from(st.rdev & 0xffff_ffff).unwrap_or(0),
        ),
        fsid: u64::from(r.export.id()) << 32 | u64::from(st.vfs),
        fileid: st.ino,
        atime: Time::from_nanos(st.atime),
        mtime: Time::from_nanos(st.mtime),
        ctime: Time::from_nanos(st.ctime),
        change: if st.change == 0 {
            u64::try_from(st.ctime).unwrap_or(0)
        } else {
            st.change
        },
        owner: st.owner.clone(),
        group: st.group.clone(),
        ccsid: st.ccsid,
    }
}

/// What `wcc_data` keeps of `st` before a change.
pub(crate) fn wcc_of(st: &Stat) -> WccAttr {
    WccAttr {
        size: st.size,
        mtime: Time::from_nanos(st.mtime),
        ctime: Time::from_nanos(st.ctime),
    }
}

/// Whether `st` is a directory.
pub(crate) fn is_dir(st: &Stat) -> bool {
    st.mode & f::S_IFMT == f::S_IFDIR
}
