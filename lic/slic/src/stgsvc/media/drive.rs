// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A media device opened for one operation. A virtual tape device is
//! the mounted entry of the `*TAP` catalog loaded on it, its next
//! volumes the catalog's other loaded entries; a virtual media library
//! is its catalog's entries as cartridges, mounted in its drives; a real
//! tape drive is `/dev/nstN` behind the `st` driver; a virtual optical
//! device is the mounted entry of its `*OPT` catalog, in the volume set
//! `/QOPT` shows. A volume nobody named is asked of the operator
//! (`CPA4088`, or `CPA4089` naming the volume); a retry reply looks
//! again. A write that meets a file that has not expired asks `CPA6754`
//! when its job may overwrite (`TapeJob::active_files`), else ends with
//! `CPF6754`. Refusals are `Fail`s: the escape with its data.

use std::cell::RefCell;
use std::fmt::Display;
use std::rc::Rc;

use medio::msg::{self, Field};
use medio::{CatalogTapes, LibraryVolumes, MediaError};
use tape::{TapeError, TapeTransport, VirtualTape, VolumeProvider, VolumeRequest};
use wm::obj::devmed::MediaKind;

use super::devices::{self, NOT_AVAILABLE};
use super::fail::Fail;
use super::operator;
use crate::ipl::system::System;

/// Device not ready: no volume.
pub(crate) const NOT_READY: &str = "CPF6760";
/// An optical device with no volume.
pub(crate) const OPT_EMPTY: &str = "OPT1660";
/// Not a tape device.
const NOT_TAPE: &str = "CPF6721";
/// Not an optical device.
const NOT_OPTICAL: &str = "OPT1530";
/// The reply that has the next volume looked for again.
pub(crate) const RETRY: &str = "G";

/// Escape `id` with the device as `&1` and `why` as the cause.
pub(crate) fn dev_fail(id: &str, dev: &str, why: impl Display) -> Fail {
    Fail::new(id, &[Field::Char(dev, 10)]).detail(why)
}

/// A media device ready for an operation.
#[derive(Debug, Clone)]
pub(crate) struct Target {
    /// The device description's name.
    pub dev: String,
    /// Tape, optical or library.
    pub kind: MediaKind,
    /// Its resource.
    pub rsrc: String,
    /// Over an image catalog.
    pub virtual_: bool,
}

/// Device `dev`, varied on: `CPF9814` when there is no such device,
/// `CPF6718` when it is not varied on.
pub(crate) fn device(system: &mut System, dev: &str) -> Result<Target, Fail> {
    let dev = dev.to_ascii_uppercase();
    let d = devices::read(system, &dev).map_err(|e| {
        if e.starts_with(devices::NOT_FOUND) {
            Fail::new("CPF9814", &[Field::Char(&dev, 10)])
        } else {
            Fail::from(e)
        }
    })?;
    if !devices::usable(&d) {
        return Err(dev_fail(
            NOT_AVAILABLE,
            &dev,
            format_args!("device {dev} is {}", d.status.text()),
        ));
    }
    let m = d.media.clone().unwrap_or_default();
    Ok(Target {
        dev,
        kind: m.kind,
        virtual_: m.is_virtual(),
        rsrc: m.rsrcname,
    })
}

/// `device` for the callers that take `msgid text`.
pub(crate) fn target(system: &mut System, dev: &str) -> Result<Target, String> {
    device(system, dev).map_err(String::from)
}

/// An operation on a tape, whichever transport the device is.
pub(crate) trait TapeJob {
    /// What it gives back.
    type Out;
    /// Run it on the drive, with `provider` for further volumes.
    fn run<D: TapeTransport + ?Sized>(
        &mut self,
        drive: &mut D,
        provider: &mut dyn VolumeProvider<D>,
    ) -> Result<Self::Out, MediaError>;

    /// The flag that lets a write overwrite files that have not
    /// expired, for a job that asks the operator first (`CPA6754`);
    /// with `None` such a file ends it (`CPF6754`).
    fn active_files(&mut self) -> Option<&mut bool> {
        None
    }
}

/// What a job gave, and the volume it ended on (empty when unknown).
type Ran<T> = (Result<T, MediaError>, String);

/// The volume requests of one operation.
type Asked = Rc<RefCell<Vec<VolumeRequest>>>;

/// A drive with no further volumes: each request is kept and declined.
struct NoVolumes(Asked);

impl<D: ?Sized> VolumeProvider<D> for NoVolumes {
    fn next_volume(&mut self, _: &mut D, req: &VolumeRequest) -> Result<(), TapeError> {
        self.0.borrow_mut().push(req.clone());
        Err(TapeError::Cancelled)
    }
}

/// Ask the operator for the next volume of `req` on `dev`: whether the
/// reply was retry.
pub(crate) fn ask_volume(system: &mut System, dev: &str, req: &VolumeRequest) -> bool {
    let m = msg::next_volume(dev, req.expected.as_deref());
    operator::ask_msg(system, &m, "C").is(RETRY)
}

/// A real drive's next volume: the finished one is unloaded, the
/// operator puts the next in and replies retry; asked again while the
/// drive has none.
struct Operator<'a> {
    system: &'a mut System,
    dev: String,
}

impl<D: TapeTransport + ?Sized> VolumeProvider<D> for Operator<'_> {
    fn next_volume(&mut self, drive: &mut D, req: &VolumeRequest) -> Result<(), TapeError> {
        match drive.unload() {
            Ok(()) | Err(tape::TransportError::NotLoaded) => {}
            Err(e) => return Err(e.into()),
        }
        loop {
            if !ask_volume(self.system, &self.dev, req) {
                return Err(TapeError::Cancelled);
            }
            if drive.status()?.loaded {
                return Ok(());
            }
        }
    }
}

/// The error as the command's escape, the operator told; `vol` the
/// volume in the device.
fn failed(system: &mut System, t: &Target, e: &MediaError, vol: &str) -> Fail {
    let m = msg::escape(e, &t.dev, vol);
    operator::report(system, &t.rsrc, e, &m);
    Fail::from(m)
}

/// Mount entry `vol` of the catalog loaded on `dev` when it is in the
/// catalog and not mounted.
fn mount(dev: &str, vol: &str) -> Result<(), String> {
    let Some(c) = super::catalog_on(dev) else {
        return Ok(());
    };
    if c.mounted()
        .is_some_and(|e| e.volume.eq_ignore_ascii_case(vol))
    {
        return Ok(());
    }
    let Some(e) = c
        .entries
        .iter()
        .find(|e| e.volume.eq_ignore_ascii_case(vol))
    else {
        return Ok(());
    };
    let (name, index) = (c.name.clone(), e.index);
    super::with_catalogs(|r| {
        r.lodimgclge(&name, index, imgclg::ops::EntryOption::Load)?;
        r.lodimgclge(&name, index, imgclg::ops::EntryOption::Mount)
    })
}

/// `ENDOPT(*UNLOAD)` on a virtual tape device: the entry `vol` goes
/// back to `*LOADED` and the next loaded entry after it (or the first)
/// is mounted. With no other loaded entry it stays mounted.
fn unload_entry(dev: &str, vol: &str) -> Result<(), String> {
    let Some(c) = super::catalog_on(dev) else {
        return Ok(());
    };
    let loaded: Vec<&imgclg::Entry> = c
        .entries
        .iter()
        .filter(|e| e.status != imgclg::EntryStatus::Available)
        .collect();
    let Some(at) = loaded
        .iter()
        .position(|e| e.volume.eq_ignore_ascii_case(vol))
    else {
        return Ok(());
    };
    let next = loaded[at + 1..].iter().chain(loaded[..at].iter()).next();
    match next {
        Some(e) => mount(dev, &e.volume.clone()),
        None => Ok(()),
    }
}

/// Run `job` on tape device `dev`. `vols` is the `VOL` list (empty for
/// `*MOUNTED`); `write` says the job writes. A file that has not
/// expired in the way of a write is asked about when the job allows it.
pub(crate) fn tape_job<J: TapeJob>(
    system: &mut System,
    dev: &str,
    vols: &[String],
    write: bool,
    job: &mut J,
) -> Result<J::Out, Fail> {
    let t = device(system, dev)?;
    loop {
        let (r, vol) = match t.kind {
            MediaKind::Optical => return Err(dev_fail(NOT_TAPE, &t.dev, "an optical device")),
            MediaKind::TapeLibrary => library_tape(system, &t, vols, write, job)?,
            MediaKind::Tape if t.virtual_ => virtual_tape(system, &t, vols, write, job)?,
            MediaKind::Tape => real_tape(system, &t, job)?,
        };
        let e = match r {
            Ok(out) => return Ok(out),
            Err(e) => e,
        };
        if write
            && let Some(q) = msg::active_file_inquiry(&e, &t.dev)
            && let Some(allowed) = job.active_files()
            && !*allowed
        {
            let a = operator::ask_msg(system, &q, "C");
            if a.is("I") {
                *allowed = true;
                continue;
            }
            return Err(Fail::from(msg::escape(&e, &t.dev, &vol)).detail(a.shown()));
        }
        return Err(failed(system, &t, &e, &vol));
    }
}

/// `tape_job` for the callers that take `msgid text`. A write ended by
/// an active file leads with `operator::ACTIVE_FILE`, for a caller that
/// asks the operator itself.
pub(crate) fn with_tape<J: TapeJob>(
    system: &mut System,
    dev: &str,
    vols: &[String],
    write: bool,
    job: &mut J,
) -> Result<J::Out, String> {
    tape_job(system, dev, vols, write, job).map_err(|f| {
        if write && f.is("CPF6754") {
            format!("{} {f}", operator::ACTIVE_FILE)
        } else {
            f.into()
        }
    })
}

/// Run `job` on the mounted entry of the catalog on a virtual device.
fn virtual_tape<J: TapeJob>(
    system: &mut System,
    tgt: &Target,
    vols: &[String],
    write: bool,
    job: &mut J,
) -> Result<Ran<J::Out>, Fail> {
    if let Some(v) = vols.first() {
        mount(&tgt.dev, v)?;
    }
    let not_ready = |why: &str| dev_fail(NOT_READY, &tgt.dev, why);
    let clg = super::catalog_on(&tgt.dev).ok_or_else(|| not_ready("no image catalog is loaded"))?;
    if clg.kind != imgclg::ImgType::Tap {
        return Err(not_ready(&format!(
            "image catalog {} is not a tape catalog",
            clg.name
        )));
    }
    let ent = clg
        .mounted()
        .ok_or_else(|| not_ready("no volume is mounted"))?
        .clone();
    if write && ent.write_protect {
        return Err(Fail::new("CPF6768", &[Field::Char(&tgt.dev, 10)]));
    }
    let path = clg.path_of(&ent);
    let file = super::catalogs::open_image(&path)
        .map_err(|x| Fail::new("CPF6708", &[]).detail(format_args!("{path}: {x}")))?;
    let mut tape = VirtualTape::open(file)
        .map_err(|x| dev_fail("CPF6772", &tgt.dev, format_args!("{path}: {x}")))?;
    if tape.header().write_protect != ent.write_protect {
        let _ = tape.set_write_protect(ent.write_protect);
    }
    let entries: Vec<(String, String)> = clg
        .entries
        .iter()
        .filter(|x| x.status != imgclg::EntryStatus::Available)
        .map(|x| (x.volume.clone(), clg.path_of(x)))
        .collect();
    let dev = tgt.dev.clone();
    let (r, current) = {
        let sys = &mut *system;
        let mut prov = CatalogTapes::new(
            entries,
            Some(ent.volume.clone()),
            Box::new(|p: &str| super::catalogs::open_image(p)),
            Box::new(move |r| ask_volume(sys, &dev, r)),
        );
        let r = job.run(&mut tape, &mut prov);
        (r, prov.current.clone())
    };
    let unloaded = !tape.is_loaded();
    drop(tape);
    // The last volume used stays mounted, unless the job unloaded it.
    let last = current.unwrap_or_else(|| ent.volume.clone());
    if last != ent.volume {
        mount(&tgt.dev, &last)?;
    }
    if unloaded {
        unload_entry(&tgt.dev, &last)?;
    }
    Ok((r, last))
}

/// Run `job` on a virtual media library: the `VOL` list's first
/// cartridge mounted (or for `*MOUNTED` the one in a drive), further
/// ones moved in as the job asks, and a cartridge the job unloaded sent
/// home.
fn library_tape<J: TapeJob>(
    system: &mut System,
    tgt: &Target,
    vols: &[String],
    write: bool,
    job: &mut J,
) -> Result<Ran<J::Out>, Fail> {
    super::library::with_library(system, &tgt.dev, |system, t, lib| {
        let lib_err = |e: MediaError| Fail::from(msg::escape(&e, &t.dev, ""));
        for v in vols {
            lib.check_usable(v).map_err(lib_err)?;
        }
        let (i, vol) = match vols.first() {
            Some(v) => (lib.mount(v).map_err(lib_err)?, v.clone()),
            None => lib.mounted().ok_or_else(|| {
                dev_fail(
                    NOT_READY,
                    &t.dev,
                    "no cartridge is mounted; name one with VOL",
                )
            })?,
        };
        let protected = lib
            .changer
            .drive(i)
            .is_some_and(|d| d.header().write_protect);
        if write && protected {
            return Err(Fail::new("CPF6768", &[Field::Char(&t.dev, 10)]));
        }
        let spares = lib.spares();
        let dev = t.dev.clone();
        let r = {
            let sys = &mut *system;
            let mut prov = LibraryVolumes::new(
                i,
                spares,
                Some(vol.clone()),
                Box::new(move |r| ask_volume(sys, &dev, r)),
            );
            job.run(&mut lib.changer, &mut prov)
        };
        if let Err(e) = lib.after(i) {
            eprintln!("qsstg: media library {}: {e}", t.dev);
        }
        Ok((r, vol))
    })
}

/// Run `job` on a real drive.
fn real_tape<J: TapeJob>(
    system: &mut System,
    t: &Target,
    job: &mut J,
) -> Result<Ran<J::Out>, Fail> {
    let node = devices::node_of(system, &t.rsrc).ok_or_else(|| {
        dev_fail(
            NOT_READY,
            &t.dev,
            format_args!("resource {} not found", t.rsrc),
        )
    })?;
    let path = std::path::PathBuf::from(format!("/dev/n{node}"));
    let io = tape::st::LinuxSt::open(&path)
        .map_err(|e| dev_fail(NOT_READY, &t.dev, format_args!("{}: {e}", path.display())))?;
    let mut drive = tape::st::StTransport::new(io);
    let r = {
        let mut prov = Operator {
            system: &mut *system,
            dev: t.dev.clone(),
        };
        job.run(&mut drive, &mut prov)
    };
    Ok((r, String::new()))
}

/// The optical volume a device presents: the mounted entry of its
/// catalog (`vol` mounted first when the catalog has it), as a volume
/// of the set `/QOPT` shows. The device and the volume's identifier.
pub(crate) fn optical_volume(
    system: &mut System,
    dev: &str,
    vol: Option<&str>,
) -> Result<(Target, String), Fail> {
    let t = device(system, dev)?;
    let empty = |why: &str| dev_fail(OPT_EMPTY, &t.dev, why);
    if t.kind != MediaKind::Optical {
        return Err(Fail::new(NOT_OPTICAL, &[Field::Char(&t.dev, 10)]));
    }
    if !t.virtual_ {
        return Err(empty(
            "a real drive's volume is read with ADDIMGCLGE FROMDEV",
        ));
    }
    if let Some(v) = vol {
        mount(&t.dev, v)?;
    }
    let c = super::catalog_on(&t.dev).ok_or_else(|| empty("no image catalog is loaded"))?;
    let e = c.mounted().ok_or_else(|| empty("no volume is mounted"))?;
    if let Some(v) = vol
        && !e.volume.eq_ignore_ascii_case(v)
    {
        return Err(Fail::new(
            "OPT1332",
            &[Field::Char(&t.dev, 10), Field::Char(v, 32)],
        ));
    }
    Ok((t.clone(), e.volume.clone()))
}

/// `optical_volume` for the callers that take `msgid text`.
pub(crate) fn optical(
    system: &mut System,
    dev: &str,
    vol: Option<&str>,
) -> Result<(Target, String), String> {
    optical_volume(system, dev, vol).map_err(String::from)
}

/// The loaded optical volumes of the catalog on `dev`, in index order.
pub(crate) fn optical_volumes(dev: &str) -> Vec<String> {
    super::catalog_on(dev).map_or_else(Vec::new, |c| {
        c.entries
            .iter()
            .filter(|e| e.status != imgclg::EntryStatus::Available)
            .map(|e| e.volume.clone())
            .collect()
    })
}

/// Mount optical volume `vol` on `dev` (the catalog's entry), so the
/// volume set holds it.
pub(crate) fn mount_optical(dev: &str, vol: &str) -> Result<(), String> {
    mount(dev, vol)
}

/// The optical volume `vol` wherever it is loaded (`DSPOPT VOL(x)
/// DEV(*ALL)`): the device it is on, and the identifier.
pub(crate) fn optical_by_volume(system: &mut System, vol: &str) -> Result<(Target, String), Fail> {
    let devs = devices::all(system)?;
    for (n, d) in devs {
        if d.media_kind() != Some(MediaKind::Optical) {
            continue;
        }
        if super::catalog_on(&n)
            .and_then(|c| c.mounted().cloned())
            .is_some_and(|e| e.volume.eq_ignore_ascii_case(vol))
        {
            return optical_volume(system, &n, Some(vol));
        }
    }
    Err(Fail::new("OPT1331", &[Field::Char(vol, 32)]))
}

/// The error of an optical operation on volume `vol` as the escape,
/// the operator told.
pub(crate) fn optical_fail(system: &mut System, t: &Target, e: &MediaError, vol: &str) -> Fail {
    failed(system, t, e, vol)
}

/// The mounted volume of a virtual tape device, opened, and its
/// identifier.
fn open_virtual(t: &Target) -> Result<(VirtualTape<std::fs::File>, String), Fail> {
    let not_ready = |why: &str| dev_fail(NOT_READY, &t.dev, why);
    let clg = super::catalog_on(&t.dev).ok_or_else(|| not_ready("no image catalog is loaded"))?;
    let ent = clg
        .mounted()
        .ok_or_else(|| not_ready("no volume is mounted"))?;
    let path = clg.path_of(ent);
    let file = super::catalogs::open_image(&path)
        .map_err(|x| Fail::new("CPF6708", &[]).detail(format_args!("{path}: {x}")))?;
    let tape = VirtualTape::open(file)
        .map_err(|x| dev_fail("CPF6772", &t.dev, format_args!("{path}: {x}")))?;
    Ok((tape, ent.volume.clone()))
}

/// What `DUPTAP` did.
pub(crate) struct Dup {
    /// Files copied.
    pub files: usize,
    /// The source volume.
    pub from_vol: String,
    /// The target volume.
    pub to_vol: String,
}

/// `DUPTAP` between two virtual tape devices.
pub(crate) fn dup_tape(
    system: &mut System,
    from: &str,
    to: &str,
    opts: &tape::volume::DupOptions,
) -> Result<Dup, Fail> {
    let (src, dst) = (device(system, from)?, device(system, to)?);
    for t in [&src, &dst] {
        if !(t.virtual_ && t.kind == MediaKind::Tape) {
            return Err(Fail::new("CPF6708", &[]).detail(format_args!(
                "{}: DUPTAP copies between virtual tape devices in this release",
                t.dev
            )));
        }
    }
    let (mut from_t, from_vol) = open_virtual(&src)?;
    let (mut to_t, to_vol) = open_virtual(&dst)?;
    let asked: Asked = Rc::new(RefCell::new(Vec::new()));
    let (mut pa, mut pb) = (NoVolumes(Rc::clone(&asked)), NoVolumes(Rc::clone(&asked)));
    let r = tape::volume::duptap(&mut from_t, &mut pa, &mut to_t, &mut pb, opts);
    for q in asked.borrow().iter() {
        ask_volume(system, &dst.dev, q);
    }
    match r {
        Ok(v) => Ok(Dup {
            files: v.len(),
            from_vol,
            to_vol,
        }),
        Err(e) => Err(failed(system, &dst, &MediaError::Tape(e), &to_vol)),
    }
}
