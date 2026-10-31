// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `open()`, `creat()`, `close()`, `dup()`, `dup2()` and `fcntl()`.

use sec::auth::bits::Auth;

use super::audit::AuditKind;
use super::flags::{self as f};
use super::{Ctx, FdEntry, Ifs, Open, OpenKind, Rest, now};
use crate::auth::data::DataAuth;
use crate::auth::need::Op;
use crate::backend::{self, BFile, Backend, Kind};
use crate::errno::{Errno, Result};
use crate::lock::OpenRef;
use crate::lock::range::{RangeKind, RangeLock};
use crate::lock::share::{Access, Share};
use crate::meta::{CrtObjAud, Flags, Meta};
use crate::resolve::{Node, NodeType, Res};
use crate::text::TextState;
use crate::vfs::qsys::{self, QsysNode};

/// `open()`'s arguments after the path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OpenArgs {
    /// `oflag`.
    pub oflag: u32,
    /// `mode` for a new file.
    pub mode: u32,
    /// `conversion_id`: a CCSID with `O_CCSID`, a code page with
    /// `O_CODEPAGE`; 0 is the job's.
    pub conversion_id: u32,
    /// With `O_TEXT_CREAT`: the CCSID the data is in (0: the job's).
    pub text_conversion_id: u32,
}

impl OpenArgs {
    /// Flags only.
    #[must_use]
    pub fn new(oflag: u32) -> Self {
        Self {
            oflag,
            ..Self::default()
        }
    }

    /// With a mode.
    #[must_use]
    pub fn mode(mut self, mode: u32) -> Self {
        self.mode = mode;
        self
    }

    /// With a conversion id.
    #[must_use]
    pub fn conv(mut self, id: u32) -> Self {
        self.conversion_id = id;
        self
    }

    /// With the `O_TEXT_CREAT` data CCSID.
    #[must_use]
    pub fn text_conv(mut self, id: u32) -> Self {
        self.text_conversion_id = id;
        self
    }
}

fn access_of(oflag: u32) -> Result<(Access, Op)> {
    Ok(match oflag & f::O_ACCMODE {
        f::O_RDONLY => (
            Access {
                read: true,
                write: false,
            },
            Op::OpenRead,
        ),
        f::O_WRONLY => (
            Access {
                read: false,
                write: true,
            },
            Op::OpenWrite,
        ),
        f::O_RDWR => (
            Access {
                read: true,
                write: true,
            },
            Op::OpenRdWr,
        ),
        _ => return Err(Errno::EINVAL),
    })
}

/// A CCSID from a conversion id, 0 being the job's.
fn ccsid_from(id: u32, codepage: bool, job: u16) -> Result<u16> {
    let v = if id == 0 {
        job
    } else {
        u16::try_from(id).map_err(|_| Errno::EINVAL)?
    };
    Ok(if codepage {
        ccsid::conv::ccsid_of_codepage(v)
    } else {
        v
    })
}

/// The record for a new object in a directory whose record is `parent`:
/// the caller owns it with every object authority; the primary group,
/// private authorities, authorization list and `*PUBLIC` object
/// authorities come from the parent; data authorities from `mode`
/// (less the umask), or with `inherit` from the parent's; auditing from
/// the parent's `*CRTOBJAUD`, scanning from its `*CRTOBJSCAN`.
pub(crate) fn new_meta(
    cx: &Ctx<'_>,
    parent: &Meta,
    kind: Kind,
    mode: u32,
    inherit: bool,
    ccsid: u16,
    generation: u32,
) -> Meta {
    let mut m = Meta::new(&cx.principal.user, ccsid, now());
    let part = |shift: u32| DataAuth::from_mode_bits(mode >> shift);
    let (own, grp, publ) = if inherit {
        (
            DataAuth::of(parent.owner_auth),
            DataAuth::of(parent.group_auth),
            DataAuth::of(parent.public),
        )
    } else {
        (part(6), part(3), part(0))
    };
    m.owner_auth = own.with_objects(Auth::ALL);
    m.group = parent.group.clone().filter(|g| *g != m.owner);
    // Kept without a group too: a later CHGPGP gives it to the new group.
    m.group_auth = grp.with_objects(parent.group_auth);
    m.public = publ.with_objects(parent.public);
    m.private = parent
        .private
        .iter()
        .filter(|(who, _)| *who != m.owner)
        .cloned()
        .collect();
    m.autl.clone_from(&parent.autl);
    m.audit = match parent.crtobjaud {
        CrtObjAud::SysVal => cx.env.crtobjaud(),
        CrtObjAud::Value(v) => v,
    };
    m.scan = parent.crtobjscan;
    if kind == Kind::Dir {
        m.crtobjaud = parent.crtobjaud;
        m.crtobjscan = parent.crtobjscan;
        m.flags
            .set(Flags::INHALWCKPWRT, parent.flags.has(Flags::INHALWCKPWRT));
    }
    m.flags
        .set(Flags::ALWCKPWRT, parent.flags.has(Flags::INHALWCKPWRT));
    m.flags
        .set(Flags::SETGID, mode & 0o2000 != 0 && kind == Kind::Dir);
    m.flags
        .set(Flags::RSTDRNMUNL, mode & 0o1000 != 0 && kind == Kind::Dir);
    if let Some(j) = &parent.journal
        && j.inherit
    {
        m.journal = Some(j.clone());
    }
    m.generation = generation;
    m
}

impl<B: Backend> Ifs<B> {
    fn alloc_fd(&self, rest: &mut Rest<B::File>, cx: &Ctx<'_>, open: u64, min: i32) -> Result<i32> {
        let max = self.cfg.max_fds;
        let os = self.owner_state(rest, cx.owner);
        if os.fds.len() >= max {
            return Err(Errno::EMFILE);
        }
        let mut fd = min.max(0);
        while os.fds.contains_key(&fd) {
            fd += 1;
        }
        if usize::try_from(fd).map_err(|_| Errno::EMFILE)? >= max {
            return Err(Errno::EMFILE);
        }
        os.fds.insert(
            fd,
            FdEntry {
                open,
                cloexec: false,
            },
        );
        Ok(fd)
    }

    /// `creat()`: `open(path, O_WRONLY | O_CREAT | O_TRUNC, mode)`.
    pub fn creat(&self, cx: &Ctx<'_>, path: &str, mode: u32) -> Result<i32> {
        self.open(
            cx,
            path,
            OpenArgs::new(f::O_WRONLY | f::O_CREAT | f::O_TRUNC).mode(mode),
        )
    }

    /// `open()`.
    #[allow(clippy::too_many_lines)]
    pub fn open(&self, cx: &Ctx<'_>, path: &str, a: OpenArgs) -> Result<i32> {
        let (access, op) = access_of(a.oflag)?;
        let share = Share::from_flags(a.oflag).ok_or(Errno::EINVAL)?;
        if a.oflag & f::O_CCSID != 0 && a.oflag & f::O_CODEPAGE != 0 {
            return Err(Errno::EINVAL);
        }
        let mut st = self.lock();
        let cwd = Self::cwd_of(&mut st, cx.owner);
        let umask = st
            .rest
            .owners
            .get(&cx.owner)
            .map_or(self.cfg.umask, |o| o.umask);
        let (mut sys, rest) = self.split(&mut st);
        let follow = a.oflag & f::O_NOFOLLOW == 0;
        let creating = a.oflag & f::O_CREAT != 0;
        let codepage = a.oflag & f::O_CODEPAGE != 0;
        let conv_given = a.oflag & (f::O_CCSID | f::O_CODEPAGE) != 0;
        let mut created: Option<B::File> = None;
        let (r, m) = match sys.resolve(&cwd, path, follow, Some(cx)) {
            Ok(r) => {
                if creating && a.oflag & f::O_EXCL != 0 {
                    return Err(Errno::EEXIST);
                }
                let (m, _) = sys.meta(&r.node, Some(cx))?;
                (r, m)
            }
            Err(Errno::ENOENT) if creating => {
                let (dir, name) = sys.resolve_parent(&cwd, path, Some(cx))?;
                crate::path::check_name(&name)?;
                if Self::read_only_node(&sys, &dir.node) {
                    return Err(Errno::EROFS);
                }
                let (dm, _) = sys.meta(&dir.node, Some(cx))?;
                cx.require_dir(&dm, &dir.node, &dir.path(), Op::Create)?;
                match &dir.node {
                    Node::Opt { vol, path, .. } => {
                        let mut p = path.clone();
                        p.push(name.clone());
                        self.opt.create(vol, &p)?;
                        let r = sys.child(&dir, &name)?;
                        let (m, _) = sys.meta(&r.node, Some(cx))?;
                        (r, m)
                    }
                    Node::Back { rel, .. } => {
                        let file_ccsid = if conv_given {
                            ccsid_from(a.conversion_id, codepage, cx.job_ccsid)?
                        } else {
                            self.cfg.default_ccsid
                        };
                        let generation = Self::next_generation(rest);
                        let inherit = a.oflag & f::O_INHERITMODE != 0;
                        let m = new_meta(
                            cx,
                            &dm,
                            Kind::File,
                            a.mode & !umask & 0o7777,
                            inherit,
                            file_ccsid,
                            generation,
                        );
                        let crel = backend::join(rel, &name);
                        let file = self.b.create(&crel, crate::auth::data::mode_of(&m))?;
                        crate::meta::write(&self.b, &crel, Kind::File, &m, cx.env)?;
                        sys.index.added(sys.b, rel, &name);
                        created = Some(file);
                        let r = sys.child(&dir, &name)?;
                        cx.audit_always(
                            AuditKind::Create,
                            &m,
                            &r.node,
                            &r.path(),
                            "CREATE",
                            String::new(),
                        );
                        (r, m)
                    }
                    Node::Qsys(QsysNode::Obj(lib, file, t), _) if t == "*FILE" => {
                        let (mbr, mt) = qsys::parse_component(&name)?;
                        if mt != "*MBR" {
                            return Err(Errno::ENOTSUP);
                        }
                        self.qsys.create_member(lib, file, &mbr, cx.principal)?;
                        let r = sys.child(&dir, &name)?;
                        let (m, _) = sys.meta(&r.node, Some(cx))?;
                        (r, m)
                    }
                    _ => return Err(Errno::ENOTSUP),
                }
            }
            Err(e) => return Err(e),
        };
        let path_s = r.path();
        match r.node.ntype() {
            NodeType::Dir if access.write || a.oflag & (f::O_TRUNC | f::O_CREAT) != 0 => {
                return Err(Errno::EISDIR);
            }
            NodeType::Symlink => return Err(Errno::ELOOP),
            NodeType::Socket => return Err(Errno::ENXIO),
            NodeType::Fifo | NodeType::Blk | NodeType::Native => return Err(Errno::ENOTSUP),
            NodeType::Dir | NodeType::File | NodeType::Chr => {}
        }
        let truncate = a.oflag & f::O_TRUNC != 0;
        if (access.write || truncate) && Self::read_only_node(&sys, &r.node) {
            return Err(Errno::EROFS);
        }
        if created.is_none() {
            let need_op = if r.node.is_dir() {
                Op::Readdir
            } else if truncate && !access.write {
                Op::OpenRdWr
            } else {
                op
            };
            cx.require(&m, &r.node, &path_s, need_op)?;
        }
        let writes = access.write || truncate;
        if writes {
            if m.flags.has(Flags::RDONLY) {
                return Err(Errno::EROOBJ);
            }
            if let Some(co) = &m.checkout
                && co.user != cx.principal.user
            {
                return Err(Errno::EBUSY);
            }
        }
        // Text conversion.
        let text = if a.oflag & f::O_TEXTDATA != 0 {
            let (file_ccsid, open_ccsid) = if created.is_some() && a.oflag & f::O_TEXT_CREAT != 0 {
                (
                    m.ccsid,
                    ccsid_from(a.text_conversion_id, false, cx.job_ccsid)?,
                )
            } else if created.is_some() {
                (m.ccsid, m.ccsid)
            } else if conv_given {
                (
                    m.ccsid,
                    ccsid_from(a.conversion_id, codepage, cx.job_ccsid)?,
                )
            } else {
                (m.ccsid, cx.job_ccsid)
            };
            TextState::new(file_ccsid, open_ccsid).map_err(|_| Errno::ECONVERT)?
        } else {
            None
        };
        let key = r.node.key();
        let id = rest.next_open;
        rest.locks.open(
            key,
            &path_s,
            OpenRef {
                id,
                owner: cx.owner,
                access,
                share,
            },
        )?;
        let text_mode = a.oflag & f::O_TEXTDATA != 0;
        let kind = match self.open_kind(&r, created, writes, truncate, text_mode) {
            Ok(k) => k,
            Err(e) => {
                rest.locks.close(key, id, cx.owner, false);
                return Err(e);
            }
        };
        rest.next_open += 1;
        rest.opens.insert(
            id,
            Open {
                refs: 1,
                key,
                path: path_s.clone(),
                kind,
                oflag: a.oflag,
                access,
                pos: 0,
                text,
            },
        );
        let fd = match self.alloc_fd(rest, cx, id, 0) {
            Ok(fd) => fd,
            Err(e) => {
                rest.opens.remove(&id);
                rest.locks.close(key, id, cx.owner, false);
                return Err(e);
            }
        };
        if truncate && let Node::Back { .. } = r.node {
            self.mark_changed(cx, &r.node, &m);
        }
        cx.audit_access(&m, &r.node, &path_s, op);
        Ok(fd)
    }

    /// The archive flags after a change.
    pub(crate) fn mark_changed(&self, cx: &Ctx<'_>, node: &Node, m: &Meta) {
        if !m.flags.has(Flags::PCARCHIVE) || !m.flags.has(Flags::SYSARCHIVE) {
            let mut m = m.clone();
            m.flags.set(Flags::PCARCHIVE, true);
            m.flags.set(Flags::SYSARCHIVE, true);
            if let Err(e) = self.put_meta(cx, node, &m) {
                tracing::warn!(error = %e, "IFS archive flags not updated");
            }
        }
    }

    fn open_kind(
        &self,
        r: &Res,
        created: Option<B::File>,
        write: bool,
        truncate: bool,
        text: bool,
    ) -> Result<OpenKind<B::File>> {
        if r.node.is_dir() {
            return Ok(OpenKind::Dir);
        }
        Ok(match &r.node {
            Node::Back { rel, .. } => {
                let file = match created {
                    Some(f) => f,
                    None => self.b.open(rel, write)?,
                };
                if truncate {
                    file.set_len(0)?;
                }
                OpenKind::File(file)
            }
            Node::Dev { dev, .. } => OpenKind::Dev(*dev),
            Node::Qsys(QsysNode::Mbr(lib, file, mbr), _) => {
                let shape = self.qsys.read_member(lib, file, mbr)?;
                let data = if truncate {
                    Vec::new()
                } else {
                    qsys::member_to_stream(&shape, text)
                };
                OpenKind::Member {
                    lib: lib.clone(),
                    file: file.clone(),
                    mbr: mbr.clone(),
                    shape,
                    data,
                    dirty: truncate,
                    text,
                }
            }
            Node::Qsys(QsysNode::Obj(lib, name, t), _) => {
                let data = if truncate {
                    Vec::new()
                } else {
                    self.qsys.read_stream(lib, name, t)?
                };
                OpenKind::Stream {
                    lib: lib.clone(),
                    name: name.clone(),
                    objtype: t.clone(),
                    data,
                    dirty: truncate,
                }
            }
            Node::Opt { vol, path, entry } => {
                let data = if !write && !truncate {
                    None
                } else if !self.opt.writable(vol) {
                    return Err(Errno::EROFS);
                } else if truncate {
                    Some(Vec::new())
                } else {
                    let len = usize::try_from(entry.size).map_err(|_| Errno::EFBIG)?;
                    let mut d = vec![0u8; len];
                    let mut done = 0;
                    while done < len {
                        let n = self.opt.read(vol, path, done as u64, &mut d[done..])?;
                        if n == 0 {
                            return Err(Errno::EIO);
                        }
                        done += n;
                    }
                    Some(d)
                };
                OpenKind::Opt {
                    vol: vol.clone(),
                    path: path.clone(),
                    size: entry.size,
                    data,
                    dirty: truncate,
                }
            }
            _ => return Err(Errno::ENOTSUP),
        })
    }

    /// Write an open's buffered data back and flush a partial character.
    pub(crate) fn finish(&self, mut o: Open<B::File>) -> Result<()> {
        if let Some(t) = &mut o.text {
            let tail = t.finish_write();
            if !tail.is_empty() {
                let pos = o.pos;
                Self::raw_write(&mut o, &tail, pos)?;
            }
        }
        self.flush(&mut o)
    }

    /// Write a member or stream back when it changed.
    pub(crate) fn flush(&self, o: &mut Open<B::File>) -> Result<()> {
        match &mut o.kind {
            OpenKind::Member {
                lib,
                file,
                mbr,
                shape,
                data,
                dirty,
                text,
            } if *dirty => {
                let records = qsys::stream_to_records(shape, data, *text);
                self.qsys.write_member(lib, file, mbr, records)?;
                *dirty = false;
            }
            OpenKind::Stream {
                lib,
                name,
                objtype,
                data,
                dirty,
            } if *dirty => {
                self.qsys.write_stream(lib, name, objtype, data)?;
                *dirty = false;
            }
            OpenKind::Opt {
                vol,
                path,
                data: Some(data),
                dirty,
                ..
            } if *dirty => {
                self.opt.put(vol, path, data)?;
                self.opt.sync(vol)?;
                *dirty = false;
            }
            _ => {}
        }
        Ok(())
    }

    /// `close()`. Byte-range locks the owner holds on the object go
    /// (POSIX), whichever descriptor took them.
    pub fn close(&self, cx: &Ctx<'_>, fd: i32) -> Result<()> {
        let mut st = self.lock();
        let rest = &mut st.rest;
        let os = rest.owners.get_mut(&cx.owner).ok_or(Errno::EBADF)?;
        let e = os.fds.remove(&fd).ok_or(Errno::EBADF)?;
        let o = rest.opens.get_mut(&e.open).ok_or(Errno::EBADF)?;
        let key = o.key;
        o.refs -= 1;
        let last = o.refs == 0;
        rest.locks.set(
            key,
            "",
            RangeLock::new(cx.owner, RangeKind::Unlock, 0, 0),
            false,
        )?;
        if last {
            let o = rest.opens.remove(&e.open).expect("present");
            rest.locks.close(key, e.open, cx.owner, true);
            self.finish(o)?;
        }
        Ok(())
    }

    /// `dup()`.
    pub fn dup(&self, cx: &Ctx<'_>, fd: i32) -> Result<i32> {
        self.dup_min(cx, fd, 0)
    }

    fn dup_min(&self, cx: &Ctx<'_>, fd: i32, min: i32) -> Result<i32> {
        let mut st = self.lock();
        let rest = &mut st.rest;
        let open = rest
            .owners
            .get(&cx.owner)
            .and_then(|o| o.fds.get(&fd))
            .ok_or(Errno::EBADF)?
            .open;
        let new = self.alloc_fd(rest, cx, open, min)?;
        rest.opens.get_mut(&open).expect("open").refs += 1;
        Ok(new)
    }

    /// `dup2()`: `new` names the same open; whatever it named is closed.
    pub fn dup2(&self, cx: &Ctx<'_>, fd: i32, new: i32) -> Result<i32> {
        if new < 0 || usize::try_from(new).unwrap_or(usize::MAX) >= self.cfg.max_fds {
            return Err(Errno::EBADF);
        }
        let open = {
            let st = self.lock();
            st.rest
                .owners
                .get(&cx.owner)
                .and_then(|o| o.fds.get(&fd))
                .ok_or(Errno::EBADF)?
                .open
        };
        if fd == new {
            return Ok(new);
        }
        if self
            .lock()
            .rest
            .owners
            .get(&cx.owner)
            .is_some_and(|o| o.fds.contains_key(&new))
        {
            self.close(cx, new)?;
        }
        let mut st = self.lock();
        let rest = &mut st.rest;
        let os = self.owner_state(rest, cx.owner);
        os.fds.insert(
            new,
            FdEntry {
                open,
                cloexec: false,
            },
        );
        rest.opens.get_mut(&open).ok_or(Errno::EBADF)?.refs += 1;
        Ok(new)
    }

    /// `fcntl()` for the integer commands: `F_DUPFD`, `F_GETFD`,
    /// `F_SETFD`, `F_GETFL`, `F_SETFL` (only `O_APPEND`, `O_NONBLOCK`,
    /// `O_SYNC`, `O_DSYNC` change). Locks are [`Ifs::lock_range`].
    pub fn fcntl(&self, cx: &Ctx<'_>, fd: i32, cmd: i32, arg: i32) -> Result<i32> {
        if cmd == f::F_DUPFD {
            return self.dup_min(cx, fd, arg);
        }
        let mut st = self.lock();
        let rest = &mut st.rest;
        let ent = rest
            .owners
            .get_mut(&cx.owner)
            .and_then(|o| o.fds.get_mut(&fd))
            .ok_or(Errno::EBADF)?;
        match cmd {
            f::F_GETFD => Ok(if ent.cloexec { f::FD_CLOEXEC } else { 0 }),
            f::F_SETFD => {
                ent.cloexec = arg & f::FD_CLOEXEC != 0;
                Ok(0)
            }
            f::F_GETFL => {
                let o = rest.opens.get(&ent.open).ok_or(Errno::EBADF)?;
                Ok(i32::from_ne_bytes(o.oflag.to_ne_bytes()))
            }
            f::F_SETFL => {
                let open = ent.open;
                let o = rest.opens.get_mut(&open).ok_or(Errno::EBADF)?;
                let changeable = f::O_APPEND | f::O_NONBLOCK | f::O_SYNC | f::O_DSYNC;
                let arg = u32::from_ne_bytes(arg.to_ne_bytes());
                o.oflag = (o.oflag & !changeable) | (arg & changeable);
                Ok(0)
            }
            f::F_GETOWN | f::F_SETOWN => Err(Errno::ENOTSUP),
            _ => Err(Errno::EINVAL),
        }
    }

    /// `fcntl(F_GETLK / F_SETLK / F_SETLKW)` on a byte range. `whence`
    /// and `start` are as in `struct flock`. For `F_GETLK` the answer is
    /// the blocking lock (owner, kind, absolute start, length) or `None`.
    /// `F_SETLKW` answers `EAGAIN` while it would wait (the caller
    /// retries) and `EDEADLK` when waiting would never end.
    #[allow(clippy::too_many_arguments)]
    pub fn lock_range(
        &self,
        cx: &Ctx<'_>,
        fd: i32,
        cmd: i32,
        kind: RangeKind,
        whence: i32,
        start: i64,
        len: i64,
    ) -> Result<Option<RangeLock>> {
        let mut st = self.lock();
        let rest = &mut st.rest;
        let open = rest
            .owners
            .get(&cx.owner)
            .and_then(|o| o.fds.get(&fd))
            .ok_or(Errno::EBADF)?
            .open;
        let o = rest.opens.get(&open).ok_or(Errno::EBADF)?;
        let base: i64 = match whence {
            f::SEEK_SET => 0,
            f::SEEK_CUR => i64::try_from(o.pos).map_err(|_| Errno::EOVERFLOW)?,
            f::SEEK_END => i64::try_from(Self::size_of(o)?).map_err(|_| Errno::EOVERFLOW)?,
            _ => return Err(Errno::EINVAL),
        };
        let mut abs = base.checked_add(start).ok_or(Errno::EINVAL)?;
        let mut len = len;
        if len < 0 {
            abs = abs.checked_add(len).ok_or(Errno::EINVAL)?;
            len = -len;
        }
        let abs = u64::try_from(abs).map_err(|_| Errno::EINVAL)?;
        let len = u64::try_from(len).map_err(|_| Errno::EINVAL)?;
        let want = RangeLock::new(cx.owner, kind, abs, len);
        let (key, path) = (o.key, o.path.clone());
        let access = o.access;
        match cmd {
            f::F_GETLK | f::F_GETLK64 => Ok(rest.locks.get(key, &want)),
            f::F_SETLK | f::F_SETLK64 | f::F_SETLKW | f::F_SETLKW64 => {
                match kind {
                    RangeKind::Read if !access.read => return Err(Errno::EBADF),
                    RangeKind::Write if !access.write => return Err(Errno::EBADF),
                    _ => {}
                }
                let blocking = matches!(cmd, f::F_SETLKW | f::F_SETLKW64);
                rest.locks.set(key, &path, want, blocking)?;
                Ok(None)
            }
            _ => Err(Errno::EINVAL),
        }
    }
}

impl<B: Backend> Ifs<B> {
    /// The canonical path a descriptor was opened by (it follows
    /// renames made through the IFS).
    pub fn fd_path(&self, cx: &Ctx<'_>, fd: i32) -> Result<String> {
        let st = self.lock();
        let id = st
            .rest
            .owners
            .get(&cx.owner)
            .and_then(|o| o.fds.get(&fd))
            .ok_or(Errno::EBADF)?
            .open;
        Ok(st.rest.opens.get(&id).ok_or(Errno::EBADF)?.path.clone())
    }

    /// The path a `*at` call names: `p` itself when absolute, else `p`
    /// under the directory `dirfd` is open on (`AT_FDCWD` is `None`).
    pub fn at_path(&self, cx: &Ctx<'_>, dirfd: Option<i32>, p: &str) -> Result<String> {
        match dirfd {
            _ if p.starts_with('/') => Ok(p.to_string()),
            None => Ok(p.to_string()),
            Some(fd) => {
                let dir = self.fd_path(cx, fd)?;
                let st = self.stat(cx, &dir)?;
                if st.mode & f::S_IFMT != f::S_IFDIR {
                    return Err(Errno::ENOTDIR);
                }
                // The directory's path is canonical (no links), so the
                // text join resolves as the resolver would from it.
                Ok(if dir == "/" {
                    format!("/{p}")
                } else {
                    format!("{dir}/{p}")
                })
            }
        }
    }

    /// `openat()`.
    pub fn openat(&self, cx: &Ctx<'_>, dirfd: Option<i32>, p: &str, a: OpenArgs) -> Result<i32> {
        let path = self.at_path(cx, dirfd, p)?;
        self.open(cx, &path, a)
    }

    /// `fchdir()`.
    pub fn fchdir(&self, cx: &Ctx<'_>, fd: i32) -> Result<()> {
        let p = self.fd_path(cx, fd)?;
        self.chdir(cx, &p)
    }
}
