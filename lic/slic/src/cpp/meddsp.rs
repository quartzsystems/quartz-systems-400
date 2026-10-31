// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the media displays show, read once for the panel, the text
//! form, the printed report and the outfile: `DSPTAP` (a volume's
//! labels or saves), `DSPTAPCTG` (a library's cartridges), `DSPOPT`
//! (an optical volume's attributes, directories, files or saves) and
//! `WRKOPTVOL` (the optical volumes the system knows). The outfiles'
//! records are `medout.rs`'s.

use medio::MediaError;
use tape::label::{Code, Date, Expiry, LabelSet, RecordFormat};
use tape::volume::{FileReader, ReadOptions, VolumeKind};
use tape::{MAX_BLOCK, TapeTransport, VolumeProvider};
use wm::obj::devmed::MediaKind;

use crate::ipl::system::System;
use crate::stgsvc::media::drive::{self, TapeJob};
use crate::stgsvc::media::{self, devices, stream};

/// `MM/DD/YY` of a label date.
pub(crate) fn mdy(d: Date) -> String {
    let (y, m, day) = ymd(d);
    format!("{m:02}/{day:02}/{:02}", y % 100)
}

/// Year, month and day of a label date.
pub(crate) fn ymd(d: Date) -> (u16, u8, u8) {
    const DAYS: [u16; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let leap = (d.year.is_multiple_of(4) && !d.year.is_multiple_of(100)) || d.year.is_multiple_of(400);
    let mut left = d.day.max(1);
    for (i, n) in DAYS.iter().enumerate() {
        let n = if i == 1 && leap { 29 } else { *n };
        if left <= n {
            return (d.year, u8::try_from(i + 1).unwrap_or(12), u8::try_from(left).unwrap_or(31));
        }
        left -= n;
    }
    (d.year, 12, 31)
}

/// `*MOUNTED`, `*ALL` and blank as no volume.
fn named(v: &str) -> Option<String> {
    let v = v.trim().trim_matches('\'').to_ascii_uppercase();
    (!v.is_empty() && !v.starts_with('*')).then_some(v)
}

// ------------------------------------------------------------------ tape

/// One file of a tape volume.
pub(crate) struct TapFile {
    /// The file sequence number.
    pub seq: u32,
    /// The file label.
    pub label: String,
    /// The volume sequence number within the file.
    pub volseq: u32,
    /// Created, `MM/DD/YY` or blank.
    pub created: String,
    /// `*NONE`, `*PERM` or `MM/DD/YY`.
    pub expires: String,
    /// Data blocks on this volume.
    pub blocks: u64,
    /// `EOF`, `EOV` (continues), or `*NONE` (no trailer).
    pub end: &'static str,
    /// The two trailer labels (the header's when there is no
    /// trailer), 80 characters each, as the outfile keeps them.
    pub trl: (String, String),
    /// `F`, `V` or `U`.
    pub record_format: &'static str,
    /// Record and block lengths.
    pub record_length: u32,
    pub block_length: u32,
    /// The writer's system code.
    pub system_code: String,
}

/// A save on tape: what its media header says.
pub(crate) struct TapSave {
    /// The file sequence number.
    pub seq: u32,
    /// The library saved.
    pub library: String,
    /// The save command.
    pub command: String,
    /// Unix seconds when the save began.
    pub created: u64,
    /// The stream is compressed.
    pub compressed: bool,
    /// `VxRyMz`.
    pub release: String,
    /// The saving system's name and serial number.
    pub system: String,
    pub serial: String,
}

/// One object of a save on tape.
pub(crate) struct TapObj {
    /// The file sequence number of the save.
    pub seq: u32,
    /// Object, type, library, attribute, size, text.
    pub row: Vec<String>,
}

/// What `DSPTAP` shows.
pub(crate) struct TapView {
    /// The device.
    pub dev: String,
    /// The volume identifier, `*NL` or `*BLANK`.
    pub vol: String,
    /// The owner.
    pub owner: String,
    /// `SL`, `NL` or blank.
    pub vol_type: &'static str,
    /// `*EBCDIC` or `*ASCII`, blank when not labeled.
    pub code: &'static str,
    /// The volume label as written, 80 characters, blank when none.
    pub vol1: String,
    /// The density the drive reports (`*VRT256K` on a virtual drive).
    pub density: String,
    /// The files in the sequence range, with the label asked for.
    pub files: Vec<TapFile>,
    /// For `DATA(*SAVRST)`: the objects of each save.
    pub objects: Vec<TapObj>,
    /// For `DATA(*SAVRST)`: each save's header.
    pub saves: Vec<TapSave>,
}

/// DSPTAP's job: the labels, and for `*SAVRST` each save's stream.
struct Dsp {
    vol: Option<String>,
    label: Option<String>,
    range: (u32, u32),
    saves: bool,
    end_opt: tape::EndOpt,
}

type DspOut = (tape::volume::VolumeLabels, Vec<(u32, String, Vec<u8>)>, String);

impl TapeJob for Dsp {
    type Out = DspOut;
    fn run<D: TapeTransport + ?Sized>(&mut self, d: &mut D, p: &mut dyn VolumeProvider<D>) -> Result<DspOut, MediaError> {
        let l = tape::volume::labels(d, self.vol.as_deref(), tape::EndOpt::Rewind)?;
        let density = d.status().map(|s| s.density).unwrap_or_default();
        let mut out = Vec::new();
        if self.saves {
            let wanted: Vec<(u32, String)> = l
                .files
                .iter()
                .filter(|f| f.label.volume_seq <= 1)
                .filter(|f| (self.range.0..=self.range.1).contains(&f.label.file_seq))
                .filter(|f| self.label.as_ref().is_none_or(|x| *x == f.label.file_id))
                .map(|f| (f.label.file_seq, f.label.file_id.clone()))
                .collect();
            for (seq, id) in wanted {
                let opts = ReadOptions {
                    seq: Some(seq),
                    ..ReadOptions::default()
                };
                let mut r = FileReader::open(&mut *d, &mut *p, opts)?;
                let mut buf = vec![0u8; MAX_BLOCK];
                let mut bytes = Vec::new();
                while let Some(n) = r.read_block(&mut buf)? {
                    bytes.extend_from_slice(&buf[..n]);
                }
                r.finish()?;
                out.push((seq, id, bytes));
            }
        }
        self.end_opt.apply(d)?;
        Ok((l, out, density))
    }
}

/// A label written in ASCII, as text.
fn label_text(rec: &[u8]) -> String {
    String::from_utf8_lossy(rec).into_owned()
}

/// What `DSPTAP` asks for: the device, `VOL`, `LABEL`, the sequence
/// range, `DATA` and `ENDOPT`.
pub(crate) struct TapAsk<'a> {
    pub dev: &'a str,
    pub vol: &'a str,
    pub label: &'a str,
    pub range: (u32, u32),
    pub data: &'a str,
    pub end_opt: &'a str,
}

/// Read the volume on the device.
pub(crate) fn tape_view(system: &mut System, a: &TapAsk<'_>) -> Result<TapView, String> {
    let label = a.label.trim().to_ascii_uppercase();
    let mut job = Dsp {
        vol: named(a.vol),
        label: (!label.is_empty() && label != "*ALL").then_some(label),
        range: a.range,
        saves: a.data == "*SAVRST",
        end_opt: stream::end_opt(a.end_opt),
    };
    let vl: Vec<String> = job.vol.iter().cloned().collect();
    let (l, saves, density) = drive::with_tape(system, a.dev, &vl, false, &mut job)?;
    let (owner, vol_type, code, vol1) = match &l.kind {
        VolumeKind::Labeled { vol1, code } => (
            vol1.owner.clone(),
            "*SL",
            if *code == Code::Ascii { "*ASCII" } else { "*EBCDIC" },
            vol1.encode(Code::Ascii).map(|r| label_text(&r)).unwrap_or_default(),
        ),
        VolumeKind::Unlabeled => (String::new(), "*NL", "", String::new()),
        VolumeKind::Blank => (String::new(), "", "", String::new()),
    };
    let files = if a.data == "*VOLLBL" {
        Vec::new()
    } else {
        l.files
            .iter()
            .filter(|f| (a.range.0..=a.range.1).contains(&f.label.file_seq))
            .filter(|f| job.label.as_ref().is_none_or(|x| *x == f.label.file_id))
            .map(|f| {
                let trl = f
                    .label
                    .encode(f.trailer, Code::Ascii)
                    .map(|(a, b)| (label_text(&a), label_text(&b)))
                    .unwrap_or_default();
                TapFile {
                    seq: f.label.file_seq,
                    label: f.label.file_id.clone(),
                    volseq: f.label.volume_seq,
                    created: f.label.created.map(mdy).unwrap_or_default(),
                    expires: match f.label.expires {
                        Expiry::None => "*NONE".into(),
                        Expiry::Perm => "*PERM".into(),
                        Expiry::Date(d) => mdy(d),
                    },
                    blocks: f.label.block_count,
                    end: match f.trailer {
                        LabelSet::EndOfFile => "EOF",
                        LabelSet::EndOfVolume => "EOV",
                        LabelSet::Header => "*NONE",
                    },
                    trl,
                    record_format: match f.label.record_format {
                        RecordFormat::Fixed => "F",
                        RecordFormat::Variable => "V",
                        RecordFormat::Undefined => "U",
                    },
                    record_length: f.label.record_length,
                    block_length: f.label.block_length,
                    system_code: f.label.system_code.clone(),
                }
            })
            .collect()
    };
    let mut objects = Vec::new();
    let mut headers = Vec::new();
    for (seq, _, bytes) in &saves {
        let Ok((_, rows)) = super::savcmd::stream_listing(bytes) else { continue };
        if let Ok((head, _, index)) = savfmt::read_index(&mut std::io::Cursor::new(bytes.as_slice())) {
            let library = index
                .iter()
                .find(|e| e.owner_index == savfmt::object::NO_OWNER && e.type_code != store::obj::types::LIB)
                .map(|e| savfmt::text(&e.library))
                .unwrap_or_default();
            headers.push(TapSave {
                seq: *seq,
                library,
                command: head.command.clone(),
                created: head.created,
                compressed: head.compression != savfmt::Compression::None,
                release: format!("V{}R{}M{}", head.release.0, head.release.1, head.release.2),
                system: head.system.clone(),
                serial: head.serial.clone(),
            });
        }
        objects.extend(rows.into_iter().map(|row| TapObj { seq: *seq, row }));
    }
    Ok(TapView {
        dev: a.dev.to_ascii_uppercase(),
        vol: l.kind.id(),
        owner,
        vol_type,
        code,
        vol1,
        density,
        files,
        objects,
        saves: headers,
    })
}

// ------------------------------------------------------------- cartridges

/// One cartridge as `DSPTAPCTG` shows it.
pub(crate) struct CtgView {
    /// The library device.
    pub dev: String,
    /// The row `WRKTAPCTG` lists.
    pub row: media::library::CtgRow,
}

/// The cartridges of library `dev` that `ids` and `cgy` select;
/// `CPF9814` when there is no such device, `CPF6745` when it is not a
/// tape media library.
pub(crate) fn cartridges(system: &mut System, dev: &str, ids: &[String], cgy: &str) -> Result<Vec<CtgView>, String> {
    let dev = dev.trim().to_ascii_uppercase();
    let d = devices::read(system, &dev).map_err(|e| {
        if e.starts_with(devices::NOT_FOUND) {
            format!("CPF9814 Device {dev} not found.")
        } else {
            e
        }
    })?;
    if d.media_kind() != Some(MediaKind::TapeLibrary) {
        return Err(format!("CPF6745 device {dev} is not a tape media library"));
    }
    let rows = media::library::rows(system, &dev, ids, cgy)?;
    if rows.is_empty() {
        // Nothing matched: the cartridge asked for, else the category.
        if let Some(id) = ids.iter().find(|i| !i.starts_with('*')) {
            return Err(format!("CPF67A1 Cartridge {id} is not in media library {dev}."));
        }
        let cgy = if cgy.trim().is_empty() { "*ALL" } else { cgy.trim() };
        return Err(format!("CPF67EE Media library {dev} has no cartridges in category {cgy}."));
    }
    Ok(rows.into_iter().map(|row| CtgView { dev: dev.clone(), row }).collect())
}

// ---------------------------------------------------------------- optical

/// What `DSPOPT DATA(*VOLATR)` shows.
pub(crate) struct OptVolume {
    /// The device it is on.
    pub dev: String,
    /// The attributes.
    pub attrs: optical::volume::ops::VolumeAttrs,
}

/// What `DSPOPT` shows for its `DATA`.
pub(crate) enum OptView {
    /// `*VOLATR`.
    Volume(OptVolume),
    /// `*DIRATR`: the device, volume and directories.
    Dirs(String, String, Vec<optical::volume::ops::DirAttrs>),
    /// `*FILATR`: the device, volume and files.
    Files(String, String, Vec<optical::volume::ops::FileAttrs>),
    /// `*SAVRST`: the device, volume, and each optical file's save
    /// (path, description, rows).
    Saves(String, String, Vec<(String, super::savcmd::SavfListing)>),
}

/// `MM/DD/YY` of an ISO timestamp.
pub(crate) fn opt_mdy(t: &optical::iso::Timestamp) -> String {
    format!("{:02}/{:02}/{:02}", t.month, t.day, t.year.rem_euclid(100))
}

/// `HH:MM:SS` of an ISO timestamp.
pub(crate) fn opt_hms(t: &optical::iso::Timestamp) -> String {
    format!("{:02}:{:02}:{:02}", t.hour, t.minute, t.second)
}

/// The volume `VOL` names on `DEV` (or wherever it is loaded) and what
/// `DATA` asks of it; `PATH` for the directories and files.
pub(crate) fn optical_view(system: &mut System, vol: &str, dev: &str, data: &str, path: &str) -> Result<OptView, String> {
    let v = named(vol);
    let dev = dev.trim().to_ascii_uppercase();
    let (t, volid) = if !dev.is_empty() && !dev.starts_with('*') {
        drive::optical(system, &dev, v.as_deref())?
    } else {
        match &v {
            Some(v) => drive::optical_by_volume(system, v)?,
            None => return Err("CPD0084 DEV is needed with VOL(*MOUNTED)".into()),
        }
    };
    let set = media::volume_set();
    let path = path.trim().trim_matches('\'');
    let path = if path.is_empty() { "/" } else { path };
    let opt = |e: optical::Error| format!("{} {e}", e.msgid());
    Ok(match data {
        "*DIRATR" => OptView::Dirs(
            t.dev,
            volid.clone(),
            set.with(&volid, |v| optical::volume::ops::dir_attrs(v, path, true)).map_err(opt)?,
        ),
        "*FILATR" => OptView::Files(
            t.dev,
            volid.clone(),
            set.with(&volid, |v| optical::volume::ops::file_attrs(v, path)).map_err(opt)?,
        ),
        "*SAVRST" => {
            let mut files: Vec<String> = set
                .with(&volid, |v| optical::volume::ops::file_attrs(v, "/"))
                .map_err(opt)?
                .into_iter()
                .map(|f| f.path)
                .collect();
            if let Ok(sys) = set.with(&volid, |v| optical::volume::ops::file_attrs(v, "/QSAVSYS")) {
                files.extend(sys.into_iter().map(|f| f.path));
            }
            let lp = format!("/{}", instmedia::layout::LP_DIR);
            if let Ok(l) = set.with(&volid, |v| optical::volume::ops::file_attrs(v, &lp)) {
                files.extend(l.into_iter().map(|f| f.path));
            }
            let mut saves = Vec::new();
            for f in files {
                let bytes = set.with(&volid, |v| v.read_all(&f)).map_err(opt)?;
                if let Ok(l) = super::savcmd::stream_listing(&bytes) {
                    saves.push((f, l));
                }
            }
            OptView::Saves(t.dev, volid, saves)
        }
        _ => OptView::Volume(OptVolume {
            dev: t.dev,
            attrs: set.with(&volid, |v| optical::volume::ops::volume_attrs(v)).map_err(opt)?,
        }),
    })
}

/// One optical volume `WRKOPTVOL` lists.
pub(crate) struct OptVolRow {
    /// The volume identifier.
    pub vol: String,
    /// The device it is in.
    pub dev: String,
    /// `*PRIMARY` for every volume here.
    pub vol_type: &'static str,
    /// `*DVD-RAM` for a writable volume, `*CD-ROM` otherwise.
    pub media_type: &'static str,
    /// `*NONE`: volumes have no authorization list here.
    pub autl: &'static str,
    /// The image catalog.
    pub catalog: String,
    /// Write protected.
    pub protected: bool,
}

/// The optical volumes on optical devices `DEV` (`*ALL` for each) that
/// `VOL` (generic) selects, by device, then volume.
pub(crate) fn optical_volumes(system: &mut System, dev: &str, vol: &str) -> Result<Vec<OptVolRow>, String> {
    let dev = dev.trim().to_ascii_uppercase();
    let vol = vol.trim().trim_matches('\'').to_ascii_uppercase();
    let wanted = |v: &str| {
        vol.is_empty() || vol == "*ALL" || v.eq_ignore_ascii_case(&vol) || vol.strip_suffix('*').is_some_and(|p| v.to_ascii_uppercase().starts_with(p))
    };
    let devs = devices::all(system)?;
    let mut out = Vec::new();
    for (n, _) in devs.iter().filter(|(_, d)| d.media_kind() == Some(MediaKind::Optical)) {
        if !(dev.is_empty() || dev == "*ALL" || *n == dev) {
            continue;
        }
        let Some(c) = media::catalog_on(n) else { continue };
        for e in c.entries.iter().filter(|e| e.status != imgclg::EntryStatus::Available) {
            if !wanted(&e.volume) {
                continue;
            }
            out.push(OptVolRow {
                vol: e.volume.clone(),
                dev: n.clone(),
                vol_type: "*PRIMARY",
                media_type: if e.write_protect { "*CD-ROM" } else { "*DVD-RAM" },
                autl: "*NONE",
                catalog: c.name.clone(),
                protected: e.write_protect,
            });
        }
    }
    if out.is_empty() && !(dev.is_empty() || dev == "*ALL") && !devs.iter().any(|(n, _)| *n == dev) {
        return Err(format!("CPF9814 Device {dev} not found."));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_dates_as_the_displays_show_them() {
        assert_eq!(mdy(Date { year: 2026, day: 1 }), "01/01/26");
        assert_eq!(mdy(Date { year: 2024, day: 60 }), "02/29/24");
        assert_eq!(mdy(Date { year: 2026, day: 365 }), "12/31/26");
    }
}
