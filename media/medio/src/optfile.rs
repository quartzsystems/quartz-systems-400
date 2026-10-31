// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A save as one file on an optical volume: `OPTFILE('*')` is the
//! generated name (`/MYLIB`, `/QSAVIFS.nnn`, `/QSAVSYS/<KIND>`),
//! `OPTFILE('dir/*')` the generated name in that directory, anything
//! else the path itself.
//!
//! A save larger than the volume's room continues on further volumes
//! when the writer has an [`OpticalVolumes`]: each volume holds a part
//! under the same path and, beside it, the part's label `<path>.QSPART`
//! (the save's identifier, the part's number, whether more follow). A
//! save on one volume has no label, so its file stays the bare stream.

use std::fmt::Write as _;

use optical::VolumeSet;
use optical::volume::layout::{SaveTarget, generated_save_path};
use tape::{TapeError, VolumeRequest};

use crate::{MediaError, MediaReader, MediaWriter, Result, Written};

/// The part label's suffix.
pub const PART_SUFFIX: &str = ".QSPART";
const PART_MAGIC: &str = "QS400-OPTPART 1";
/// Bytes kept free on a volume for the directory records of a part.
const MARGIN: u64 = 16 * 2048;
/// Parts are whole sectors.
const SECTOR: u64 = 2048;

/// The path `OPTFILE` names for a save of `target`.
#[must_use]
pub fn save_path(optfile: &str, target: SaveTarget<'_>) -> String {
    let f = optfile.trim().trim_matches('\'');
    let generated = generated_save_path(target);
    if f.is_empty() || f == "*" {
        return generated;
    }
    let path = if f.starts_with('/') {
        f.to_string()
    } else {
        format!("/{f}")
    };
    match path.strip_suffix("/*") {
        Some(dir) => {
            let name = generated.rsplit('/').next().unwrap_or(&generated);
            format!("{dir}/{name}")
        }
        None => path,
    }
}

/// The volumes after the first of a save spanning optical volumes.
pub trait OpticalVolumes {
    /// Make the next volume ready in the volume set for `req` (its
    /// `volume_seq` the part wanted, `file_label` the path); its
    /// identifier, or `None` to end the operation.
    fn next_volume(&mut self, req: &VolumeRequest) -> Option<String>;
}

/// One part's label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    /// The save's identifier, the same on every part.
    pub save: String,
    /// The part's number, from 1.
    pub part: u32,
    /// More parts follow.
    pub more: bool,
    /// Bytes in this part.
    pub bytes: u64,
}

impl Part {
    /// The label as text.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut s = format!("{PART_MAGIC}\n");
        let _ = writeln!(s, "save\t{}", self.save);
        let _ = writeln!(s, "part\t{}", self.part);
        let _ = writeln!(s, "more\t{}", u8::from(self.more));
        let _ = writeln!(s, "bytes\t{}", self.bytes);
        s
    }

    /// Read a label back.
    pub fn parse(text: &str) -> Result<Self> {
        let mut lines = text.lines();
        let bad = || MediaError::Continuation("the part label is damaged".into());
        if lines.next() != Some(PART_MAGIC) {
            return Err(bad());
        }
        let mut p = Self {
            save: String::new(),
            part: 0,
            more: false,
            bytes: 0,
        };
        for l in lines {
            match l.split_once('\t') {
                Some(("save", v)) => p.save = v.to_string(),
                Some(("part", v)) => p.part = v.parse().map_err(|_| bad())?,
                Some(("more", v)) => p.more = v == "1",
                Some(("bytes", v)) => p.bytes = v.parse().map_err(|_| bad())?,
                _ => {}
            }
        }
        if p.part == 0 || p.save.is_empty() {
            return Err(bad());
        }
        Ok(p)
    }
}

/// The save's identifier: FNV-1a over its bytes and length.
fn save_id(data: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in data.iter().chain(&(data.len() as u64).to_le_bytes()) {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016X}")
}

/// Writes a save to a file on a loaded volume.
pub struct OpticalWriter<'a> {
    set: &'a VolumeSet,
    volume: String,
    path: String,
    replace: bool,
    next: Option<&'a mut dyn OpticalVolumes>,
}

impl<'a> OpticalWriter<'a> {
    /// A writer of `path` on `volume`; `replace` lets an existing file
    /// go (`CLEAR(*ALL)` or `*REPLACE`).
    #[must_use]
    pub fn new(set: &'a VolumeSet, volume: &str, path: &str, replace: bool) -> Self {
        Self {
            set,
            volume: volume.to_string(),
            path: path.to_string(),
            replace,
            next: None,
        }
    }

    /// Let the save go on to further volumes.
    #[must_use]
    pub fn spanning(mut self, next: &'a mut dyn OpticalVolumes) -> Self {
        self.next = Some(next);
        self
    }
}

/// Each directory above `path`, made where missing.
fn make_parents(v: &mut optical::Volume, path: &str) -> optical::Result<()> {
    let mut at = String::new();
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    for p in parts.iter().take(parts.len().saturating_sub(1)) {
        at.push('/');
        at.push_str(p);
        if v.stat(&at).is_err() {
            v.mkdir(&at)?;
        }
    }
    Ok(())
}

/// Put `data` at `path` (and the label `part`, when given) on `v`;
/// nothing stays when it does not fit.
fn put(v: &mut optical::Volume, path: &str, data: &[u8], part: Option<&Part>) -> optical::Result<()> {
    make_parents(v, path)?;
    let side = format!("{path}{PART_SUFFIX}");
    match part {
        Some(p) => v.put_file(&side, p.to_text().into_bytes(), true)?,
        None => {
            if v.stat(&side).is_ok() {
                v.remove(&side)?;
            }
        }
    }
    if let Err(e) = v.put_file(path, data.to_vec(), true) {
        if part.is_some() {
            let _ = v.remove(&side);
        }
        return Err(e);
    }
    Ok(())
}

impl OpticalWriter<'_> {
    /// Write the rest of the save from `off` on `vol` as part `n`: all of
    /// it, or as much as fits when another volume may follow. The bytes
    /// written.
    fn part(&mut self, vol: &str, data: &[u8], off: usize, n: u32, id: &str) -> Result<usize> {
        let path = self.path.clone();
        let rest = &data[off..];
        let spans = self.next.is_some();
        let last = Part {
            save: id.to_string(),
            part: n,
            more: false,
            bytes: rest.len() as u64,
        };
        let whole = self.set.with(vol, |v| put(v, &path, rest, (n > 1).then_some(&last)));
        let over = match whole {
            Ok(()) => return Ok(rest.len()),
            Err(optical::Error::VolumeFull { need, capacity }) if spans => need - capacity,
            Err(e) => return Err(e.into()),
        };
        // As much as fits, with the label saying more follow.
        let mut fit = (rest.len() as u64).saturating_sub(over + MARGIN) / SECTOR * SECTOR;
        let mut full = optical::Error::VolumeFull {
            need: rest.len() as u64 + over,
            capacity: rest.len() as u64,
        };
        for _ in 0..8 {
            if fit == 0 {
                break;
            }
            let len = usize::try_from(fit).unwrap_or(usize::MAX).min(rest.len());
            let p = Part {
                save: id.to_string(),
                part: n,
                more: true,
                bytes: len as u64,
            };
            match self.set.with(vol, |v| put(v, &path, &rest[..len], Some(&p))) {
                Ok(()) => return Ok(len),
                Err(optical::Error::VolumeFull { need, capacity }) => {
                    fit = fit.saturating_sub(need - capacity + MARGIN) / SECTOR * SECTOR;
                    full = optical::Error::VolumeFull { need, capacity };
                }
                Err(e) => return Err(e.into()),
            }
        }
        Err(full.into())
    }
}

impl MediaWriter for OpticalWriter<'_> {
    fn write_stream(&mut self, data: &[u8]) -> Result<Written> {
        let path = self.path.clone();
        let id = save_id(data);
        let mut vol = self.volume.clone();
        let mut volumes = Vec::new();
        let (mut off, mut n) = (0usize, 1u32);
        loop {
            let exists = self.set.with(&vol, |v| Ok(v.stat(&path).is_ok()))?;
            if exists && !self.replace {
                return Err(MediaError::Exists(path));
            }
            let wrote = self.part(&vol, data, off, n, &id)?;
            self.set.sync(&vol)?;
            volumes.push(vol.clone());
            off += wrote;
            if off >= data.len() {
                break;
            }
            n += 1;
            let req = VolumeRequest {
                volume_seq: n,
                expected: None,
                previous: vol.clone(),
                file_label: path.clone(),
                file_seq: 0,
                writing: true,
            };
            let next = self.next.as_mut().ok_or(TapeError::Cancelled)?;
            vol = next.next_volume(&req).ok_or(TapeError::Cancelled)?;
        }
        Ok(Written {
            seq: 0,
            volumes,
            file: self.path.clone(),
            bytes: data.len() as u64,
        })
    }
}

/// Reads a save from a file on a loaded volume.
pub struct OpticalReader<'a> {
    set: &'a VolumeSet,
    volume: String,
    path: String,
    next: Option<&'a mut dyn OpticalVolumes>,
}

impl<'a> OpticalReader<'a> {
    /// A reader of `path` on `volume`.
    #[must_use]
    pub fn new(set: &'a VolumeSet, volume: &str, path: &str) -> Self {
        Self {
            set,
            volume: volume.to_string(),
            path: path.to_string(),
            next: None,
        }
    }

    /// Follow a save onto further volumes.
    #[must_use]
    pub fn spanning(mut self, next: &'a mut dyn OpticalVolumes) -> Self {
        self.next = Some(next);
        self
    }
}

/// The file and its part label on `vol`.
fn read_part(set: &VolumeSet, vol: &str, path: &str) -> Result<(Vec<u8>, Option<Part>)> {
    let side = format!("{path}{PART_SUFFIX}");
    let (data, label) = set.with(vol, |v| {
        if v.stat(path).is_err() {
            return Err(optical::Error::NotFound(path.to_string()));
        }
        let label = v.read_all(&side).ok();
        Ok((v.read_all(path)?, label))
    })?;
    let part = label
        .map(|b| Part::parse(&String::from_utf8_lossy(&b)))
        .transpose()?;
    Ok((data, part))
}

impl MediaReader for OpticalReader<'_> {
    fn read_stream(&mut self) -> Result<(Vec<u8>, Vec<String>)> {
        let path = self.path.clone();
        let (mut data, mut part) = read_part(self.set, &self.volume, &path)?;
        let mut vols = vec![self.volume.clone()];
        let Some(first) = part.clone() else {
            return Ok((data, vols));
        };
        if first.part != 1 {
            return Err(MediaError::Continuation(format!(
                "volume {} holds part {} of {path}; mount the first volume",
                self.volume, first.part
            )));
        }
        while let Some(p) = part.filter(|p| p.more) {
            let req = VolumeRequest {
                volume_seq: p.part + 1,
                expected: None,
                previous: vols.last().cloned().unwrap_or_default(),
                file_label: path.clone(),
                file_seq: 0,
                writing: false,
            };
            let next = self.next.as_mut().ok_or_else(|| {
                MediaError::Continuation(format!("{path} continues on another volume"))
            })?;
            let vol = next.next_volume(&req).ok_or(TapeError::Cancelled)?;
            let (more, label) = read_part(self.set, &vol, &path)?;
            match &label {
                Some(l) if l.save == first.save && l.part == p.part + 1 => {}
                _ => {
                    return Err(MediaError::Continuation(format!(
                        "volume {vol} does not hold part {} of {path}",
                        p.part + 1
                    )));
                }
            }
            data.extend_from_slice(&more);
            vols.push(vol);
            part = label;
        }
        Ok((data, vols))
    }
}
