// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! User-defined file systems: `CRTUDFS`, `DLTUDFS`, `MOUNT`, `UNMOUNT`,
//! `DSPUDFS`. A UDFS is a backend directory in the hidden area
//! (`UDFS/QASP01/NAME.udfs`), seen as the block special file
//! `/dev/QASP01/NAME.udfs`; mounting it over a directory puts its tree
//! there. Mounts last until the next IPL, as IBM's do.

use sec::profile::spcaut::SpecialAuth;

use super::{Ctx, Ifs, now};
use crate::auth::data::{RWX, RX};
use crate::backend::{self, Backend, Kind, Nanos};
use crate::errno::{Errno, Result};
use crate::meta::{self, CrtObjAud, Flags, Meta};
use crate::path::{self, fold};
use crate::resolve::Node;
use crate::vfs::{FsKind, UDFS_BASE};

/// `DSPUDFS`'s answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UdfsInfo {
    /// `/dev/QASPnn/NAME.udfs`.
    pub path: String,
    /// `CASE(*MIXED)` (true) or `CASE(*MONO)`.
    pub case_sensitive: bool,
    /// Owner.
    pub owner: String,
    /// Text.
    pub text: String,
    /// Created.
    pub created: Nanos,
    /// Where it is mounted.
    pub mounted_at: Vec<String>,
    /// `*CRTOBJAUD` of its root.
    pub crtobjaud: CrtObjAud,
}

/// The ASP and name of `/dev/QASPnn/NAME.udfs`.
fn split_udfs(p: &str) -> Result<(String, String)> {
    let c = path::components(&path::join_lexical("/", p));
    match c.as_slice() {
        [dev, asp, name]
            if fold::same(dev, "dev")
                && asp.len() == 6
                && fold::same(&asp[..4], "QASP")
                && asp[4..].chars().all(|d| d.is_ascii_digit())
                && name.len() > 5
                && name.to_ascii_lowercase().ends_with(".udfs") =>
        {
            path::check_name(name)?;
            Ok((asp.to_ascii_uppercase(), name.clone()))
        }
        _ => Err(Errno::EINVAL),
    }
}

impl<B: Backend> Ifs<B> {
    fn udfs_rel(asp: &str, name: &str) -> String {
        backend::join(&backend::join(UDFS_BASE, asp), name)
    }

    /// `CRTUDFS`: `case_sensitive` true is `CASE(*MIXED)` (names compare
    /// exactly), false `CASE(*MONO)` (they fold, as in root).
    pub fn crtudfs(
        &self,
        cx: &Ctx<'_>,
        udfs: &str,
        case_sensitive: bool,
        crtobjaud: CrtObjAud,
        text: &str,
    ) -> Result<()> {
        let (asp, name) = split_udfs(udfs)?;
        let _st = self.lock();
        let asp_rel = backend::join(UDFS_BASE, &asp);
        if let Ok(ents) = self.b.readdir(&asp_rel)
            && ents.iter().any(|e| fold::same(&e.name, &name))
        {
            return Err(Errno::EEXIST);
        }
        if self.b.stat(&asp_rel).is_err() {
            self.b.mkdir(&asp_rel, 0o755)?;
            let mut am = Meta::new("QSYS", ccsid::conv::UTF8, now());
            am.public = RX;
            meta::write(&self.b, &asp_rel, Kind::Dir, &am, cx.env)?;
        }
        let rel = Self::udfs_rel(&asp, &name);
        self.b.mkdir(&rel, 0o755)?;
        let mut m = Meta::new(&cx.principal.user, ccsid::conv::UTF8, now());
        m.public = RWX;
        m.flags.set(Flags::SYSDIR, true);
        m.flags.set(Flags::CASE_SENSITIVE, case_sensitive);
        m.crtobjaud = crtobjaud;
        m.text = text.chars().take(50).collect();
        meta::write(&self.b, &rel, Kind::Dir, &m, cx.env)?;
        Ok(())
    }

    fn udfs_meta(&self, cx: &Ctx<'_>, asp: &str, name: &str) -> Result<(String, Meta)> {
        let asp_rel = backend::join(UDFS_BASE, asp);
        let stored = self
            .b
            .readdir(&asp_rel)
            .map_err(|_| Errno::ENOENT)?
            .into_iter()
            .find(|e| fold::same(&e.name, name))
            .ok_or(Errno::ENOENT)?
            .name;
        let rel = backend::join(&asp_rel, &stored);
        let st = self.b.stat(&rel)?;
        let (m, _) = meta::read(&self.b, &rel, &st, cx.env)?;
        Ok((rel, m))
    }

    /// `DLTUDFS`: not while mounted; the owner or `*ALLOBJ`.
    pub fn dltudfs(&self, cx: &Ctx<'_>, udfs: &str) -> Result<()> {
        let (asp, name) = split_udfs(udfs)?;
        let (rel, m) = self.udfs_meta(cx, &asp, &name)?;
        if !crate::auth::owner_or_allobj(cx.principal, &m) {
            return Err(Errno::EPERM);
        }
        let mut st = self.lock();
        if st.mounts.all().iter().any(|mt| mt.base == rel) {
            return Err(Errno::EBUSY);
        }
        st.index.drop_tree(&rel);
        drop(st);
        remove_rel(&self.b, &rel)
    }

    /// `MOUNT TYPE(*UDFS)`: needs `*IOSYSCFG`; `over` must be a
    /// directory of a backend file system and not a mount point.
    pub fn mount(&self, cx: &Ctx<'_>, udfs: &str, over: &str, read_only: bool) -> Result<u32> {
        if !cx.principal.has(SpecialAuth::IOSYSCFG) {
            return Err(Errno::EPERM);
        }
        let (asp, name) = split_udfs(udfs)?;
        let (rel, m) = self.udfs_meta(cx, &asp, &name)?;
        let mut st = self.lock();
        let cwd = Self::cwd_of(&mut st, cx.owner);
        let (mut sys, _) = self.split(&mut st);
        let r = sys.resolve(&cwd, over, true, Some(cx))?;
        if !matches!(r.node, Node::Back { .. }) || !r.node.is_dir() {
            return Err(Errno::ENOTDIR);
        }
        if sys.mounts.all().iter().any(|mt| mt.at == r.comps) {
            return Err(Errno::EBUSY);
        }
        let shown = format!("/dev/{asp}/{}", backend::last(&rel));
        let id = st.mounts.add(
            r.comps,
            FsKind::Udfs(shown),
            rel,
            m.flags.has(Flags::CASE_SENSITIVE),
            read_only,
        );
        Ok(id)
    }

    /// `UNMOUNT`: by mount point or by the UDFS; `EBUSY` while anything
    /// in it is open or a current directory.
    pub fn unmount(&self, cx: &Ctx<'_>, path: &str) -> Result<()> {
        if !cx.principal.has(SpecialAuth::IOSYSCFG) {
            return Err(Errno::EPERM);
        }
        let mut st = self.lock();
        let target = path::components(&path::join_lexical("/", path));
        let id = st
            .mounts
            .all()
            .iter()
            .filter(|m| matches!(m.kind, FsKind::Udfs(_)))
            .find(|m| {
                m.at == target
                    || matches!(&m.kind, FsKind::Udfs(p) if path::components(p).iter().map(|c| fold::fold(c)).eq(target.iter().map(|c| fold::fold(c))))
            })
            .map(|m| (m.id, m.at.clone(), m.base.clone()))
            .ok_or(Errno::EINVAL)?;
        let (id, at, base) = id;
        if st.rest.locks.opens_on(id) > 0 || st.rest.owners.values().any(|o| o.cwd.starts_with(&at))
        {
            return Err(Errno::EBUSY);
        }
        if st.mounts.under(&at).len() > 1 {
            return Err(Errno::EBUSY);
        }
        st.mounts.remove(id);
        st.index.drop_tree(&base);
        Ok(())
    }

    /// `DSPUDFS`.
    pub fn dspudfs(&self, cx: &Ctx<'_>, udfs: &str) -> Result<UdfsInfo> {
        let (asp, name) = split_udfs(udfs)?;
        let (rel, m) = self.udfs_meta(cx, &asp, &name)?;
        let mounted_at = self
            .lock()
            .mounts
            .all()
            .iter()
            .filter(|mt| mt.base == rel)
            .map(|mt| path::to_text(&mt.at))
            .collect();
        Ok(UdfsInfo {
            path: format!("/dev/{asp}/{}", backend::last(&rel)),
            case_sensitive: m.flags.has(Flags::CASE_SENSITIVE),
            owner: m.owner,
            text: m.text,
            created: m.created,
            mounted_at,
            crtobjaud: m.crtobjaud,
        })
    }
}

/// Remove a backend subtree.
fn remove_rel<B: Backend>(b: &B, rel: &str) -> Result<()> {
    let st = b.stat(rel)?;
    if st.kind == Kind::Dir {
        for e in b.readdir(rel)? {
            remove_rel(b, &backend::join(rel, &e.name))?;
        }
        b.rmdir(rel)
    } else {
        b.unlink(rel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn udfs_names() {
        assert_eq!(
            split_udfs("/dev/qasp01/Mine.UDFS").unwrap(),
            ("QASP01".into(), "Mine.UDFS".into())
        );
        assert_eq!(split_udfs("/dev/QASP1/x.udfs"), Err(Errno::EINVAL));
        assert_eq!(split_udfs("/home/x.udfs"), Err(Errno::EINVAL));
        assert_eq!(split_udfs("/dev/QASP01/.udfs"), Err(Errno::EINVAL));
    }
}
