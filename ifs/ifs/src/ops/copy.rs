// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CPY` and `MOV`: copies go through the ordinary operations, so they
//! cross file systems (root, `/QOpenSys`, UDFS, `/QSYS.LIB` members)
//! and check authority as any caller would; a move within one file
//! system is a rename, across file systems a copy and a removal.

use super::flags as f;
use super::names::RenameMode;
use super::{Ctx, Ifs, OpenArgs};
use crate::backend::Backend;
use crate::errno::{Errno, Result};
use crate::remote::request;
use crate::resolve::{Node, NodeType};

/// How `CPY` copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CopyOpts {
    /// `SUBTREE(*ALL)`: a directory's contents too.
    pub subtree: bool,
    /// `REPLACE(*YES)`.
    pub replace: bool,
    /// `DTAFMT(*TEXT)` with `TOCCSID`: convert the data to this CCSID.
    pub to_ccsid: Option<u16>,
    /// `OWNER(*KEEP)`: keep the source's owner (needs `*ALLOBJ` or
    /// being that owner); else the caller owns the copy.
    pub keep_owner: bool,
}

const CHUNK: usize = 64 * 1024;

/// A file copy in progress, kept while its request waits.
struct FileCopy {
    src: i32,
    dst: i32,
    conv: Option<ccsid::conv::Converter>,
    /// Read (and converted) but not yet written.
    pending: Vec<u8>,
    data_done: bool,
}

impl<B: Backend> Ifs<B> {
    /// `CPY OBJ(from) TOOBJ(to)`.
    pub fn copy(&self, cx: &Ctx<'_>, from: &str, to: &str, o: CopyOpts) -> Result<()> {
        crate::lease::defer::whole(|| self.copy_in(cx, from, to, o))
    }

    fn copy_in(&self, cx: &Ctx<'_>, from: &str, to: &str, o: CopyOpts) -> Result<()> {
        // A request that waits on a remote mount runs again: what it
        // finished is not done twice.
        let done = format!("copy:{from}>{to}");
        if request::step::<bool>(&done).is_some() {
            return Ok(());
        }
        let (src, sm) = self.resolve_meta(cx, from, false)?;
        match src.node.ntype() {
            NodeType::Symlink => {
                let target = self.readlink(cx, from)?;
                let cleared = format!("copy-clear:{to}");
                if request::step::<bool>(&cleared).is_none() {
                    if o.replace && self.lstat(cx, to).is_ok() {
                        self.unlink(cx, to)?;
                    }
                    request::set_step(&cleared, true);
                }
                self.symlink(cx, &target, to)?;
            }
            NodeType::Dir => {
                let made = format!("copy-dir:{to}");
                if request::step::<bool>(&made).is_none() {
                    // A run after this request's own mkdir: make it again
                    // (it is found done), not refuse it as existing.
                    let stat = if self.remote_made(cx, to) {
                        Err(Errno::ENOENT)
                    } else {
                        self.stat(cx, to)
                    };
                    match stat {
                        Ok(s)
                            if s.objtype == "*DIR"
                                || s.objtype == "*LIB"
                                || s.objtype == "*FILE" =>
                        {
                            if !o.replace {
                                return Err(Errno::EEXIST);
                            }
                        }
                        Ok(_) => return Err(Errno::ENOTDIR),
                        Err(Errno::ENOENT) => {
                            self.mkdir(cx, to, crate::auth::data::mode_of(&sm))?;
                        }
                        Err(e) => return Err(e),
                    }
                    request::set_step(&made, true);
                }
                self.copy_meta(cx, &sm, to, o)?;
                if o.subtree {
                    for e in self.read_dir(cx, from)? {
                        self.copy(
                            cx,
                            &format!("{from}/{}", e.name),
                            &format!("{to}/{}", e.name),
                            o,
                        )?;
                    }
                }
            }
            NodeType::File | NodeType::Chr => self.copy_file(cx, from, to, &sm, o)?,
            _ => return Err(Errno::ENOTSUP),
        }
        request::set_step(&done, true);
        Ok(())
    }

    fn copy_file(
        &self,
        cx: &Ctx<'_>,
        from: &str,
        to: &str,
        sm: &crate::meta::Meta,
        o: CopyOpts,
    ) -> Result<()> {
        let key = format!("copy-file:{from}>{to}");
        let file_ccsid = o.to_ccsid.unwrap_or(sm.ccsid);
        let mut fc = if let Some(fc) = request::take_step::<FileCopy>(&key) {
            fc
        } else {
            {
                let mut flags = f::O_WRONLY | f::O_CREAT | f::O_TRUNC;
                if !o.replace {
                    flags |= f::O_EXCL;
                }
                let src = self.open(cx, from, OpenArgs::new(f::O_RDONLY))?;
                let dst = match self.open(
                    cx,
                    to,
                    OpenArgs::new(flags | f::O_CCSID)
                        .mode(crate::auth::data::mode_of(sm))
                        .conv(u32::from(file_ccsid)),
                ) {
                    Ok(d) => d,
                    Err(e) => {
                        let _ = self.close(cx, src);
                        return Err(e);
                    }
                };
                let conv = match o.to_ccsid {
                    Some(t) if t != sm.ccsid => {
                        if let Ok(c) = ccsid::conv::Converter::new(sm.ccsid, t) {
                            Some(c)
                        } else {
                            let _ = self.close(cx, src);
                            let _ = self.close(cx, dst);
                            return Err(Errno::ECONVERT);
                        }
                    }
                    _ => None,
                };
                FileCopy {
                    src,
                    dst,
                    conv,
                    pending: Vec::new(),
                    data_done: false,
                }
            }
        };
        let r = self.copy_data(cx, &mut fc);
        let r = r.and_then(|()| self.close(cx, fc.dst));
        match r {
            // Waiting on a remote mount: the descriptors and the bytes in
            // hand are kept for the next run.
            Err(Errno::EAGAIN) if crate::lease::defer::pending() => {
                request::keep_step(&key, fc);
                return Err(Errno::EAGAIN);
            }
            Err(e) => {
                let _ = self.close(cx, fc.src);
                if !fc.data_done || self.fd_path(cx, fc.dst).is_ok() {
                    let _ = self.close(cx, fc.dst);
                }
                return Err(e);
            }
            Ok(()) => {}
        }
        self.close(cx, fc.src)?;
        let mut dm = sm.clone();
        dm.ccsid = file_ccsid;
        dm.codepage = ccsid::conv::codepage_of(file_ccsid);
        self.copy_meta(cx, &dm, to, o)
    }

    /// Move the data of a file copy, from where it stands.
    fn copy_data(&self, cx: &Ctx<'_>, fc: &mut FileCopy) -> Result<()> {
        if fc.data_done {
            return Ok(());
        }
        let mut buf = vec![0u8; CHUNK];
        loop {
            while !fc.pending.is_empty() {
                let n = self.write(cx, fc.dst, &fc.pending)?;
                if n == 0 {
                    return Err(Errno::ENOSPC);
                }
                fc.pending.drain(..n);
            }
            let n = self.read(cx, fc.src, &mut buf)?;
            if n == 0 {
                if let Some(c) = &mut fc.conv {
                    c.finish(&mut fc.pending);
                    fc.conv = None;
                    continue;
                }
                fc.data_done = true;
                return Ok(());
            }
            match &mut fc.conv {
                Some(c) => c.feed(&buf[..n], &mut fc.pending),
                None => fc.pending.extend_from_slice(&buf[..n]),
            }
        }
    }

    fn copy_meta(&self, cx: &Ctx<'_>, sm: &crate::meta::Meta, to: &str, o: CopyOpts) -> Result<()> {
        let mut st = self.lock();
        let cwd = Self::cwd_of(&mut st, cx.owner);
        let (mut sys, _) = self.split(&mut st);
        let r = sys.resolve(&cwd, to, false, Some(cx))?;
        if !matches!(r.node, Node::Back { .. }) {
            return Ok(());
        }
        let (mut m, _) = sys.meta(&r.node, Some(cx))?;
        let owner = if o.keep_owner && crate::auth::owner_or_allobj(cx.principal, sm) {
            sm.owner.clone()
        } else {
            m.owner.clone()
        };
        let keep = (m.created, m.generation);
        m = sm.clone();
        m.owner = owner;
        m.set_private(&m.owner.clone(), None);
        (m.created, m.generation) = keep;
        m.checkout = None;
        m.journal = None;
        self.put_meta(cx, &r.node, &m)
    }

    /// Remove an object and, for a directory, everything in it.
    pub fn remove_tree(&self, cx: &Ctx<'_>, p: &str) -> Result<()> {
        crate::lease::defer::whole(|| self.remove_tree_in(cx, p))
    }

    fn remove_tree_in(&self, cx: &Ctx<'_>, p: &str) -> Result<()> {
        // A run after this request removed it: done.
        if let Some(r) = self.remote_removed(cx, p) {
            return r;
        }
        let s = self.lstat(cx, p)?;
        if s.objtype == "*DIR" {
            for e in self.read_dir(cx, p)? {
                self.remove_tree(cx, &format!("{p}/{}", e.name))?;
            }
            self.rmdir(cx, p)
        } else {
            self.unlink(cx, p)
        }
    }

    /// `MOV OBJ(from) TOOBJ(to)`: a rename in one file system, else a
    /// copy of the whole subtree and the removal of the source.
    pub fn mov(&self, cx: &Ctx<'_>, from: &str, to: &str, replace: bool) -> Result<()> {
        crate::lease::defer::whole(|| self.mov_in(cx, from, to, replace))
    }

    fn mov_in(&self, cx: &Ctx<'_>, from: &str, to: &str, replace: bool) -> Result<()> {
        let mode = if replace {
            RenameMode::Unlink
        } else {
            RenameMode::Keep
        };
        let copied = format!("mov:{from}>{to}");
        if request::step::<bool>(&copied).is_some() {
            return self.remove_tree(cx, from);
        }
        match self.rename(cx, from, to, mode) {
            Err(Errno::EXDEV | Errno::ENOTSUP) => {
                self.copy(
                    cx,
                    from,
                    to,
                    CopyOpts {
                        subtree: true,
                        replace,
                        to_ccsid: None,
                        keep_owner: true,
                    },
                )?;
                request::set_step(&copied, true);
                self.remove_tree(cx, from)
            }
            r => r,
        }
    }
}
