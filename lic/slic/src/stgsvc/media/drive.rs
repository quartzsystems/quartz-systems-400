// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A media device opened for one operation. A virtual tape device is
//! the mounted entry of the `*TAP` catalog loaded on it, its next
//! volumes the catalog's other loaded entries; a real tape drive is
//! `/dev/nstN` behind the `st` driver; a virtual optical device is the
//! mounted entry of its `*OPT` catalog, in the volume set `/QOPT` shows.
//! Volume requests nobody answers take their default reply (cancel).

use std::cell::RefCell;
use std::rc::Rc;

use medio::{CatalogTapes, MediaError};
use tape::{TapeError, TapeTransport, VirtualTape, VolumeProvider, VolumeRequest};
use wm::obj::devmed::MediaKind;

use super::devices::{self, NOT_AVAILABLE};
use super::operator::{self, NEXT_VOLUME};
use crate::ipl::system::System;

/// Device not ready: no volume.
pub(crate) const NOT_READY: &str = "CPF6760";
/// An optical device with no volume.
pub(crate) const OPT_EMPTY: &str = "OPT1660";

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

/// A drive with no further volumes: each request is asked and declined.
struct NoVolumes(Asked);

impl<D: ?Sized> VolumeProvider<D> for NoVolumes {
    fn next_volume(&mut self, _: &mut D, req: &VolumeRequest) -> Result<(), TapeError> {
        self.0.borrow_mut().push(req.clone());
        Err(TapeError::Cancelled)
    }
}

/// The inquiries of the volume requests nobody answered.
fn ask_operator(system: &mut System, dev: &str, asked: &Asked) {
    for r in asked.borrow().iter() {
        let vol = r.expected.clone().unwrap_or_else(|| "*MOUNTED".into());
        let seq = r.volume_seq.to_string();
        operator::inquiry(system, NEXT_VOLUME, &[(&vol, 6), (&r.file_label, 17), (dev, 10), (&seq, 4)], "C");
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
    if t.kind == MediaKind::Optical {
        return Err(format!("CPF6708 device {} is an optical device", t.dev));
    }
    let asked: Asked = Rc::new(RefCell::new(Vec::new()));
    let r = if t.virtual_ {
        virtual_tape(&t, vols, write, &asked, job)?
    } else if t.kind == MediaKind::TapeLibrary {
        return Err(format!(
            "CPF6745 device {}: tape media library operations need a drive; use the library's drive device",
            t.dev
        ));
    } else {
        real_tape(system, &t, &asked, job)?
    };
    ask_operator(system, &t.dev, &asked);
    r.map_err(|e| failed(system, &t, &e))
}

/// Run `job` on the mounted entry of the catalog on a virtual device.
fn virtual_tape<J: TapeJob>(
    tgt: &Target,
    vols: &[String],
    write: bool,
    asked: &Asked,
    job: &mut J,
) -> Result<Result<J::Out, MediaError>, String> {
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
    let seen = Rc::clone(asked);
    let mut prov = CatalogTapes::new(
        entries,
        Some(ent.volume.clone()),
        Box::new(|p: &str| super::catalogs::open_image(p)),
        Box::new(move |r| {
            seen.borrow_mut().push(r.clone());
            false
        }),
    );
    let r = job.run(&mut tape, &mut prov);
    drop(tape);
    // The last volume used stays mounted.
    if let Some(cur) = prov.current.filter(|v| *v != ent.volume) {
        mount(&tgt.dev, &cur)?;
    }
    Ok(r)
}

/// Run `job` on a real drive.
fn real_tape<J: TapeJob>(
    system: &System,
    t: &Target,
    asked: &Asked,
    job: &mut J,
) -> Result<Result<J::Out, MediaError>, String> {
    let node = devices::node_of(system, &t.rsrc).ok_or_else(|| format!("{NOT_READY} device {} not ready: resource {} not found", t.dev, t.rsrc))?;
    let path = std::path::PathBuf::from(format!("/dev/n{node}"));
    let io = tape::st::LinuxSt::open(&path).map_err(|e| format!("{NOT_READY} device {} not ready: {}: {e}", t.dev, path.display()))?;
    let mut drive = tape::st::StTransport::new(io);
    let mut prov = NoVolumes(Rc::clone(asked));
    Ok(job.run(&mut drive, &mut prov))
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
    if !src.virtual_ || !dst.virtual_ || src.kind == MediaKind::Optical || dst.kind == MediaKind::Optical {
        return Err("CPF6708 DUPTAP copies between virtual tape devices in this release".into());
    }
    let mut from_t = open_virtual(&src)?;
    let mut to_t = open_virtual(&dst)?;
    let asked: Asked = Rc::new(RefCell::new(Vec::new()));
    let (mut pa, mut pb) = (NoVolumes(Rc::clone(&asked)), NoVolumes(Rc::clone(&asked)));
    let r = tape::volume::duptap(&mut from_t, &mut pa, &mut to_t, &mut pb, opts);
    ask_operator(system, &dst.dev, &asked);
    r.map(|v| v.len()).map_err(|e| failed(system, &dst, &MediaError::Tape(e)))
}
