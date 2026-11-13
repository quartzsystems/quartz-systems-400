// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Attributes of remote objects. What maps onto the server's
//! attributes is sent there: `chmod`, `chown` (wire UID and GID),
//! `CHGOWN` and `CHGPGP` (the profiles' UID and GID), `utime`, the
//! set-ID attributes, and `CHGAUT` of the owner's, the group's or
//! `*PUBLIC`'s data authority (as mode bits). What does not (private
//! authorities, authorization lists, object authorities, PC and
//! archive flags, CCSID, journaling, check-out) is `ENOTSUP`. `access`
//! asks the server; `statvfs` and `pathconf` answer for the mount.

use super::super::authcmd::{ChgAut, Who};
use super::super::flags as f;
use super::super::{Ctx, Ifs, StatVfs};
use crate::backend::{Backend, Nanos};
use crate::errno::{Errno, Result};
use crate::lease::notify::ChangeKind;
use crate::remote::access as a;
use crate::remote::request;
use crate::remote::work::Mutation;
use crate::remote::{RAttr, RHandle, RemoteMount, SetAttr, SetTime};
use crate::resolve::Node;

/// What a path names when it is remote: the mount, handle, attributes.
type Remote = (std::sync::Arc<RemoteMount>, RHandle, RAttr, String);

impl<B: Backend> Ifs<B> {
    /// The remote object `p` names, or `None` when it is not remote.
    pub(crate) fn remote_at(&self, cx: &Ctx<'_>, p: &str, follow: bool) -> Result<Option<Remote>> {
        let mut st = self.lock();
        if !st.mounts.has_remote() {
            return Ok(None);
        }
        let cwd = Self::cwd_of(&mut st, cx.owner);
        let (mut sys, _) = self.split(&mut st);
        let r = sys.resolve(&cwd, p, follow, Some(cx))?;
        let Node::Remote { fs, handle, attr } = r.node.clone() else {
            return Ok(None);
        };
        Ok(Some((sys.remote(fs)?, handle, attr, r.path())))
    }

    /// Send `set` for remote `p`; `false` when `p` is not remote.
    pub(crate) fn remote_setattr(
        &self,
        cx: &Ctx<'_>,
        p: &str,
        follow: bool,
        set: impl FnOnce(&RAttr) -> Result<SetAttr>,
    ) -> Result<bool> {
        let Some((m, h, attr, path)) = self.remote_at(cx, p, follow)? else {
            return Ok(false);
        };
        if m.info().read_only {
            return Err(Errno::EROFS);
        }
        let mut set = set(&attr)?;
        if m.info().nosuid
            && let Some(mode) = &mut set.mode
        {
            *mode &= !0o6000;
        }
        request::note_owner(cx.owner);
        m.settle(m.mutate(Mutation::Setattr { h, set }, &request::cred()))?;
        self.leases.note(
            cx.owner,
            &path,
            &ChangeKind::Attributes,
            attr.kind == crate::remote::RKind::Dir,
        );
        Ok(true)
    }

    /// `chmod()` of a remote object.
    pub(crate) fn remote_chmod(&self, cx: &Ctx<'_>, p: &str, mode: u32) -> Result<bool> {
        self.remote_setattr(cx, p, true, |_| {
            Ok(SetAttr {
                mode: Some(mode & 0o7777),
                ..SetAttr::default()
            })
        })
    }

    /// `chown()`, `lchown()` by wire UID and GID (`u32::MAX` leaves one).
    pub(crate) fn remote_chown(
        &self,
        cx: &Ctx<'_>,
        p: &str,
        follow: bool,
        uid: u32,
        gid: u32,
    ) -> Result<bool> {
        self.remote_setattr(cx, p, follow, |_| {
            Ok(SetAttr {
                uid: (uid != u32::MAX).then_some(uid),
                gid: (gid != u32::MAX).then_some(gid),
                ..SetAttr::default()
            })
        })
    }

    /// `CHGOWN` of a remote object: the profile's UID.
    pub(crate) fn remote_chgown(
        &self,
        cx: &Ctx<'_>,
        p: &str,
        follow: bool,
        owner: &str,
    ) -> Result<bool> {
        let uid = cx.env.uid_of(owner);
        self.remote_setattr(cx, p, follow, |_| {
            Ok(SetAttr {
                uid: Some(uid.ok_or(Errno::EINVAL)?),
                ..SetAttr::default()
            })
        })
    }

    /// `CHGPGP` of a remote object: the group's GID (a remote object
    /// always has a group: `*NONE` is `ENOTSUP`).
    pub(crate) fn remote_chgpgp(
        &self,
        cx: &Ctx<'_>,
        p: &str,
        follow: bool,
        group: Option<&str>,
    ) -> Result<bool> {
        let gid = group.map(|g| cx.env.gid_of(g));
        self.remote_setattr(cx, p, follow, |_| {
            let gid = gid.ok_or(Errno::ENOTSUP)?.ok_or(Errno::EINVAL)?;
            Ok(SetAttr {
                gid: Some(gid),
                ..SetAttr::default()
            })
        })
    }

    /// `utime()` of a remote object: `None` is the server's now.
    pub(crate) fn remote_utime(
        &self,
        cx: &Ctx<'_>,
        p: &str,
        times: Option<(Nanos, Nanos)>,
    ) -> Result<bool> {
        self.remote_setattr(cx, p, true, |_| {
            let (at, mt) = match times {
                Some((x, y)) => (SetTime::At(x), SetTime::At(y)),
                None => (SetTime::Server, SetTime::Server),
            };
            Ok(SetAttr {
                atime: Some(at),
                mtime: Some(mt),
                ..SetAttr::default()
            })
        })
    }

    /// The set-ID attributes of a remote object (`Qp0lSetAttr`).
    pub(crate) fn remote_setid(
        &self,
        cx: &Ctx<'_>,
        p: &str,
        follow: bool,
        bit: u32,
        on: bool,
    ) -> Result<bool> {
        self.remote_setattr(cx, p, follow, |a| {
            let mode = if on { a.mode | bit } else { a.mode & !bit };
            Ok(SetAttr {
                mode: Some(mode & 0o7777),
                ..SetAttr::default()
            })
        })
    }

    /// `CHGAUT` of a remote object: only the data authority of the
    /// owner, the group or `*PUBLIC`, as mode bits.
    pub(crate) fn remote_chgaut(&self, cx: &Ctx<'_>, p: &str, c: &ChgAut) -> Result<bool> {
        let Some((_, _, attr, _)) = self.remote_at(cx, p, true)? else {
            return Ok(false);
        };
        if c.objects.is_some() || c.autl.is_some() || c.data_autl {
            return Err(Errno::ENOTSUP);
        }
        let Some(d) = c.data else {
            return Ok(true);
        };
        let owner = cx.env.profile_of_uid(attr.uid);
        let group = cx.env.profile_of_gid(attr.gid);
        let rwx = |da: crate::auth::data::DataAuth| u32::from(da.0);
        let mut mode = attr.mode;
        for w in &c.who {
            let shift = match w {
                Who::Public => 0,
                Who::User(u) if owner.as_deref() == Some(u.as_str()) => 6,
                Who::User(u) if group.as_deref() == Some(u.as_str()) => 3,
                Who::User(_) => return Err(Errno::ENOTSUP),
            };
            mode = (mode & !(7 << shift)) | (rwx(d) << shift);
        }
        self.remote_setattr(cx, p, true, |_| {
            Ok(SetAttr {
                mode: Some(mode & 0o7777),
                ..SetAttr::default()
            })
        })
    }

    /// `access()` of a remote object: the server's answer. `None` when
    /// `p` is not remote.
    pub(crate) fn remote_access(&self, cx: &Ctx<'_>, p: &str, amode: i32) -> Result<Option<()>> {
        let Some((m, h, attr, _)) = self.remote_at(cx, p, true)? else {
            return Ok(None);
        };
        let dir = attr.kind == crate::remote::RKind::Dir;
        let mut need = 0;
        if amode & f::R_OK != 0 {
            need |= a::READ;
        }
        if amode & f::W_OK != 0 {
            need |= a::MODIFY;
        }
        if amode & f::X_OK != 0 {
            need |= if dir { a::LOOKUP } else { a::EXECUTE };
        }
        if need != 0 {
            let have = m.settle(m.access(&h, &request::cred()))?;
            if have & need != need {
                return Err(Errno::EACCES);
            }
        }
        if amode & f::W_OK != 0 && m.info().read_only {
            return Err(Errno::EROFS);
        }
        Ok(Some(()))
    }

    /// `statvfs()` of a remote mount.
    pub(crate) fn remote_statvfs(&self, cx: &Ctx<'_>, p: &str) -> Result<Option<StatVfs>> {
        let Some((m, _, _, _)) = self.remote_at(cx, p, true)? else {
            return Ok(None);
        };
        let s = m.settle(m.statfs(&request::cred()))?;
        let bs = 4096;
        let i = m.info();
        Ok(Some(StatVfs {
            bsize: bs,
            frsize: bs,
            blocks: s.total_bytes / bs,
            bfree: s.free_bytes / bs,
            bavail: s.avail_bytes / bs,
            files: s.total_files,
            ffree: s.free_files,
            favail: s.avail_files,
            fsid: m.id(),
            flag: u64::from(i.read_only) | (u64::from(i.nosuid) << 1),
            namemax: u64::from(m.pathconf().name_max),
            name: i.name.clone(),
            case_sensitive: i.case_sensitive,
        }))
    }

    /// `pathconf()` of a remote mount: `None` when `p` is not remote,
    /// else the answer (itself `None` for no limit).
    #[allow(clippy::option_option)]
    pub(crate) fn remote_pathconf(
        &self,
        cx: &Ctx<'_>,
        p: &str,
        name: i32,
    ) -> Result<Option<Option<i64>>> {
        let Some((m, _, _, _)) = self.remote_at(cx, p, true)? else {
            return Ok(None);
        };
        let pc = m.pathconf();
        Ok(Some(match name {
            f::PC_CHOWN_RESTRICTED => Some(i64::from(pc.chown_restricted)),
            f::PC_NO_TRUNC => Some(i64::from(pc.no_trunc)),
            f::PC_THREAD_SAFE => Some(1),
            f::PC_LINK_MAX => Some(i64::from(pc.link_max)),
            f::PC_NAME_MAX => Some(i64::from(pc.name_max)),
            f::PC_PATH_MAX => Some(i64::try_from(crate::path::PATH_MAX).unwrap_or(i64::MAX)),
            f::PC_PIPE_BUF => Some(4096),
            f::PC_MAX_CANON | f::PC_MAX_INPUT | f::PC_VDISABLE => None,
            _ => return Err(Errno::EINVAL),
        }))
    }
}
