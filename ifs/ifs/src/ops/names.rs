// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Operations on names: `mkdir()`, `rmdir()`, `unlink()`, `link()`,
//! `symlink()`, `readlink()`, `rename()` (`Qp0lRenameKeep` and
//! `Qp0lRenameUnlink`), `mkfifo()` and socket nodes.

use super::audit::AuditKind;
use super::open::new_meta;
use super::{Ctx, Ifs, Rest};
use crate::auth::need::Op;
use crate::backend::{self, Backend, Kind};
use crate::errno::{Errno, Result};
use crate::meta::{self, Flags, Meta};
use crate::resolve::{Node, Res, Sys};
use crate::vfs::qsys::{self, QsysNode};

/// What `rename` does when the new name exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameMode {
    /// `Qp0lRenameKeep` (and ILE C `rename()`): fail with `EEXIST`.
    Keep,
    /// `Qp0lRenameUnlink`: replace it.
    Unlink,
}

impl<B: Backend> Ifs<B> {
    /// Whether the restricted rename and unlink rule of directory `dm`
    /// lets the caller remove or rename `m`: the object's owner, the
    /// directory's owner or `*ALLOBJ`.
    fn sticky_ok(cx: &Ctx<'_>, dm: &Meta, m: &Meta) -> bool {
        !dm.flags.has(Flags::RSTDRNMUNL)
            || cx.principal.user == m.owner
            || cx.principal.user == dm.owner
            || cx.principal.has(sec::profile::spcaut::SpecialAuth::ALLOBJ)
    }

    /// Make a new node in a backend directory with a fresh record.
    #[allow(clippy::too_many_arguments)]
    fn make_node(
        &self,
        cx: &Ctx<'_>,
        sys: &mut Sys<'_, B>,
        rest: &mut Rest<B::File>,
        dir: &Res,
        dm: &Meta,
        name: &str,
        kind: Kind,
        mode: u32,
        target: Option<&str>,
    ) -> Result<Res> {
        let Node::Back { rel, .. } = &dir.node else {
            return Err(Errno::ENOTSUP);
        };
        crate::path::check_name(name)?;
        let umask = rest
            .owners
            .get(&cx.owner)
            .map_or(self.cfg.umask, |o| o.umask);
        let ccsid = if kind == Kind::Dir {
            dm.ccsid
        } else {
            self.cfg.default_ccsid
        };
        let generation = Self::next_generation(rest);
        let mode = if kind == Kind::Symlink {
            0o777
        } else {
            mode & !umask & 0o7777
        };
        let m = new_meta(cx, dm, kind, mode, false, ccsid, generation);
        let crel = backend::join(rel, name);
        let bits = crate::auth::data::mode_of(&m);
        match kind {
            Kind::Dir => self.b.mkdir(&crel, bits)?,
            Kind::Symlink => self.b.symlink(target.unwrap_or_default(), &crel)?,
            Kind::File => drop(self.b.create(&crel, bits)?),
            k => self.b.mknod(&crel, k, bits)?,
        }
        meta::write(&self.b, &crel, kind, &m, cx.env)?;
        sys.index.added(sys.b, rel, name);
        let r = sys.child(dir, name)?;
        cx.audit_always(
            AuditKind::Create,
            &m,
            &r.node,
            &r.path(),
            "CREATE",
            String::new(),
        );
        Ok(r)
    }

    /// The parent directory of a path that must not exist yet, checked
    /// for creating in it.
    fn create_parent(
        cx: &Ctx<'_>,
        sys: &mut Sys<'_, B>,
        cwd: &[String],
        p: &str,
    ) -> Result<(Res, Meta, String)> {
        let (dir, name) = sys.resolve_parent(cwd, p, Some(cx))?;
        match sys.child(&dir, &name) {
            Ok(_) => return Err(Errno::EEXIST),
            Err(Errno::ENOENT) => {}
            Err(e) => return Err(e),
        }
        if Self::read_only_node(sys, &dir.node) {
            return Err(Errno::EROFS);
        }
        let (dm, _) = sys.meta(&dir.node, Some(cx))?;
        cx.require_dir(&dm, &dir.node, &dir.path(), Op::Create)?;
        Ok((dir, dm, name))
    }

    /// `mkdir()`. In `/QSYS.LIB`, `NAME.LIB` makes a library.
    pub fn mkdir(&self, cx: &Ctx<'_>, p: &str, mode: u32) -> Result<()> {
        let mut st = self.lock();
        let cwd = Self::cwd_of(&mut st, cx.owner);
        let (mut sys, rest) = self.split(&mut st);
        let (dir, dm, name) = Self::create_parent(cx, &mut sys, &cwd, p)?;
        if let Node::Qsys(QsysNode::Root, _) = dir.node {
            let (lib, t) = qsys::parse_component(&name)?;
            if t != "*LIB" {
                return Err(Errno::ENOTSUP);
            }
            return self.qsys.create_library(&lib, cx.principal);
        }
        if let Node::Opt { vol, path, .. } = &dir.node {
            let mut p = path.clone();
            p.push(name);
            self.opt.mkdir(vol, &p)?;
            return self.opt.sync(vol);
        }
        self.make_node(cx, &mut sys, rest, &dir, &dm, &name, Kind::Dir, mode, None)?;
        Ok(())
    }

    /// `mkfifo()`.
    pub fn mkfifo(&self, cx: &Ctx<'_>, p: &str, mode: u32) -> Result<()> {
        self.mknode(cx, p, Kind::Fifo, mode)
    }

    /// A socket node (what `bind()` of a local socket makes).
    pub fn mksock(&self, cx: &Ctx<'_>, p: &str, mode: u32) -> Result<()> {
        self.mknode(cx, p, Kind::Socket, mode)
    }

    fn mknode(&self, cx: &Ctx<'_>, p: &str, kind: Kind, mode: u32) -> Result<()> {
        let mut st = self.lock();
        let cwd = Self::cwd_of(&mut st, cx.owner);
        let (mut sys, rest) = self.split(&mut st);
        let (dir, dm, name) = Self::create_parent(cx, &mut sys, &cwd, p)?;
        self.make_node(cx, &mut sys, rest, &dir, &dm, &name, kind, mode, None)?;
        Ok(())
    }

    /// `symlink()`: a link at `p` holding `target` (not resolved).
    pub fn symlink(&self, cx: &Ctx<'_>, target: &str, p: &str) -> Result<()> {
        if target.is_empty() {
            return Err(Errno::ENOENT);
        }
        if target.len() > crate::path::PATH_MAX || target.contains('\0') {
            return Err(Errno::EINVAL);
        }
        let mut st = self.lock();
        let cwd = Self::cwd_of(&mut st, cx.owner);
        let (mut sys, rest) = self.split(&mut st);
        let (dir, dm, name) = Self::create_parent(cx, &mut sys, &cwd, p)?;
        self.make_node(
            cx,
            &mut sys,
            rest,
            &dir,
            &dm,
            &name,
            Kind::Symlink,
            0o777,
            Some(target),
        )?;
        Ok(())
    }

    /// `readlink()`.
    pub fn readlink(&self, cx: &Ctx<'_>, p: &str) -> Result<String> {
        let mut st = self.lock();
        let cwd = Self::cwd_of(&mut st, cx.owner);
        let (mut sys, _) = self.split(&mut st);
        let r = sys.resolve(&cwd, p, false, Some(cx))?;
        match &r.node {
            Node::Back { rel, st, .. } if st.kind == Kind::Symlink => self.b.readlink(rel),
            _ => Err(Errno::EINVAL),
        }
    }

    /// Resolve the object a removal names, with its directory.
    fn victim(
        cx: &Ctx<'_>,
        sys: &mut Sys<'_, B>,
        cwd: &[String],
        p: &str,
        op: Op,
    ) -> Result<(Res, Meta, Res, Meta)> {
        let (dir, name) = sys.resolve_parent(cwd, p, Some(cx))?;
        let r = sys.child(&dir, &name)?;
        let (dm, _) = sys.meta(&dir.node, Some(cx))?;
        let (m, _) = sys.meta(&r.node, Some(cx))?;
        if sys.mounts.all().iter().any(|mt| mt.at == r.comps) {
            return Err(Errno::EBUSY);
        }
        if Self::read_only_node(sys, &r.node) {
            return Err(Errno::EROFS);
        }
        cx.require_dir(&dm, &dir.node, &dir.path(), op)?;
        cx.require(&m, &r.node, &r.path(), op)?;
        if !Self::sticky_ok(cx, &dm, &m) {
            return Err(Errno::EPERM);
        }
        Ok((dir, dm, r, m))
    }

    /// `unlink()`.
    pub fn unlink(&self, cx: &Ctx<'_>, p: &str) -> Result<()> {
        let mut st = self.lock();
        let cwd = Self::cwd_of(&mut st, cx.owner);
        let (mut sys, _) = self.split(&mut st);
        let (dir, _, r, m) = Self::victim(cx, &mut sys, &cwd, p, Op::Unlink)?;
        if r.node.is_dir() {
            return Err(if matches!(r.node, Node::Qsys(..)) {
                Errno::ENOTSUP
            } else {
                Errno::EISDIR
            });
        }
        if let Some(co) = &m.checkout
            && co.user != cx.principal.user
        {
            return Err(Errno::EBUSY);
        }
        match (&dir.node, &r.node) {
            (Node::Back { rel: drel, .. }, Node::Back { rel, .. }) => {
                self.b.unlink(rel)?;
                sys.index.removed(sys.b, drel, backend::last(rel));
            }
            (_, Node::Qsys(n, _)) => self.qsys.delete(n, cx.principal)?,
            (_, Node::Opt { vol, path, .. }) => {
                self.opt.remove(vol, path)?;
                self.opt.sync(vol)?;
            }
            _ => return Err(Errno::ENOTSUP),
        }
        cx.audit_always(
            AuditKind::Delete,
            &m,
            &r.node,
            &r.path(),
            "UNLINK",
            String::new(),
        );
        Ok(())
    }

    /// `rmdir()`.
    pub fn rmdir(&self, cx: &Ctx<'_>, p: &str) -> Result<()> {
        let mut st = self.lock();
        let cwd = Self::cwd_of(&mut st, cx.owner);
        let (mut sys, rest) = self.split(&mut st);
        let (dir, _, r, m) = Self::victim(cx, &mut sys, &cwd, p, Op::Rmdir)?;
        if !r.node.is_dir() {
            return Err(Errno::ENOTDIR);
        }
        if rest.owners.values().any(|o| o.cwd == r.comps) {
            return Err(Errno::EBUSY);
        }
        match (&dir.node, &r.node) {
            (Node::Back { rel: drel, .. }, Node::Back { rel, .. }) => {
                self.b.rmdir(rel)?;
                sys.index.removed(sys.b, drel, backend::last(rel));
                sys.index.drop_tree(rel);
            }
            (_, Node::Qsys(n @ QsysNode::Lib(_), _)) => self.qsys.delete(n, cx.principal)?,
            (_, Node::Opt { vol, path, .. }) if !path.is_empty() => {
                self.opt.remove(vol, path)?;
                self.opt.sync(vol)?;
            }
            _ => return Err(Errno::ENOTSUP),
        }
        cx.audit_always(
            AuditKind::Delete,
            &m,
            &r.node,
            &r.path(),
            "RMDIR",
            String::new(),
        );
        Ok(())
    }

    /// `link()`: a new hard link `new` to `existing` (same file system,
    /// not a directory).
    pub fn link(&self, cx: &Ctx<'_>, existing: &str, new: &str) -> Result<()> {
        let mut st = self.lock();
        let cwd = Self::cwd_of(&mut st, cx.owner);
        let (mut sys, _) = self.split(&mut st);
        let src = sys.resolve(&cwd, existing, false, Some(cx))?;
        let (dir, dm, name) = {
            let (dir, name) = sys.resolve_parent(&cwd, new, Some(cx))?;
            match sys.child(&dir, &name) {
                Ok(_) => return Err(Errno::EEXIST),
                Err(Errno::ENOENT) => {}
                Err(e) => return Err(e),
            }
            let (dm, _) = sys.meta(&dir.node, Some(cx))?;
            (dir, dm, name)
        };
        crate::path::check_name(&name)?;
        let (
            Node::Back { fs, rel, st: sst },
            Node::Back {
                fs: dfs, rel: drel, ..
            },
        ) = (&src.node, &dir.node)
        else {
            return Err(Errno::ENOTSUP);
        };
        if fs != dfs {
            return Err(Errno::EXDEV);
        }
        if sst.kind == Kind::Dir {
            return Err(Errno::EPERM);
        }
        if sst.nlink >= crate::path::LINK_MAX {
            return Err(Errno::EMLINK);
        }
        let (m, _) = sys.meta(&src.node, Some(cx))?;
        cx.require_dir(&dm, &dir.node, &dir.path(), Op::Link)?;
        cx.require(&m, &src.node, &src.path(), Op::Link)?;
        self.b.link(rel, &backend::join(drel, &name))?;
        sys.index.added(sys.b, drel, &name);
        cx.audit_access(&m, &src.node, &src.path(), Op::Link);
        Ok(())
    }

    /// `rename()`: `Keep` fails when `new` exists, `Unlink` replaces it.
    #[allow(clippy::too_many_lines)]
    pub fn rename(&self, cx: &Ctx<'_>, old: &str, new: &str, mode: RenameMode) -> Result<()> {
        let mut st = self.lock();
        let cwd = Self::cwd_of(&mut st, cx.owner);
        let (mut sys, rest) = self.split(&mut st);
        let (odir, oname) = sys.resolve_parent(&cwd, old, Some(cx))?;
        let src = sys.child(&odir, &oname)?;
        if sys.mounts.all().iter().any(|mt| mt.at == src.comps) {
            return Err(Errno::EBUSY);
        }
        let (ndir, nname) = sys.resolve_parent(&cwd, new, Some(cx))?;
        crate::path::check_name(&nname)?;
        let (
            Node::Back { fs, rel, st: sst },
            Node::Back {
                fs: nfs,
                rel: ndrel,
                ..
            },
            Node::Back { rel: odrel, .. },
        ) = (&src.node, &ndir.node, &odir.node)
        else {
            return Err(Errno::ENOTSUP);
        };
        if fs != nfs {
            return Err(Errno::EXDEV);
        }
        let (odm, _) = sys.meta(&odir.node, Some(cx))?;
        let (ndm, _) = sys.meta(&ndir.node, Some(cx))?;
        let (m, _) = sys.meta(&src.node, Some(cx))?;
        cx.require_dir(&odm, &odir.node, &odir.path(), Op::Rename)?;
        cx.require_dir(&ndm, &ndir.node, &ndir.path(), Op::Rename)?;
        cx.require(&m, &src.node, &src.path(), Op::Rename)?;
        if !Self::sticky_ok(cx, &odm, &m) {
            return Err(Errno::EPERM);
        }
        if Self::read_only(&sys, *fs) {
            return Err(Errno::EROFS);
        }
        if let Some(co) = &m.checkout
            && co.user != cx.principal.user
        {
            return Err(Errno::EBUSY);
        }
        let moving_dir = sst.kind == Kind::Dir;
        // A directory into itself.
        if moving_dir && ndir.comps.starts_with(&src.comps) {
            return Err(Errno::EINVAL);
        }
        let new_rel = backend::join(ndrel, &nname);
        let mut replace = false;
        match sys.child(&ndir, &nname) {
            Ok(t) => {
                let same = t.node.key() == src.node.key();
                // The same name, or another link to the same file: nothing to do.
                if same && t.node.rel() == Some(new_rel.as_str()) {
                    return Ok(());
                }
                if !same {
                    if mode == RenameMode::Keep {
                        return Err(Errno::EEXIST);
                    }
                    let (tm, _) = sys.meta(&t.node, Some(cx))?;
                    cx.require(&tm, &t.node, &t.path(), Op::RenameReplace)?;
                    if !Self::sticky_ok(cx, &ndm, &tm) {
                        return Err(Errno::EPERM);
                    }
                    match (moving_dir, t.node.is_dir()) {
                        (true, false) => return Err(Errno::ENOTDIR),
                        (false, true) => return Err(Errno::EISDIR),
                        _ => {}
                    }
                    if sys.mounts.all().iter().any(|mt| mt.at == t.comps) {
                        return Err(Errno::EBUSY);
                    }
                    // The stored name may differ in case: remove it first.
                    let trel = t.node.rel().ok_or(Errno::ENOTSUP)?.to_string();
                    if trel == new_rel {
                        replace = true;
                    } else {
                        if t.node.is_dir() {
                            self.b.rmdir(&trel)?;
                        } else {
                            self.b.unlink(&trel)?;
                        }
                        sys.index.removed(sys.b, ndrel, backend::last(&trel));
                    }
                }
            }
            Err(Errno::ENOENT) => {}
            Err(e) => return Err(e),
        }
        self.b.rename(rel, &new_rel, replace)?;
        sys.index.removed(sys.b, odrel, backend::last(rel));
        sys.index.added(sys.b, ndrel, &nname);
        if moving_dir {
            sys.index.drop_tree(rel);
        }
        let mut new_comps = ndir.comps.clone();
        new_comps.push(nname.clone());
        // Current directories and open paths under the old name follow.
        for o in rest.owners.values_mut() {
            if o.cwd.starts_with(&src.comps) {
                let tail = o.cwd[src.comps.len()..].to_vec();
                o.cwd = new_comps.iter().cloned().chain(tail).collect();
            }
        }
        let old_text = src.path();
        let new_text = crate::path::to_text(&new_comps);
        for o in rest.opens.values_mut() {
            if o.path == old_text {
                o.path.clone_from(&new_text);
            } else if let Some(t) = o.path.strip_prefix(&format!("{old_text}/")) {
                o.path = format!("{new_text}/{t}");
            }
        }
        cx.audit_always(
            AuditKind::Move,
            &m,
            &src.node,
            &old_text,
            "RENAME",
            new_text,
        );
        cx.audit_access(&m, &src.node, &old_text, Op::Rename);
        Ok(())
    }
}
