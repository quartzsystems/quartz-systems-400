// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Walking a subtree (`Qp0lProcessSubtree`, `SAV`, `RCLLNK`, `WRKLNK`
//! patterns); a path from a file ID is `super::byid`.

use super::{Ctx, Ifs, Stat};
use crate::backend::Backend;
use crate::errno::Result;

/// How to walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WalkOpts {
    /// Follow symbolic links to directories (never into a loop: a
    /// directory already seen is not entered again).
    pub follow: bool,
    /// Stay in the starting file system.
    pub one_fs: bool,
}

/// What the visitor wants next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visit {
    /// Go on (into a directory too).
    Continue,
    /// Do not enter this directory.
    Skip,
    /// End the walk.
    Stop,
}

impl<B: Backend> Ifs<B> {
    /// Visit `p` and everything under it, parents before children,
    /// entries in name order. An entry that fails to stat is visited
    /// with its error.
    pub fn walk(
        &self,
        cx: &Ctx<'_>,
        p: &str,
        o: WalkOpts,
        visit: &mut dyn FnMut(&str, Result<&Stat>) -> Visit,
    ) -> Result<()> {
        self.walk_opt(cx, p, o, false, visit)
    }

    /// [`Self::walk`]; with `local`, remote mounts are passed by (a
    /// save, which must not see a request run twice).
    pub(crate) fn walk_opt(
        &self,
        cx: &Ctx<'_>,
        p: &str,
        o: WalkOpts,
        local: bool,
        visit: &mut dyn FnMut(&str, Result<&Stat>) -> Visit,
    ) -> Result<()> {
        let s = if o.follow {
            self.stat(cx, p)?
        } else {
            self.lstat(cx, p)?
        };
        let mut seen = std::collections::HashSet::new();
        self.walk_in(cx, p, &s, s.vfs, (o, local), visit, &mut seen)
            .map(|_| ())
    }

    #[allow(clippy::too_many_arguments)]
    fn walk_in(
        &self,
        cx: &Ctx<'_>,
        p: &str,
        s: &Stat,
        fs: u32,
        (o, local): (WalkOpts, bool),
        visit: &mut dyn FnMut(&str, Result<&Stat>) -> Visit,
        seen: &mut std::collections::HashSet<(u32, u64)>,
    ) -> Result<bool> {
        match visit(p, Ok(s)) {
            Visit::Stop => return Ok(false),
            Visit::Skip => return Ok(true),
            Visit::Continue => {}
        }
        let is_dir = s.mode & super::flags::S_IFMT == super::flags::S_IFDIR;
        if !is_dir || (o.one_fs && s.vfs != fs) || !seen.insert((s.vfs, s.ino)) {
            return Ok(true);
        }
        let ents = match self.read_dir(cx, p) {
            Ok(e) => e,
            Err(e) => return Ok(visit(p, Err(e)) != Visit::Stop),
        };
        for e in ents {
            let child = if p == "/" {
                format!("/{}", e.name)
            } else {
                format!("{p}/{}", e.name)
            };
            if local && self.remote_point(&child) {
                continue;
            }
            let cs = if o.follow {
                self.stat(cx, &child)
            } else {
                self.lstat(cx, &child)
            };
            let go = match cs {
                Ok(cs) => self.walk_in(cx, &child, &cs, fs, (o, local), visit, seen)?,
                Err(err) => visit(&child, Err(err)) != Visit::Stop,
            };
            if !go {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
