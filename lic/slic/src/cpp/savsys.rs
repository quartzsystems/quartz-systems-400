// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `SAVSYS` (docs/RECOVERY.md): the Licensed Internal Code of both
//! slots, every installed product's load (the operating system first),
//! the security data and the configuration, written in the install
//! media layout (`instmedia::savsys`) to a virtual tape or optical
//! volume, so the installer can reinstall the system from it. And the
//! recovery's restores of the security data and the configuration,
//! which the operating system install runs from such media.
//!
//! Devices: `BLOCKED(phase11)`: until the device layer resolves
//! `TAPVRTnn`/`OPTVRTnn` through the image catalog loaded on them, a
//! virtual device is what `/run/qs400/vrtdev/<DEV>` names (a volume
//! file or a raw disk; the dev word `svc.vrtdev DEV PATH` binds one).

use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::io::{Cursor, Read as _, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cmd::block::Parms;
use instmedia::Payload;
use instmedia::savsys::{Image, write_optical, write_tape};
use saverst::{ObjSel, RestoreOptions, SaveOptions, SaveTarget};
use savfmt::{Reader, SaveKind, Writer};
use sec::profile::SpecialAuth;
use stg::proto::{CmdMsg, CmdReply};
use store::obj::JobHandle;

use super::direct::failed;
use super::savcmd::{CFG_TYPES, SvcHooks, header_with, values};
use crate::ipl::system::{System, now_unix};

/// Where virtual devices are bound until the device layer comes.
pub const VRTDEV_DIR: &str = "/run/qs400/vrtdev";

/// `SAVSYS` completed (Quartz's choice in IBM's range; see
/// `docs/SVC_OPEN_QUESTIONS.md`).
const SAVED_SYSTEM: &str = "CPC3703";
/// The device is not one `SAVSYS` can write.
const NO_DEVICE: &str = "CPF9814";
/// The save failed.
const NOT_SAVED: &str = "CPF3793";

/// Run `SAVSYS`, or `None` for another command.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    (name == "SAVSYS").then(|| savsys(system, parms))
}

/// The path a virtual device is bound to.
pub fn bound(dev: &str) -> Option<PathBuf> {
    let p = Path::new(VRTDEV_DIR).join(dev.to_ascii_uppercase());
    p.exists().then_some(p)
}

fn size_of(f: &mut File) -> u64 {
    f.seek(SeekFrom::End(0)).unwrap_or(0)
}

fn read_slots() -> Result<(Vec<u8>, Vec<u8>), String> {
    let lic = licpart::locate().ok();
    let (node, _) = diskimg::locate::esp(lic.as_deref()).map_err(|e| format!("the ESP: {e}"))?;
    let read = |path: &str| -> Result<Vec<u8>, String> {
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&node)
            .map_err(|e| format!("{}: {e}", node.display()))?;
        diskimg::fat::read_file_at(f, path).map_err(|e| format!("{path} on the ESP: {e}"))
    };
    let a = read(crate::platform::efivar::PATH_A)?;
    // A system installed before slot B existed holds one image.
    let b = read(crate::platform::efivar::PATH_B).unwrap_or_else(|_| a.clone());
    Ok((a, b))
}

fn security_stream(system: &mut System) -> Result<Vec<u8>, String> {
    let head = header_of(system, SaveKind::Security);
    let storage = system.storage.as_mut().ok_or("CPF3700 object store not open")?;
    let qsys = storage.qsys.ok_or("CPF3700 object store not initialized")?;
    let hooks = SvcHooks::new(qsys);
    let mut w = Writer::new(Vec::new(), &head).map_err(|e| e.to_string())?;
    let store = &mut storage.store;
    let rep = sec::guard::as_system(|| {
        saverst::secdta::save_security(store, &hooks, qsys, now_unix(), &mut w)
    })
    .map_err(|e| e.to_string())?;
    w.finish(u32::try_from(rep.not_saved()).unwrap_or(0))
        .map(|(_, b)| b)
        .map_err(|e| e.to_string())
}

fn config_stream(system: &mut System) -> Result<Vec<u8>, String> {
    let head = header_of(system, SaveKind::Config);
    let jobh = JobHandle(u64::from(system.current_job));
    let storage = system.storage.as_mut().ok_or("CPF3700 object store not open")?;
    let qsys = storage.qsys.ok_or("CPF3700 object store not initialized")?;
    let hooks = SvcHooks::new(qsys);
    let mut w = Writer::new(Vec::new(), &head).map_err(|e| e.to_string())?;
    let opts = SaveOptions {
        select: CFG_TYPES
            .iter()
            .map(|t| ObjSel {
                name: "*ALL".into(),
                type_code: Some(*t),
            })
            .collect(),
        omit: Vec::new(),
        library_object: false,
        skip_journaled: false,
        update_history: false,
        save_file_data: false,
        spool: false,
        target_savf: None,
        private_auth: true,
        changed_since: None,
        now: now_unix(),
        target: SaveTarget {
            command: "SAVSYS".into(),
            device: "*SAVSYS".into(),
            ..SaveTarget::default()
        },
    };
    let rep = saverst::save_library(&mut storage.store, jobh, &hooks, qsys, "QSYS", &opts, &mut w)
        .map_err(|e| e.to_string())?;
    w.finish(u32::try_from(rep.not_saved()).unwrap_or(0))
        .map(|(_, b)| b)
        .map_err(|e| e.to_string())
}

fn header_of(system: &System, kind: SaveKind) -> savfmt::MediaHeader {
    header_with(system, "SAVSYS", savfmt::Compression::None, kind)
}

/// The payload of a system save: what `SAVSYS` writes. `omit` holds
/// `*CFG` and `*SECDTA` when those are left out.
pub fn image(system: &mut System, volume: &str, omit: &[String]) -> Result<Image, String> {
    let (a, b) = read_slots()?;
    let mut files = vec![(Payload::LicA, a), (Payload::LicB, b)];
    let mut products = super::lpcmd::installed_products(system);
    products.sort_by_key(|p| (p != instmedia::layout::OS_PRODUCT, p.clone()));
    if !products.iter().any(|p| p == instmedia::layout::OS_PRODUCT) {
        return Err("the operating system's product QS00SS1 is not installed".into());
    }
    for p in products {
        let s = super::lpcmd::product_stream(system, &p)?;
        files.push((Payload::Product(p), s));
    }
    if !omit.iter().any(|o| o == "*SECDTA") {
        files.push((Payload::SecData, security_stream(system)?));
    }
    if !omit.iter().any(|o| o == "*CFG") {
        files.push((Payload::Config, config_stream(system)?));
    }
    Ok(Image::new(release::RELEASE, volume, true, files, None))
}

fn today() -> tape::label::Date {
    let (y, m, d, ..) = crate::platform::rtc::unix_to_civil(now_unix());
    tape::label::Date::from_ymd(
        u16::try_from(y).unwrap_or(2026),
        u8::try_from(m).unwrap_or(1),
        u8::try_from(d).unwrap_or(1),
    )
    .unwrap_or_else(|| tape::label::Date::from_ymd(2026, 1, 1).expect("a date"))
}

/// Write `img` on the tape volume at `path`; a blank volume is made and
/// initialized first (`vol` its identifier).
fn to_tape(path: &Path, img: &Image, vol: &str) -> Result<String, String> {
    let mut f = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let mut magic = [0u8; 8];
    let blank = f.read_exact(&mut magic).is_err() || magic != *tape::virt::format::MAGIC;
    let capacity = size_of(&mut f);
    let mut t = if blank {
        if capacity < 64 << 20 {
            return Err(format!("{} is too small for a system save", path.display()));
        }
        let o = tape::VolumeOptions {
            capacity: capacity - 4096,
            density: tape::virt::DEFAULT_DENSITY.into(),
            write_protect: false,
            tag: vol.into(),
        };
        let mut t = tape::VirtualTape::create(f, &o).map_err(|e| e.to_string())?;
        tape::volume::inztap(
            &mut t,
            &tape::volume::InzOptions {
                new_vol: Some(vol.into()),
                new_owner: "QS400".into(),
                check: tape::volume::Check::No,
                ..tape::volume::InzOptions::default()
            },
        )
        .map_err(|e| e.to_string())?;
        t
    } else {
        tape::VirtualTape::open(f).map_err(|e| e.to_string())?
    };
    let files = write_tape(&mut t, img, today()).map_err(|e| e.to_string())?;
    Ok(format!("{files} files on tape volume {vol}"))
}

/// Write `img` on the optical image at `path`, made when missing.
fn to_optical(path: &Path, img: &Image, vol: &str) -> Result<String, String> {
    let bytes: u64 = img.files.iter().map(|(_, d)| d.len() as u64).sum();
    let mut v = if path.exists() {
        let store = Arc::new(optical::FileImage::open(path).map_err(|e| e.to_string())?);
        optical::Volume::open(store, false).map_err(|e| e.to_string())?
    } else {
        let store = Arc::new(optical::FileImage::create(path, &[]).map_err(|e| e.to_string())?);
        optical::Volume::create(store, vol, "System save", bytes * 2 + (64 << 20), now_unix())
            .map_err(|e| e.to_string())?
    };
    write_optical(&mut v, img, now_unix()).map_err(|e| e.to_string())?;
    Ok(format!("{} files on optical volume {}", img.files.len() + 1, v.id()))
}

fn savsys(system: &mut System, parms: &Parms) -> CmdReply {
    const NAME: &str = "SAVSYS";
    if let Err(e) = crate::stgsvc::security::require(SpecialAuth::SAVSYS, "save the system") {
        return failed(NAME, &format!("CPF2189 {e}"));
    }
    let dev = values(parms, "DEV").first().cloned().unwrap_or_default();
    let Some(path) = bound(&dev) else {
        return failed(
            NAME,
            &format!("{NO_DEVICE} device {dev} not found; bind a virtual device first"),
        );
    };
    let vol = values(parms, "VOL")
        .into_iter()
        .find(|v| !v.starts_with('*'))
        .unwrap_or_else(|| "SAVSYS".into());
    let omit = values(parms, "OMIT");
    let img = match image(system, &vol, &omit) {
        Ok(i) => i,
        Err(e) => return failed(NAME, &format!("{NOT_SAVED} {e}")),
    };
    let wrote = if dev.starts_with("OPT") {
        to_optical(&path, &img, &vol)
    } else {
        to_tape(&path, &img, &vol)
    };
    match wrote {
        Ok(what) => {
            let mut text = String::new();
            for (p, d) in &img.files {
                let _ = writeln!(text, "{} {} bytes", p.path(), d.len());
            }
            let _ = writeln!(text, "{SAVED_SYSTEM} system saved to {dev}: {what}");
            CmdReply {
                text,
                messages: vec![CmdMsg {
                    msgid: SAVED_SYSTEM.into(),
                    kind: 4,
                    data: cmd::error::data(&[(&dev, 10)]),
                }],
                ..CmdReply::default()
            }
        }
        Err(e) => failed(NAME, &format!("{NOT_SAVED} {e}")),
    }
}

/// The recovery's restores from `SAVSYS` media, in IBM's order:
/// `RSTUSRPRF USRPRF(*ALL)`, `RSTCFG OBJ(*ALL)`, then `RSTAUT` once the
/// objects are back. One line per step for the log.
pub fn recover(system: &mut System, secdta: Option<&[u8]>, cfg: Option<&[u8]>) -> Vec<String> {
    let mut out = Vec::new();
    let Some(storage) = system.storage.as_mut() else {
        return vec!["recovery: object store not open".into()];
    };
    let Some(qsys) = storage.qsys else {
        return vec!["recovery: object store not initialized".into()];
    };
    let hooks = SvcHooks::new(qsys);
    let store = &mut storage.store;
    let now = now_unix();
    if let Some(s) = secdta {
        let r = Reader::new(Cursor::new(s)).map_err(|e| e.to_string()).and_then(|mut r| {
            let opts = saverst::secdta::ProfileOptions {
                new_only: false,
                users: Vec::new(),
                omit: Vec::new(),
                now,
            };
            sec::guard::as_system(|| {
                saverst::secdta::restore_profiles(store, JobHandle(0), &hooks, qsys, &mut r, &opts)
            })
            .map_err(|e| e.to_string())
        });
        out.push(match r {
            Ok(rep) => format!(
                "RSTUSRPRF: {} user profiles restored, {} not; {} authorization lists",
                rep.profiles(),
                rep.profiles_not(),
                rep.autls()
            ),
            Err(e) => format!("RSTUSRPRF: {e}"),
        });
    }
    if let Some(c) = cfg {
        let r = Reader::new(Cursor::new(c)).map_err(|e| e.to_string()).and_then(|mut r| {
            let opts = RestoreOptions {
                libraries: vec!["QSYS".into()],
                select: CFG_TYPES
                    .iter()
                    .map(|t| ObjSel {
                        name: "*ALL".into(),
                        type_code: Some(*t),
                    })
                    .collect(),
                system: "QS400".into(),
                now,
                ..RestoreOptions::default()
            };
            sec::guard::as_system(|| saverst::restore(store, JobHandle(0), &hooks, qsys, &mut r, &opts))
                .map_err(|e| e.to_string())
        });
        out.push(match r {
            Ok(rep) => format!(
                "RSTCFG: {} configuration objects restored",
                rep.objects.iter().filter(|o| o.msgid.is_none()).count()
            ),
            Err(e) => format!("RSTCFG: {e}"),
        });
    }
    // BLOCKED(phase11): RSTLIB SAVLIB(*NONSYS) and RST from the files
    // after the system save wait for the device layer's SAVLIB and SAV
    // to tape; the operator runs them after the IPL.
    if secdta.is_some() {
        let r = sec::guard::as_system(|| saverst::secdta::rstaut(store, &hooks, qsys, &[]));
        out.push(match r {
            Ok(done) => format!(
                "RSTAUT: authorities restored for {} profiles",
                done.len()
            ),
            Err(e) => format!("RSTAUT: {e}"),
        });
    }
    out
}

/// Bind a virtual device to a volume file or disk (`svc.vrtdev DEV
/// PATH`, dev builds; `BLOCKED(phase11)` until image catalogs load on
/// devices). An empty path unbinds it.
pub fn bind(dev: &str, path: &str) -> Result<String, String> {
    let dev = dev.to_ascii_uppercase();
    if !(dev.starts_with("TAPVRT") || dev.starts_with("OPTVRT")) || dev.len() > 10 {
        return Err(format!("{dev} is not a virtual device name (TAPVRTnn, OPTVRTnn)"));
    }
    std::fs::create_dir_all(VRTDEV_DIR).map_err(|e| e.to_string())?;
    let link = Path::new(VRTDEV_DIR).join(&dev);
    let _ = std::fs::remove_file(&link);
    if path.is_empty() {
        return Ok(format!("{dev} unbound"));
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(path, &link).map_err(|e| format!("{dev}: {e}"))?;
    #[cfg(not(unix))]
    return Err(format!("{dev}: links need Linux ({})", link.display()));
    #[cfg(unix)]
    Ok(format!("{dev} is {path}"))
}
