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
//! (`CPA4075`, `operator::inquiry`); a retry reply looks again.

use std::cell::RefCell;
use std::rc::Rc;

use medio::{CatalogTapes, LibraryVolumes, MediaError};
use tape::{TapeError, TapeTransport, VirtualTape, VolumeProvider, VolumeRequest};
use wm::obj::devmed::MediaKind;

use super::devices::{self, NOT_AVAILABLE};
use super::operator::{self, NEXT_VOLUME};
use crate::ipl::system::System;

/// Device not ready: no volume.
pub(crate) const NOT_READY: &str = "CPF6760";
/// An optical device with no volume.
pub(crate) const OPT_EMPTY: &str = "OPT1660";
/// The reply that has the next volume looked for again.
pub(crate) const RETRY: &str = "G";

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

/// Device `dev`, varied on (`CPF6721` when it is not).
pub(crate) fn target(system: &mut System, dev: &str) -> Result<Target, String> {
    let dev = dev.to_ascii_uppercase();
    let d = devices::read(system, &dev)?;
    if !devices::usable(&d) {
        return Err(format!("{NOT_AVAILABLE} device {dev} is not available ({})", d.status.text()));
    }
    let m = d.media.clone().unwrap_or_default();
    Ok(Target {
        dev,
        kind: m.kind,
        virtual_: m.is_virtual(),
        rsrc: m.rsrcname,
    })
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
}

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
    let vol = req.expected.clone().unwrap_or_else(|| "*MOUNTED".into());
    let seq = req.volume_seq.to_string();
    operator::inquiry(system, NEXT_VOLUME, &[(&vol, 6), (&req.file_label, 17), (dev, 10), (&seq, 4)], "C").is(RETRY)
}

/// A real drive's next volume: the operator puts it in and replies
/// retry.
struct Operator<'a> {
    system: &'a mut System,
    dev: String,
}

impl<D: ?Sized> VolumeProvider<D> for Operator<'_> {
    fn next_volume(&mut self, _: &mut D, req: &VolumeRequest) -> Result<(), TapeError> {
        if ask_volume(self.system, &self.dev, req) {
            Ok(())
        } else {
            Err(TapeError::Cancelled)
        }
    }
}

/// The error as the command's refusal, the operator told.
fn failed(system: &mut System, t: &Target, e: &MediaError) -> String {
    operator::report(system, &t.rsrc, &t.dev, e);
    format!("{} {e}", e.msgid())
}

/// Mount entry `vol` of the catalog loaded on `dev` when it is in the
/// catalog and not mounted.
fn mount(dev: &str, vol: &str) -> Result<(), String> {
    let Some(c) = super::catalog_on(dev) else { return Ok(()) };
    if c.mounted().is_some_and(|e| e.volume.eq_ignore_ascii_case(vol)) {
        return Ok(());
    }
    let Some(e) = c.entries.iter().find(|e| e.volume.eq_ignore_ascii_case(vol)) else {
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
    let Some(c) = super::catalog_on(dev) else { return Ok(()) };
    let loaded: Vec<&imgclg::Entry> = c.entries.iter().filter(|e| e.status != imgclg::EntryStatus::Available).collect();
    let Some(at) = loaded.iter().position(|e| e.volume.eq_ignore_ascii_case(vol)) else {
        return Ok(());
    };
    let next = loaded[at + 1..].iter().chain(loaded[..at].iter()).next();
    match next {
        Some(e) => mount(dev, &e.volume.clone()),
        None => Ok(()),
    }
}

/// Run `job` on tape device `dev`. `vols` is the `VOL` list (empty for
/// `*MOUNTED`); `write` says the job writes.
pub(crate) fn with_tape<J: TapeJob>(
    system: &mut System,
    dev: &str,
    vols: &[String],
    write: bool,
    job: &mut J,
) -> Result<J::Out, String> {
    let t = target(system, dev)?;
    match t.kind {
        MediaKind::Optical => Err(format!("CPF6708 device {} is an optical device", t.dev)),
        MediaKind::TapeLibrary => library_tape(system, &t, vols, write, job),
        MediaKind::Tape if t.virtual_ => virtual_tape(system, &t, vols, write, job),
        MediaKind::Tape => real_tape(system, &t, job),
    }
}

/// Run `job` on the mounted entry of the catalog on a virtual device.
fn virtual_tape<J: TapeJob>(
    system: &mut System,
    tgt: &Target,
    vols: &[String],
    write: bool,
    job: &mut J,
) -> Result<J::Out, String> {
    if let Some(v) = vols.first() {
        mount(&tgt.dev, v)?;
    }
    let clg = super::catalog_on(&tgt.dev).ok_or_else(|| format!("{NOT_READY} device {} not ready: no image catalog is loaded", tgt.dev))?;
    if clg.kind != imgclg::ImgType::Tap {
        return Err(format!("{NOT_READY} device {} not ready: image catalog {} is not a tape catalog", tgt.dev, clg.name));
    }
    let ent = clg
        .mounted()
        .ok_or_else(|| format!("{NOT_READY} device {} not ready: no volume is mounted", tgt.dev))?
        .clone();
    if write && ent.write_protect {
        return Err(format!("CPF6768 volume {} on device {} is write protected", ent.volume, tgt.dev));
    }
    let path = clg.path_of(&ent);
    let file = super::catalogs::open_image(&path).map_err(|x| format!("CPF6708 {path}: {x}"))?;
    let mut tape = VirtualTape::open(file).map_err(|x| format!("CPF6772 {path}: {x}"))?;
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
    r.map_err(|e| failed(system, tgt, &e))
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
) -> Result<J::Out, String> {
    super::library::with_library(system, &tgt.dev, |system, t, lib| {
        let lib_err = |e: MediaError| super::library::refused(&e);
        for v in vols {
            lib.check_usable(v).map_err(lib_err)?;
        }
        let (i, vol) = match vols.first() {
            Some(v) => (lib.mount(v).map_err(lib_err)?, v.clone()),
            None => lib
                .mounted()
                .ok_or_else(|| format!("{NOT_READY} device {}: no cartridge is mounted; name one with VOL", t.dev))?,
        };
        let protected = lib.changer.drive(i).is_some_and(|d| d.header().write_protect);
        if write && protected {
            return Err(format!("CPF6768 volume {vol} on device {} is write protected", t.dev));
        }
        let spares = lib.spares();
        let dev = t.dev.clone();
        let r = {
            let sys = &mut *system;
            let mut prov = LibraryVolumes::new(i, spares, Some(vol), Box::new(move |r| ask_volume(sys, &dev, r)));
            job.run(&mut lib.changer, &mut prov)
        };
        if let Err(e) = lib.after(i) {
            eprintln!("qsstg: media library {}: {e}", t.dev);
        }
        r.map_err(|e| failed(system, t, &e))
    })
}

/// Run `job` on a real drive.
fn real_tape<J: TapeJob>(system: &mut System, t: &Target, job: &mut J) -> Result<J::Out, String> {
    let node = devices::node_of(system, &t.rsrc).ok_or_else(|| format!("{NOT_READY} device {} not ready: resource {} not found", t.dev, t.rsrc))?;
    let path = std::path::PathBuf::from(format!("/dev/n{node}"));
    let io = tape::st::LinuxSt::open(&path).map_err(|e| format!("{NOT_READY} device {} not ready: {}: {e}", t.dev, path.display()))?;
    let mut drive = tape::st::StTransport::new(io);
    let r = {
        let mut prov = Operator {
            system: &mut *system,
            dev: t.dev.clone(),
        };
        job.run(&mut drive, &mut prov)
    };
    r.map_err(|e| failed(system, t, &e))
}

/// The optical volume a device presents: the mounted entry of its
/// catalog (`vol` mounted first when the catalog has it), as a volume
/// of the set `/QOPT` shows. The device and the volume's identifier.
pub(crate) fn optical(system: &mut System, dev: &str, vol: Option<&str>) -> Result<(Target, String), String> {
    let t = target(system, dev)?;
    if t.kind != MediaKind::Optical {
        return Err(format!("OPT1203 device {} is not an optical device", t.dev));
    }
    if !t.virtual_ {
        return Err(format!("{OPT_EMPTY} optical device {} is empty", t.dev));
    }
    if let Some(v) = vol {
        mount(&t.dev, v)?;
    }
    let c = super::catalog_on(&t.dev).ok_or_else(|| format!("{OPT_EMPTY} optical device {} is empty", t.dev))?;
    let e = c.mounted().ok_or_else(|| format!("{OPT_EMPTY} optical device {} is empty", t.dev))?;
    if let Some(v) = vol
        && !e.volume.eq_ignore_ascii_case(v)
    {
        return Err(format!("OPT1115 volume {v} not found on device {}", t.dev));
    }
    Ok((t.clone(), e.volume.clone()))
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
pub(crate) fn optical_by_volume(system: &mut System, vol: &str) -> Result<(Target, String), String> {
    let devs = devices::all(system)?;
    for (n, d) in devs {
        if d.media_kind() != Some(MediaKind::Optical) {
            continue;
        }
        if super::catalog_on(&n).and_then(|c| c.mounted().cloned()).is_some_and(|e| e.volume.eq_ignore_ascii_case(vol)) {
            return optical(system, &n, Some(vol));
        }
    }
    Err(format!("OPT1115 volume {vol} not found"))
}

/// The error of an optical operation as the refusal, the operator told.
pub(crate) fn optical_failed(system: &mut System, t: &Target, e: &MediaError) -> String {
    failed(system, t, e)
}

/// The mounted volume of a virtual tape device, opened.
fn open_virtual(t: &Target) -> Result<VirtualTape<std::fs::File>, String> {
    let c = super::catalog_on(&t.dev).ok_or_else(|| format!("{NOT_READY} device {} not ready: no image catalog is loaded", t.dev))?;
    let e = c
        .mounted()
        .ok_or_else(|| format!("{NOT_READY} device {} not ready: no volume is mounted", t.dev))?;
    let path = c.path_of(e);
    let f = super::catalogs::open_image(&path).map_err(|x| format!("CPF6708 {path}: {x}"))?;
    VirtualTape::open(f).map_err(|x| format!("CPF6772 {path}: {x}"))
}

/// `DUPTAP` between two virtual tape devices: how many files.
pub(crate) fn dup_tape(system: &mut System, from: &str, to: &str, opts: &tape::volume::DupOptions) -> Result<usize, String> {
    let (src, dst) = (target(system, from)?, target(system, to)?);
    let tape_ok = |t: &Target| t.virtual_ && t.kind == MediaKind::Tape;
    if !tape_ok(&src) || !tape_ok(&dst) {
        return Err("CPF6708 DUPTAP copies between virtual tape devices in this release".into());
    }
    let mut from_t = open_virtual(&src)?;
    let mut to_t = open_virtual(&dst)?;
    let asked: Asked = Rc::new(RefCell::new(Vec::new()));
    let (mut pa, mut pb) = (NoVolumes(Rc::clone(&asked)), NoVolumes(Rc::clone(&asked)));
    let r = tape::volume::duptap(&mut from_t, &mut pa, &mut to_t, &mut pb, opts);
    for q in asked.borrow().iter() {
        ask_volume(system, &dst.dev, q);
    }
    r.map(|v| v.len()).map_err(|e| failed(system, &dst, &MediaError::Tape(e)))
}
