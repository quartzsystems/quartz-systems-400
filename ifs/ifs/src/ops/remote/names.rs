// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Names in remote directories: creating files, directories, links and
//! FIFOs, removing (a file still open is hidden under `.nfs` and
//! removed at its last close, unless the server keeps such files
//! itself), renaming and linking within one mount (across mounts
//! `EXDEV`). Each change is made once for the request; `ro` and
//! `nosuid` are enforced here before any call.

use super::super::audit::AuditKind;
use super::super::flags as f;
use super::super::{Ctx, Ifs, Rest};
use crate::backend::{Backend, Kind};
use crate::errno::{Errno, Result};
use crate::lease::notify::ChangeKind;
use crate::meta::Meta;
use crate::remote::mount::Touch;
use crate::remote::request;
use crate::remote::work::{Answer, Mutation};
use crate::remote::{CreateMode, RKind, SetAttr};
use crate::resolve::{Node, Res, Sys};

/// The record a remote node shows (owner from its UID, data authorities
/// from its mode).
pub(crate) fn meta_of(sys: &Sys<'_, impl Backend>, node: &Node, cx: &Ctx<'_>) -> Meta {
    sys.meta(node, Some(cx))
        .map_or_else(|_| Meta::new("QDFTOWN", 65535, 0), |(m, _)| m)
}

impl<B: Backend> Ifs<B> {
    /// What the request already did to `name` in remote `dir`: its
    /// result, when an earlier run queued the change.
    pub(crate) fn remote_replayed(
        sys: &Sys<'_, B>,
        dir: &Res,
        name: &str,
        touch: Touch,
    ) -> Option<Result<Answer>> {
        let Node::Remote { fs, handle, .. } = &dir.node else {
            return None;
        };
        let m = sys.remote(*fs).ok()?;
        let nb = m.encode(name).ok()?;
        let n = m.replayed(touch, handle, &nb)?;
        Some(m.settle(n))
    }

    /// [`Self::remote_replayed`] of the last name of `p`.
    pub(crate) fn remote_replay_at(
        cx: &Ctx<'_>,
        sys: &mut Sys<'_, B>,
        cwd: &[String],
        p: &str,
        touches: &[Touch],
    ) -> Option<Result<()>> {
        if !sys.mounts.has_remote() {
            return None;
        }
        let (dir, name) = sys.resolve_parent(cwd, p, Some(cx)).ok()?;
        touches
            .iter()
            .find_map(|t| Self::remote_replayed(sys, &dir, &name, *t))
            .map(|r| r.map(|_| ()))
    }

    /// Whether this request already made the last name of `p` in a
    /// remote directory (a run after the change was queued).
    pub(crate) fn remote_made(&self, cx: &Ctx<'_>, p: &str) -> bool {
        let mut st = self.lock();
        let cwd = Self::cwd_of(&mut st, cx.owner);
        let (mut sys, _) = self.split(&mut st);
        Self::remote_replay_at(cx, &mut sys, &cwd, p, &[Touch::Make]).is_some()
    }

    /// What this request's earlier run did removing the last name of
    /// `p` in a remote directory, if it did.
    pub(crate) fn remote_removed(&self, cx: &Ctx<'_>, p: &str) -> Option<Result<()>> {
        let mut st = self.lock();
        let cwd = Self::cwd_of(&mut st, cx.owner);
        let (mut sys, _) = self.split(&mut st);
        Self::remote_replay_at(cx, &mut sys, &cwd, p, &[Touch::Remove, Touch::Rename])
    }

    fn umask_of(&self, rest: &Rest<B::File>, cx: &Ctx<'_>) -> u32 {
        rest.owners
            .get(&cx.owner)
            .map_or(self.cfg.umask, |o| o.umask)
    }

    /// Make a node in remote directory `dir`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn remote_make(
        &self,
        cx: &Ctx<'_>,
        sys: &mut Sys<'_, B>,
        rest: &Rest<B::File>,
        dir: &Res,
        name: &str,
        kind: Kind,
        mode: u32,
        target: Option<&str>,
    ) -> Result<Res> {
        let Node::Remote { fs, handle, .. } = &dir.node else {
            return Err(Errno::ENOTSUP);
        };
        crate::path::check_name(name)?;
        let m = sys.remote(*fs)?;
        if m.info().read_only {
            return Err(Errno::EROFS);
        }
        request::note_owner(cx.owner);
        let mut mode = mode & !self.umask_of(rest, cx) & 0o7777;
        if m.info().nosuid {
            mode &= !0o6000;
        }
        let set = SetAttr {
            mode: Some(mode),
            ..SetAttr::default()
        };
        let nb = m.encode(name)?;
        let cred = request::cred();
        let mu = match kind {
            Kind::Dir => Mutation::Mkdir {
                dir: handle.clone(),
                name: nb,
                set,
            },
            Kind::Symlink => Mutation::Symlink {
                dir: handle.clone(),
                name: nb,
                target: m.encode(target.unwrap_or_default())?,
                set,
            },
            Kind::Fifo => Mutation::Mknod {
                dir: handle.clone(),
                name: nb,
                kind: RKind::Fifo,
                set,
            },
            Kind::File => Mutation::Create {
                dir: handle.clone(),
                name: nb,
                how: CreateMode::Guarded(set),
            },
            Kind::Socket | Kind::CharDev => return Err(Errno::ENOTSUP),
        };
        let Answer::Node(h, attr) = m.settle(m.mutate(mu, &cred))? else {
            return Err(Errno::EIO);
        };
        let mut comps = dir.comps.clone();
        comps.push(name.to_string());
        let r = Res {
            comps,
            node: Node::Remote {
                fs: *fs,
                handle: h,
                attr,
            },
        };
        self.leases
            .note(cx.owner, &r.path(), &ChangeKind::Added, kind == Kind::Dir);
        let meta = meta_of(sys, &r.node, cx);
        cx.audit_always(
            AuditKind::Create,
            &meta,
            &r.node,
            &r.path(),
            "CREATE",
            String::new(),
        );
        Ok(r)
    }

    /// `open(O_CREAT)` of a name not in remote directory `dir`:
    /// exclusive with `O_EXCL` (a retried create succeeds once), then
    /// its mode.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn remote_create(
        &self,
        cx: &Ctx<'_>,
        sys: &mut Sys<'_, B>,
        rest: &Rest<B::File>,
        dir: &Res,
        name: &str,
        oflag: u32,
        mode: u32,
    ) -> Result<(Res, Meta)> {
        let Node::Remote { fs, handle, .. } = &dir.node else {
            return Err(Errno::ENOTSUP);
        };
        crate::path::check_name(name)?;
        let m = sys.remote(*fs)?;
        if m.info().read_only {
            return Err(Errno::EROFS);
        }
        request::note_owner(cx.owner);
        let mut mode = mode & !self.umask_of(rest, cx) & 0o7777;
        if m.info().nosuid {
            mode &= !0o6000;
        }
        let set = SetAttr {
            mode: Some(mode),
            ..SetAttr::default()
        };
        let cred = request::cred();
        let excl = oflag & f::O_EXCL != 0;
        let how = if excl {
            CreateMode::Exclusive(request::current().map_or(0, |t| t.id))
        } else {
            CreateMode::Unchecked(set)
        };
        let Answer::Node(h, mut attr) = m.settle(m.mutate(
            Mutation::Create {
                dir: handle.clone(),
                name: m.encode(name)?,
                how,
            },
            &cred,
        ))?
        else {
            return Err(Errno::EIO);
        };
        if excl {
            // An exclusive create sets no attributes: the mode after.
            if let Answer::Attr(a) =
                m.settle(m.mutate(Mutation::Setattr { h: h.clone(), set }, &cred))?
            {
                attr = a;
            }
        }
        let mut comps = dir.comps.clone();
        comps.push(name.to_string());
        let r = Res {
            comps,
            node: Node::Remote {
                fs: *fs,
                handle: h,
                attr,
            },
        };
        self.leases
            .note(cx.owner, &r.path(), &ChangeKind::Added, false);
        let meta = meta_of(sys, &r.node, cx);
        cx.audit_always(
            AuditKind::Create,
            &meta,
            &r.node,
            &r.path(),
            "CREATE",
            String::new(),
        );
        Ok((r, meta))
    }

    /// `unlink()` of `r` in remote directory `dir`.
    pub(crate) fn remote_unlink(
        &self,
        cx: &Ctx<'_>,
        sys: &Sys<'_, B>,
        rest: &Rest<B::File>,
        dir: &Res,
        r: &Res,
    ) -> Result<()> {
        let (Node::Remote { fs, handle: dh, .. }, Node::Remote { handle, attr, .. }) =
            (&dir.node, &r.node)
        else {
            return Err(Errno::EXDEV);
        };
        if r.node.is_dir() {
            return Err(Errno::EISDIR);
        }
        let m = sys.remote(*fs)?;
        if m.info().read_only {
            return Err(Errno::EROFS);
        }
        request::note_owner(cx.owner);
        let cred = request::cred();
        let name = m.encode(r.comps.last().map_or("", String::as_str))?;
        let open = rest.opens.values().any(|o| o.key == r.node.key());
        if open && !m.info().keeps_open_unlinked {
            // Still open: hidden, removed at its last close.
            let tok = request::current().map_or(0, |t| t.id);
            let hidden = format!(".nfs{:016x}{tok:08x}", attr.fileid).into_bytes();
            // Known before the rename is sent: a later run of this
            // request finds the rename done and does not come here.
            m.hide(handle, dh, &hidden);
            let r = m.settle(m.mutate(
                Mutation::Rename {
                    from_dir: dh.clone(),
                    from: name,
                    to_dir: dh.clone(),
                    to: hidden.clone(),
                },
                &cred,
            ));
            if let Err(e) = r {
                if e != Errno::EAGAIN {
                    m.take_hidden(handle);
                }
                return Err(e);
            }
        } else {
            m.settle(m.mutate(
                Mutation::Remove {
                    dir: dh.clone(),
                    name,
                    h: Some(handle.clone()),
                },
                &cred,
            ))?;
        }
        self.leases
            .note(cx.owner, &r.path(), &ChangeKind::Removed, false);
        let meta = meta_of(sys, &r.node, cx);
        cx.audit_always(
            AuditKind::Delete,
            &meta,
            &r.node,
            &r.path(),
            "UNLINK",
            String::new(),
        );
        Ok(())
    }

    /// `rmdir()` of `r` in remote directory `dir`.
    pub(crate) fn remote_rmdir(
        &self,
        cx: &Ctx<'_>,
        sys: &Sys<'_, B>,
        dir: &Res,
        r: &Res,
    ) -> Result<()> {
        let (Node::Remote { fs, handle: dh, .. }, Node::Remote { handle, .. }) =
            (&dir.node, &r.node)
        else {
            return Err(Errno::EXDEV);
        };
        if !r.node.is_dir() {
            return Err(Errno::ENOTDIR);
        }
        let m = sys.remote(*fs)?;
        if m.info().read_only {
            return Err(Errno::EROFS);
        }
        request::note_owner(cx.owner);
        let name = m.encode(r.comps.last().map_or("", String::as_str))?;
        m.settle(m.mutate(
            Mutation::Rmdir {
                dir: dh.clone(),
                name,
                h: Some(handle.clone()),
            },
            &request::cred(),
        ))?;
        self.leases.note_rmdir(cx.owner, &r.path());
        let meta = meta_of(sys, &r.node, cx);
        cx.audit_always(
            AuditKind::Delete,
            &meta,
            &r.node,
            &r.path(),
            "RMDIR",
            String::new(),
        );
        Ok(())
    }

    /// `rename()` within one remote mount; `keep` fails when the new
    /// name exists.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn remote_rename(
        &self,
        cx: &Ctx<'_>,
        sys: &mut Sys<'_, B>,
        rest: &mut Rest<B::File>,
        odir: &Res,
        src: &Res,
        ndir: &Res,
        nname: &str,
        keep: bool,
    ) -> Result<()> {
        let (
            Node::Remote { fs, handle: oh, .. },
            Node::Remote {
                fs: nfs,
                handle: nh,
                ..
            },
            Node::Remote { .. },
        ) = (&odir.node, &ndir.node, &src.node)
        else {
            return Err(Errno::EXDEV);
        };
        if fs != nfs {
            return Err(Errno::EXDEV);
        }
        crate::path::check_name(nname)?;
        let m = sys.remote(*fs)?;
        if m.info().read_only {
            return Err(Errno::EROFS);
        }
        if src.node.is_dir() && ndir.comps.starts_with(&src.comps) {
            return Err(Errno::EINVAL);
        }
        request::note_owner(cx.owner);
        match sys.child(ndir, nname) {
            Ok(t) if t.node.key() == src.node.key() => {
                if t.comps == src.comps {
                    return Ok(());
                }
            }
            Ok(t) => {
                if keep {
                    return Err(Errno::EEXIST);
                }
                match (src.node.is_dir(), t.node.is_dir()) {
                    (true, false) => return Err(Errno::ENOTDIR),
                    (false, true) => return Err(Errno::EISDIR),
                    _ => {}
                }
            }
            Err(Errno::ENOENT) => {}
            Err(e) => return Err(e),
        }
        let from = m.encode(src.comps.last().map_or("", String::as_str))?;
        m.settle(m.mutate(
            Mutation::Rename {
                from_dir: oh.clone(),
                from,
                to_dir: nh.clone(),
                to: m.encode(nname)?,
            },
            &request::cred(),
        ))?;
        let mut new_comps = ndir.comps.clone();
        new_comps.push(nname.to_string());
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
        self.leases
            .note_rename(cx.owner, &old_text, &new_text, src.node.is_dir());
        let meta = meta_of(sys, &src.node, cx);
        cx.audit_always(
            AuditKind::Move,
            &meta,
            &src.node,
            &old_text,
            "RENAME",
            new_text,
        );
        Ok(())
    }

    /// `link()` within one remote mount.
    pub(crate) fn remote_link(
        &self,
        cx: &Ctx<'_>,
        sys: &Sys<'_, B>,
        src: &Res,
        dir: &Res,
        name: &str,
    ) -> Result<()> {
        let (
            Node::Remote { fs, handle, .. },
            Node::Remote {
                fs: dfs,
                handle: dh,
                ..
            },
        ) = (&src.node, &dir.node)
        else {
            return Err(Errno::EXDEV);
        };
        if fs != dfs {
            return Err(Errno::EXDEV);
        }
        if src.node.is_dir() {
            return Err(Errno::EPERM);
        }
        crate::path::check_name(name)?;
        let m = sys.remote(*fs)?;
        if m.info().read_only {
            return Err(Errno::EROFS);
        }
        request::note_owner(cx.owner);
        m.settle(m.mutate(
            Mutation::Link {
                h: handle.clone(),
                dir: dh.clone(),
                name: m.encode(name)?,
            },
            &request::cred(),
        ))?;
        let mut comps = dir.comps.clone();
        comps.push(name.to_string());
        self.leases.note(
            cx.owner,
            &crate::path::to_text(&comps),
            &ChangeKind::Added,
            false,
        );
        Ok(())
    }
}
