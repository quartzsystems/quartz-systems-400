// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the optical commands do to a volume: `INZOPT`, the `DSPOPT`
//! data, `CHKOPT` and `DUPOPT`.

use std::sync::Arc;

use super::layout::{Label, MAX_TEXT, check_volume_id};
use super::{Volume, tree_of, write_tree};
use crate::error::{Error, Result};
use crate::iso::tree::Dir;
use crate::iso::{EntryKind, Timestamp};
use crate::source::ImageStore;

/// `INZOPT` parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InzOpt {
    /// `NEWVOL`: the new volume identifier.
    pub new_volume: String,
    /// `CHECK(*YES)`: refuse a volume that holds files.
    pub check: bool,
    /// `CLEAR(*YES)`: overwrite the old image with zeros first.
    pub clear: bool,
    /// `TEXT`.
    pub text: String,
    /// Capacity in bytes; `None` keeps the old volume's.
    pub capacity: Option<u64>,
    /// Unix time to stamp.
    pub epoch: u64,
}

/// Initialize the image in `store` as an empty writable volume. A plain
/// ISO 9660 image cannot be initialized; an empty store can.
pub fn inzopt(store: Arc<dyn ImageStore>, p: &InzOpt) -> Result<Volume> {
    let volid = check_volume_id(&p.new_volume)?;
    let old_len = store.size()?;
    let mut capacity = p.capacity;
    if old_len > 0 {
        match Volume::open(store.clone(), false) {
            Ok(v) => {
                if !v.is_virtual_writable() {
                    return Err(Error::ReadOnly);
                }
                if p.check && !v.list("")?.is_empty() {
                    return Err(Error::ActiveFiles);
                }
                capacity = capacity.or(v.label().map(|l| l.capacity));
            }
            // Not a volume yet (zeros, damage): initialize over it.
            Err(Error::Malformed(_)) => {}
            Err(e) => return Err(e),
        }
    }
    let capacity = capacity.ok_or_else(|| Error::Invalid("no capacity for the volume".into()))?;
    if p.clear && old_len > 0 {
        store.replace(&mut |w| {
            let zeros = vec![0u8; 64 * 1024];
            let mut left = old_len;
            while left > 0 {
                let n = usize::try_from(left.min(zeros.len() as u64)).unwrap_or(zeros.len());
                w.write_all(&zeros[..n])?;
                left -= n as u64;
            }
            Ok(())
        })?;
    }
    let label = Label {
        volid,
        text: p.text.chars().take(MAX_TEXT).collect(),
        capacity,
        created: p.epoch,
        changed: p.epoch,
    };
    write_tree(store.as_ref(), &label, &mut Dir::default())?;
    Volume::open(store, false)
}

/// `DSPOPT DATA(*VOLATR)`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct VolumeAttrs {
    /// Volume identifier.
    pub volume_id: String,
    /// Text description (writable volumes).
    pub text: String,
    /// Media format, `*ISO9660`.
    pub media_format: &'static str,
    /// A Quartz writable volume.
    pub writable_layout: bool,
    /// Write protected.
    pub write_protected: bool,
    /// Capacity in bytes.
    pub capacity: u64,
    /// Bytes in use.
    pub used: u64,
    /// Bytes still free.
    pub free: u64,
    /// Block size.
    pub block_size: u32,
    /// When the volume was made.
    pub created: Option<Timestamp>,
    /// When it last changed.
    pub changed: Option<Timestamp>,
    /// Joliet names.
    pub joliet: bool,
    /// Rock Ridge names.
    pub rock_ridge: bool,
    /// Volume set identifier.
    pub volume_set: String,
    /// Volumes in the set.
    pub set_size: u16,
    /// This volume's number in the set.
    pub sequence: u16,
    /// Bootable (an El Torito entry).
    pub bootable: bool,
}

/// `DSPOPT DATA(*DIRATR)`: one directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirAttrs {
    /// Path, `/`-separated, `/` for the root.
    pub path: String,
    /// Files directly in it.
    pub files: u64,
    /// Directories directly in it.
    pub dirs: u64,
    /// Bytes of its files.
    pub bytes: u64,
}

/// `DSPOPT DATA(*FILATR)`: one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileAttrs {
    /// Path.
    pub path: String,
    /// Bytes.
    pub size: u64,
}

/// The volume's attributes.
pub fn volume_attrs(v: &Volume) -> Result<VolumeAttrs> {
    let info = v.image().info();
    let used = if v.is_dirty() {
        v.projected_size()?
    } else {
        v.image().size()
    };
    let capacity = v.capacity().max(used);
    let label = v.label();
    Ok(VolumeAttrs {
        volume_id: v.id(),
        text: label.map(|l| l.text.clone()).unwrap_or_default(),
        media_format: "*ISO9660",
        writable_layout: label.is_some(),
        write_protected: v.write_protected() || label.is_none(),
        capacity,
        used,
        free: capacity - used,
        block_size: 2048,
        created: label.map_or(info.created, |l| Some(Timestamp::from_unix(l.created))),
        changed: label.map(|l| Timestamp::from_unix(l.changed)),
        joliet: info.joliet,
        rock_ridge: info.rock_ridge,
        volume_set: info.volume_set_id.clone(),
        set_size: info.set_size,
        sequence: info.sequence,
        bootable: info.boot.is_some(),
    })
}

fn join(base: &str, name: &str) -> String {
    if base.is_empty() || base == "/" {
        format!("/{name}")
    } else {
        format!("{base}/{name}")
    }
}

/// Every directory from `path` down (`subdirs`) or `path` alone.
pub fn dir_attrs(v: &Volume, path: &str, subdirs: bool) -> Result<Vec<DirAttrs>> {
    let start = if path.trim_matches('/').is_empty() {
        "/".to_string()
    } else {
        format!("/{}", path.trim_matches('/'))
    };
    let mut out = Vec::new();
    let mut todo = vec![start];
    while let Some(p) = todo.pop() {
        if !v.stat(&p)?.is_dir() {
            return Err(Error::NotDir(p));
        }
        let kids = v.list(&p)?;
        let mut a = DirAttrs {
            path: p.clone(),
            files: 0,
            dirs: 0,
            bytes: 0,
        };
        let mut sub = Vec::new();
        for k in kids {
            if k.is_dir() {
                a.dirs += 1;
                sub.push(join(&p, &k.name));
            } else {
                a.files += 1;
                a.bytes += k.size;
            }
        }
        out.push(a);
        if subdirs {
            sub.reverse();
            todo.extend(sub);
        }
    }
    Ok(out)
}

/// The files directly in `path`.
pub fn file_attrs(v: &Volume, path: &str) -> Result<Vec<FileAttrs>> {
    Ok(v.list(path)?
        .into_iter()
        .filter(|e| e.kind == EntryKind::File)
        .map(|e| FileAttrs {
            path: join(path.trim_end_matches('/'), &e.name),
            size: e.size,
        })
        .collect())
}

/// What `CHKOPT` found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChkReport {
    /// Directories looked at.
    pub dirs: u64,
    /// Files read.
    pub files: u64,
    /// Bytes read.
    pub bytes: u64,
    /// Files or directories that could not be read: path and message id
    /// with the error.
    pub errors: Vec<(String, String)>,
}

/// `CHKOPT`: read every file of the volume to its end.
pub fn chkopt(v: &Volume) -> ChkReport {
    let mut r = ChkReport::default();
    let mut todo = vec!["/".to_string()];
    let mut buf = vec![0u8; 64 * 1024];
    while let Some(dir) = todo.pop() {
        r.dirs += 1;
        let kids = match v.list(&dir) {
            Ok(k) => k,
            Err(e) => {
                r.errors.push((dir, format!("{} {e}", e.msgid())));
                continue;
            }
        };
        for k in kids {
            let p = join(&dir, &k.name);
            if k.is_dir() {
                todo.push(p);
                continue;
            }
            let mut off = 0u64;
            let res = loop {
                if off >= k.size {
                    break Ok(());
                }
                match v.read(&p, off, &mut buf) {
                    Ok(0) => break Err(Error::bad("file ends early")),
                    Ok(n) => off += n as u64,
                    Err(e) => break Err(e),
                }
            };
            match res {
                Ok(()) => {
                    r.files += 1;
                    r.bytes += k.size;
                }
                Err(e) => r.errors.push((p, format!("{} {e}", e.msgid()))),
            }
        }
    }
    r
}

/// `DUPOPT` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DupOpt {
    /// `NEWVOL`: the copy's identifier (`None`: the source's).
    pub new_volume: Option<String>,
    /// Text (`None`: the source's).
    pub text: Option<String>,
    /// The copy's capacity (`None`: the target's, else the source's).
    pub capacity: Option<u64>,
    /// Unix time to stamp.
    pub epoch: u64,
}

/// `DUPOPT`: copy every file and directory of `from` into `to` as a
/// writable volume. Pending changes of `from` must be flushed first.
pub fn dupopt(from: &Volume, to: Arc<dyn ImageStore>, p: &DupOpt) -> Result<Volume> {
    if from.is_dirty() {
        return Err(Error::Invalid(
            "the source volume has changes not written".into(),
        ));
    }
    let mut tree = tree_of(from.image(), from.store())?;
    if from.label().is_some() {
        tree.entries.remove(super::layout::LABEL_FILE);
    }
    let volid = check_volume_id(&p.new_volume.clone().unwrap_or_else(|| from.id()))?;
    let target_cap = if to.size()? > 0 {
        Volume::open(to.clone(), false)
            .ok()
            .and_then(|v| v.label().map(|l| l.capacity))
    } else {
        None
    };
    let capacity = p.capacity.or(target_cap).unwrap_or_else(|| from.capacity());
    let label = Label {
        volid,
        text: p
            .text
            .clone()
            .unwrap_or_else(|| from.label().map(|l| l.text.clone()).unwrap_or_default()),
        capacity,
        created: p.epoch,
        changed: p.epoch,
    };
    // A copy onto the store being read would lose the data under it.
    if Arc::ptr_eq(&(from.store().clone() as Arc<dyn ImageStore>), &to) {
        return Err(Error::Invalid(
            "a volume cannot be copied onto itself".into(),
        ));
    }
    write_tree(to.as_ref(), &label, &mut tree)?;
    Volume::open(to, false)
}
