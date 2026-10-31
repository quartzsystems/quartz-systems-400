// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The ISO 9660 reader: volume descriptors, a directory walk (path
//! tables are not trusted), Joliet names when there is a Joliet tree,
//! else Rock Ridge `NM` names, else level 2 names; multi-extent files.
//! Every length and position is checked: a damaged image is an error,
//! never a panic.

use std::collections::BTreeSet;

use crate::error::{Error, Result};
use crate::iso::boot::{self, BootEntry};
use crate::iso::date::Timestamp;
use crate::iso::name::{self, from_ucs2, strip_version};
use crate::iso::{PVD_SECTOR, SECTOR, SECTOR_U16, susp};
use crate::source::ReadAt;

/// Descriptors scanned before giving up on a terminator.
const MAX_DESCRIPTORS: u32 = 64;
/// Largest directory read.
const MAX_DIR: u32 = 16 * 1024 * 1024;
/// Deepest walk.
const MAX_DEPTH: usize = 64;
/// Most entries one walk returns.
const MAX_ENTRIES: usize = 1 << 20;

/// What the descriptors say.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VolumeInfo {
    /// Primary volume identifier.
    pub volume_id: String,
    /// Joliet volume identifier, when there is a Joliet tree.
    pub joliet_volume_id: Option<String>,
    /// System identifier.
    pub system_id: String,
    /// Volume set identifier.
    pub volume_set_id: String,
    /// Publisher.
    pub publisher: String,
    /// Data preparer.
    pub preparer: String,
    /// Application.
    pub application: String,
    /// Volume space in sectors.
    pub sectors: u32,
    /// Volumes in the set.
    pub set_size: u16,
    /// This volume's number.
    pub sequence: u16,
    /// Creation time.
    pub created: Option<Timestamp>,
    /// Names come from the Joliet tree.
    pub joliet: bool,
    /// The primary tree carries Rock Ridge.
    pub rock_ridge: bool,
    /// The El Torito default entry.
    pub boot: Option<BootEntry>,
}

/// File or directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// A directory.
    Dir,
    /// A file.
    File,
}

/// One directory entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Its name (Joliet, Rock Ridge or level 2, without `;1`).
    pub name: String,
    /// File or directory.
    pub kind: EntryKind,
    /// Bytes.
    pub size: u64,
    /// `(sector, bytes)` per extent.
    pub extents: Vec<(u32, u32)>,
    /// Recording time.
    pub recorded: Timestamp,
}

impl Entry {
    /// Whether it is a directory.
    #[must_use]
    pub fn is_dir(&self) -> bool {
        self.kind == EntryKind::Dir
    }
}

/// An opened image.
#[derive(Debug)]
pub struct IsoImage<R> {
    src: R,
    size: u64,
    info: VolumeInfo,
    root: Entry,
}

fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().expect("4"))
}

fn le16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(b[at..at + 2].try_into().expect("2"))
}

fn a_str(b: &[u8]) -> String {
    String::from_utf8_lossy(b)
        .trim_end_matches([' ', '\0'])
        .to_string()
}

fn j_str(b: &[u8]) -> String {
    from_ucs2(b).trim_end_matches([' ', '\0']).to_string()
}

struct Rec<'a> {
    id: &'a [u8],
    flags: u8,
    extent: u32,
    len: u32,
    date: Timestamp,
    su: &'a [u8],
}

fn parse_rec(r: &[u8]) -> Result<Rec<'_>> {
    if r.len() < 34 {
        return Err(Error::bad("directory record too short"));
    }
    let id_len = usize::from(r[32]);
    if 33 + id_len > r.len() || id_len == 0 {
        return Err(Error::bad("directory record identifier overruns it"));
    }
    let su_at = 33 + id_len + usize::from(id_len % 2 == 0);
    Ok(Rec {
        id: &r[33..33 + id_len],
        flags: r[25],
        extent: le32(r, 2),
        len: le32(r, 10),
        date: Timestamp::from_record(&r[18..25]),
        su: r.get(su_at..).unwrap_or(&[]),
    })
}

impl<R: ReadAt> IsoImage<R> {
    /// Open an image, preferring Joliet names.
    pub fn open(src: R) -> Result<Self> {
        Self::open_with(src, true)
    }

    /// Open an image; `joliet`: use the Joliet tree when there is one.
    pub fn open_with(src: R, joliet: bool) -> Result<Self> {
        let size = src.size()?;
        let mut info = VolumeInfo::default();
        let mut pvd_root = None;
        let mut svd_root = None;
        let mut catalog = None;
        let mut d = vec![0u8; SECTOR];
        for i in 0..MAX_DESCRIPTORS {
            let off = u64::from(PVD_SECTOR + i) * SECTOR as u64;
            if off + SECTOR as u64 > size {
                break;
            }
            src.read_exact_at(off, &mut d)?;
            if &d[1..6] != b"CD001" {
                break;
            }
            match d[0] {
                0 => catalog = catalog.or(boot::catalog_lba(&d)),
                1 if pvd_root.is_none() => {
                    if le16(&d, 128) != SECTOR_U16 {
                        return Err(Error::bad("logical block size is not 2048"));
                    }
                    info.system_id = a_str(&d[8..40]);
                    info.volume_id = a_str(&d[40..72]);
                    info.sectors = le32(&d, 80);
                    info.set_size = le16(&d, 120);
                    info.sequence = le16(&d, 124);
                    info.volume_set_id = a_str(&d[190..318]);
                    info.publisher = a_str(&d[318..446]);
                    info.preparer = a_str(&d[446..574]);
                    info.application = a_str(&d[574..702]);
                    info.created = Timestamp::from_long(&d[813..830]);
                    pvd_root = Some(d[156..190].to_vec());
                }
                2 if matches!(&d[88..91], b"%/@" | b"%/C" | b"%/E") && svd_root.is_none() => {
                    info.joliet_volume_id = Some(j_str(&d[40..72]));
                    svd_root = Some(d[156..190].to_vec());
                }
                255 => break,
                _ => {}
            }
        }
        let pvd_root = pvd_root.ok_or_else(|| Error::bad("no primary volume descriptor"))?;
        if let Some(cat) = catalog {
            let mut c = vec![0u8; SECTOR];
            let off = u64::from(cat) * SECTOR as u64;
            if off + SECTOR as u64 <= size {
                src.read_exact_at(off, &mut c)?;
                info.boot = boot::parse(&c);
            }
        }
        let use_joliet = joliet && svd_root.is_some();
        let root_rec = if use_joliet {
            svd_root.expect("checked")
        } else {
            pvd_root
        };
        let r = parse_rec(&root_rec)?;
        let root = Entry {
            name: String::new(),
            kind: EntryKind::Dir,
            size: u64::from(r.len),
            extents: vec![(r.extent, r.len)],
            recorded: r.date,
        };
        let mut img = Self {
            src,
            size,
            info,
            root,
        };
        img.info.joliet = use_joliet;
        // Rock Ridge: `SP` in the primary root's `.` record.
        if let Some((e, l)) = img.primary_root(&root_rec) {
            let bytes = img.dir_bytes(e, l)?;
            if let Ok(dot) = parse_rec(&bytes[..usize::from(bytes[0]).min(bytes.len())]) {
                img.info.rock_ridge = susp::has_sp(dot.su);
            }
        }
        Ok(img)
    }

    fn primary_root(&self, rec: &[u8]) -> Option<(u32, u32)> {
        if self.info.joliet {
            // The primary root is in the PVD; read it again.
            let mut d = vec![0u8; SECTOR];
            self.src
                .read_exact_at(u64::from(PVD_SECTOR) * SECTOR as u64, &mut d)
                .ok()?;
            let r = parse_rec(&d[156..190]).ok()?;
            return Some((r.extent, r.len));
        }
        let r = parse_rec(rec).ok()?;
        Some((r.extent, r.len))
    }

    /// The descriptors' information.
    pub fn info(&self) -> &VolumeInfo {
        &self.info
    }

    /// The source.
    pub fn source(&self) -> &R {
        &self.src
    }

    /// Give the source back.
    pub fn into_inner(self) -> R {
        self.src
    }

    /// Image bytes.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// The root directory.
    pub fn root(&self) -> &Entry {
        &self.root
    }

    fn dir_bytes(&self, extent: u32, len: u32) -> Result<Vec<u8>> {
        if len > MAX_DIR || len == 0 {
            return Err(Error::bad("directory length out of range"));
        }
        let off = u64::from(extent) * SECTOR as u64;
        if off + u64::from(len) > self.size {
            return Err(Error::bad("directory beyond the image"));
        }
        let mut b = vec![0u8; len as usize];
        self.src.read_exact_at(off, &mut b)?;
        Ok(b)
    }

    /// The entries of a directory.
    pub fn list(&self, dir: &Entry) -> Result<Vec<Entry>> {
        if !dir.is_dir() {
            return Err(Error::NotDir(dir.name.clone()));
        }
        let (extent, len) = *dir.extents.first().ok_or_else(|| Error::bad("no extent"))?;
        let b = self.dir_bytes(extent, len)?;
        let mut out: Vec<Entry> = Vec::new();
        let mut pending: Option<Entry> = None;
        let mut at = 0usize;
        while at < b.len() {
            let rl = usize::from(b[at]);
            if rl == 0 {
                at = (at / SECTOR + 1) * SECTOR;
                continue;
            }
            if at + rl > b.len() {
                return Err(Error::bad("directory record overruns the directory"));
            }
            let r = parse_rec(&b[at..at + rl])?;
            at += rl;
            if r.id == [0] || r.id == [1] {
                continue;
            }
            let name = self.name_of(&r);
            let kind = if r.flags & 2 != 0 {
                EntryKind::Dir
            } else {
                EntryKind::File
            };
            match &mut pending {
                Some(p) if p.name == name && p.kind == kind => {
                    p.extents.push((r.extent, r.len));
                    p.size += u64::from(r.len);
                }
                Some(_) => return Err(Error::bad("multi-extent file broken off")),
                None => {
                    pending = Some(Entry {
                        name,
                        kind,
                        size: u64::from(r.len),
                        extents: vec![(r.extent, r.len)],
                        recorded: r.date,
                    });
                }
            }
            if r.flags & 0x80 == 0 {
                out.push(pending.take().expect("set above"));
                if out.len() > MAX_ENTRIES {
                    return Err(Error::bad("too many entries"));
                }
            }
        }
        if pending.is_some() {
            return Err(Error::bad("multi-extent file without its last record"));
        }
        Ok(out)
    }

    fn name_of(&self, r: &Rec<'_>) -> String {
        if self.info.joliet {
            return strip_version(&from_ucs2(r.id));
        }
        if self.info.rock_ridge
            && let Some(n) = susp::name(r.su)
        {
            return n;
        }
        strip_version(&String::from_utf8_lossy(r.id))
    }

    /// The entry at `path` (`/A/B`); names match exactly first, then
    /// without regard to case.
    pub fn lookup(&self, path: &str) -> Result<Entry> {
        let mut cur = self.root.clone();
        for part in path.split('/').filter(|p| !p.is_empty()) {
            let kids = self.list(&cur)?;
            let found = kids
                .iter()
                .find(|e| e.name == part)
                .or_else(|| {
                    kids.iter()
                        .find(|e| e.name.to_uppercase() == part.to_uppercase())
                })
                .cloned()
                .ok_or_else(|| Error::NotFound(path.to_string()))?;
            cur = found;
        }
        Ok(cur)
    }

    /// The entries of the directory at `path`.
    pub fn read_dir(&self, path: &str) -> Result<Vec<Entry>> {
        let e = self.lookup(path)?;
        self.list(&e)
    }

    /// Read file bytes at `off`.
    pub fn read_at(&self, e: &Entry, off: u64, buf: &mut [u8]) -> Result<usize> {
        if e.is_dir() {
            return Err(Error::IsDir(e.name.clone()));
        }
        let mut base = 0u64;
        for &(sector, len) in &e.extents {
            let len = u64::from(len);
            if off < base + len {
                let within = off - base;
                let n = usize::try_from((len - within).min(buf.len() as u64)).unwrap_or(buf.len());
                let at = u64::from(sector) * SECTOR as u64 + within;
                if at + n as u64 > self.size {
                    return Err(Error::bad(format!("{} extends past the image", e.name)));
                }
                self.src.read_exact_at(at, &mut buf[..n])?;
                return Ok(n);
            }
            base += len;
        }
        Ok(0)
    }

    /// A whole file.
    pub fn read_file(&self, e: &Entry) -> Result<Vec<u8>> {
        let len = usize::try_from(e.size).map_err(|_| Error::Invalid("file too large".into()))?;
        for &(sector, l) in &e.extents {
            if u64::from(sector) * SECTOR as u64 + u64::from(l) > self.size {
                return Err(Error::bad(format!("{} extends past the image", e.name)));
            }
        }
        let mut v = vec![0u8; len];
        let mut done = 0;
        while done < len {
            let n = self.read_at(e, done as u64, &mut v[done..])?;
            if n == 0 {
                break;
            }
            done += n;
        }
        Ok(v)
    }

    /// Every entry under the root, depth first: `(path, entry)` with
    /// `/`-separated names. Loops and over-deep trees are errors.
    pub fn walk(&self) -> Result<Vec<(String, Entry)>> {
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        seen.insert(self.root.extents[0].0);
        self.walk_into(&self.root, "", 0, &mut seen, &mut out)?;
        Ok(out)
    }

    fn walk_into(
        &self,
        dir: &Entry,
        prefix: &str,
        depth: usize,
        seen: &mut BTreeSet<u32>,
        out: &mut Vec<(String, Entry)>,
    ) -> Result<()> {
        if depth > MAX_DEPTH {
            return Err(Error::bad("directories nested too deep"));
        }
        for e in self.list(dir)? {
            let path = if prefix.is_empty() {
                e.name.clone()
            } else {
                format!("{prefix}/{}", e.name)
            };
            if out.len() >= MAX_ENTRIES {
                return Err(Error::bad("too many entries"));
            }
            out.push((path.clone(), e.clone()));
            if e.is_dir() {
                if !seen.insert(e.extents[0].0) {
                    return Err(Error::bad("directory loop"));
                }
                self.walk_into(&e, &path, depth + 1, seen, out)?;
            }
        }
        Ok(())
    }
}

/// Whether `src` holds an ISO 9660 volume at its start.
pub fn is_iso(src: &dyn ReadAt) -> bool {
    let mut b = [0u8; 6];
    src.read_exact_at(u64::from(PVD_SECTOR) * SECTOR as u64, &mut b)
        .is_ok()
        && &b[1..6] == b"CD001"
}

/// A level 2 name's check, for callers that build names themselves.
#[must_use]
pub fn is_level2(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= name::MAX_FILE_ID
        && id.matches('.').count() <= 1
        && id.bytes().all(|b| name::is_d_char(b) || b == b'.')
}
