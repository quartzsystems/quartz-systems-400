// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! From a file ID to the object, for callers that keep file IDs across
//! calls and restarts (a file server's handles). A file ID names its
//! file system, the object's number and its generation
//! ([`crate::meta::generation`]). The path of every object resolved is
//! remembered ([`IdCache`]), so an ID seen since the service started is
//! found by one resolution; any other is searched for: a backend file
//! system by a walk of its tree (which remembers every path it passes),
//! `/QSYS.LIB` level by level (libraries, objects, members) comparing the
//! numbers its names hash to, a collision being refused. What is found
//! is checked again by resolving its path: an object gone, or one whose
//! generation differs, is `ESTALE`.
//!
//! The xfs could open an object by its kernel handle
//! (`open_by_handle_at`), but `rustix` has no wrapper for it and this
//! crate forbids `unsafe`, so the walk is the slow path after an IPL.

use std::collections::{HashMap, VecDeque};

use super::audit::FileId;
use super::{Ctx, Ifs, OpenArgs, Stat};
use crate::backend::{self, Backend, Kind};
use crate::errno::{Errno, Result};
use crate::resolve::{Node, Res};
use crate::vfs::ids;
use crate::vfs::qsys::{self, QsysNode};

/// Paths remembered at most.
pub(crate) const CACHE_CAP: usize = 65_536;

/// Remembered paths by file system and object number.
#[derive(Debug, Default)]
pub(crate) struct IdCache {
    paths: HashMap<(u32, u64), String>,
    order: VecDeque<(u32, u64)>,
}

impl IdCache {
    /// Remember `path` for (`fs`, `ino`), dropping the oldest past the cap.
    pub fn note(&mut self, fs: u32, ino: u64, path: &str) {
        let k = (fs, ino);
        match self.paths.get_mut(&k) {
            Some(p) if p == path => return,
            Some(p) => {
                path.clone_into(p);
                return;
            }
            None => {}
        }
        if self.paths.len() >= CACHE_CAP
            && let Some(old) = self.order.pop_front()
        {
            self.paths.remove(&old);
        }
        self.paths.insert(k, path.to_string());
        self.order.push_back(k);
    }

    /// The path remembered for (`fs`, `ino`).
    #[must_use]
    pub fn get(&self, fs: u32, ino: u64) -> Option<String> {
        self.paths.get(&(fs, ino)).cloned()
    }

    /// How many paths are remembered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.paths.len()
    }
}

/// Why an ID was not found.
#[derive(Clone, Copy)]
enum Miss {
    /// No mount has the file system.
    NoFs,
    /// No object has the number, or its generation differs.
    Gone,
    /// The file system cannot be searched by ID.
    Unsearchable,
    /// Two `/QSYS.LIB` names hash to the number.
    Ambiguous,
}

impl<B: Backend> Ifs<B> {
    /// Remember a resolved node's path under its file ID.
    pub(crate) fn note_id(&self, r: &Res) {
        if matches!(r.node, Node::Back { .. } | Node::Qsys(..)) {
            self.ids
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .note(r.node.fs(), r.node.ino(), &r.path());
        }
    }

    /// The paths remembered by file ID (for tests and displays).
    pub fn id_cache_len(&self) -> usize {
        self.ids
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    /// Forget every remembered path (what a restart does).
    pub fn forget_ids(&self) {
        *self
            .ids
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = IdCache::default();
    }

    /// The generation of the object at canonical `path` when it is the
    /// object `fs`/`ino`.
    fn verify(&self, path: &str, fs: u32, ino: u64) -> Option<u32> {
        let mut st = self.lock();
        let (mut sys, _) = self.split(&mut st);
        let r = sys.resolve(&[], path, false, None).ok()?;
        if r.node.fs() != fs || r.node.ino() != ino {
            return None;
        }
        sys.meta(&r.node, None).ok().map(|(m, _)| m.generation)
    }

    /// The canonical path of the object `id` names, without authority
    /// checks. `any_generation`: generation 0 in `id` matches any
    /// (`Qp0lGetPathFromFileID`); otherwise it must equal the object's
    /// (0 only an object without one).
    fn find_id(&self, id: FileId, any_generation: bool) -> std::result::Result<String, Miss> {
        let mount = self.lock().mounts.get(id.fs).cloned().ok_or(Miss::NoFs)?;
        let ok = |g: u32| g == id.generation || (any_generation && id.generation == 0);
        let at = crate::path::to_text(&mount.at);
        let cached = self
            .ids
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(id.fs, id.ino);
        for p in cached.into_iter().chain(std::iter::once(at.clone())) {
            if let Some(g) = self.verify(&p, id.fs, id.ino) {
                return if ok(g) { Ok(p) } else { Err(Miss::Gone) };
            }
        }
        let found = if mount.is_backend() {
            self.walk_for(&mount.base, &mount.at, id)
        } else if id.fs == ids::QSYS {
            self.qsys_for(id.ino)?
        } else {
            return Err(Miss::Unsearchable);
        };
        let p = found.ok_or(Miss::Gone)?;
        match self.verify(&p, id.fs, id.ino) {
            Some(g) if ok(g) => Ok(p),
            _ => Err(Miss::Gone),
        }
    }

    /// Walk a backend file system for the object `id.ino`, remembering
    /// every path passed; its canonical path if found where it resolves.
    fn walk_for(&self, base: &str, at: &[String], id: FileId) -> Option<String> {
        let mut stack = vec![(base.to_string(), at.to_vec())];
        while let Some((rel, comps)) = stack.pop() {
            let Ok(st) = self.b.stat(&rel) else { continue };
            let p = crate::path::to_text(&comps);
            if st.ino == id.ino {
                // Under a mount point the path reaches the mount instead.
                if self.verify(&p, id.fs, id.ino).is_some() {
                    return Some(p);
                }
            } else {
                self.ids
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .note(id.fs, st.ino, &p);
            }
            if st.kind == Kind::Dir
                && let Ok(mut ents) = self.b.readdir(&rel)
            {
                ents.sort_by(|a, b| b.name.cmp(&a.name));
                for e in ents {
                    let mut c = comps.clone();
                    c.push(e.name.clone());
                    stack.push((backend::join(&rel, &e.name), c));
                }
            }
        }
        None
    }

    /// The `/QSYS.LIB` node whose number is `ino`, by levels.
    fn qsys_for(&self, ino: u64) -> std::result::Result<Option<String>, Miss> {
        let num = |n: &QsysNode| crate::resolve::qsys_key(n).ino;
        let one = |found: Vec<QsysNode>| match found.as_slice() {
            [] => Ok(None),
            [n] => Ok(Some(qsys::path_of(n))),
            _ => Err(Miss::Ambiguous),
        };
        let libs: Vec<String> = self
            .qsys
            .libraries()
            .map_err(|_| Miss::Gone)?
            .into_iter()
            .map(|e| e.name)
            .collect();
        let found: Vec<QsysNode> = libs
            .iter()
            .map(|l| QsysNode::Lib(l.clone()))
            .filter(|n| num(n) == ino)
            .collect();
        if !found.is_empty() {
            return one(found);
        }
        let mut found = Vec::new();
        let mut files = Vec::new();
        for lib in std::iter::once("QSYS".to_string()).chain(libs) {
            let Ok(objs) = self.qsys.objects(&lib) else {
                continue;
            };
            for e in objs.into_iter().filter(|e| e.objtype != "*LIB") {
                let n = QsysNode::Obj(lib.clone(), e.name.clone(), e.objtype.clone());
                if num(&n) == ino {
                    found.push(n);
                }
                if e.objtype == "*FILE" && e.attr.starts_with("PF") {
                    files.push((lib.clone(), e.name));
                }
            }
        }
        if !found.is_empty() {
            return one(found);
        }
        for (lib, file) in files {
            let Ok(mbrs) = self.qsys.members(&lib, &file) else {
                continue;
            };
            for e in mbrs {
                let n = QsysNode::Mbr(lib.clone(), file.clone(), e.name);
                if num(&n) == ino {
                    found.push(n);
                }
            }
        }
        one(found)
    }

    /// The path and attributes of the object `id` names, as the caller
    /// sees them (the path's resolution checks its authority): what a
    /// file server needs from a handle in one call.
    ///
    /// # Errors
    ///
    /// `ESTALE` when no object has the ID (gone, another generation, or
    /// its file system no longer mounted); `EBADFID` when two
    /// `/QSYS.LIB` names share its number; `ENOTSUP` for a file system
    /// that cannot be searched by ID (`/dev`, `/QOPT`); the path's
    /// resolution's (`EACCES` ...).
    pub fn stat_by_id(&self, cx: &Ctx<'_>, id: FileId) -> Result<(String, Stat)> {
        let p = self.find_id(id, false).map_err(stale)?;
        let s = self.lstat(cx, &p)?;
        if s.ino != id.ino || s.ino_gen != id.generation {
            return Err(Errno::ESTALE);
        }
        Ok((p, s))
    }

    /// Open the object `id` names as `open()` opens its path: the
    /// descriptor and the path.
    ///
    /// # Errors
    ///
    /// As [`Self::stat_by_id`] and `open()`.
    pub fn open_by_id(&self, cx: &Ctx<'_>, id: FileId, args: OpenArgs) -> Result<(i32, String)> {
        let p = self.find_id(id, false).map_err(stale)?;
        let fd = self.open(cx, &p, args)?;
        match self.fstat(cx, fd) {
            Ok(s) if s.ino == id.ino => Ok((fd, p)),
            other => {
                let _ = self.close(cx, fd);
                Err(other.err().unwrap_or(Errno::ESTALE))
            }
        }
    }

    /// `Qp0lGetPathFromFileID`: the path of the object with this file ID
    /// (generation 0 matches any), which the caller must be able to reach.
    ///
    /// # Errors
    ///
    /// `EBADFID` for a file system that is not mounted, `ENOENT` when no
    /// object has the ID, `ENOTSUP` for a view that cannot be searched.
    pub fn path_from_file_id(&self, cx: &Ctx<'_>, id: FileId) -> Result<String> {
        let p = self.find_id(id, true).map_err(|m| match m {
            Miss::NoFs | Miss::Ambiguous => Errno::EBADFID,
            Miss::Gone => Errno::ENOENT,
            Miss::Unsearchable => Errno::ENOTSUP,
        })?;
        self.lstat(cx, &p).map(|_| p)
    }
}

fn stale(m: Miss) -> Errno {
    match m {
        Miss::NoFs | Miss::Gone => Errno::ESTALE,
        Miss::Ambiguous => Errno::EBADFID,
        Miss::Unsearchable => Errno::ENOTSUP,
    }
}
