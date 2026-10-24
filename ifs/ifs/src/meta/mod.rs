// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The one metadata layer. Everything QS/400 decides about an object
//! (owner, group, authorities, CCSID, IBM attributes, auditing,
//! check-out, journaling) is one versioned record ([`codec`]) in one
//! extended attribute, [`XATTR`]. Only this module reads or writes it;
//! the backend's mode bits and owner are mirrors written from it for
//! tools, never read for decisions. An object without a valid record
//! gets [`fallback`] and `RCLLNK` ([`reclaim`]) writes one.

pub mod codec;
pub mod reclaim;

use sec::auth::bits::Auth;

use crate::backend::{BStat, Backend, Kind, Nanos};
use crate::errno::Result;

/// The extended attribute holding the record.
pub const XATTR: &str = "trusted.qs400.meta";

/// The owner of objects found without a record whose UID maps to no
/// profile (IBM's default owner).
pub const DEFAULT_OWNER: &str = "QDFTOWN";

/// IBM attribute flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Flags(pub u32);

impl Flags {
    /// `*HIDDEN`.
    pub const HIDDEN: u32 = 0x0001;
    /// `*RDONLY`.
    pub const RDONLY: u32 = 0x0002;
    /// `*SYSTEM`.
    pub const SYSTEM: u32 = 0x0004;
    /// `*PCARCHIVE`: changed since a PC client last archived it.
    pub const PCARCHIVE: u32 = 0x0008;
    /// `*SYSARCHIVE`: changed since last saved.
    pub const SYSARCHIVE: u32 = 0x0010;
    /// `*ALWSAV`: may be saved.
    pub const ALWSAV: u32 = 0x0020;
    /// `*ALWCKPWRT`: may be written while a save checkpoints it.
    pub const ALWCKPWRT: u32 = 0x0040;
    /// `*INHALWCKPWRT` (directories): new objects get `*ALWCKPWRT`.
    pub const INHALWCKPWRT: u32 = 0x0080;
    /// `*RSTDRNMUNL` (`S_ISVTX`): restricted rename and unlink.
    pub const RSTDRNMUNL: u32 = 0x0100;
    /// `S_ISUID`.
    pub const SETUID: u32 = 0x0200;
    /// `S_ISGID`.
    pub const SETGID: u32 = 0x0400;
    /// A file system's root (UDFS) or a QS/400 system directory.
    pub const SYSDIR: u32 = 0x0800;
    /// A UDFS root created `CASE(*MIXED)`: its names compare exactly.
    pub const CASE_SENSITIVE: u32 = 0x1000;

    /// Whether `bit` is set.
    #[must_use]
    pub fn has(self, bit: u32) -> bool {
        self.0 & bit != 0
    }

    /// Set or clear `bit`.
    pub fn set(&mut self, bit: u32, on: bool) {
        if on {
            self.0 |= bit;
        } else {
            self.0 &= !bit;
        }
    }
}

/// `*DISKSTGOPT` and `*MAINSTGOPT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
#[repr(u8)]
pub enum StgOpt {
    /// `*NORMAL`.
    #[default]
    Normal = 0,
    /// `*MINIMIZE`.
    Minimize = 1,
    /// `*DYNAMIC`.
    Dynamic = 2,
}

/// `*CRTOBJSCAN` and `*SCAN` (stored; QS/400 has no scanning exits).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
#[repr(u8)]
pub enum Scan {
    /// `*NO`.
    No = 0,
    /// `*YES`.
    #[default]
    Yes = 1,
    /// `*CHGONLY`.
    ChgOnly = 2,
}

/// An object's auditing value (`OBJAUD`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
#[repr(u8)]
pub enum ObjAudit {
    /// `*NONE`.
    #[default]
    None = 0,
    /// `*USRPRF`: as the user's `OBJAUD` says.
    UsrPrf = 1,
    /// `*CHANGE`.
    Change = 2,
    /// `*ALL`.
    All = 3,
}

impl ObjAudit {
    /// The IBM value.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "*NONE",
            Self::UsrPrf => "*USRPRF",
            Self::Change => "*CHANGE",
            Self::All => "*ALL",
        }
    }

    /// From the IBM value.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_uppercase().as_str() {
            "*NONE" => Self::None,
            "*USRPRF" => Self::UsrPrf,
            "*CHANGE" => Self::Change,
            "*ALL" => Self::All,
            _ => return None,
        })
    }
}

/// A directory's `*CRTOBJAUD`: the auditing value new objects in it get.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum CrtObjAud {
    /// `*SYSVAL`: `QCRTOBJAUD`.
    #[default]
    SysVal,
    /// A value.
    Value(ObjAudit),
}

/// Who checked an object out, and when.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CheckOut {
    /// The user profile.
    pub user: String,
    /// When.
    pub time: Nanos,
}

/// What `STRJRN` stored (no entries are written; see `docs/IFS.md`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Journal {
    /// `LIB/JOURNAL`.
    pub journal: String,
    /// `IMAGES(*AFTER)` 0, `*BOTH` 1.
    pub images: u8,
    /// `OMTJRNE(*OPNCLO)`.
    pub omit_open_close: bool,
    /// New objects in a directory inherit journaling.
    pub inherit: bool,
    /// When journaling started.
    pub start: Nanos,
    /// The journal identifier.
    pub jid: [u8; 10],
}

/// The record.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Meta {
    /// Owner profile.
    pub owner: String,
    /// The owner's authority.
    pub owner_auth: Auth,
    /// Primary group profile.
    pub group: Option<String>,
    /// The primary group's authority.
    pub group_auth: Auth,
    /// `*PUBLIC` authority.
    pub public: Auth,
    /// `*PUBLIC` comes from the authorization list (`*AUTL`).
    pub public_from_autl: bool,
    /// Private authorities, sorted by profile.
    pub private: Vec<(String, Auth)>,
    /// The authorization list securing the object.
    pub autl: Option<String>,
    /// Data CCSID.
    pub ccsid: u16,
    /// Data code page.
    pub codepage: u16,
    /// IBM attribute flags.
    pub flags: Flags,
    /// `*DISKSTGOPT`.
    pub disk_stg: StgOpt,
    /// `*MAINSTGOPT`.
    pub main_stg: StgOpt,
    /// `*CRTOBJSCAN` (directories).
    pub crtobjscan: Scan,
    /// `*SCAN`.
    pub scan: Scan,
    /// `OBJAUD`.
    pub audit: ObjAudit,
    /// `*CRTOBJAUD` (directories).
    pub crtobjaud: CrtObjAud,
    /// Check-out.
    pub checkout: Option<CheckOut>,
    /// Journaling, stored only.
    pub journal: Option<Journal>,
    /// Creation time.
    pub created: Nanos,
    /// Generation, distinguishing objects that reuse an inode number.
    pub generation: u32,
    /// Text description (`*TEXT`, 50 characters).
    pub text: String,
}

impl Meta {
    /// A record for a new object owned by `owner` with everything else
    /// at its default.
    #[must_use]
    pub fn new(owner: &str, ccsid: u16, created: Nanos) -> Self {
        Self {
            owner: owner.to_string(),
            owner_auth: Auth::ALL,
            group: None,
            group_auth: Auth::EXCLUDE,
            public: Auth::EXCLUDE,
            public_from_autl: false,
            private: Vec::new(),
            autl: None,
            ccsid,
            codepage: ccsid::conv::codepage_of(ccsid),
            flags: Flags(Flags::ALWSAV | Flags::SYSARCHIVE | Flags::PCARCHIVE),
            disk_stg: StgOpt::Normal,
            main_stg: StgOpt::Normal,
            crtobjscan: Scan::Yes,
            scan: Scan::Yes,
            audit: ObjAudit::None,
            crtobjaud: CrtObjAud::SysVal,
            checkout: None,
            journal: None,
            created,
            generation: 0,
            text: String::new(),
        }
    }

    /// A profile's private authority.
    #[must_use]
    pub fn private_of(&self, who: &str) -> Option<Auth> {
        self.private.iter().find(|(p, _)| p == who).map(|(_, a)| *a)
    }

    /// Set (`Some`) or remove (`None`) a private authority.
    pub fn set_private(&mut self, who: &str, a: Option<Auth>) {
        self.private.retain(|(p, _)| p != who);
        if let Some(a) = a {
            self.private.push((who.to_string(), a));
            self.private.sort_by(|x, y| x.0.cmp(&y.0));
        }
    }
}

/// Where a record came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The stored record.
    Stored,
    /// No record: the fallback.
    Missing,
    /// A record that would not decode: the fallback.
    Invalid,
}

/// Profile names for the mirrored UID and GID of a node without a
/// record, and the reverse for the mirror.
pub trait IdMap {
    /// The profile with this UID.
    fn profile_of_uid(&self, uid: u32) -> Option<String>;
    /// The group profile with this GID.
    fn profile_of_gid(&self, gid: u32) -> Option<String>;
    /// A profile's UID.
    fn uid_of(&self, profile: &str) -> Option<u32>;
    /// A group profile's GID.
    fn gid_of(&self, profile: &str) -> Option<u32>;
}

/// The record for a node that has none: the owner and group from the
/// mirrored UID and GID ([`DEFAULT_OWNER`] and none when they map to no
/// profile), data authorities from its mode bits (`*R`, `*W`, `*X` for
/// owner, group and other), every object authority for the owner,
/// none for anyone else, CCSID 1208 for stream files and directories.
#[must_use]
pub fn fallback(st: &BStat, ids: &dyn IdMap) -> Meta {
    use crate::auth::data::DataAuth;
    let owner = ids
        .profile_of_uid(st.uid)
        .unwrap_or_else(|| DEFAULT_OWNER.to_string());
    let mut m = Meta::new(&owner, ccsid::conv::UTF8, st.ctime);
    let part = |shift: u32| DataAuth::from_mode_bits((st.mode >> shift) & 7);
    m.owner_auth = part(6).with_objects(Auth::ALL);
    m.group = ids.profile_of_gid(st.gid).filter(|g| *g != owner);
    m.group_auth = part(3).auth();
    m.public = part(0).auth();
    m.flags.set(Flags::RSTDRNMUNL, st.mode & 0o1000 != 0);
    m.flags.set(Flags::SETUID, st.mode & 0o4000 != 0);
    m.flags.set(Flags::SETGID, st.mode & 0o2000 != 0);
    if st.kind == Kind::Dir {
        m.crtobjaud = CrtObjAud::SysVal;
    }
    m
}

/// Read a node's record, or the fallback.
pub fn read<B: Backend>(b: &B, p: &str, st: &BStat, ids: &dyn IdMap) -> Result<(Meta, Source)> {
    match b.xattr_get(p, XATTR)? {
        Some(raw) => match codec::decode(&raw) {
            Ok(m) => Ok((m, Source::Stored)),
            Err(_) => Ok((fallback(st, ids), Source::Invalid)),
        },
        None => Ok((fallback(st, ids), Source::Missing)),
    }
}

/// Write a node's record and mirror its mode bits, owner and group
/// into the backend.
pub fn write<B: Backend>(b: &B, p: &str, kind: Kind, m: &Meta, ids: &dyn IdMap) -> Result<()> {
    b.xattr_set(p, XATTR, &codec::encode(m))?;
    mirror(b, p, kind, m, ids)
}

/// Write only the mirror.
pub fn mirror<B: Backend>(b: &B, p: &str, kind: Kind, m: &Meta, ids: &dyn IdMap) -> Result<()> {
    if kind != Kind::Symlink {
        b.set_mode(p, crate::auth::data::mode_of(m))?;
    }
    let uid = ids.uid_of(&m.owner).unwrap_or(0);
    let gid = m.group.as_deref().and_then(|g| ids.gid_of(g)).unwrap_or(0);
    b.set_owner(p, uid, gid)
}
