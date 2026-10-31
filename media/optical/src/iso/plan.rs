// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Where everything goes: identifiers per directory, path table order,
//! directory sizes and every extent. Computed from names and lengths
//! only, so a writable volume can ask for an image's size cheaply.

use std::collections::BTreeSet;

use crate::error::{Error, Result};
use crate::iso::hybrid::{HybridLayout, TAIL_SECTORS};
use crate::iso::name::{self, check_joliet};
use crate::iso::susp;
use crate::iso::tree::{Content, Dir, Node};
use crate::iso::{Options, SECTOR, SECTOR_U32};

/// One child record of a directory.
#[derive(Debug)]
pub(crate) struct Child {
    pub name: String,
    pub pid: Vec<u8>,
    pub jid: Vec<u8>,
    pub what: Kid,
}

/// What a child is.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Kid {
    Dir(usize),
    File(usize),
}

/// One directory.
#[derive(Debug)]
pub(crate) struct PDir {
    pub parent: usize,
    pub pid: Vec<u8>,
    pub jid: Vec<u8>,
    pub children: Vec<Child>,
    pub p_extent: u32,
    pub p_size: u32,
    pub j_extent: u32,
    pub j_size: u32,
    /// Number in the primary and Joliet path tables (1-based).
    pub p_num: u16,
    pub j_num: u16,
}

/// One file (links included).
#[derive(Debug)]
pub(crate) struct PFile<'a> {
    pub path: Vec<String>,
    pub len: u64,
    pub content: Option<&'a Content>,
    /// The file a link shares extents with.
    pub link_to: Option<usize>,
    pub extents: Vec<(u32, u32)>,
}

/// The whole layout.
#[derive(Debug)]
pub(crate) struct Plan<'a> {
    pub dirs: Vec<PDir>,
    pub files: Vec<PFile<'a>>,
    pub p_order: Vec<usize>,
    pub j_order: Vec<usize>,
    pub boot_record: Option<u32>,
    pub svd: Option<u32>,
    pub terminator: u32,
    pub catalog: Option<u32>,
    pub pt: [u32; 4],
    pub pt_len: [u32; 2],
    pub boot_file: Option<usize>,
    pub first_file: u32,
    pub total: u32,
    pub hybrid: Option<HybridLayout>,
}

/// Bytes of a directory record with an identifier of `id` bytes and
/// `su` bytes of system use.
pub(crate) fn record_len(id: usize, su: usize) -> usize {
    let base = 33 + id + usize::from(id.is_multiple_of(2));
    base + su + (su % 2)
}

/// Bytes after packing records that never cross a sector.
fn packed(lens: &[usize]) -> u32 {
    let mut sectors = 1u32;
    let mut at = 0usize;
    for &l in lens {
        if at + l > SECTOR {
            sectors += 1;
            at = 0;
        }
        at += l;
    }
    sectors * SECTOR_U32
}

fn sectors_of(len: u64) -> Result<u32> {
    u32::try_from(len.div_ceil(SECTOR as u64)).map_err(|_| Error::Invalid("image too large".into()))
}

/// The record system use bytes of an entry (empty without Rock Ridge).
pub(crate) fn entry_su(o: &Options, name: &str, dir: bool) -> Vec<u8> {
    if o.rock_ridge {
        susp::entry(name, dir, o.date().record_form())
    } else {
        Vec::new()
    }
}

/// System use of `.` (root: with `SP` and `ER`) and `..`.
pub(crate) fn dot_su(o: &Options, root_dot: bool) -> Vec<u8> {
    if !o.rock_ridge {
        return Vec::new();
    }
    let mut v = Vec::new();
    if root_dot {
        v.extend(susp::sp());
        v.extend(susp::er());
    }
    v.extend(susp::px(true));
    v.extend(susp::tf(o.date().record_form()));
    v
}

/// Records a file of `len` bytes needs.
pub(crate) fn pieces(len: u64, limit: u32) -> u64 {
    len.div_ceil(u64::from(limit)).max(1)
}

impl<'a> Plan<'a> {
    pub(crate) fn new(root: &'a Dir, o: &Options) -> Result<Self> {
        let mut p = Plan {
            dirs: Vec::new(),
            files: Vec::new(),
            p_order: Vec::new(),
            j_order: Vec::new(),
            boot_record: None,
            svd: None,
            terminator: 0,
            catalog: None,
            pt: [0; 4],
            pt_len: [0; 2],
            boot_file: None,
            first_file: 0,
            total: 0,
            hybrid: None,
        };
        p.dirs.push(PDir {
            parent: 0,
            pid: vec![0],
            jid: vec![0],
            children: Vec::new(),
            p_extent: 0,
            p_size: 0,
            j_extent: 0,
            j_size: 0,
            p_num: 1,
            j_num: 1,
        });
        p.walk(root, 0, &mut Vec::new(), o)?;
        p.links(root)?;
        p.orders(o);
        p.allocate(o)?;
        Ok(p)
    }

    fn walk(&mut self, d: &'a Dir, me: usize, at: &mut Vec<String>, o: &Options) -> Result<()> {
        let mut used = BTreeSet::new();
        for (real, node) in &d.entries {
            let dir = matches!(node, Node::Dir(_));
            let base = name::primary_base(real, dir);
            let mut pid = base.clone();
            let mut n = 1;
            while used.contains(&pid) {
                pid = name::primary_variant(&base, dir, n);
                n += 1;
            }
            used.insert(pid.clone());
            let mut pid = pid.into_bytes();
            let mut jid = name::ucs2(real);
            if o.joliet {
                check_joliet(real)?;
            }
            if !dir {
                pid.extend_from_slice(b";1");
                jid.extend(name::ucs2(";1"));
            }
            if o.rock_ridge && record_len(pid.len(), entry_su(o, real, dir).len()) > 255 {
                return Err(Error::BadName(real.clone()));
            }
            at.push(real.clone());
            let what = match node {
                Node::Dir(sub) => {
                    let idx = self.dirs.len();
                    self.dirs.push(PDir {
                        parent: me,
                        pid: pid.clone(),
                        jid: jid.clone(),
                        children: Vec::new(),
                        p_extent: 0,
                        p_size: 0,
                        j_extent: 0,
                        j_size: 0,
                        p_num: 0,
                        j_num: 0,
                    });
                    self.walk(sub, idx, at, o)?;
                    Kid::Dir(idx)
                }
                Node::File(c) => {
                    self.files.push(PFile {
                        path: at.clone(),
                        len: c.len(),
                        content: Some(c),
                        link_to: None,
                        extents: Vec::new(),
                    });
                    Kid::File(self.files.len() - 1)
                }
                Node::Link(_) => {
                    self.files.push(PFile {
                        path: at.clone(),
                        len: 0,
                        content: None,
                        link_to: None,
                        extents: Vec::new(),
                    });
                    Kid::File(self.files.len() - 1)
                }
            };
            at.pop();
            self.dirs[me].children.push(Child {
                name: real.clone(),
                pid,
                jid,
                what,
            });
        }
        Ok(())
    }

    fn links(&mut self, root: &Dir) -> Result<()> {
        for i in 0..self.files.len() {
            if self.files[i].content.is_some() {
                continue;
            }
            let Some(Node::Link(target)) = root.get(&self.files[i].path) else {
                continue;
            };
            let t = self
                .files
                .iter()
                .position(|f| f.path == *target && f.content.is_some())
                .ok_or_else(|| Error::NotFound(target.join("/")))?;
            self.files[i].link_to = Some(t);
            self.files[i].len = self.files[t].len;
        }
        Ok(())
    }

    fn orders(&mut self, o: &Options) {
        for joliet in [false, true] {
            if joliet && !o.joliet {
                continue;
            }
            let mut order = vec![0usize];
            let mut i = 0;
            while i < order.len() {
                let d = order[i];
                let mut kids: Vec<(&[u8], usize)> = self.dirs[d]
                    .children
                    .iter()
                    .filter_map(|c| match c.what {
                        Kid::Dir(x) => Some((if joliet { &c.jid[..] } else { &c.pid[..] }, x)),
                        Kid::File(_) => None,
                    })
                    .collect();
                kids.sort();
                order.extend(kids.into_iter().map(|k| k.1));
                i += 1;
            }
            for (n, &d) in order.iter().enumerate() {
                let num = u16::try_from(n + 1).unwrap_or(u16::MAX);
                if joliet {
                    self.dirs[d].j_num = num;
                } else {
                    self.dirs[d].p_num = num;
                }
            }
            if joliet {
                self.j_order = order;
            } else {
                self.p_order = order;
            }
        }
    }

    /// Path table bytes for one order.
    fn pt_len(&self, joliet: bool) -> usize {
        let order = if joliet { &self.j_order } else { &self.p_order };
        order
            .iter()
            .map(|&d| {
                let id = if joliet {
                    &self.dirs[d].jid
                } else {
                    &self.dirs[d].pid
                };
                8 + id.len() + id.len() % 2
            })
            .sum()
    }

    /// Record lengths of a directory in its tree.
    pub(crate) fn dir_records(&self, d: usize, joliet: bool, o: &Options) -> Vec<usize> {
        let rr = !joliet && o.rock_ridge;
        let dot = if rr { dot_su(o, d == 0).len() } else { 0 };
        let dotdot = if rr { dot_su(o, false).len() } else { 0 };
        let mut lens = vec![record_len(1, dot), record_len(1, dotdot)];
        for c in self.sorted_children(d, joliet) {
            let (id, su) = if joliet {
                (c.jid.len(), 0)
            } else {
                let dir = matches!(c.what, Kid::Dir(_));
                (
                    c.pid.len(),
                    if rr {
                        entry_su(o, &c.name, dir).len()
                    } else {
                        0
                    },
                )
            };
            let n = match c.what {
                Kid::Dir(_) => 1,
                Kid::File(f) => pieces(self.files[f].len, o.extent_limit),
            };
            for _ in 0..n {
                lens.push(record_len(id, su));
            }
        }
        lens
    }

    /// Children in record order.
    pub(crate) fn sorted_children(&self, d: usize, joliet: bool) -> Vec<&Child> {
        let mut v: Vec<&Child> = self.dirs[d].children.iter().collect();
        if joliet {
            v.sort_by(|a, b| a.jid.cmp(&b.jid));
        } else {
            v.sort_by(|a, b| a.pid.cmp(&b.pid));
        }
        v
    }

    #[allow(clippy::too_many_lines)]
    fn allocate(&mut self, o: &Options) -> Result<()> {
        if self.dirs.len() > usize::from(u16::MAX) {
            return Err(Error::Invalid("too many directories".into()));
        }
        let mut next = 17u32;
        if o.boot.is_some() {
            self.boot_record = Some(next);
            next += 1;
        }
        if o.joliet {
            self.svd = Some(next);
            next += 1;
        }
        self.terminator = next;
        next += 1;
        if o.boot.is_some() {
            self.catalog = Some(next);
            next += 1;
        }
        let lens = [self.pt_len(false), self.pt_len(true)];
        self.pt_len = [
            u32::try_from(lens[0]).unwrap_or(u32::MAX),
            u32::try_from(lens[1]).unwrap_or(u32::MAX),
        ];
        for (i, slot) in self.pt.iter_mut().enumerate() {
            let len = lens[i / 2];
            if i >= 2 && !o.joliet {
                continue;
            }
            *slot = next;
            next += sectors_of(len as u64)?;
        }
        for joliet in [false, true] {
            if joliet && !o.joliet {
                continue;
            }
            let order = if joliet {
                self.j_order.clone()
            } else {
                self.p_order.clone()
            };
            for d in order {
                let size = packed(&self.dir_records(d, joliet, o));
                let pd = &mut self.dirs[d];
                if joliet {
                    pd.j_extent = next;
                    pd.j_size = size;
                } else {
                    pd.p_extent = next;
                    pd.p_size = size;
                }
                next += size / SECTOR_U32;
            }
        }
        self.first_file = next;
        // The boot image goes last, so the hybrid's partitions do not overlap.
        let boot = match &o.boot {
            Some(path) => Some(
                self.files
                    .iter()
                    .position(|f| f.path == *path && f.content.is_some())
                    .ok_or_else(|| Error::NotFound(path.join("/")))?,
            ),
            None => None,
        };
        self.boot_file = boot;
        let limit = u64::from(o.extent_limit);
        let mut order: Vec<usize> = (0..self.files.len())
            .filter(|&i| self.files[i].content.is_some() && Some(i) != boot)
            .collect();
        order.extend(boot);
        for i in order {
            let len = self.files[i].len;
            if Some(i) == boot && len > limit {
                return Err(Error::Invalid("boot image larger than one extent".into()));
            }
            if len == 0 {
                self.files[i].extents = vec![(0, 0)];
                continue;
            }
            let mut left = len;
            let mut ext = Vec::new();
            while left > 0 {
                let piece = left.min(limit);
                ext.push((next, u32::try_from(piece).expect("under the limit")));
                next = next
                    .checked_add(sectors_of(piece)?)
                    .ok_or_else(|| Error::Invalid("image too large".into()))?;
                left -= piece;
            }
            self.files[i].extents = ext;
        }
        for i in 0..self.files.len() {
            if let Some(t) = self.files[i].link_to {
                let e = self.files[t].extents.clone();
                self.files[i].extents = e;
            }
        }
        if o.hybrid {
            let b =
                boot.ok_or_else(|| Error::Invalid("a hybrid image needs a boot image".into()))?;
            next += TAIL_SECTORS;
            let f = &self.files[b];
            self.hybrid = Some(HybridLayout::new(
                u64::from(next) * SECTOR as u64,
                f.extents[0].0,
                f.len,
            )?);
        }
        self.total = next;
        Ok(())
    }
}
