// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The ISO 9660 writer: [`IsoBuilder`] collects a tree and writes the
//! image in one pass. Level 2 primary names, a Joliet tree, Rock Ridge
//! names in the primary tree, El Torito, and optionally the hybrid
//! MBR and GPT. Deterministic: one timestamp, sorted records, derived
//! GUIDs.

use std::io::{self, Write};
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::iso::boot;
use crate::iso::hybrid::{Guids, HybridLayout};
use crate::iso::name::{self, MAX_JOLIET_VOLUME_ID};
use crate::iso::plan::{Kid, Plan, dot_su, entry_su};
use crate::iso::tree::{Content, Dir, Node, split};
use crate::iso::{Options, PVD_SECTOR, SECTOR, SECTOR_U16, SECTOR_U32};
use crate::source::ReadAt;

/// Where a file landed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileExtent {
    /// Its path, `/`-separated real names.
    pub path: String,
    /// `(sector, bytes)` per extent.
    pub extents: Vec<(u32, u32)>,
    /// Length.
    pub len: u64,
}

/// The finished layout, for checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// Image sectors.
    pub total_sectors: u32,
    /// Every file.
    pub files: Vec<FileExtent>,
    /// The boot image's extent: `(sector, bytes)`.
    pub boot: Option<(u32, u64)>,
    /// The hybrid region table.
    pub hybrid: Option<HybridLayout>,
}

/// Builds one image.
#[derive(Debug, Clone)]
pub struct IsoBuilder {
    root: Dir,
    opts: Options,
}

impl IsoBuilder {
    /// A new image. `volume_id` is recorded as given in Joliet and as
    /// d-characters (32 at most) in the primary descriptor. `epoch`:
    /// Unix time for every timestamp. Joliet and Rock Ridge are on.
    pub fn new(volume_id: &str, epoch: u64) -> Self {
        Self {
            root: Dir::default(),
            opts: Options::new(volume_id, epoch),
        }
    }

    /// From a tree and options already made.
    #[must_use]
    pub fn from_parts(root: Dir, opts: Options) -> Self {
        Self { root, opts }
    }

    /// The options.
    pub fn options_mut(&mut self) -> &mut Options {
        &mut self.opts
    }

    /// Write a Joliet tree (default on).
    pub fn joliet(&mut self, on: bool) -> &mut Self {
        self.opts.joliet = on;
        self
    }

    /// Write Rock Ridge names (default on).
    pub fn rock_ridge(&mut self, on: bool) -> &mut Self {
        self.opts.rock_ridge = on;
        self
    }

    /// Prepend the hybrid MBR and GPT (needs a boot image).
    pub fn hybrid(&mut self, on: bool) -> &mut Self {
        self.opts.hybrid = on;
        self
    }

    /// Publisher, data preparer and application identifiers.
    pub fn identifiers(&mut self, publisher: &str, preparer: &str, application: &str) -> &mut Self {
        self.opts.publisher = publisher.to_string();
        self.opts.preparer = preparer.to_string();
        self.opts.application = application.to_string();
        self
    }

    /// The volume set: its identifier, size and this volume's number.
    pub fn volume_set(&mut self, id: &str, size: u16, sequence: u16) -> &mut Self {
        self.opts.volume_set = id.to_string();
        self.opts.set_size = size.max(1);
        self.opts.sequence = sequence.max(1);
        self
    }

    /// Largest extent; bigger files are recorded as several extents.
    /// Rounded down to whole sectors; at least one sector.
    pub fn extent_limit(&mut self, bytes: u32) -> &mut Self {
        self.opts.extent_limit = (bytes / SECTOR_U32).max(1) * SECTOR_U32;
        self
    }

    /// Add a file at `path` (`A/B/NAME`, parents made as needed).
    pub fn add_file(&mut self, path: &str, data: Vec<u8>) -> Result<&mut Self> {
        self.add_content(path, Content::Bytes(data))
    }

    /// Add a file whose bytes are ranges of another source.
    pub fn add_ranges(
        &mut self,
        path: &str,
        src: Arc<dyn ReadAt>,
        ranges: Vec<(u64, u64)>,
    ) -> Result<&mut Self> {
        self.add_content(path, Content::Ranges { src, ranges })
    }

    /// Add a file.
    pub fn add_content(&mut self, path: &str, c: Content) -> Result<&mut Self> {
        let p = split(path)?;
        self.root.insert(&p, Node::File(c))?;
        Ok(self)
    }

    /// Add a directory (with its parents).
    pub fn add_dir(&mut self, path: &str) -> Result<&mut Self> {
        let p = split(path)?;
        self.root.mkdir_all(&p)?;
        Ok(self)
    }

    /// Record `path` as a second name for the file `target`: both
    /// records point at the same extents.
    pub fn add_link(&mut self, path: &str, target: &str) -> Result<&mut Self> {
        let p = split(path)?;
        let t = split(target)?;
        match self.root.get(&t) {
            Some(Node::File(_)) => {}
            _ => return Err(Error::NotFound(target.to_string())),
        }
        self.root.insert(&p, Node::Link(t))?;
        Ok(self)
    }

    /// Mark a file as the El Torito EFI boot image (a FAT volume).
    pub fn boot_image(&mut self, path: &str) -> Result<&mut Self> {
        let p = split(path)?;
        match self.root.get(&p) {
            Some(Node::File(_)) => {}
            _ => return Err(Error::NotFound(path.to_string())),
        }
        self.opts.boot = Some(p);
        Ok(self)
    }

    /// Bytes the image will take.
    pub fn image_size(&self) -> Result<u64> {
        image_size(&self.root, &self.opts)
    }

    /// Where everything goes.
    pub fn layout(&self) -> Result<Layout> {
        let plan = Plan::new(&self.root, &self.opts)?;
        Ok(Layout {
            total_sectors: plan.total,
            files: plan
                .files
                .iter()
                .map(|f| FileExtent {
                    path: f.path.join("/"),
                    extents: f.extents.clone(),
                    len: f.len,
                })
                .collect(),
            boot: plan
                .boot_file
                .map(|b| (plan.files[b].extents[0].0, plan.files[b].len)),
            hybrid: plan.hybrid.clone(),
        })
    }

    /// Serialize the image.
    pub fn build(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        self.write_to(&mut out)?;
        Ok(out)
    }

    /// Stream the image; returns its length.
    pub fn write_to(&self, out: &mut dyn Write) -> Result<u64> {
        write_image(&self.root, &self.opts, out)
    }
}

/// Bytes the image of `root` would take.
pub fn image_size(root: &Dir, o: &Options) -> Result<u64> {
    Ok(u64::from(Plan::new(root, o)?.total) * SECTOR as u64)
}

fn both_u32(dst: &mut [u8], v: u32) {
    dst[..4].copy_from_slice(&v.to_le_bytes());
    dst[4..8].copy_from_slice(&v.to_be_bytes());
}

fn both_u16(dst: &mut [u8], v: u16) {
    dst[..2].copy_from_slice(&v.to_le_bytes());
    dst[2..4].copy_from_slice(&v.to_be_bytes());
}

/// Space-padded a-character field.
fn pad_str(dst: &mut [u8], s: &str) {
    dst.fill(b' ');
    for (d, b) in dst.iter_mut().zip(s.bytes()) {
        *d = b;
    }
}

/// A directory record.
fn record(id: &[u8], extent: u32, len: u32, flags: u8, date: [u8; 7], su: &[u8]) -> Vec<u8> {
    let mut r = vec![0u8; 33];
    both_u32(&mut r[2..10], extent);
    both_u32(&mut r[10..18], len);
    r[18..25].copy_from_slice(&date);
    r[25] = flags;
    both_u16(&mut r[28..32], 1);
    r[32] = u8::try_from(id.len()).expect("short identifier");
    r.extend_from_slice(id);
    if r.len() % 2 == 1 {
        r.push(0);
    }
    r.extend_from_slice(su);
    if r.len() % 2 == 1 {
        r.push(0);
    }
    r[0] = u8::try_from(r.len()).expect("record under 256 bytes");
    r
}

fn pack(records: &[Vec<u8>], size: u32) -> Vec<u8> {
    let mut out = vec![0u8; size as usize];
    let mut sector = 0usize;
    let mut at = 0usize;
    for r in records {
        if at + r.len() > SECTOR {
            sector += 1;
            at = 0;
        }
        let off = sector * SECTOR + at;
        out[off..off + r.len()].copy_from_slice(r);
        at += r.len();
    }
    out
}

#[allow(clippy::many_single_char_names)]
fn dir_bytes(plan: &Plan<'_>, d: usize, joliet: bool, o: &Options) -> Vec<u8> {
    let date = o.date().record_form();
    let rr = !joliet && o.rock_ridge;
    let me = &plan.dirs[d];
    let parent = &plan.dirs[me.parent];
    let (ext, size, pext, psize) = if joliet {
        (me.j_extent, me.j_size, parent.j_extent, parent.j_size)
    } else {
        (me.p_extent, me.p_size, parent.p_extent, parent.p_size)
    };
    let su = |root_dot| if rr { dot_su(o, root_dot) } else { Vec::new() };
    let mut recs = vec![
        record(&[0], ext, size, 2, date, &su(d == 0)),
        record(&[1], pext, psize, 2, date, &su(false)),
    ];
    for c in plan.sorted_children(d, joliet) {
        let id = if joliet { &c.jid } else { &c.pid };
        match c.what {
            Kid::Dir(x) => {
                let sd = &plan.dirs[x];
                let (e, s) = if joliet {
                    (sd.j_extent, sd.j_size)
                } else {
                    (sd.p_extent, sd.p_size)
                };
                let su = if rr {
                    entry_su(o, &c.name, true)
                } else {
                    Vec::new()
                };
                recs.push(record(id, e, s, 2, date, &su));
            }
            Kid::File(f) => {
                let su = if rr {
                    entry_su(o, &c.name, false)
                } else {
                    Vec::new()
                };
                let ext = &plan.files[f].extents;
                for (i, &(e, l)) in ext.iter().enumerate() {
                    let flags = if i + 1 < ext.len() { 0x80 } else { 0 };
                    recs.push(record(id, e, l, flags, date, &su));
                }
            }
        }
    }
    pack(&recs, size)
}

fn path_table(plan: &Plan<'_>, joliet: bool, big: bool) -> Vec<u8> {
    let order = if joliet { &plan.j_order } else { &plan.p_order };
    let mut v = Vec::new();
    for &d in order {
        let pd = &plan.dirs[d];
        let parent = &plan.dirs[pd.parent];
        let (id, ext, pnum) = if joliet {
            (&pd.jid, pd.j_extent, parent.j_num)
        } else {
            (&pd.pid, pd.p_extent, parent.p_num)
        };
        v.push(u8::try_from(id.len()).expect("short"));
        v.push(0);
        if big {
            v.extend_from_slice(&ext.to_be_bytes());
            v.extend_from_slice(&pnum.to_be_bytes());
        } else {
            v.extend_from_slice(&ext.to_le_bytes());
            v.extend_from_slice(&pnum.to_le_bytes());
        }
        v.extend_from_slice(id);
        if id.len() % 2 == 1 {
            v.push(0);
        }
    }
    v
}

#[allow(clippy::many_single_char_names)]
fn descriptor(plan: &Plan<'_>, o: &Options, joliet: bool) -> Vec<u8> {
    let mut d = vec![0u8; SECTOR];
    d[0] = if joliet { 2 } else { 1 };
    d[1..6].copy_from_slice(b"CD001");
    d[6] = 1;
    let text = |dst: &mut [u8], s: &str| {
        if joliet {
            name::ucs2_field(dst, s);
        } else {
            pad_str(dst, s);
        }
    };
    text(&mut d[8..40], &o.system_id);
    if joliet {
        let id: String = o.volume_id.chars().take(MAX_JOLIET_VOLUME_ID).collect();
        name::ucs2_field(&mut d[40..72], &id);
        // UCS-2 level 3.
        d[88..91].copy_from_slice(b"%/E");
    } else {
        pad_str(&mut d[40..72], &name::primary_volume_id(&o.volume_id));
    }
    both_u32(&mut d[80..88], plan.total);
    both_u16(&mut d[120..124], o.set_size);
    both_u16(&mut d[124..128], o.sequence);
    both_u16(&mut d[128..132], SECTOR_U16);
    let (len, l, m, root) = if joliet {
        (plan.pt_len[1], plan.pt[2], plan.pt[3], &plan.dirs[0])
    } else {
        (plan.pt_len[0], plan.pt[0], plan.pt[1], &plan.dirs[0])
    };
    both_u32(&mut d[132..140], len);
    d[140..144].copy_from_slice(&l.to_le_bytes());
    d[148..152].copy_from_slice(&m.to_be_bytes());
    let (e, s) = if joliet {
        (root.j_extent, root.j_size)
    } else {
        (root.p_extent, root.p_size)
    };
    let root_rec = record(&[0], e, s, 2, o.date().record_form(), &[]);
    d[156..156 + root_rec.len()].copy_from_slice(&root_rec);
    let set = if o.volume_set.is_empty() {
        o.volume_id.clone()
    } else {
        o.volume_set.clone()
    };
    text(&mut d[190..318], &set);
    text(&mut d[318..446], &o.publisher);
    text(&mut d[446..574], &o.preparer);
    text(&mut d[574..702], &o.application);
    for off in [702, 739, 776] {
        text(&mut d[off..off + 37], "");
    }
    let date = o.date().long_form();
    for off in [813, 830, 864] {
        d[off..off + 17].copy_from_slice(&date);
    }
    // No expiration date.
    d[847..863].fill(b'0');
    d[881] = 1;
    d
}

fn put(out: &mut [u8], sector: u32, data: &[u8]) {
    let off = sector as usize * SECTOR;
    out[off..off + data.len()].copy_from_slice(data);
}

/// Write the image of `root` to `out`.
pub fn write_image(root: &Dir, o: &Options, out: &mut dyn Write) -> Result<u64> {
    let plan = Plan::new(root, o)?;
    let mut meta = vec![0u8; plan.first_file as usize * SECTOR];
    put(&mut meta, PVD_SECTOR, &descriptor(&plan, o, false));
    if let Some(s) = plan.svd {
        put(&mut meta, s, &descriptor(&plan, o, true));
    }
    if let (Some(br), Some(cat), Some(b)) = (plan.boot_record, plan.catalog, plan.boot_file) {
        put(&mut meta, br, &boot::record(cat));
        let f = &plan.files[b];
        put(&mut meta, cat, &boot::catalog(f.extents[0].0, f.len));
    }
    let mut term = vec![0u8; SECTOR];
    term[0] = 255;
    term[1..6].copy_from_slice(b"CD001");
    term[6] = 1;
    put(&mut meta, plan.terminator, &term);
    put(&mut meta, plan.pt[0], &path_table(&plan, false, false));
    put(&mut meta, plan.pt[1], &path_table(&plan, false, true));
    if o.joliet {
        put(&mut meta, plan.pt[2], &path_table(&plan, true, false));
        put(&mut meta, plan.pt[3], &path_table(&plan, true, true));
    }
    for &d in &plan.p_order {
        put(
            &mut meta,
            plan.dirs[d].p_extent,
            &dir_bytes(&plan, d, false, o),
        );
    }
    for &d in &plan.j_order {
        put(
            &mut meta,
            plan.dirs[d].j_extent,
            &dir_bytes(&plan, d, true, o),
        );
    }
    let guids = Guids::derived(&o.seed());
    if let Some(h) = &plan.hybrid {
        meta[..16 * SECTOR].copy_from_slice(&h.system_area(&guids));
    }
    out.write_all(&meta)?;
    let mut at = plan.first_file;
    // Files in extent order.
    let mut order: Vec<usize> = (0..plan.files.len())
        .filter(|&i| plan.files[i].content.is_some() && plan.files[i].len > 0)
        .collect();
    order.sort_by_key(|&i| plan.files[i].extents[0].0);
    let mut buf = vec![0u8; 64 * 1024];
    for i in order {
        let f = &plan.files[i];
        let c = f.content.expect("filtered");
        debug_assert_eq!(f.extents[0].0, at);
        let mut done = 0u64;
        while done < f.len {
            let want = usize::try_from((f.len - done).min(buf.len() as u64)).expect("64 KiB");
            let n = c.read_at(done, &mut buf[..want])?;
            if n == 0 {
                return Err(Error::Io(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!("{} is shorter than recorded", f.path.join("/")),
                )));
            }
            out.write_all(&buf[..n])?;
            done += n as u64;
        }
        let pad = (SECTOR as u64 - f.len % SECTOR as u64) % SECTOR as u64;
        out.write_all(&vec![0u8; usize::try_from(pad).expect("small")])?;
        at += u32::try_from(f.len.div_ceil(SECTOR as u64)).expect("planned");
    }
    if let Some(h) = &plan.hybrid {
        out.write_all(&h.tail(&guids))?;
        at = plan.total;
    }
    debug_assert_eq!(at, plan.total);
    Ok(u64::from(plan.total) * SECTOR as u64)
}
