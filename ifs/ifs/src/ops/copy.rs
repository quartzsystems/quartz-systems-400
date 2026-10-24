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

impl<B: Backend> Ifs<B> {
    /// `CPY OBJ(from) TOOBJ(to)`.
    pub fn copy(&self, cx: &Ctx<'_>, from: &str, to: &str, o: CopyOpts) -> Result<()> {
        let (src, sm) = self.resolve_meta(cx, from, false)?;
        match src.node.ntype() {
            NodeType::Symlink => {
                let target = self.readlink(cx, from)?;
                if o.replace && self.lstat(cx, to).is_ok() {
                    self.unlink(cx, to)?;
                }
                self.symlink(cx, &target, to)
            }
            NodeType::Dir => {
                match self.stat(cx, to) {
                    Ok(s) if s.objtype == "*DIR" || s.objtype == "*LIB" || s.objtype == "*FILE" => {
                        if !o.replace {
                            return Err(Errno::EEXIST);
                        }
                    }
                    Ok(_) => return Err(Errno::ENOTDIR),
                    Err(Errno::ENOENT) => self.mkdir(cx, to, crate::auth::data::mode_of(&sm))?,
                    Err(e) => return Err(e),
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
                Ok(())
            }
            NodeType::File | NodeType::Chr => self.copy_file(cx, from, to, &sm, o),
            _ => Err(Errno::ENOTSUP),
        }
    }

    fn copy_file(
        &self,
        cx: &Ctx<'_>,
        from: &str,
        to: &str,
        sm: &crate::meta::Meta,
        o: CopyOpts,
    ) -> Result<()> {
        let mut flags = f::O_WRONLY | f::O_CREAT | f::O_TRUNC;
        if !o.replace {
            flags |= f::O_EXCL;
        }
        let file_ccsid = o.to_ccsid.unwrap_or(sm.ccsid);
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
        let result = (|| {
            let mut conv = match o.to_ccsid {
                Some(t) if t != sm.ccsid => {
                    Some(ccsid::conv::Converter::new(sm.ccsid, t).map_err(|_| Errno::ECONVERT)?)
                }
                _ => None,
            };
            let mut buf = vec![0u8; CHUNK];
            loop {
                let n = self.read(cx, src, &mut buf)?;
                if n == 0 {
                    break;
                }
                match &mut conv {
                    Some(c) => {
                        let mut out = Vec::new();
                        c.feed(&buf[..n], &mut out);
                        self.write(cx, dst, &out)?;
                    }
                    None => {
                        self.write(cx, dst, &buf[..n])?;
                    }
                }
            }
            if let Some(c) = &mut conv {
                let mut out = Vec::new();
                c.finish(&mut out);
                self.write(cx, dst, &out)?;
            }
            Ok(())
        })();
        let c1 = self.close(cx, src);
        let c2 = self.close(cx, dst);
        result.and(c1).and(c2)?;
        let mut dm = sm.clone();
        dm.ccsid = file_ccsid;
        dm.codepage = ccsid::conv::codepage_of(file_ccsid);
        self.copy_meta(cx, &dm, to, o)
    }

    /// Give a copy the source's authorities and attributes (not its
    /// check-out or journaling); the owner is the caller unless kept.
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
        let mode = if replace {
            RenameMode::Unlink
        } else {
            RenameMode::Keep
        };
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
                self.remove_tree(cx, from)
            }
            r => r,
        }
    }
}
