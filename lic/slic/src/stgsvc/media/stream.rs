// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A save stream to and from a device: the media parameters of the
//! save and restore commands (`DEV`, `VOL`, `SEQNBR`, `LABEL`,
//! `EXPDATE`, `ENDOPT`, `OPTFILE`, `CLEAR`) and the one call each
//! command makes once its stream is built, or before it reads one.

use cmd::block::Parms;
use medio::optfile::{OpticalReader, OpticalWriter, save_path};
use medio::tapefile::{RestoreFile, SaveFile, TapeReader, TapeWriter};
use medio::{MediaError, MediaReader, MediaWriter, Written};
use optical::volume::layout::SaveTarget;
use tape::label::{Date, Expiry};
use tape::{EndOpt, SeqNbr, TapeTransport, VolumeProvider};
use wm::obj::devmed::MediaKind;

use super::drive::{self, TapeJob};
use super::operator;
use crate::ipl::system::{System, now_unix};

/// The save was ended (an inquiry took its default reply).
pub(crate) const SAVE_ENDED: &str = "CPF3794";

/// The media parameters of one command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Media {
    /// The device (the first `DEV` value).
    pub dev: String,
    /// `VOL`, empty for `*MOUNTED`.
    pub vols: Vec<String>,
    /// `SEQNBR`; 0 or less for `*END` or `*SEARCH`.
    pub seq: i64,
    /// `LABEL`, empty when it is a special value.
    pub label: String,
    /// `EXPDATE`.
    pub expires: Expiry,
    /// `ENDOPT`.
    pub end_opt: EndOpt,
    /// `OPTFILE`.
    pub optfile: String,
    /// `CLEAR` other than `*NONE`.
    pub clear: bool,
}

/// Today as a label date.
pub(crate) fn today() -> Date {
    let days = i64::try_from(now_unix() / 86_400).unwrap_or(0);
    // Civil date from days since 1970-01-01.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u8::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let m = u8::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    let y = u16::try_from(yoe + era * 400 + i64::from(m <= 2)).unwrap_or(2026);
    Date::from_ymd(y, m, d).unwrap_or(Date { year: 2026, day: 1 })
}

/// `ENDOPT` as the tape layer takes it.
pub(crate) fn end_opt(s: &str) -> EndOpt {
    match s {
        "*LEAVE" => EndOpt::Leave,
        "*UNLOAD" => EndOpt::Unload,
        _ => EndOpt::Rewind,
    }
}

/// `EXPDATE`: `*PERM`, `*NONE`, or a date as `CYYMMDD`.
fn expiry(s: &str) -> Expiry {
    let s = s.trim();
    if s == "*NONE" {
        return Expiry::None;
    }
    let Ok(n) = s.parse::<u32>() else {
        return Expiry::Perm;
    };
    let (c, yy, mm, dd) = (n / 1_000_000, n / 10_000 % 100, n / 100 % 100, n % 100);
    let year = u16::try_from(1900 + c * 100 + yy).unwrap_or(2099);
    Date::from_ymd(year, u8::try_from(mm).unwrap_or(1), u8::try_from(dd).unwrap_or(1))
        .map_or(Expiry::Perm, Expiry::Date)
}

/// A volume list's values, without `*MOUNTED`.
fn volumes(parms: &Parms) -> Vec<String> {
    parms
        .list("VOL")
        .iter()
        .map(|v| v.source().trim().trim_matches('\'').to_ascii_uppercase())
        .filter(|v| !v.is_empty() && !v.starts_with('*'))
        .collect()
}

/// The media parameters of `parms`, for device `dev`.
pub(crate) fn of(parms: &Parms, dev: &str) -> Media {
    let label = parms.text("LABEL");
    let label = label.trim().trim_matches('\'');
    Media {
        dev: dev.to_ascii_uppercase(),
        vols: volumes(parms),
        seq: parms.int("SEQNBR").unwrap_or(0),
        label: if label.starts_with('*') { String::new() } else { label.to_ascii_uppercase() },
        expires: expiry(&parms.text("EXPDATE")),
        end_opt: end_opt(&parms.name("ENDOPT")),
        optfile: {
            let f = parms.text("OPTFILE");
            if f.is_empty() { "*".into() } else { f }
        },
        clear: !matches!(parms.name("CLEAR").as_str(), "" | "*NONE" | "0"),
    }
}

/// Whether `dev` names a device rather than a save file.
pub(crate) fn is_device(dev: &str) -> bool {
    !dev.is_empty() && !dev.starts_with('*') && !dev.starts_with('/') && !dev.starts_with('\'')
}

struct SaveJob<'b> {
    file: SaveFile,
    bytes: &'b [u8],
}

impl TapeJob for SaveJob<'_> {
    type Out = Written;
    fn run<D: TapeTransport + ?Sized>(&mut self, d: &mut D, p: &mut dyn VolumeProvider<D>) -> Result<Written, MediaError> {
        TapeWriter::new(d, p, self.file.clone()).write_stream(self.bytes)
    }
}

struct RestoreJob {
    file: RestoreFile,
}

impl TapeJob for RestoreJob {
    type Out = (Vec<u8>, Vec<String>);
    fn run<D: TapeTransport + ?Sized>(&mut self, d: &mut D, p: &mut dyn VolumeProvider<D>) -> Result<Self::Out, MediaError> {
        TapeReader::new(d, p, self.file.clone()).read_stream()
    }
}

/// The job writing, for the labels.
fn job_name(system: &System) -> String {
    system
        .jobs
        .get(system.current_job)
        .map_or_else(|| "QSYS".to_string(), |j| j.name.clone())
}

/// The optical volumes after the first of a save on device `dev`: the
/// `VOL` list's next one, else the catalog's next loaded entry after
/// the one mounted; with none, the operator is asked.
struct CatalogOptical<'a> {
    system: &'a mut System,
    dev: String,
    vols: Vec<String>,
    pick: medio::NextVolume,
}

impl<'a> CatalogOptical<'a> {
    fn new(system: &'a mut System, dev: &str, vols: &[String], first: &str) -> Self {
        Self {
            pick: medio::NextVolume::new(drive::optical_volumes(dev), Some(first.to_string()), true),
            system,
            dev: dev.to_string(),
            vols: vols.to_vec(),
        }
    }
}

impl medio::optfile::OpticalVolumes for CatalogOptical<'_> {
    fn next_volume(&mut self, req: &tape::VolumeRequest) -> Option<String> {
        let expected = usize::try_from(req.volume_seq).ok().and_then(|n| self.vols.get(n - 1)).cloned();
        let req = tape::VolumeRequest { expected, ..req.clone() };
        let (sys, dev) = (&mut *self.system, self.dev.clone());
        let mut ask: medio::catalog::Asker<'_> = Box::new(|r| drive::ask_volume(sys, &dev, r));
        let v = self.pick.next(&req, &mut ask)?;
        drop(ask);
        drive::mount_optical(&self.dev, &v).ok()?;
        self.pick.used(&v);
        Some(v)
    }
}

/// Write a save to the mounted optical volume of `m.dev`, on to further
/// volumes when it does not fit. An existing file is asked about
/// (`OPT1260`): the reply I replaces it.
fn save_optical(system: &mut System, m: &Media, target: SaveTarget<'_>, bytes: &[u8]) -> Result<Written, String> {
    let (t, vol) = drive::optical(system, &m.dev, m.vols.first().map(String::as_str))?;
    let path = save_path(&m.optfile, target);
    let set = super::volume_set();
    let mut replace = m.clear;
    loop {
        let wrote = {
            let mut next = CatalogOptical::new(system, &m.dev, &m.vols, &vol);
            OpticalWriter::new(&set, &vol, &path, replace).spanning(&mut next).write_stream(bytes)
        };
        match wrote {
            Ok(w) => return Ok(w),
            Err(MediaError::Exists(p)) if !replace => {
                let a = operator::inquiry(system, "OPT1260", &[(&p, 64), (&vol, 32)], "C");
                if a.is("G") {
                    replace = true;
                    // Start again on the first volume.
                    drive::mount_optical(&m.dev, &vol)?;
                    continue;
                }
                return Err(format!("{SAVE_ENDED} {p} is already on volume {vol}; {}", a.shown()));
            }
            Err(e) => return Err(drive::optical_failed(system, &t, &e)),
        }
    }
}

/// Write a save's stream to `m.dev`; `label` is the file label when
/// `LABEL` is a special value, `target` names an optical file.
pub(crate) fn save(system: &mut System, m: &Media, label: &str, target: SaveTarget<'_>, bytes: &[u8]) -> Result<Written, String> {
    let t = drive::target(system, &m.dev)?;
    if t.kind == MediaKind::Optical {
        return save_optical(system, m, target, bytes);
    }
    let mut file = SaveFile {
        vols: m.vols.clone(),
        seq: u32::try_from(m.seq).ok().filter(|n| *n > 0).map_or(SeqNbr::End, SeqNbr::Number),
        label: if m.label.is_empty() { label.to_string() } else { m.label.clone() },
        expires: m.expires,
        end_opt: m.end_opt,
        today: today(),
        allow_active: m.clear,
        job: job_name(system),
    };
    loop {
        let mut job = SaveJob { file: file.clone(), bytes };
        match drive::with_tape(system, &m.dev, &m.vols, true, &mut job) {
            Err(e) if e.starts_with(operator::ACTIVE_FILE) && !file.allow_active => {
                let a = operator::inquiry(system, operator::ACTIVE_FILE, &[(&m.dev, 10)], "C");
                if a.is("I") {
                    file.allow_active = true;
                    continue;
                }
                return Err(format!("{SAVE_ENDED} {e}; {}", a.shown()));
            }
            r => return r,
        }
    }
}

/// Read a save's stream from `m.dev`: the tape file with `label` when
/// `LABEL` is a special value (`None` for any), or the optical file
/// `target` names.
pub(crate) fn restore(system: &mut System, m: &Media, label: Option<&str>, target: SaveTarget<'_>) -> Result<Vec<u8>, String> {
    let t = drive::target(system, &m.dev)?;
    if t.kind == MediaKind::Optical {
        let (t, vol) = drive::optical(system, &m.dev, m.vols.first().map(String::as_str))?;
        let path = save_path(&m.optfile, target);
        let set = super::volume_set();
        let r = {
            let mut next = CatalogOptical::new(system, &m.dev, &m.vols, &vol);
            OpticalReader::new(&set, &vol, &path).spanning(&mut next).read_stream()
        };
        return r.map(|(b, _)| b).map_err(|e| drive::optical_failed(system, &t, &e));
    }
    let file = RestoreFile {
        vols: m.vols.clone(),
        seq: u32::try_from(m.seq).ok().filter(|n| *n > 0),
        label: if m.label.is_empty() { label.map(str::to_string) } else { Some(m.label.clone()) },
        end_opt: m.end_opt,
    };
    let mut job = RestoreJob { file };
    drive::with_tape(system, &m.dev, &m.vols, false, &mut job).map(|(b, _)| b)
}

struct Labels;

impl TapeJob for Labels {
    type Out = Vec<String>;
    fn run<D: TapeTransport + ?Sized>(&mut self, d: &mut D, _: &mut dyn VolumeProvider<D>) -> Result<Vec<String>, MediaError> {
        let l = tape::volume::labels(d, None, EndOpt::Rewind)?;
        Ok(l.files.into_iter().map(|f| f.label.file_id).collect())
    }
}

/// The files on the medium in `m.dev`: tape file labels, or the names
/// of the files in an optical volume's root.
pub(crate) fn file_labels(system: &mut System, m: &Media) -> Result<Vec<String>, String> {
    let t = drive::target(system, &m.dev)?;
    if t.kind == MediaKind::Optical {
        let (_, vol) = drive::optical(system, &m.dev, m.vols.first().map(String::as_str))?;
        let files = super::volume_set()
            .with(&vol, |v| optical::volume::ops::file_attrs(v, "/"))
            .map_err(|e| format!("{} {e}", e.msgid()))?;
        return Ok(files
            .into_iter()
            .map(|f| f.path.trim_start_matches('/').to_ascii_uppercase())
            .collect());
    }
    drive::with_tape(system, &m.dev, &m.vols, false, &mut Labels)
}
