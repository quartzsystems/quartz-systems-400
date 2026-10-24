// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Attributes: `Qp0lGetAttr`/`Qp0lSetAttr` (the attribute IDs of
//! `QSYSINC/H(QP0LSTDI)`), `chmod()`, `utime()`, check-out and check-in
//! (`CHKOUT`, `CHKIN`), and `STRJRN`/`ENDJRN` on stream files (stored).

use super::audit::{AuditKind, FileId};
use super::{Ctx, Ifs, now};
use crate::auth::need::Op;
use crate::backend::{Backend, Nanos};
use crate::errno::{Errno, Result};
use crate::meta::{CheckOut, CrtObjAud, Flags, Journal, Meta, ObjAudit, Scan, StgOpt};
use crate::resolve::Node;

macro_rules! attr_ids {
    ($($name:ident = $v:literal),* $(,)?) => {
        /// A `QP0L_ATTR_*` attribute ID.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[repr(u32)]
        pub enum AttrId {
            $(
                #[allow(missing_docs)]
                $name = $v,
            )*
        }

        impl AttrId {
            /// Every ID.
            pub const ALL: &'static [AttrId] = &[$(AttrId::$name),*];

            /// From the number.
            #[must_use]
            pub fn from_u32(v: u32) -> Option<Self> {
                match v {
                    $($v => Some(Self::$name),)*
                    _ => None,
                }
            }
        }
    };
}

attr_ids! {
    ObjType = 0, DataSize = 1, AllocSize = 2, ExtendedAttrSize = 3, CreateTime = 4,
    AccessTime = 5, ChangeTime = 6, ModifyTime = 7, StgFree = 8, CheckedOut = 9,
    LocalRemote = 10, Auth = 11, FileId = 12, Asp = 13, DataSize64 = 14, AllocSize64 = 15,
    UsageInformation = 16, PcReadOnly = 17, PcHidden = 18, PcSystem = 19, PcArchive = 20,
    SystemArchive = 21, Codepage = 22, FileFormat = 23, UdfsDefaultFormat = 24,
    JournalInformation = 25, Alwckpwrt = 26, Ccsid = 27, Signed = 28, SysSigned = 29,
    MultSigs = 30, DiskStgOpt = 31, MainStgOpt = 32, DirFormat = 33, Audit = 34,
    Crtobjscan = 35, Scan = 36, ScanInfo = 37, Alwsav = 38, Rstdrnmunl = 39,
    JournalExtendedInformation = 40, Crtobjaud = 41, SystemUse = 42, Temporary = 43,
    UdfsTemporary = 44, UdfsPreferredStorageUnit = 45, InheritAlwckpwrt = 46,
    SysRestrictsSave = 47, Text = 48, Runexit = 49, Crtrunexit = 50, ResetDate = 200,
    Suid = 300, Sgid = 301,
}

/// An attribute's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attr {
    /// `*STMF` ...
    ObjType(String),
    /// Data size.
    DataSize(u64),
    /// Allocated size.
    AllocSize(u64),
    /// Extended attributes' size.
    ExtendedAttrSize(u64),
    /// Created.
    CreateTime(Nanos),
    /// Last access.
    AccessTime(Nanos),
    /// Last attribute change.
    ChangeTime(Nanos),
    /// Last data change.
    ModifyTime(Nanos),
    /// Storage freed (never, here).
    StgFree(bool),
    /// Check-out.
    CheckedOut(Option<CheckOut>),
    /// Local (true) or remote.
    LocalRemote(bool),
    /// Owner, primary group and the caller's authorities.
    Auth {
        /// Owner.
        owner: String,
        /// Primary group.
        group: Option<String>,
        /// The caller's authority bits.
        have: u32,
    },
    /// The file ID.
    FileId(FileId),
    /// ASP number.
    Asp(u16),
    /// `*RDONLY`.
    PcReadOnly(bool),
    /// `*HIDDEN`.
    PcHidden(bool),
    /// `*SYSTEM`.
    PcSystem(bool),
    /// `*PCARCHIVE`.
    PcArchive(bool),
    /// `*SYSARCHIVE`.
    SystemArchive(bool),
    /// Code page.
    Codepage(u16),
    /// `*TYPE2` stream file (1).
    FileFormat(u8),
    /// Journaling.
    JournalInformation(Option<Journal>),
    /// `*ALWCKPWRT`.
    Alwckpwrt(bool),
    /// CCSID.
    Ccsid(u16),
    /// Signed (never).
    Signed(bool),
    /// Signed by the system (never).
    SysSigned(bool),
    /// More than one signature (never).
    MultSigs(bool),
    /// `*DISKSTGOPT`.
    DiskStgOpt(StgOpt),
    /// `*MAINSTGOPT`.
    MainStgOpt(StgOpt),
    /// `*TYPE2` directory (1).
    DirFormat(u8),
    /// `OBJAUD`.
    Audit(ObjAudit),
    /// `*CRTOBJSCAN`.
    Crtobjscan(Scan),
    /// `*SCAN`.
    Scan(Scan),
    /// `*ALWSAV`.
    Alwsav(bool),
    /// `*RSTDRNMUNL`.
    Rstdrnmunl(bool),
    /// `*CRTOBJAUD`.
    Crtobjaud(CrtObjAud),
    /// System use (none, 0).
    SystemUse(u8),
    /// Temporary object (never).
    Temporary(bool),
    /// `*INHALWCKPWRT`.
    InheritAlwckpwrt(bool),
    /// The system restricts saving (never).
    SysRestrictsSave(bool),
    /// Text description.
    Text(String),
    /// `S_ISUID`.
    Suid(bool),
    /// `S_ISGID`.
    Sgid(bool),
}

/// The attributes `Qp0lGetAttr` answers for `QP0L_ALL_ATTRIBUTES`.
fn all_attrs(node: &Node, m: &Meta, st: &super::Stat, have: u32) -> Vec<Attr> {
    let dir = node.is_dir();
    let mut v = vec![
        Attr::ObjType(st.objtype.clone()),
        Attr::DataSize(st.size),
        Attr::AllocSize(st.allocsize),
        Attr::ExtendedAttrSize(0),
        Attr::CreateTime(m.created),
        Attr::AccessTime(st.atime),
        Attr::ChangeTime(st.ctime),
        Attr::ModifyTime(st.mtime),
        Attr::StgFree(false),
        Attr::CheckedOut(m.checkout.clone()),
        Attr::LocalRemote(true),
        Attr::Auth {
            owner: m.owner.clone(),
            group: m.group.clone(),
            have,
        },
        Attr::FileId(FileId {
            fs: node.fs(),
            ino: node.ino(),
            generation: m.generation,
        }),
        Attr::Asp(1),
        Attr::PcReadOnly(m.flags.has(Flags::RDONLY)),
        Attr::PcHidden(m.flags.has(Flags::HIDDEN)),
        Attr::PcSystem(m.flags.has(Flags::SYSTEM)),
        Attr::PcArchive(m.flags.has(Flags::PCARCHIVE)),
        Attr::SystemArchive(m.flags.has(Flags::SYSARCHIVE)),
        Attr::Codepage(m.codepage),
        Attr::Ccsid(m.ccsid),
        Attr::JournalInformation(m.journal.clone()),
        Attr::Alwckpwrt(m.flags.has(Flags::ALWCKPWRT)),
        Attr::Signed(false),
        Attr::SysSigned(false),
        Attr::MultSigs(false),
        Attr::DiskStgOpt(m.disk_stg),
        Attr::MainStgOpt(m.main_stg),
        Attr::Audit(m.audit),
        Attr::Scan(m.scan),
        Attr::Alwsav(m.flags.has(Flags::ALWSAV)),
        Attr::Rstdrnmunl(m.flags.has(Flags::RSTDRNMUNL)),
        Attr::SystemUse(0),
        Attr::Temporary(false),
        Attr::SysRestrictsSave(false),
        Attr::Text(m.text.clone()),
        Attr::Suid(m.flags.has(Flags::SETUID)),
        Attr::Sgid(m.flags.has(Flags::SETGID)),
    ];
    if dir {
        v.push(Attr::DirFormat(1));
        v.push(Attr::Crtobjscan(m.crtobjscan));
        v.push(Attr::Crtobjaud(m.crtobjaud));
        v.push(Attr::InheritAlwckpwrt(m.flags.has(Flags::INHALWCKPWRT)));
    } else {
        v.push(Attr::FileFormat(1));
    }
    v
}

fn id_of(a: &Attr) -> AttrId {
    match a {
        Attr::ObjType(_) => AttrId::ObjType,
        Attr::DataSize(_) => AttrId::DataSize,
        Attr::AllocSize(_) => AttrId::AllocSize,
        Attr::ExtendedAttrSize(_) => AttrId::ExtendedAttrSize,
        Attr::CreateTime(_) => AttrId::CreateTime,
        Attr::AccessTime(_) => AttrId::AccessTime,
        Attr::ChangeTime(_) => AttrId::ChangeTime,
        Attr::ModifyTime(_) => AttrId::ModifyTime,
        Attr::StgFree(_) => AttrId::StgFree,
        Attr::CheckedOut(_) => AttrId::CheckedOut,
        Attr::LocalRemote(_) => AttrId::LocalRemote,
        Attr::Auth { .. } => AttrId::Auth,
        Attr::FileId(_) => AttrId::FileId,
        Attr::Asp(_) => AttrId::Asp,
        Attr::PcReadOnly(_) => AttrId::PcReadOnly,
        Attr::PcHidden(_) => AttrId::PcHidden,
        Attr::PcSystem(_) => AttrId::PcSystem,
        Attr::PcArchive(_) => AttrId::PcArchive,
        Attr::SystemArchive(_) => AttrId::SystemArchive,
        Attr::Codepage(_) => AttrId::Codepage,
        Attr::FileFormat(_) => AttrId::FileFormat,
        Attr::JournalInformation(_) => AttrId::JournalInformation,
        Attr::Alwckpwrt(_) => AttrId::Alwckpwrt,
        Attr::Ccsid(_) => AttrId::Ccsid,
        Attr::Signed(_) => AttrId::Signed,
        Attr::SysSigned(_) => AttrId::SysSigned,
        Attr::MultSigs(_) => AttrId::MultSigs,
        Attr::DiskStgOpt(_) => AttrId::DiskStgOpt,
        Attr::MainStgOpt(_) => AttrId::MainStgOpt,
        Attr::DirFormat(_) => AttrId::DirFormat,
        Attr::Audit(_) => AttrId::Audit,
        Attr::Crtobjscan(_) => AttrId::Crtobjscan,
        Attr::Scan(_) => AttrId::Scan,
        Attr::Alwsav(_) => AttrId::Alwsav,
        Attr::Rstdrnmunl(_) => AttrId::Rstdrnmunl,
        Attr::Crtobjaud(_) => AttrId::Crtobjaud,
        Attr::SystemUse(_) => AttrId::SystemUse,
        Attr::Temporary(_) => AttrId::Temporary,
        Attr::InheritAlwckpwrt(_) => AttrId::InheritAlwckpwrt,
        Attr::SysRestrictsSave(_) => AttrId::SysRestrictsSave,
        Attr::Text(_) => AttrId::Text,
        Attr::Suid(_) => AttrId::Suid,
        Attr::Sgid(_) => AttrId::Sgid,
    }
}

impl<B: Backend> Ifs<B> {
    /// `Qp0lGetAttr`: the requested attributes (all when `ids` is
    /// empty); IDs that do not apply to the object are left out.
    pub fn get_attrs(
        &self,
        cx: &Ctx<'_>,
        p: &str,
        ids: &[AttrId],
        follow: bool,
    ) -> Result<Vec<Attr>> {
        let (r, m) = self.resolve_meta(cx, p, follow)?;
        let st = super::stat::stat_of(&r.node, &m, cx.env);
        let have = crate::auth::have(cx.principal, &m, cx.env).0;
        let mut all = all_attrs(&r.node, &m, &st, have);
        if !ids.is_empty() {
            all.retain(|a| ids.contains(&id_of(a)));
        }
        Ok(all)
    }

    /// Change a node's record under the state lock, after `op`'s check.
    fn update(
        &self,
        cx: &Ctx<'_>,
        p: &str,
        follow: bool,
        op: Op,
        f: impl FnOnce(&mut Meta) -> Result<()>,
    ) -> Result<Meta> {
        let mut st = self.lock();
        let cwd = Self::cwd_of(&mut st, cx.owner);
        let (mut sys, _) = self.split(&mut st);
        let r = sys.resolve(&cwd, p, follow, Some(cx))?;
        let (mut m, _) = sys.meta(&r.node, Some(cx))?;
        cx.require(&m, &r.node, &r.path(), op)?;
        if Self::read_only(&sys, r.node.fs()) {
            return Err(Errno::EROFS);
        }
        let before = m.clone();
        f(&mut m)?;
        if m != before {
            self.put_meta(cx, &r.node, &m)?;
            cx.audit_access(&m, &r.node, &r.path(), op);
            if m.audit != before.audit || m.crtobjaud != before.crtobjaud {
                cx.audit_always(
                    AuditKind::AuditChange,
                    &m,
                    &r.node,
                    &r.path(),
                    "CHGAUD",
                    m.audit.name().into(),
                );
            }
        }
        Ok(m)
    }

    /// `Qp0lSetAttr` for one attribute.
    pub fn set_attr(&self, cx: &Ctx<'_>, p: &str, a: &Attr, follow: bool) -> Result<()> {
        let op = match a {
            Attr::Audit(_) | Attr::Crtobjaud(_) => Op::SetAudit,
            Attr::PcReadOnly(_)
            | Attr::PcHidden(_)
            | Attr::PcSystem(_)
            | Attr::PcArchive(_)
            | Attr::SystemArchive(_)
            | Attr::Ccsid(_)
            | Attr::Codepage(_)
            | Attr::Text(_) => Op::SetAttr,
            Attr::Suid(_) | Attr::Sgid(_) => Op::Chmod,
            _ => Op::SetAttrMgt,
        };
        let a = a.clone();
        self.update(cx, p, follow, op, move |m| {
            let fl = &mut m.flags;
            match a {
                Attr::PcReadOnly(v) => fl.set(Flags::RDONLY, v),
                Attr::PcHidden(v) => fl.set(Flags::HIDDEN, v),
                Attr::PcSystem(v) => fl.set(Flags::SYSTEM, v),
                Attr::PcArchive(v) => fl.set(Flags::PCARCHIVE, v),
                Attr::SystemArchive(v) => fl.set(Flags::SYSARCHIVE, v),
                Attr::Alwckpwrt(v) => fl.set(Flags::ALWCKPWRT, v),
                Attr::InheritAlwckpwrt(v) => fl.set(Flags::INHALWCKPWRT, v),
                Attr::Alwsav(v) => fl.set(Flags::ALWSAV, v),
                Attr::Rstdrnmunl(v) => fl.set(Flags::RSTDRNMUNL, v),
                Attr::Suid(v) => fl.set(Flags::SETUID, v),
                Attr::Sgid(v) => fl.set(Flags::SETGID, v),
                Attr::Ccsid(c) => {
                    ccsid::conv::Codec::of(c).map_err(|_| Errno::EINVAL)?;
                    m.ccsid = c;
                    m.codepage = ccsid::conv::codepage_of(c);
                }
                Attr::Codepage(c) => {
                    let c = ccsid::conv::ccsid_of_codepage(c);
                    ccsid::conv::Codec::of(c).map_err(|_| Errno::EINVAL)?;
                    m.ccsid = c;
                    m.codepage = ccsid::conv::codepage_of(c);
                }
                Attr::DiskStgOpt(s) => m.disk_stg = s,
                Attr::MainStgOpt(s) => m.main_stg = s,
                Attr::Audit(v) => m.audit = v,
                Attr::Crtobjaud(v) => m.crtobjaud = v,
                Attr::Crtobjscan(s) => m.crtobjscan = s,
                Attr::Scan(s) => m.scan = s,
                Attr::Text(t) => {
                    if t.chars().count() > 50 {
                        return Err(Errno::EINVAL);
                    }
                    m.text = t;
                }
                _ => return Err(Errno::EINVAL),
            }
            Ok(())
        })?;
        Ok(())
    }

    /// `chmod()`: the owner, group and `*PUBLIC` data authorities from
    /// the mode bits.
    pub fn chmod(&self, cx: &Ctx<'_>, p: &str, mode: u32) -> Result<()> {
        let m = self.update(cx, p, true, Op::Chmod, |m| {
            crate::auth::data::apply_mode(m, mode);
            Ok(())
        })?;
        let (r, _) = self.resolve_meta(cx, p, true)?;
        cx.audit_always(
            AuditKind::AuthChange,
            &m,
            &r.node,
            &r.path(),
            "CHMOD",
            format!("{mode:04o}"),
        );
        Ok(())
    }

    /// `utime()`: `None` sets both times to now (owner or `*W`); given
    /// times need the owner or `*ALLOBJ`.
    pub fn utime(&self, cx: &Ctx<'_>, p: &str, times: Option<(Nanos, Nanos)>) -> Result<()> {
        let op = if times.is_some() {
            Op::UtimeSet
        } else {
            Op::UtimeNow
        };
        let mut st = self.lock();
        let cwd = Self::cwd_of(&mut st, cx.owner);
        let (mut sys, _) = self.split(&mut st);
        let r = sys.resolve(&cwd, p, true, Some(cx))?;
        let (m, _) = sys.meta(&r.node, Some(cx))?;
        cx.require(&m, &r.node, &r.path(), op)?;
        let Node::Back { rel, .. } = &r.node else {
            return Err(Errno::ENOTSUP);
        };
        let (a, mt) = times.unwrap_or_else(|| (now(), now()));
        self.b.set_times(rel, Some(a), Some(mt))?;
        cx.audit_access(&m, &r.node, &r.path(), op);
        Ok(())
    }

    /// `CHKOUT`: needs `*W`; checked out by someone else is `EBUSY`.
    pub fn check_out(&self, cx: &Ctx<'_>, p: &str) -> Result<()> {
        let user = cx.principal.user.clone();
        self.update(cx, p, true, Op::CheckOut, move |m| {
            match &m.checkout {
                Some(c) if c.user != user => return Err(Errno::EBUSY),
                Some(_) => return Ok(()),
                None => {}
            }
            m.checkout = Some(CheckOut { user, time: now() });
            Ok(())
        })?;
        Ok(())
    }

    /// `CHKIN`: by the user who checked it out, the owner or `*ALLOBJ`.
    pub fn check_in(&self, cx: &Ctx<'_>, p: &str) -> Result<()> {
        let mut st = self.lock();
        let cwd = Self::cwd_of(&mut st, cx.owner);
        let (mut sys, _) = self.split(&mut st);
        let r = sys.resolve(&cwd, p, true, Some(cx))?;
        let (mut m, _) = sys.meta(&r.node, Some(cx))?;
        let Some(co) = &m.checkout else {
            return Err(Errno::EINVAL);
        };
        if co.user != cx.principal.user && !crate::auth::owner_or_allobj(cx.principal, &m) {
            return Err(Errno::EPERM);
        }
        m.checkout = None;
        self.put_meta(cx, &r.node, &m)?;
        cx.audit_access(&m, &r.node, &r.path(), Op::CheckOut);
        Ok(())
    }

    /// `STRJRN` on a stream file or directory: stored only.
    pub fn start_journal(&self, cx: &Ctx<'_>, p: &str, j: Journal) -> Result<()> {
        self.update(cx, p, true, Op::Journal, move |m| {
            if m.journal.is_some() {
                return Err(Errno::EJOURNALED);
            }
            m.journal = Some(Journal { start: now(), ..j });
            Ok(())
        })?;
        Ok(())
    }

    /// `ENDJRN`.
    pub fn end_journal(&self, cx: &Ctx<'_>, p: &str) -> Result<()> {
        self.update(cx, p, true, Op::Journal, |m| {
            if m.journal.take().is_none() {
                return Err(Errno::EJRNINACTIVE);
            }
            Ok(())
        })?;
        Ok(())
    }
}
