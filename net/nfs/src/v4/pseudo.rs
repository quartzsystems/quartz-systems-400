// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The pseudo file system (RFC 7530 section 7): read-only directories
//! made of the path components of the exports a client may use, so
//! `PUTROOTFH` and `LOOKUP` reach each export by its name. An export's
//! name is its `EXNAME=` (else its path), below its `NFSROOT=` (else
//! `/`). An export marked `ROOT` is the root itself; one marked
//! `PUBLIC` holds the public file handle (else the public handle is
//! the root's). An export inside another is reached through the outer
//! one's directories, not through the pseudo file system.
//!
//! Pseudo handles are 32 bytes like the real ones: format byte 2, the
//! node's number (FNV-1a over its name, eight bytes), zeros, and the
//! same 24-bit check.

use std::net::IpAddr;
use std::sync::Arc;

use crate::server::export::{ExportTable, ExportView, within};
use crate::server::handle::LEN;

/// The format byte of a pseudo handle.
pub const FORMAT: u8 = 2;

/// The `fsid` of the pseudo file system (no export ID makes it).
pub const FSID: (u64, u64) = (1 << 32, 0);

fn fnv64(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, x| {
        (h ^ u64::from(*x)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn check(b: &[u8]) -> u32 {
    let h = b.iter().fold(0x811c_9dc5_u32, |h, x| {
        (h ^ u32::from(*x)).wrapping_mul(0x0100_0193)
    });
    h & 0x00ff_ffff
}

/// Whether names under `path` compare without case.
fn folds(path: &str) -> bool {
    let top = path.trim_start_matches('/').split('/').next().unwrap_or("");
    !top.eq_ignore_ascii_case("QOpenSys")
}

/// The number of the pseudo directory `path`.
#[must_use]
pub fn node_id(path: &str) -> u64 {
    let key = if folds(path) {
        path.to_ascii_uppercase()
    } else {
        path.to_string()
    };
    fnv64(key.as_bytes()) & 0x7fff_ffff_ffff_ffff
}

/// A pseudo handle's bytes.
#[must_use]
pub fn handle(id: u64) -> [u8; LEN] {
    let mut b = [0u8; LEN];
    b[0] = FORMAT;
    b[1..9].copy_from_slice(&id.to_be_bytes());
    let c = check(&b[..29]).to_be_bytes();
    b[29..].copy_from_slice(&c[1..]);
    b
}

/// The node of a pseudo handle, `None` when `b` is not one.
#[must_use]
pub fn parse(b: &[u8]) -> Option<u64> {
    if b.len() != LEN || b[0] != FORMAT || b[9..29].iter().any(|x| *x != 0) {
        return None;
    }
    let c = check(&b[..29]).to_be_bytes();
    if b[29..] != c[1..] {
        return None;
    }
    let mut n = [0u8; 8];
    n.copy_from_slice(&b[1..9]);
    Some(u64::from_be_bytes(n))
}

fn norm(p: &str) -> String {
    let parts: Vec<&str> = p.split('/').filter(|c| !c.is_empty()).collect();
    format!("/{}", parts.join("/"))
}

/// What a pseudo directory holds.
#[derive(Debug, Clone)]
pub enum Child {
    /// Another pseudo directory (its index).
    Node(usize),
    /// An export's root.
    Export(Arc<dyn ExportView>),
}

/// A pseudo directory.
#[derive(Debug, Clone)]
pub struct Node {
    /// Its number.
    pub id: u64,
    /// Its name from the root.
    pub path: String,
    /// Its parent's index (none for the root).
    pub parent: Option<usize>,
    /// Its entries, in name order.
    pub children: Vec<(String, Child)>,
}

/// What `PUTROOTFH` names.
#[derive(Debug, Clone)]
pub enum Root {
    /// A pseudo directory (index 0).
    Pseudo,
    /// An export marked `ROOT`.
    Export(Arc<dyn ExportView>),
}

/// The pseudo file system one client sees.
#[derive(Debug, Clone)]
pub struct Tree {
    /// The root.
    pub root: Root,
    /// The directories; 0 is `/`.
    pub nodes: Vec<Node>,
    /// The exports in it and the number of the place each is at.
    pub places: Vec<(Arc<dyn ExportView>, u64)>,
    /// The export holding the public handle.
    pub public: Option<Arc<dyn ExportView>>,
}

impl Tree {
    /// The tree of the exports `peer` may use with version 4.
    pub fn build(exports: &dyn ExportTable, peer: IpAddr) -> Self {
        let mut usable: Vec<(String, Arc<dyn ExportView>)> = exports
            .all()
            .into_iter()
            .filter(|e| e.allows(peer) && e.version_allowed(4))
            .filter_map(|e| {
                let root = norm(&e.v4_nfsroot());
                let name = norm(&e.v4_name());
                let rest = within(&root, &name)?;
                Some((norm(rest), e))
            })
            .collect();
        usable.sort_by_key(|(p, _)| p.len());
        let public = usable
            .iter()
            .find(|(_, e)| e.v4_public())
            .map(|(_, e)| Arc::clone(e));
        let mut t = Self {
            root: Root::Pseudo,
            nodes: vec![Node {
                id: node_id("/"),
                path: "/".into(),
                parent: None,
                children: Vec::new(),
            }],
            places: Vec::new(),
            public,
        };
        if let Some((_, e)) = usable.iter().find(|(_, e)| e.v4_root()) {
            t.places.push((Arc::clone(e), 0));
            t.root = Root::Export(Arc::clone(e));
            return t;
        }
        let mut placed: Vec<String> = Vec::new();
        for (path, e) in usable {
            // Inside an export already placed: reached through it.
            if placed.iter().any(|p| within(p, &path).is_some()) {
                continue;
            }
            if path == "/" {
                t.places.push((Arc::clone(&e), node_id("/")));
                t.root = Root::Export(e);
                return t;
            }
            let mut at = 0usize;
            let comps: Vec<&str> = path.trim_start_matches('/').split('/').collect();
            for (i, c) in comps.iter().enumerate() {
                let last = i + 1 == comps.len();
                let sub = format!("{}/{c}", t.nodes[at].path.trim_end_matches('/'));
                if last {
                    t.nodes[at]
                        .children
                        .push(((*c).to_string(), Child::Export(Arc::clone(&e))));
                    t.places.push((Arc::clone(&e), node_id(&sub)));
                    break;
                }
                let found = t.nodes[at].children.iter().find_map(|(n, ch)| match ch {
                    Child::Node(k) if same(n, c, &sub) => Some(*k),
                    _ => None,
                });
                at = if let Some(k) = found {
                    k
                } else {
                    let k = t.nodes.len();
                    t.nodes.push(Node {
                        id: node_id(&sub),
                        path: sub,
                        parent: Some(at),
                        children: Vec::new(),
                    });
                    t.nodes[at].children.push(((*c).to_string(), Child::Node(k)));
                    k
                };
            }
            placed.push(path);
        }
        for n in &mut t.nodes {
            n.children.sort_by(|a, b| a.0.cmp(&b.0));
        }
        t
    }

    /// The directory with number `id`.
    #[must_use]
    pub fn find(&self, id: u64) -> Option<usize> {
        self.nodes.iter().position(|n| n.id == id)
    }

    /// The entry `name` of directory `at`.
    #[must_use]
    pub fn lookup(&self, at: usize, name: &str) -> Option<&Child> {
        let n = self.nodes.get(at)?;
        let sub = format!("{}/{name}", n.path.trim_end_matches('/'));
        n.children
            .iter()
            .find(|(c, _)| same(c, name, &sub))
            .map(|(_, ch)| ch)
    }

    /// The number of the place export `id` is at, when it is in the tree.
    #[must_use]
    pub fn place_of(&self, id: u32) -> Option<u64> {
        self.places
            .iter()
            .find(|(e, _)| e.id() == id)
            .map(|(_, p)| *p)
    }

    /// The directory holding export `id`'s place.
    #[must_use]
    pub fn parent_of(&self, id: u32) -> Option<usize> {
        self.nodes.iter().position(|n| {
            n.children
                .iter()
                .any(|(_, c)| matches!(c, Child::Export(e) if e.id() == id))
        })
    }
}

fn same(a: &str, b: &str, path: &str) -> bool {
    if folds(path) {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::export::{HostPat, MemExport, MemExports};
    use ifs::ops::audit::FileId;

    #[test]
    fn handles_round_trip() {
        let id = node_id("/home");
        let h = handle(id);
        assert_eq!(parse(&h), Some(id));
        let mut bad = h;
        bad[3] ^= 1;
        assert_eq!(parse(&bad), None);
        assert_eq!(node_id("/HOME"), id);
        assert_ne!(node_id("/QOpenSys/a"), node_id("/QOpenSys/A"));
    }

    #[test]
    fn the_tree_of_two_exports_and_a_host_allowed_one() {
        let t = MemExports::new();
        t.put(MemExport::new(1, "/home/ann", FileId::default()));
        let mut b = MemExport::new(2, "/data/x", FileId::default());
        b.access = Some(vec![HostPat::Addr("10.0.0.1".parse().unwrap())]);
        t.put(b);
        t.put(MemExport::new(3, "/home/ann/deep", FileId::default()));
        let all = Tree::build(&t, "10.0.0.1".parse().unwrap());
        let home = match all.lookup(0, "HOME") {
            Some(Child::Node(k)) => *k,
            c => panic!("{c:?}"),
        };
        assert!(matches!(all.lookup(home, "ann"), Some(Child::Export(e)) if e.id() == 1));
        assert!(all.lookup(0, "data").is_some());
        assert_eq!(all.place_of(3), None);
        assert_eq!(all.parent_of(1), Some(home));
        let one = Tree::build(&t, "10.0.0.2".parse().unwrap());
        assert!(one.lookup(0, "data").is_none());
        assert!(one.lookup(0, "home").is_some());
    }
}
