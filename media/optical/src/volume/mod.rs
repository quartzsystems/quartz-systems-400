// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An optical volume as the system uses it: any ISO 9660 image, read
//! only, or a writable virtual volume (the Quartz layout of
//! [`layout`]). A writable volume keeps a working tree: unchanged files
//! stay ranges of the image, changed ones are held in memory, and
//! [`Volume::flush`] rewrites the image deterministically. Names match
//! without regard to case, as `/QOPT` does.

pub mod layout;
pub mod ops;

use std::sync::Arc;

use crate::error::{Error, Result};
use crate::iso::tree::{Content, Dir, Node, split};
use crate::iso::write::{image_size, write_image};
use crate::iso::{EntryKind, IsoImage, Options};
use crate::source::{ImageStore, ReadAt};
use layout::{APPLICATION, LABEL_FILE, Label, MAX_NAME, MAX_TEXT, check_volume_id};

/// One entry of a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolEntry {
    /// Its name.
    pub name: String,
    /// File or directory.
    pub kind: EntryKind,
    /// Bytes (files).
    pub size: u64,
}

impl VolEntry {
    /// Whether it is a directory.
    #[must_use]
    pub fn is_dir(&self) -> bool {
        self.kind == EntryKind::Dir
    }
}

type Image = IsoImage<Arc<dyn ImageStore>>;

/// A loaded volume.
pub struct Volume {
    store: Arc<dyn ImageStore>,
    image: Image,
    label: Option<Label>,
    protect: bool,
    tree: Option<Dir>,
    dirty: bool,
}

impl std::fmt::Debug for Volume {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Volume")
            .field("id", &self.id())
            .field("writable", &self.is_writable())
            .field("dirty", &self.dirty)
            .finish_non_exhaustive()
    }
}

fn fold(s: &str) -> String {
    s.to_uppercase()
}

/// Content of a file in an image: its extents as ranges.
fn ranges(e: &crate::iso::Entry) -> Vec<(u64, u64)> {
    e.extents
        .iter()
        .filter(|x| x.1 > 0)
        .map(|&(s, l)| (u64::from(s) * crate::iso::SECTOR as u64, u64::from(l)))
        .collect()
}

/// The tree of an image, files as ranges of `src`.
pub(crate) fn tree_of(img: &Image, src: &Arc<dyn ImageStore>) -> Result<Dir> {
    let mut root = Dir::default();
    let rd: Arc<dyn ReadAt> = src.clone();
    for (path, e) in img.walk()? {
        let p = split(&path)?;
        if e.is_dir() {
            root.mkdir_all(&p)?;
        } else {
            let c = Content::Ranges {
                src: rd.clone(),
                ranges: ranges(&e),
            };
            root.mkdir_all(&p[..p.len() - 1])?;
            root.dir_mut(&p[..p.len() - 1])
                .expect("made above")
                .entries
                .insert(p[p.len() - 1].clone(), Node::File(c));
        }
    }
    Ok(root)
}

/// The options a writable volume is written with.
pub(crate) fn volume_options(label: &Label) -> Options {
    let mut o = Options::new(&label.volid, label.changed);
    o.application = APPLICATION.to_string();
    o.volume_set.clone_from(&label.volid);
    o
}

/// Write the volume of `root` (the label put in its root) into `store`.
pub(crate) fn write_tree(store: &dyn ImageStore, label: &Label, root: &mut Dir) -> Result<()> {
    root.entries.insert(
        LABEL_FILE.to_string(),
        Node::File(Content::Bytes(label.to_text().into_bytes())),
    );
    let o = volume_options(label);
    let need = image_size(root, &o)?;
    if label.capacity > 0 && need > label.capacity {
        return Err(Error::VolumeFull {
            need,
            capacity: label.capacity,
        });
    }
    let mut err = None;
    store.replace(&mut |w| {
        write_image(root, &o, w).map(|_| ()).map_err(|e| {
            let io = std::io::Error::other(e.to_string());
            err = Some(e);
            io
        })
    })?;
    match err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

impl Volume {
    /// Open the image in `store`. A volume with the Quartz label is
    /// writable unless `write_protect`; any other ISO 9660 image is read
    /// only.
    pub fn open(store: Arc<dyn ImageStore>, write_protect: bool) -> Result<Self> {
        let image = IsoImage::open(store.clone())?;
        let label = match image.lookup(LABEL_FILE) {
            Ok(e) if !e.is_dir() && e.size < 64 * 1024 => {
                let bytes = image.read_file(&e)?;
                Label::parse(&String::from_utf8_lossy(&bytes)).ok()
            }
            _ => None,
        };
        Ok(Self {
            store,
            image,
            label,
            protect: write_protect,
            tree: None,
            dirty: false,
        })
    }

    /// Initialize `store` as a new empty writable volume and open it.
    pub fn create(
        store: Arc<dyn ImageStore>,
        volid: &str,
        text: &str,
        capacity: u64,
        epoch: u64,
    ) -> Result<Self> {
        let label = Label {
            volid: check_volume_id(volid)?,
            text: text.chars().take(MAX_TEXT).collect(),
            capacity,
            created: epoch,
            changed: epoch,
        };
        write_tree(store.as_ref(), &label, &mut Dir::default())?;
        Self::open(store, false)
    }

    /// The volume identifier: the label's, else the descriptor's.
    #[must_use]
    pub fn id(&self) -> String {
        match &self.label {
            Some(l) => l.volid.clone(),
            None => self.image.info().volume_id.clone(),
        }
    }

    /// The label of a writable volume.
    #[must_use]
    pub fn label(&self) -> Option<&Label> {
        self.label.as_ref()
    }

    /// The image as last written.
    #[must_use]
    pub fn image(&self) -> &Image {
        &self.image
    }

    /// The backing store.
    #[must_use]
    pub fn store(&self) -> &Arc<dyn ImageStore> {
        &self.store
    }

    /// A Quartz writable volume (whether or not protected).
    #[must_use]
    pub fn is_virtual_writable(&self) -> bool {
        self.label.is_some()
    }

    /// Writes are allowed now.
    #[must_use]
    pub fn is_writable(&self) -> bool {
        self.label.is_some() && !self.protect
    }

    /// Whether the volume is write protected.
    #[must_use]
    pub fn write_protected(&self) -> bool {
        self.protect
    }

    /// Turn write protection on or off.
    pub fn set_write_protect(&mut self, on: bool) {
        self.protect = on;
    }

    /// Changes not yet written to the image.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    fn writable(&mut self) -> Result<&mut Dir> {
        if self.label.is_none() {
            return Err(Error::ReadOnly);
        }
        if self.protect {
            return Err(Error::WriteProtected);
        }
        if self.tree.is_none() {
            self.tree = Some(tree_of(&self.image, &self.store)?);
        }
        Ok(self.tree.as_mut().expect("loaded above"))
    }

    /// Bytes the image would take now.
    pub fn projected_size(&self) -> Result<u64> {
        let label = self.label.as_ref().ok_or(Error::ReadOnly)?;
        match &self.tree {
            Some(t) => image_size(t, &volume_options(label)),
            None => Ok(self.image.size()),
        }
    }

    /// Bytes allowed.
    #[must_use]
    pub fn capacity(&self) -> u64 {
        self.label
            .as_ref()
            .map_or_else(|| self.image.size(), |l| l.capacity)
    }

    fn check_capacity(&self) -> Result<()> {
        let cap = self.capacity();
        let need = self.projected_size()?;
        if cap > 0 && need > cap {
            return Err(Error::VolumeFull {
                need,
                capacity: cap,
            });
        }
        Ok(())
    }

    /// Map a path to the tree's real names (case-insensitive).
    fn resolve(tree: &Dir, path: &str) -> Result<Vec<String>> {
        let parts = split(path)?;
        let mut out = Vec::with_capacity(parts.len());
        let mut d = tree;
        for (i, p) in parts.iter().enumerate() {
            let key = d
                .entries
                .keys()
                .find(|k| *k == p || fold(k) == fold(p))
                .cloned()
                .ok_or_else(|| Error::NotFound(path.to_string()))?;
            if i == 0 && key == LABEL_FILE {
                return Err(Error::NotFound(path.to_string()));
            }
            match d.entries.get(&key) {
                Some(Node::Dir(sub)) => d = sub,
                Some(_) if i + 1 < parts.len() => return Err(Error::NotDir(path.to_string())),
                _ => {}
            }
            out.push(key);
        }
        Ok(out)
    }

    /// The parent's real path and the new name, checked to be free.
    fn new_name(tree: &Dir, path: &str) -> Result<(Vec<String>, String)> {
        let parts = split(path)?;
        let (name, parent) = parts
            .split_last()
            .ok_or_else(|| Error::Invalid("the root cannot be made".into()))?;
        if name.chars().count() > MAX_NAME {
            return Err(Error::BadName(name.clone()));
        }
        crate::iso::name::check_joliet(name)?;
        let parent = Self::resolve(tree, &parent.join("/"))?;
        let d = tree
            .dir(&parent)
            .ok_or_else(|| Error::NotDir(path.to_string()))?;
        if (parent.is_empty() && fold(name) == fold(LABEL_FILE))
            || d.entries.keys().any(|k| fold(k) == fold(name))
        {
            return Err(Error::Exists(path.to_string()));
        }
        Ok((parent, name.clone()))
    }

    /// The entries of a directory.
    pub fn list(&self, path: &str) -> Result<Vec<VolEntry>> {
        if let Some(t) = &self.tree {
            let p = Self::resolve(t, path)?;
            let d = t.dir(&p).ok_or_else(|| Error::NotDir(path.to_string()))?;
            let root = p.is_empty();
            return Ok(d
                .entries
                .iter()
                .filter(|(k, _)| !(root && *k == LABEL_FILE))
                .map(|(k, n)| entry_of(k, n))
                .collect());
        }
        let e = self.image.lookup(path)?;
        let root = split(path)?.is_empty();
        Ok(self
            .image
            .list(&e)?
            .into_iter()
            .filter(|e| !(root && self.label.is_some() && e.name == LABEL_FILE))
            .map(|e| VolEntry {
                name: e.name,
                kind: e.kind,
                size: e.size,
            })
            .collect())
    }

    /// One entry (the root for an empty path).
    pub fn stat(&self, path: &str) -> Result<VolEntry> {
        if let Some(t) = &self.tree {
            let p = Self::resolve(t, path)?;
            return match p.last() {
                None => Ok(VolEntry {
                    name: String::new(),
                    kind: EntryKind::Dir,
                    size: 0,
                }),
                Some(name) => Ok(entry_of(name, t.get(&p).expect("resolved"))),
            };
        }
        let parts = split(path)?;
        if self.label.is_some() && parts.len() == 1 && fold(&parts[0]) == fold(LABEL_FILE) {
            return Err(Error::NotFound(path.to_string()));
        }
        let e = self.image.lookup(path)?;
        Ok(VolEntry {
            size: if e.is_dir() { 0 } else { e.size },
            name: e.name,
            kind: e.kind,
        })
    }

    /// Read file bytes at `off`.
    pub fn read(&self, path: &str, off: u64, buf: &mut [u8]) -> Result<usize> {
        if let Some(t) = &self.tree {
            let p = Self::resolve(t, path)?;
            return match t.get(&p) {
                Some(Node::File(c)) => Ok(c.read_at(off, buf)?),
                _ => Err(Error::IsDir(path.to_string())),
            };
        }
        if self.label.is_some()
            && split(path)?.len() == 1
            && fold(path.trim_matches('/')) == fold(LABEL_FILE)
        {
            return Err(Error::NotFound(path.to_string()));
        }
        let e = self.image.lookup(path)?;
        self.image.read_at(&e, off, buf)
    }

    /// A whole file.
    pub fn read_all(&self, path: &str) -> Result<Vec<u8>> {
        let size = self.stat(path)?.size;
        let mut v =
            vec![0u8; usize::try_from(size).map_err(|_| Error::Invalid("file too large".into()))?];
        let mut done = 0;
        while done < v.len() {
            let n = self.read(path, done as u64, &mut v[done..])?;
            if n == 0 {
                return Err(Error::bad(format!("{path} is shorter than recorded")));
            }
            done += n;
        }
        Ok(v)
    }

    /// Put a whole file. `replace`: an existing file is replaced, else it
    /// is an error. Parents must exist.
    pub fn put_file(&mut self, path: &str, data: Vec<u8>, replace: bool) -> Result<()> {
        let tree = self.writable()?;
        if let Ok(p) = Self::resolve(tree, path) {
            if !replace {
                return Err(Error::Exists(path.to_string()));
            }
            let (name, parent) = p.split_last().ok_or_else(|| Error::IsDir("/".into()))?;
            let d = tree.dir_mut(parent).expect("resolved");
            let slot = d.entries.get_mut(name).expect("resolved");
            if matches!(slot, Node::Dir(_)) {
                return Err(Error::IsDir(path.to_string()));
            }
            let old = std::mem::replace(slot, Node::File(Content::Bytes(data)));
            if let Err(e) = self.check_capacity() {
                let tree = self.tree.as_mut().expect("loaded");
                *tree
                    .dir_mut(parent)
                    .expect("resolved")
                    .entries
                    .get_mut(name)
                    .expect("resolved") = old;
                return Err(e);
            }
        } else {
            let (parent, name) = Self::new_name(tree, path)?;
            tree.dir_mut(&parent)
                .expect("resolved")
                .entries
                .insert(name.clone(), Node::File(Content::Bytes(data)));
            if let Err(e) = self.check_capacity() {
                let tree = self.tree.as_mut().expect("loaded");
                tree.dir_mut(&parent)
                    .expect("resolved")
                    .entries
                    .remove(&name);
                return Err(e);
            }
        }
        self.dirty = true;
        Ok(())
    }

    /// Make an empty file; an existing name is an error.
    pub fn create_file(&mut self, path: &str) -> Result<()> {
        let tree = self.writable()?;
        if Self::resolve(tree, path).is_ok() {
            return Err(Error::Exists(path.to_string()));
        }
        self.put_file(path, Vec::new(), false)
    }

    /// Make a directory; its parent must exist.
    pub fn mkdir(&mut self, path: &str) -> Result<()> {
        let tree = self.writable()?;
        let (parent, name) = Self::new_name(tree, path)?;
        tree.dir_mut(&parent)
            .expect("resolved")
            .entries
            .insert(name.clone(), Node::Dir(Dir::default()));
        if let Err(e) = self.check_capacity() {
            let tree = self.tree.as_mut().expect("loaded");
            tree.dir_mut(&parent)
                .expect("resolved")
                .entries
                .remove(&name);
            return Err(e);
        }
        self.dirty = true;
        Ok(())
    }

    /// Remove a file or an empty directory.
    pub fn remove(&mut self, path: &str) -> Result<()> {
        let tree = self.writable()?;
        let p = Self::resolve(tree, path)?;
        let (name, parent) = p
            .split_last()
            .ok_or_else(|| Error::Invalid("the root cannot be removed".into()))?;
        let d = tree.dir_mut(parent).expect("resolved");
        if let Some(Node::Dir(sub)) = d.entries.get(name)
            && !sub.entries.is_empty()
        {
            return Err(Error::NotEmpty(path.to_string()));
        }
        d.entries.remove(name);
        self.dirty = true;
        Ok(())
    }

    /// Write bytes into a file at `off`, growing it as needed.
    pub fn write(&mut self, path: &str, off: u64, data: &[u8]) -> Result<usize> {
        let end = off
            .checked_add(data.len() as u64)
            .ok_or_else(|| Error::Invalid("offset too large".into()))?;
        let len = self.stat(path)?.size;
        if end > len {
            self.truncate(path, end)?;
        }
        let tree = self.writable()?;
        let p = Self::resolve(tree, path)?;
        let buf = Self::bytes_mut(tree, &p)?;
        let o = usize::try_from(off).map_err(|_| Error::Invalid("offset too large".into()))?;
        buf[o..o + data.len()].copy_from_slice(data);
        self.dirty = true;
        Ok(data.len())
    }

    fn bytes_mut<'t>(tree: &'t mut Dir, p: &[String]) -> Result<&'t mut Vec<u8>> {
        let (name, parent) = p.split_last().ok_or_else(|| Error::IsDir("/".into()))?;
        let slot = tree
            .dir_mut(parent)
            .and_then(|d| d.entries.get_mut(name))
            .ok_or_else(|| Error::NotFound(p.join("/")))?;
        let Node::File(c) = slot else {
            return Err(Error::IsDir(p.join("/")));
        };
        if let Content::Ranges { .. } = c {
            *c = Content::Bytes(c.to_vec()?);
        }
        match c {
            Content::Bytes(b) => Ok(b),
            Content::Ranges { .. } => unreachable!("made bytes above"),
        }
    }

    /// Set a file's length.
    pub fn truncate(&mut self, path: &str, len: u64) -> Result<()> {
        let tree = self.writable()?;
        let p = Self::resolve(tree, path)?;
        let new = usize::try_from(len).map_err(|_| Error::Invalid("length too large".into()))?;
        let buf = Self::bytes_mut(tree, &p)?;
        let old = buf.len();
        buf.resize(new, 0);
        if new > old
            && let Err(e) = self.check_capacity()
        {
            let tree = self.tree.as_mut().expect("loaded");
            Self::bytes_mut(tree, &p)?.truncate(old);
            return Err(e);
        }
        self.dirty = true;
        Ok(())
    }

    /// Change the text description.
    pub fn set_text(&mut self, text: &str) -> Result<()> {
        self.writable()?;
        let l = self.label.as_mut().expect("writable has a label");
        l.text = text.chars().take(MAX_TEXT).collect();
        self.dirty = true;
        Ok(())
    }

    /// Write the changes to the image, stamped `epoch`; returns whether
    /// anything was written.
    pub fn flush(&mut self, epoch: u64) -> Result<bool> {
        if !self.dirty {
            return Ok(false);
        }
        let mut label = self.label.clone().ok_or(Error::ReadOnly)?;
        label.changed = epoch;
        let mut tree = self.tree.take().expect("dirty means loaded");
        if let Err(e) = write_tree(self.store.as_ref(), &label, &mut tree) {
            // The image is unchanged; the changes stay pending.
            self.tree = Some(tree);
            return Err(e);
        }
        let fresh = Self::open(self.store.clone(), self.protect)?;
        *self = fresh;
        Ok(true)
    }
}

fn entry_of(name: &str, n: &Node) -> VolEntry {
    match n {
        Node::Dir(_) => VolEntry {
            name: name.to_string(),
            kind: EntryKind::Dir,
            size: 0,
        },
        Node::File(c) => VolEntry {
            name: name.to_string(),
            kind: EntryKind::File,
            size: c.len(),
        },
        Node::Link(_) => VolEntry {
            name: name.to_string(),
            kind: EntryKind::File,
            size: 0,
        },
    }
}
