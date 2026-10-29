// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `ipc::Wire` codecs of everything that crosses between a job and
//! the storage service: arguments, answers and errors. An `Errno` goes
//! as its IBM number; enums as one-byte tags.

use ipc::{Dec, Wire, WireError, wire_enum, wire_struct};

use crate::auth::data::DataAuth;
use crate::errno::Errno;
use crate::lock::range::{RangeKind, RangeLock};
use crate::lock::share::{Access, Share};
use crate::lock::{Held, LockInfo, ObjKey, Owner};
use crate::meta::reclaim::{Problem, Repair, Report};
use crate::meta::{CheckOut, CrtObjAud, Journal, ObjAudit, Scan, StgOpt};
use crate::ops::audit::FileId;
use crate::ops::{
    Attr, AttrId, AuthEntry, AuthList, ChgAut, CopyOpts, DirEnt, OpenArgs, RenameMode, Role, Stat,
    StatVfs, UdfsInfo, WalkOpts, Who,
};
use crate::resolve::NodeType;

use super::WalkEntry;

fn bad<T>(what: &'static str, tag: u8) -> Result<T, WireError> {
    Err(WireError::Tag { what, tag })
}

impl Wire for Errno {
    fn put(&self, out: &mut Vec<u8>) {
        self.code().put(out);
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        let code = i32::get(d)?;
        Errno::from_code(code).map_or_else(
            || bad("Errno", u8::try_from(code % 256).unwrap_or(u8::MAX)),
            Ok,
        )
    }
}

impl Wire for Owner {
    fn put(&self, out: &mut Vec<u8>) {
        self.0.put(out);
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        u64::get(d).map(Self)
    }
}

impl Wire for DataAuth {
    fn put(&self, out: &mut Vec<u8>) {
        self.0.put(out);
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        u8::get(d).map(Self)
    }
}

impl Wire for AttrId {
    fn put(&self, out: &mut Vec<u8>) {
        (*self as u32).put(out);
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        let v = u32::get(d)?;
        AttrId::from_u32(v).map_or_else(|| bad("AttrId", u8::try_from(v).unwrap_or(u8::MAX)), Ok)
    }
}

wire_enum!(NodeType {
    0 => Dir,
    1 => File,
    2 => Symlink,
    3 => Fifo,
    4 => Socket,
    5 => Chr,
    6 => Blk,
    7 => Native,
});
wire_enum!(RenameMode { 0 => Keep, 1 => Unlink });
wire_enum!(RangeKind { 0 => Read, 1 => Write, 2 => Unlock });
wire_enum!(Share { 0 => ReadOnly, 1 => WriteOnly, 2 => ReadWrite, 3 => None });
wire_enum!(StgOpt { 0 => Normal, 1 => Minimize, 2 => Dynamic });
wire_enum!(Scan { 0 => No, 1 => Yes, 2 => ChgOnly });
wire_enum!(ObjAudit { 0 => None, 1 => UsrPrf, 2 => Change, 3 => All });
wire_enum!(Role { 0 => Public, 1 => Owner, 2 => Group, 3 => Private });

wire_struct!(DirEnt { name, ino, ntype });
wire_struct!(Stat {
    mode,
    ino,
    nlink,
    uid,
    gid,
    size,
    atime,
    mtime,
    ctime,
    dev,
    blksize,
    allocsize,
    objtype,
    codepage,
    ccsid,
    rdev,
    vfs,
    ino_gen,
    owner,
    group,
    crtime
});
wire_struct!(StatVfs {
    bsize,
    frsize,
    blocks,
    bfree,
    bavail,
    files,
    ffree,
    favail,
    fsid,
    flag,
    namemax,
    name,
    case_sensitive
});
wire_struct!(OpenArgs {
    oflag,
    mode,
    conversion_id,
    text_conversion_id
});
wire_struct!(RangeLock {
    owner,
    kind,
    start,
    end
});
wire_struct!(ObjKey { fs, ino });
wire_struct!(Access { read, write });
wire_struct!(LockInfo { path, owner, what });
wire_struct!(FileId {
    fs,
    ino,
    generation
});
wire_struct!(CheckOut { user, time });
wire_struct!(Journal {
    journal,
    images,
    omit_open_close,
    inherit,
    start,
    jid
});
wire_struct!(ChgAut {
    who,
    data,
    data_autl,
    objects,
    autl
});
wire_struct!(AuthEntry {
    who,
    role,
    data,
    objects,
    auth
});
wire_struct!(AuthList {
    path,
    objtype,
    owner,
    group,
    autl,
    entries
});
wire_struct!(CopyOpts {
    subtree,
    replace,
    to_ccsid,
    keep_owner
});
wire_struct!(WalkOpts { follow, one_fs });
wire_struct!(WalkEntry { path, stat });
wire_struct!(UdfsInfo {
    path,
    case_sensitive,
    owner,
    text,
    created,
    mounted_at,
    crtobjaud
});
wire_struct!(Report {
    objects,
    repaired,
    problems
});

impl Wire for Held {
    fn put(&self, out: &mut Vec<u8>) {
        match self {
            Self::Open(a, s) => {
                out.push(0);
                a.put(out);
                s.put(out);
            }
            Self::Range(r) => {
                out.push(1);
                r.put(out);
            }
        }
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        match u8::get(d)? {
            0 => Ok(Self::Open(Access::get(d)?, Share::get(d)?)),
            1 => Ok(Self::Range(RangeLock::get(d)?)),
            t => bad("Held", t),
        }
    }
}

impl Wire for CrtObjAud {
    fn put(&self, out: &mut Vec<u8>) {
        match self {
            Self::SysVal => out.push(0),
            Self::Value(v) => {
                out.push(1);
                v.put(out);
            }
        }
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        match u8::get(d)? {
            0 => Ok(Self::SysVal),
            1 => Ok(Self::Value(ObjAudit::get(d)?)),
            t => bad("CrtObjAud", t),
        }
    }
}

impl Wire for Who {
    fn put(&self, out: &mut Vec<u8>) {
        match self {
            Self::Public => out.push(0),
            Self::User(u) => {
                out.push(1);
                u.put(out);
            }
        }
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        match u8::get(d)? {
            0 => Ok(Self::Public),
            1 => Ok(Self::User(String::get(d)?)),
            t => bad("Who", t),
        }
    }
}

impl Wire for Repair {
    fn put(&self, out: &mut Vec<u8>) {
        let (tag, text): (u8, Option<&String>) = match self {
            Self::MissingMeta => (0, None),
            Self::InvalidMeta => (1, None),
            Self::Owner(s) => (2, Some(s)),
            Self::Group(s) => (3, Some(s)),
            Self::Private(s) => (4, Some(s)),
            Self::Autl(s) => (5, Some(s)),
            Self::CheckOut(s) => (6, Some(s)),
            Self::StrayAttr(s) => (7, Some(s)),
            Self::Mirror => (8, None),
        };
        out.push(tag);
        if let Some(s) = text {
            s.put(out);
        }
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        Ok(match u8::get(d)? {
            0 => Self::MissingMeta,
            1 => Self::InvalidMeta,
            2 => Self::Owner(String::get(d)?),
            3 => Self::Group(String::get(d)?),
            4 => Self::Private(String::get(d)?),
            5 => Self::Autl(String::get(d)?),
            6 => Self::CheckOut(String::get(d)?),
            7 => Self::StrayAttr(String::get(d)?),
            8 => Self::Mirror,
            t => return bad("Repair", t),
        })
    }
}

impl Wire for Problem {
    fn put(&self, out: &mut Vec<u8>) {
        match self {
            Self::LinkCount { nlink, found } => {
                out.push(0);
                nlink.put(out);
                found.put(out);
            }
            Self::FoldCollision(s) => {
                out.push(1);
                s.put(out);
            }
        }
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        match u8::get(d)? {
            0 => Ok(Self::LinkCount {
                nlink: u32::get(d)?,
                found: u32::get(d)?,
            }),
            1 => Ok(Self::FoldCollision(String::get(d)?)),
            t => bad("Problem", t),
        }
    }
}

/// An attribute goes as its ID, then the value its variant carries.
impl Wire for Attr {
    fn put(&self, out: &mut Vec<u8>) {
        self.id().put(out);
        match self {
            Attr::ObjType(s) | Attr::Text(s) => s.put(out),
            Attr::DataSize(v) | Attr::AllocSize(v) | Attr::ExtendedAttrSize(v) => v.put(out),
            Attr::CreateTime(t)
            | Attr::AccessTime(t)
            | Attr::ChangeTime(t)
            | Attr::ModifyTime(t) => {
                t.put(out);
            }
            Attr::StgFree(b)
            | Attr::LocalRemote(b)
            | Attr::PcReadOnly(b)
            | Attr::PcHidden(b)
            | Attr::PcSystem(b)
            | Attr::PcArchive(b)
            | Attr::SystemArchive(b)
            | Attr::Alwckpwrt(b)
            | Attr::Signed(b)
            | Attr::SysSigned(b)
            | Attr::MultSigs(b)
            | Attr::Alwsav(b)
            | Attr::Rstdrnmunl(b)
            | Attr::Temporary(b)
            | Attr::InheritAlwckpwrt(b)
            | Attr::SysRestrictsSave(b)
            | Attr::Suid(b)
            | Attr::Sgid(b) => b.put(out),
            Attr::CheckedOut(c) => c.put(out),
            Attr::Auth { owner, group, have } => {
                owner.put(out);
                group.put(out);
                have.put(out);
            }
            Attr::FileId(id) => id.put(out),
            Attr::Asp(n) | Attr::Codepage(n) | Attr::Ccsid(n) => n.put(out),
            Attr::FileFormat(n) | Attr::DirFormat(n) | Attr::SystemUse(n) => n.put(out),
            Attr::JournalInformation(j) => j.put(out),
            Attr::DiskStgOpt(s) | Attr::MainStgOpt(s) => s.put(out),
            Attr::Audit(a) => a.put(out),
            Attr::Crtobjscan(s) | Attr::Scan(s) => s.put(out),
            Attr::Crtobjaud(c) => c.put(out),
        }
    }

    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        use AttrId as I;
        Ok(match AttrId::get(d)? {
            I::ObjType => Attr::ObjType(String::get(d)?),
            I::Text => Attr::Text(String::get(d)?),
            I::DataSize | I::DataSize64 => Attr::DataSize(u64::get(d)?),
            I::AllocSize | I::AllocSize64 => Attr::AllocSize(u64::get(d)?),
            I::ExtendedAttrSize => Attr::ExtendedAttrSize(u64::get(d)?),
            I::CreateTime => Attr::CreateTime(i64::get(d)?),
            I::AccessTime => Attr::AccessTime(i64::get(d)?),
            I::ChangeTime => Attr::ChangeTime(i64::get(d)?),
            I::ModifyTime => Attr::ModifyTime(i64::get(d)?),
            I::StgFree => Attr::StgFree(bool::get(d)?),
            I::LocalRemote => Attr::LocalRemote(bool::get(d)?),
            I::PcReadOnly => Attr::PcReadOnly(bool::get(d)?),
            I::PcHidden => Attr::PcHidden(bool::get(d)?),
            I::PcSystem => Attr::PcSystem(bool::get(d)?),
            I::PcArchive => Attr::PcArchive(bool::get(d)?),
            I::SystemArchive => Attr::SystemArchive(bool::get(d)?),
            I::Alwckpwrt => Attr::Alwckpwrt(bool::get(d)?),
            I::Signed => Attr::Signed(bool::get(d)?),
            I::SysSigned => Attr::SysSigned(bool::get(d)?),
            I::MultSigs => Attr::MultSigs(bool::get(d)?),
            I::Alwsav => Attr::Alwsav(bool::get(d)?),
            I::Rstdrnmunl => Attr::Rstdrnmunl(bool::get(d)?),
            I::Temporary => Attr::Temporary(bool::get(d)?),
            I::InheritAlwckpwrt => Attr::InheritAlwckpwrt(bool::get(d)?),
            I::SysRestrictsSave => Attr::SysRestrictsSave(bool::get(d)?),
            I::Suid => Attr::Suid(bool::get(d)?),
            I::Sgid => Attr::Sgid(bool::get(d)?),
            I::CheckedOut => Attr::CheckedOut(Option::get(d)?),
            I::Auth => Attr::Auth {
                owner: String::get(d)?,
                group: Option::get(d)?,
                have: u32::get(d)?,
            },
            I::FileId => Attr::FileId(FileId::get(d)?),
            I::Asp => Attr::Asp(u16::get(d)?),
            I::Codepage => Attr::Codepage(u16::get(d)?),
            I::Ccsid => Attr::Ccsid(u16::get(d)?),
            I::FileFormat => Attr::FileFormat(u8::get(d)?),
            I::DirFormat => Attr::DirFormat(u8::get(d)?),
            I::SystemUse => Attr::SystemUse(u8::get(d)?),
            I::JournalInformation | I::JournalExtendedInformation => {
                Attr::JournalInformation(Option::get(d)?)
            }
            I::DiskStgOpt => Attr::DiskStgOpt(StgOpt::get(d)?),
            I::MainStgOpt => Attr::MainStgOpt(StgOpt::get(d)?),
            I::Audit => Attr::Audit(ObjAudit::get(d)?),
            I::Crtobjscan => Attr::Crtobjscan(Scan::get(d)?),
            I::Scan => Attr::Scan(Scan::get(d)?),
            I::Crtobjaud => Attr::Crtobjaud(CrtObjAud::get(d)?),
            I::UsageInformation
            | I::UdfsDefaultFormat
            | I::ScanInfo
            | I::UdfsTemporary
            | I::UdfsPreferredStorageUnit
            | I::Runexit
            | I::Crtrunexit
            | I::ResetDate => return bad("Attr", 0xFF),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round<T: Wire + PartialEq + std::fmt::Debug>(v: &T) {
        let back: T = ipc::decode(&ipc::encode(v)).unwrap();
        assert_eq!(&back, v);
    }

    #[test]
    fn round_trips() {
        round(&Errno::EACCES);
        round::<Result<i32, Errno>>(&Err(Errno::ENOENT));
        round(&Stat {
            objtype: "*STMF".into(),
            group: Some("DEV".into()),
            ..Stat::default()
        });
        round(&DirEnt {
            name: "a".into(),
            ino: 7,
            ntype: NodeType::Symlink,
        });
        round(&Held::Range(RangeLock::new(
            Owner(3),
            RangeKind::Write,
            1,
            0,
        )));
        round(&Attr::Auth {
            owner: "BOB".into(),
            group: None,
            have: 7,
        });
        round(&Attr::CheckedOut(Some(CheckOut {
            user: "ANN".into(),
            time: 5,
        })));
        round(&Attr::Crtobjaud(CrtObjAud::Value(ObjAudit::All)));
        round(&Attr::DataSize(9));
        round(&Repair::Owner("X".into()));
        round(&Problem::LinkCount { nlink: 2, found: 1 });
        round(&ChgAut {
            who: vec![Who::Public, Who::User("BOB".into())],
            data: Some(DataAuth(7)),
            data_autl: false,
            objects: Some(sec::auth::bits::Auth::ALL),
            autl: Some(None),
        });
        assert!(ipc::decode::<Errno>(&ipc::encode(&1i32)).is_err());
    }
}
