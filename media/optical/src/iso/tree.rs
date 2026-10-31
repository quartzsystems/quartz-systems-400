// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The file tree an image is written from: directories keyed by real
//! name, files holding bytes or ranges of another source, and links
//! that share another file's extents.

use std::collections::BTreeMap;
use std::io;
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::iso::name::check_name;
use crate::source::ReadAt;

/// A file's bytes.
#[derive(Clone)]
pub enum Content {
    /// In memory.
    Bytes(Vec<u8>),
    /// Byte ranges `(offset, len)` of a source, in order.
    Ranges {
        /// The source.
        src: Arc<dyn ReadAt>,
        /// The ranges.
        ranges: Vec<(u64, u64)>,
    },
}

impl std::fmt::Debug for Content {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bytes(b) => write!(f, "Bytes({})", b.len()),
            Self::Ranges { ranges, .. } => write!(f, "Ranges({ranges:?})"),
        }
    }
}

impl Content {
    /// Length in bytes.
    #[must_use]
    pub fn len(&self) -> u64 {
        match self {
            Self::Bytes(b) => b.len() as u64,
            Self::Ranges { ranges, .. } => ranges.iter().map(|r| r.1).sum(),
        }
    }

    /// Whether it is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Read at `off` within the content.
    pub fn read_at(&self, off: u64, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Bytes(b) => b.as_slice().read_at(off, buf),
            Self::Ranges { src, ranges } => {
                let mut base = 0u64;
                for &(start, len) in ranges {
                    if off < base + len {
                        let within = off - base;
                        let n = usize::try_from((len - within).min(buf.len() as u64))
                            .unwrap_or(buf.len());
                        return src.read_at(start + within, &mut buf[..n]);
                    }
                    base += len;
                }
                Ok(0)
            }
        }
    }

    /// The whole content in memory.
    pub fn to_vec(&self) -> io::Result<Vec<u8>> {
        match self {
            Self::Bytes(b) => Ok(b.clone()),
            Self::Ranges { .. } => {
                let len = usize::try_from(self.len())
                    .map_err(|_| io::Error::other("file too large for memory"))?;
                let mut v = vec![0u8; len];
                let mut done = 0;
                while done < len {
                    let n = self.read_at(done as u64, &mut v[done..])?;
                    if n == 0 {
                        return Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "source shorter than its ranges",
                        ));
                    }
                    done += n;
                }
                Ok(v)
            }
        }
    }
}

/// One entry.
#[derive(Debug, Clone)]
pub enum Node {
    /// A directory.
    Dir(Dir),
    /// A file.
    File(Content),
    /// Another file's extents under a second name (its path).
    Link(Vec<String>),
}

/// A directory.
#[derive(Debug, Clone, Default)]
pub struct Dir {
    /// Entries by real name.
    pub entries: BTreeMap<String, Node>,
}

/// Split a path (`/A/B`, `A/B`) into components.
pub fn split(path: &str) -> Result<Vec<String>> {
    let parts: Vec<String> = path
        .split('/')
        .filter(|p| !p.is_empty())
        .map(ToString::to_string)
        .collect();
    for p in &parts {
        check_name(p)?;
    }
    Ok(parts)
}

impl Dir {
    /// The node at `path` (the root for an empty path is not a node).
    #[must_use]
    pub fn get(&self, path: &[String]) -> Option<&Node> {
        let (last, parents) = path.split_last()?;
        let mut d = self;
        for p in parents {
            match d.entries.get(p) {
                Some(Node::Dir(sub)) => d = sub,
                _ => return None,
            }
        }
        d.entries.get(last)
    }

    /// The directory at `path`.
    #[must_use]
    pub fn dir(&self, path: &[String]) -> Option<&Dir> {
        if path.is_empty() {
            return Some(self);
        }
        match self.get(path)? {
            Node::Dir(d) => Some(d),
            _ => None,
        }
    }

    /// The directory at `path`, mutable.
    pub fn dir_mut(&mut self, path: &[String]) -> Option<&mut Dir> {
        let mut d = self;
        for p in path {
            match d.entries.get_mut(p) {
                Some(Node::Dir(sub)) => d = sub,
                _ => return None,
            }
        }
        Some(d)
    }

    /// The directory at `path`, made with its parents where missing.
    pub fn mkdir_all(&mut self, path: &[String]) -> Result<&mut Dir> {
        let mut d = self;
        for (i, p) in path.iter().enumerate() {
            let e = d
                .entries
                .entry(p.clone())
                .or_insert_with(|| Node::Dir(Dir::default()));
            match e {
                Node::Dir(sub) => d = sub,
                _ => return Err(Error::NotDir(path[..=i].join("/"))),
            }
        }
        Ok(d)
    }

    /// Put `node` at `path`, parents made as needed; an existing entry
    /// of that name is an error.
    pub fn insert(&mut self, path: &[String], node: Node) -> Result<()> {
        let Some((last, parents)) = path.split_last() else {
            return Err(Error::Invalid("the root cannot be replaced".into()));
        };
        let d = self.mkdir_all(parents)?;
        if d.entries.contains_key(last) {
            return Err(Error::Exists(path.join("/")));
        }
        d.entries.insert(last.clone(), node);
        Ok(())
    }

    /// Every file and link under this directory, depth first by name:
    /// `(path, node)`.
    #[must_use]
    pub fn files(&self) -> Vec<(Vec<String>, &Node)> {
        let mut out = Vec::new();
        self.collect(&mut Vec::new(), &mut out);
        out
    }

    fn collect<'a>(&'a self, at: &mut Vec<String>, out: &mut Vec<(Vec<String>, &'a Node)>) {
        for (name, n) in &self.entries {
            at.push(name.clone());
            match n {
                Node::Dir(d) => d.collect(at, out),
                _ => out.push((at.clone(), n)),
            }
            at.pop();
        }
    }
}
