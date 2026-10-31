// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The image catalog commands over a [`Registry`] of catalogs:
//! `CRTIMGCLG`, `DLTIMGCLG`, `ADDIMGCLGE`, `RMVIMGCLGE`, `CHGIMGCLGE`,
//! `LODIMGCLGE`, `LODIMGCLG`, `VFYIMGCLG`, and what `/QOPT` presents.
//! Every change is written to the catalog's metadata file at once.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use optical::volume::layout::{check_volume_id, volume_id_from};
use optical::{Volume, VolumeSet};

use crate::error::{Error, Result};
use crate::fs::{CatalogFs, file_name, join, parent};
use crate::model::{Catalog, CatalogStatus, Entry, EntryStatus, ImgType, MAX_ENTRIES, META_FILE};
use crate::tape::TapeImages;

const MIB: u64 = 1024 * 1024;
/// The file a directory may hold to order `ADDVRTVOL(*DIR)`: one image
/// file name per line.
pub const VOLUME_LIST: &str = "VOLUME_LIST";

/// `IMGSIZ` in MiB: a number, or `*CD650`, `*DVD2600`, `*DVD4700`.
pub fn imgsiz(s: &str) -> Result<u64> {
    let mb = match s.to_ascii_uppercase().as_str() {
        "*CD650" => 650,
        "*DVD2600" => 2600,
        "*DVD4700" => 4700,
        n => n
            .parse::<u64>()
            .map_err(|_| Error::Invalid(format!("IMGSIZ({s}) is not a size")))?,
    };
    if !(1..=16_000_000).contains(&mb) {
        return Err(Error::Invalid(format!("IMGSIZ({s}) is out of range")));
    }
    Ok(mb)
}

/// An object name: 1 to 10 of `A-Z 0-9 $ # @ _`, not starting with a
/// digit or `_`.
pub fn check_name(name: &str) -> Result<String> {
    let up = name.trim().to_ascii_uppercase();
    let ok = !up.is_empty()
        && up.len() <= 10
        && up
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || "$#@_".contains(c))
        && up.starts_with(|c: char| c.is_ascii_uppercase() || "$#@".contains(c));
    if ok {
        Ok(up)
    } else {
        Err(Error::Invalid(format!("{name} is not a valid name")))
    }
}

/// `ADDVRTVOL` of `CRTIMGCLG`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddVrtVol {
    /// `*NONE`.
    None,
    /// `*DIR`: every image already in the directory.
    Dir,
    /// A number of new volumes of `size_mb` MiB.
    New {
        /// How many.
        count: u16,
        /// Each one's size.
        size_mb: u64,
    },
}

/// `CRTIMGCLG` parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrtImgClg {
    /// `IMGCLG`.
    pub name: String,
    /// `DIR`.
    pub dir: String,
    /// `TYPE`.
    pub kind: ImgType,
    /// `CRTDIR(*YES)`.
    pub create_dir: bool,
    /// `ADDVRTVOL`.
    pub add: AddVrtVol,
    /// `TEXT`.
    pub text: String,
    /// Unix time now.
    pub epoch: u64,
}

/// `FROMFILE` of `ADDIMGCLGE`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FromFile {
    /// An image file: a name in the catalog directory, or a path to copy
    /// into it.
    Path(String),
    /// `*NEW`: a new empty image.
    New,
}

/// `TOFILE` of `ADDIMGCLGE`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToFile {
    /// `*FROMFILE`: the source's name.
    FromFile,
    /// `*GEN`: `<catalog><index>` (`.ISO` for optical).
    Gen,
    /// A name.
    Name(String),
}

/// `ADDIMGCLGE` parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddImgClgE {
    /// `FROMFILE`.
    pub from: FromFile,
    /// `TOFILE`.
    pub to_file: ToFile,
    /// `IMGCLGIDX`: `None` is `*AVAIL`.
    pub index: Option<u16>,
    /// `IMGSIZ` in MiB, for `*NEW`.
    pub size_mb: Option<u64>,
    /// `VOLNAM` for `*NEW` (`None`: from the file name).
    pub volume: Option<String>,
    /// `WRTPTC` (`None`: `*NO` for writable images, else `*YES`).
    pub write_protect: Option<bool>,
    /// `TEXT`.
    pub text: String,
    /// Unix time now.
    pub epoch: u64,
}

impl AddImgClgE {
    /// Add an existing file, every other parameter at its default.
    #[must_use]
    pub fn file(path: &str) -> Self {
        Self {
            from: FromFile::Path(path.to_string()),
            to_file: ToFile::FromFile,
            index: None,
            size_mb: None,
            volume: None,
            write_protect: None,
            text: String::new(),
            epoch: 0,
        }
    }

    /// A new image of `size_mb` MiB named `to_file`.
    #[must_use]
    pub fn new_image(to_file: &str, size_mb: u64) -> Self {
        Self {
            from: FromFile::New,
            to_file: ToFile::Name(to_file.to_string()),
            size_mb: Some(size_mb),
            ..Self::file("")
        }
    }
}

/// `CHGIMGCLGE` parameters; `None` keeps the value.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChgImgClgE {
    /// `WRTPTC`.
    pub write_protect: Option<bool>,
    /// `TEXT`.
    pub text: Option<String>,
}

/// `OPTION` of `LODIMGCLGE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryOption {
    /// `*LOAD`.
    Load,
    /// `*UNLOAD`.
    Unload,
    /// `*MOUNT`.
    Mount,
}

/// `OPTION` of `LODIMGCLG`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadOption {
    /// `*LOAD`.
    Load,
    /// `*UNLOAD`.
    Unload,
}

/// `TYPE` of `VFYIMGCLG`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfyType {
    /// `*OTHER`: every image is there and reads.
    Other,
    /// `*PTF`: and the volumes form one complete, ordered set.
    Ptf,
    /// `*LIC`: and the first volume is installation media.
    Lic,
    /// `*UPGRADE`: as `*LIC`.
    Upgrade,
}

/// `VFYIMGCLG` parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VfyImgClg {
    /// `TYPE`.
    pub kind: VfyType,
    /// `SORT(*YES)`: put the entries in volume sequence order.
    pub sort: bool,
}

/// One problem `VFYIMGCLG` found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// The image file is gone.
    Missing(u16),
    /// The image does not read: why.
    Unreadable(u16, String),
    /// The image's volume is not the one recorded: what it is now.
    VolumeChanged(u16, String),
    /// Volumes of more than one set.
    MixedSets,
    /// The set has `need` volumes, the catalog `have`.
    Incomplete {
        /// Entries.
        have: usize,
        /// Volumes in the set.
        need: usize,
    },
    /// A sequence number is missing or repeated.
    Sequence(u16),
    /// Entries are not in sequence order (and `SORT(*NO)`).
    OutOfOrder,
    /// The first volume is not QS/400 installation media.
    NotInstallMedia,
}

/// What `VFYIMGCLG` found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VfyReport {
    /// Entries checked.
    pub checked: usize,
    /// Problems.
    pub problems: Vec<Problem>,
    /// Entries were renumbered into sequence order.
    pub sorted: bool,
}

impl VfyReport {
    /// No problems.
    #[must_use]
    pub fn ok(&self) -> bool {
        self.problems.is_empty()
    }
}

/// A `WRKIMGCLG` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClgRow {
    /// Name.
    pub name: String,
    /// `*READY` or `*NOTREADY`.
    pub status: &'static str,
    /// `*OPT` or `*TAP`.
    pub kind: &'static str,
    /// Text.
    pub text: String,
}

/// A `WRKIMGCLGE` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryRow {
    /// Index.
    pub index: u16,
    /// `*MOUNTED`, `*LOADED` or `*AVAILABLE`.
    pub status: &'static str,
    /// `*YES` or `*NO`.
    pub write_protect: &'static str,
    /// Volume.
    pub volume: String,
    /// Image file name.
    pub file: String,
    /// Text.
    pub text: String,
}

/// What `WRKIMGCLGE` shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClgDetail {
    /// Name.
    pub name: String,
    /// Status.
    pub status: &'static str,
    /// Type.
    pub kind: &'static str,
    /// Device, `*NONE` when not loaded.
    pub device: String,
    /// Directory.
    pub dir: String,
    /// Text.
    pub text: String,
    /// The entries.
    pub entries: Vec<EntryRow>,
}

/// The catalogs.
pub struct Registry {
    catalogs: BTreeMap<String, Catalog>,
    fs: Arc<dyn CatalogFs>,
    tape: Arc<dyn TapeImages>,
    presented: BTreeSet<String>,
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registry")
            .field("catalogs", &self.catalogs.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl Registry {
    /// No catalogs yet.
    pub fn new(fs: Arc<dyn CatalogFs>, tape: Arc<dyn TapeImages>) -> Self {
        Self {
            catalogs: BTreeMap::new(),
            fs,
            tape,
            presented: BTreeSet::new(),
        }
    }

    /// The catalogs whose directories are `dirs`, from their metadata
    /// files.
    pub fn load(fs: Arc<dyn CatalogFs>, tape: Arc<dyn TapeImages>, dirs: &[&str]) -> Result<Self> {
        let mut r = Self::new(fs, tape);
        for d in dirs {
            let text = r.fs.read(&join(d, META_FILE))?;
            let c = Catalog::parse(&String::from_utf8_lossy(&text))?;
            r.catalogs.insert(c.name.clone(), c);
        }
        Ok(r)
    }

    /// The file system.
    pub fn fs(&self) -> &Arc<dyn CatalogFs> {
        &self.fs
    }

    /// A catalog.
    pub fn catalog(&self, name: &str) -> Result<&Catalog> {
        let n = name.to_ascii_uppercase();
        self.catalogs.get(&n).ok_or(Error::NotFound(n))
    }

    fn catalog_mut(&mut self, name: &str) -> Result<&mut Catalog> {
        let n = name.to_ascii_uppercase();
        self.catalogs.get_mut(&n).ok_or(Error::NotFound(n))
    }

    /// The catalog names, sorted.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.catalogs.keys().cloned().collect()
    }

    fn save(&self, name: &str) -> Result<()> {
        let c = self.catalog(name)?;
        self.fs
            .write(&join(&c.dir, META_FILE), c.to_text().as_bytes())?;
        Ok(())
    }

    /// `CRTIMGCLG`.
    pub fn crtimgclg(&mut self, p: &CrtImgClg) -> Result<()> {
        let name = check_name(&p.name)?;
        if self.catalogs.contains_key(&name) {
            return Err(Error::Exists(name));
        }
        let dir = format!("/{}", p.dir.trim_matches('/'));
        if let Some(c) = self
            .catalogs
            .values()
            .find(|c| c.dir.eq_ignore_ascii_case(&dir))
        {
            return Err(Error::DirInUse {
                dir,
                catalog: c.name.clone(),
            });
        }
        if !self.fs.is_dir(&dir) {
            if !p.create_dir {
                return Err(Error::NoDir(dir));
            }
            self.fs.create_dir_all(&dir)?;
        }
        if self.fs.exists(&join(&dir, META_FILE)) {
            return Err(Error::DirInUse {
                dir,
                catalog: "(another system's)".into(),
            });
        }
        self.catalogs.insert(
            name.clone(),
            Catalog {
                name: name.clone(),
                kind: p.kind,
                dir: dir.clone(),
                text: p.text.clone(),
                status: CatalogStatus::NotReady,
                device: None,
                created: p.epoch,
                entries: Vec::new(),
            },
        );
        let added = match &p.add {
            AddVrtVol::None => Ok(()),
            AddVrtVol::Dir => self.add_dir(&name, p.epoch),
            AddVrtVol::New { count, size_mb } => (0..*count).try_for_each(|_| {
                let mut a = AddImgClgE::new_image("", *size_mb);
                a.to_file = ToFile::Gen;
                a.epoch = p.epoch;
                self.addimgclge(&name, &a).map(|_| ())
            }),
        };
        if let Err(e) = added {
            self.catalogs.remove(&name);
            let _ = self.fs.remove_file(&join(&dir, META_FILE));
            return Err(e);
        }
        self.save(&name)
    }

    /// `ADDVRTVOL(*DIR)`: the images in the directory, in `VOLUME_LIST`
    /// order when there is one, else by name.
    fn add_dir(&mut self, name: &str, epoch: u64) -> Result<()> {
        let c = self.catalog(name)?;
        let dir = c.dir.clone();
        let kind = c.kind;
        let list = join(&dir, VOLUME_LIST);
        let names: Vec<String> = if self.fs.exists(&list) {
            String::from_utf8_lossy(&self.fs.read(&list)?)
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(ToString::to_string)
                .collect()
        } else {
            self.fs
                .list(&dir)?
                .into_iter()
                .filter(|n| n != META_FILE && n != VOLUME_LIST && !self.fs.is_dir(&join(&dir, n)))
                .filter(|n| self.probe(kind, &join(&dir, n)).is_ok())
                .collect()
        };
        for n in names {
            let mut a = AddImgClgE::file(&n);
            a.epoch = epoch;
            self.addimgclge(name, &a)?;
        }
        Ok(())
    }

    /// The volume identifier and writability of an image file.
    fn probe(&self, kind: ImgType, path: &str) -> Result<(String, bool)> {
        if !self.fs.exists(path) {
            return Err(Error::NoImage(path.to_string()));
        }
        let bad = |why: String| Error::BadImage {
            path: path.to_string(),
            why,
        };
        match kind {
            ImgType::Opt => {
                let store = self.fs.image(path)?;
                let v = Volume::open(store, false).map_err(|e| bad(e.to_string()))?;
                Ok((v.id(), v.is_virtual_writable()))
            }
            ImgType::Tap => {
                let id = self.tape.volume_id(self.fs.as_ref(), path).map_err(bad)?;
                Ok((id, true))
            }
        }
    }

    /// `DLTIMGCLG`; `keep(*NO)` deletes the image files too.
    pub fn dltimgclg(&mut self, name: &str, keep: bool) -> Result<()> {
        let c = self.catalog(name)?.clone();
        if c.status == CatalogStatus::Ready {
            return Err(Error::Loaded(c.name));
        }
        if !keep {
            for e in &c.entries {
                let _ = self.fs.remove_file(&c.path_of(e));
            }
        }
        let _ = self.fs.remove_file(&join(&c.dir, META_FILE));
        if !keep {
            let _ = self.fs.remove_dir(&c.dir);
        }
        self.catalogs.remove(&c.name);
        Ok(())
    }

    /// `ADDIMGCLGE`; returns the entry's index.
    #[allow(clippy::too_many_lines)]
    pub fn addimgclge(&mut self, name: &str, p: &AddImgClgE) -> Result<u16> {
        let c = self.catalog(name)?;
        let (kind, dir, cname) = (c.kind, c.dir.clone(), c.name.clone());
        let index = match p.index {
            Some(i) if !(1..=MAX_ENTRIES).contains(&i) => {
                return Err(Error::Invalid(format!("IMGCLGIDX({i}) is out of range")));
            }
            Some(i) if c.entry(i).is_some() => return Err(Error::IndexInUse(i)),
            Some(i) => i,
            None => (1..=MAX_ENTRIES)
                .find(|i| c.entry(*i).is_none())
                .ok_or_else(|| Error::Full(cname.clone()))?,
        };
        let gen_name = || match kind {
            ImgType::Opt => format!("{cname}{index:03}.ISO"),
            ImgType::Tap => format!("{cname}{index:03}"),
        };
        let (file, volume, writable) = match &p.from {
            FromFile::New => {
                let size = p
                    .size_mb
                    .ok_or_else(|| Error::Invalid("IMGSIZ is needed for FROMFILE(*NEW)".into()))?;
                let file = match &p.to_file {
                    ToFile::Name(n) if !n.is_empty() => n.clone(),
                    ToFile::Gen | ToFile::Name(_) => gen_name(),
                    ToFile::FromFile => {
                        return Err(Error::Invalid(
                            "TOFILE(*FROMFILE) cannot go with FROMFILE(*NEW)".into(),
                        ));
                    }
                };
                let path = join(&dir, &file);
                if self.fs.exists(&path) {
                    return Err(Error::Invalid(format!("{path} already exists")));
                }
                let volume = match &p.volume {
                    Some(v) => check_volume_id(v)?,
                    None => volume_id_from(&file),
                };
                match kind {
                    ImgType::Opt => {
                        let store = self.fs.create_image(&path)?;
                        Volume::create(store, &volume, &p.text, size * MIB, p.epoch)?;
                    }
                    ImgType::Tap => {
                        self.tape
                            .create(self.fs.as_ref(), &path, &volume, size)
                            .map_err(|why| Error::BadImage {
                                path: path.clone(),
                                why,
                            })?;
                    }
                }
                (file, volume, true)
            }
            FromFile::Path(src) => {
                let in_dir = !src.contains('/') || parent(src).eq_ignore_ascii_case(&dir);
                let file = if in_dir {
                    file_name(src).to_string()
                } else {
                    match &p.to_file {
                        ToFile::FromFile => file_name(src).to_string(),
                        ToFile::Gen => gen_name(),
                        ToFile::Name(n) => n.clone(),
                    }
                };
                let path = join(&dir, &file);
                if !in_dir {
                    if !self.fs.exists(src) {
                        return Err(Error::NoImage(src.clone()));
                    }
                    if self.fs.exists(&path) {
                        return Err(Error::Invalid(format!("{path} already exists")));
                    }
                    self.probe(kind, src)?;
                    self.fs.copy(src, &path)?;
                }
                let (volume, writable) = self.probe(kind, &path).inspect_err(|_| {
                    if !in_dir {
                        let _ = self.fs.remove_file(&path);
                    }
                })?;
                (file, volume, writable)
            }
        };
        let c = self.catalog_mut(name)?;
        if c.entries
            .iter()
            .any(|e| e.volume.eq_ignore_ascii_case(&volume))
        {
            return Err(Error::DuplicateVolume(volume));
        }
        if c.entries.iter().any(|e| e.file.eq_ignore_ascii_case(&file)) {
            return Err(Error::Invalid(format!("{file} is already in the catalog")));
        }
        c.entries.push(Entry {
            index,
            file,
            volume,
            status: EntryStatus::Loaded,
            write_protect: p.write_protect.unwrap_or(!writable) || !writable,
            text: p.text.clone(),
        });
        c.entries.sort_by_key(|e| e.index);
        if c.status == CatalogStatus::Ready {
            c.settle();
        }
        self.save(name)?;
        Ok(index)
    }

    /// `RMVIMGCLGE`; `keep(*NO)` deletes the image file.
    pub fn rmvimgclge(&mut self, name: &str, index: u16, keep: bool) -> Result<()> {
        let c = self.catalog_mut(name)?;
        let at = c
            .entries
            .iter()
            .position(|e| e.index == index)
            .ok_or_else(|| Error::NoEntry {
                catalog: c.name.clone(),
                index,
            })?;
        let e = c.entries.remove(at);
        if c.status == CatalogStatus::Ready {
            c.settle();
        }
        let path = c.path_of(&e);
        if !keep {
            let _ = self.fs.remove_file(&path);
        }
        self.save(name)
    }

    fn entry_mut(c: &mut Catalog, index: u16) -> Result<&mut Entry> {
        let cname = c.name.clone();
        c.entries
            .iter_mut()
            .find(|e| e.index == index)
            .ok_or(Error::NoEntry {
                catalog: cname,
                index,
            })
    }

    /// `CHGIMGCLGE`.
    pub fn chgimgclge(&mut self, name: &str, index: u16, p: &ChgImgClgE) -> Result<()> {
        let c = self.catalog(name)?;
        let e = c.entry(index).ok_or_else(|| Error::NoEntry {
            catalog: c.name.clone(),
            index,
        })?;
        if p.write_protect == Some(false) {
            let (_, writable) = self.probe(c.kind, &c.path_of(e))?;
            if !writable {
                return Err(Error::ReadOnlyImage(index));
            }
        }
        let c = self.catalog_mut(name)?;
        let e = Self::entry_mut(c, index)?;
        if let Some(w) = p.write_protect {
            e.write_protect = w;
        }
        if let Some(t) = &p.text {
            e.text.clone_from(t);
        }
        self.save(name)
    }

    /// `LODIMGCLGE`: load, unload or mount one entry.
    pub fn lodimgclge(&mut self, name: &str, index: u16, opt: EntryOption) -> Result<()> {
        let c = self.catalog_mut(name)?;
        Self::entry_mut(c, index)?;
        match opt {
            EntryOption::Load => {
                let e = Self::entry_mut(c, index)?;
                if e.status == EntryStatus::Available {
                    e.status = EntryStatus::Loaded;
                }
            }
            EntryOption::Unload => {
                Self::entry_mut(c, index)?.status = EntryStatus::Available;
            }
            EntryOption::Mount => {
                for e in &mut c.entries {
                    if e.status == EntryStatus::Mounted {
                        e.status = EntryStatus::Loaded;
                    }
                }
                Self::entry_mut(c, index)?.status = EntryStatus::Mounted;
            }
        }
        if c.status == CatalogStatus::Ready {
            c.settle();
        }
        self.save(name)
    }

    /// `LODIMGCLG`: load the catalog on `device`, or unload it.
    pub fn lodimgclg(&mut self, name: &str, opt: LoadOption, device: &str) -> Result<()> {
        let dev = device.to_ascii_uppercase();
        let n = self.catalog(name)?.name.clone();
        match opt {
            LoadOption::Load => {
                if let Some(other) = self
                    .catalogs
                    .values()
                    .find(|c| c.name != n && c.device.as_deref() == Some(dev.as_str()))
                {
                    return Err(Error::DeviceInUse(format!(
                        "device {dev} already holds image catalog {}",
                        other.name
                    )));
                }
                let c = self.catalog_mut(&n)?;
                if let Some(d) = &c.device
                    && *d != dev
                {
                    return Err(Error::DeviceInUse(format!(
                        "image catalog {n} is loaded on device {d}"
                    )));
                }
                c.status = CatalogStatus::Ready;
                c.device = Some(dev);
                c.settle();
            }
            LoadOption::Unload => {
                let c = self.catalog_mut(&n)?;
                c.status = CatalogStatus::NotReady;
                c.device = None;
            }
        }
        self.save(&n)
    }

    /// `VFYIMGCLG`.
    pub fn vfyimgclg(&mut self, name: &str, p: &VfyImgClg) -> Result<VfyReport> {
        let c = self.catalog(name)?.clone();
        let mut r = VfyReport {
            checked: c.entries.len(),
            ..VfyReport::default()
        };
        // (index, set id, set size, sequence, install media)
        let mut sets = Vec::new();
        for e in &c.entries {
            let path = c.path_of(e);
            if !self.fs.exists(&path) {
                r.problems.push(Problem::Missing(e.index));
                continue;
            }
            match self.probe(c.kind, &path) {
                Err(err) => r
                    .problems
                    .push(Problem::Unreadable(e.index, err.to_string())),
                Ok((vol, _)) if !vol.eq_ignore_ascii_case(&e.volume) => {
                    r.problems.push(Problem::VolumeChanged(e.index, vol));
                }
                Ok(_) => {}
            }
            if c.kind == ImgType::Opt
                && let Ok(store) = self.fs.image(&path)
                && let Ok(v) = Volume::open(store, true)
            {
                let info = v.image().info();
                let install = v.image().lookup(optical::manifest::PATH).is_ok();
                sets.push((
                    e.index,
                    info.volume_set_id.clone(),
                    info.set_size,
                    info.sequence,
                    install,
                ));
            }
        }
        if p.kind != VfyType::Other && c.kind == ImgType::Opt && r.problems.is_empty() {
            Self::check_set(&mut r, &sets, *p);
            if r.problems.is_empty() && p.sort {
                let mut order: Vec<(u16, u16)> = sets.iter().map(|s| (s.3, s.0)).collect();
                order.sort_unstable();
                let in_order = order.windows(2).all(|w| w[0].1 < w[1].1);
                if !in_order
                    || order
                        .iter()
                        .enumerate()
                        .any(|(i, o)| usize::from(o.1) != i + 1)
                {
                    let cm = self.catalog_mut(name)?;
                    let mut renumbered = Vec::new();
                    for (n, (_, old)) in order.iter().enumerate() {
                        let mut e = cm.entry(*old).expect("checked").clone();
                        e.index = u16::try_from(n + 1).expect("256 at most");
                        renumbered.push(e);
                    }
                    cm.entries = renumbered;
                    r.sorted = true;
                    self.save(name)?;
                }
            }
        }
        Ok(r)
    }

    fn check_set(r: &mut VfyReport, sets: &[(u16, String, u16, u16, bool)], p: VfyImgClg) {
        let Some(first) = sets.first() else {
            r.problems.push(Problem::Incomplete { have: 0, need: 1 });
            return;
        };
        if sets.iter().any(|s| s.1 != first.1 || s.2 != first.2) {
            r.problems.push(Problem::MixedSets);
            return;
        }
        let need = usize::from(first.2.max(1));
        if sets.len() != need {
            r.problems.push(Problem::Incomplete {
                have: sets.len(),
                need,
            });
        }
        let mut seen = BTreeSet::new();
        for s in sets {
            if s.3 == 0 || usize::from(s.3) > need || !seen.insert(s.3) {
                r.problems.push(Problem::Sequence(s.0));
            }
        }
        if !p.sort && sets.windows(2).any(|w| w[0].3 > w[1].3) {
            r.problems.push(Problem::OutOfOrder);
        }
        if matches!(p.kind, VfyType::Lic | VfyType::Upgrade)
            && !sets.iter().any(|s| s.3 == 1 && s.4)
        {
            r.problems.push(Problem::NotInstallMedia);
        }
    }

    /// The `WRKIMGCLG` list.
    #[must_use]
    pub fn wrkimgclg(&self) -> Vec<ClgRow> {
        self.catalogs
            .values()
            .map(|c| ClgRow {
                name: c.name.clone(),
                status: c.status.as_str(),
                kind: c.kind.as_str(),
                text: c.text.clone(),
            })
            .collect()
    }

    /// The `WRKIMGCLGE` display of one catalog.
    pub fn wrkimgclge(&self, name: &str) -> Result<ClgDetail> {
        let c = self.catalog(name)?;
        Ok(ClgDetail {
            name: c.name.clone(),
            status: c.status.as_str(),
            kind: c.kind.as_str(),
            device: c.device.clone().unwrap_or_else(|| "*NONE".into()),
            dir: c.dir.clone(),
            text: c.text.clone(),
            entries: c
                .entries
                .iter()
                .map(|e| EntryRow {
                    index: e.index,
                    status: e.status.as_str(),
                    write_protect: if e.write_protect { "*YES" } else { "*NO" },
                    volume: e.volume.clone(),
                    file: e.file.clone(),
                    text: e.text.clone(),
                })
                .collect(),
        })
    }

    /// The optical volumes the loaded catalogs present: the mounted
    /// entry of each `*READY` `*OPT` catalog, `(volume, path, write
    /// protected)`.
    #[must_use]
    pub fn presented(&self) -> Vec<(String, String, bool)> {
        self.catalogs
            .values()
            .filter(|c| c.kind == ImgType::Opt && c.status == CatalogStatus::Ready)
            .filter_map(|c| {
                c.mounted()
                    .map(|e| (e.volume.clone(), c.path_of(e), e.write_protect))
            })
            .collect()
    }

    /// Make `set` (what `/QOPT` shows) hold exactly the volumes the
    /// catalogs present, leaving volumes others put there alone.
    pub fn sync_volumes(&mut self, set: &VolumeSet) -> Result<()> {
        let want = self.presented();
        let names: BTreeSet<String> = want.iter().map(|w| w.0.clone()).collect();
        for gone in self
            .presented
            .difference(&names)
            .cloned()
            .collect::<Vec<_>>()
        {
            set.remove(&gone)?;
            self.presented.remove(&gone);
        }
        for (vol, path, wp) in want {
            if self.presented.contains(&vol) {
                set.with(&vol, |v| {
                    v.set_write_protect(wp);
                    Ok(())
                })?;
                continue;
            }
            let store = self.fs.image(&path)?;
            let v = Volume::open(store, wp)?;
            set.insert(v)?;
            self.presented.insert(vol);
        }
        Ok(())
    }
}
