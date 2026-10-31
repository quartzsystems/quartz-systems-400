// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Install devices (docs/INSTALL.md, "Install devices"): where the
//! operating system install reads install media from.
//!
//! - **Optical**: the boot media the D IPL came from (`OPT01`), as the
//!   media locator found it: a CD, the hybrid image written raw, or its
//!   tree on FAT.
//! - **Virtual**: any other disk whose first bytes are a virtual tape
//!   volume (`QSTAPVOL`, `TAPVRTnn`) or an ISO 9660 image (`OPTVRTnn`)
//!   holding the install media layout, such as `SAVSYS` output copied
//!   out of an image catalog.
//! - **Tape**: the tape drives the hardware table names (`TAPnn`, by
//!   `tape::device::naming` over `/sys/class/scsi_tape`), read through
//!   the `st` driver's no-rewind node, with a cartridge of install media
//!   or `SAVSYS` output loaded.
//!
//! The disks found here are never offered as load sources or units.

use std::fs::{File, OpenOptions};
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use instmedia::layout::{Contents, Payload};
use instmedia::savsys::{DirSource, IsoSource, Source, TapeSource};
use release::InstallKind;
use svctools::install::os::{DeviceKind, InstallDevice};
use tape::TapeTransport;

use crate::cpp::savsys::{Medium, Pending};
use crate::ipl::media;

/// What carries a medium.
#[derive(Debug, Clone)]
pub enum Carrier {
    /// The boot media, mounted to read.
    Boot(media::Found),
    /// A virtual tape volume on a disk.
    Tape(PathBuf),
    /// An ISO 9660 image on a disk.
    Iso(PathBuf),
    /// A tape drive: its `st` node (`/dev/nstN`).
    Drive(PathBuf),
}

/// The tape drives under the sysfs root `sys`: resource name and
/// no-rewind node, in the hardware table's order.
pub fn tape_drives(sys: &Path) -> Vec<(String, PathBuf)> {
    let mut t = crate::hwrsrc::Table::default();
    t.add_tapes(sys);
    t.resources
        .iter()
        .filter(|r| r.kind == crate::hwrsrc::Kind::Tape)
        .map(|r| (r.name.clone(), PathBuf::from(format!("/dev/n{}", r.location))))
        .collect()
}

/// Install media on a tape, or `None` (no cartridge, unlabeled, or
/// another kind of data).
fn probe<D: TapeTransport + ?Sized>(t: &mut D) -> Option<Contents> {
    instmedia::savsys::recognize_tape(t).ok().flatten()
}

/// A drive's node opened as a transport.
fn open_drive(node: &Path) -> Result<tape::st::StTransport<tape::st::LinuxSt>, String> {
    tape::st::LinuxSt::open(node)
        .map(tape::st::StTransport::new)
        .map_err(|e| format!("{}: {e}", node.display()))
}

/// How many files follow the system save on a tape.
fn user_count<D: TapeTransport + ?Sized>(t: &mut D) -> usize {
    instmedia::savsys::user_files(t).map_or(0, |f| f.len())
}

/// The user data after a system save on `found`, for the storage
/// service to restore once the install is done: `None` when nothing
/// follows the save, or the medium is not a tape.
pub fn user_data(found: &Found) -> Option<Pending> {
    let (medium, files) = match &found.carrier {
        Carrier::Tape(dev) => {
            let f = OpenOptions::new().read(true).write(true).open(dev).ok()?;
            let mut t = tape::VirtualTape::open(f).ok()?;
            (Medium::Disk(dev.clone()), user_count(&mut t))
        }
        Carrier::Drive(node) => {
            let mut t = open_drive(node).ok()?;
            (Medium::Drive(found.device.name.clone()), user_count(&mut t))
        }
        Carrier::Boot(_) | Carrier::Iso(_) => return None,
    };
    (files > 0).then(|| Pending {
        device: found.device.name.clone(),
        medium,
        files,
    })
}

/// A device with install media.
#[derive(Debug, Clone)]
pub struct Found {
    /// What the panels show.
    pub device: InstallDevice,
    /// Where it is.
    pub carrier: Carrier,
    /// What it holds.
    pub contents: Contents,
}

static SCAN: Mutex<Option<Vec<Found>>> = Mutex::new(None);

fn head(dev: &Path, n: usize) -> Option<Vec<u8>> {
    let mut f = File::open(dev).ok()?;
    let mut b = vec![0u8; n];
    f.read_exact(&mut b).ok()?;
    Some(b)
}

fn tape_contents(dev: &Path) -> Option<Contents> {
    let f = OpenOptions::new().read(true).write(true).open(dev).ok()?;
    let mut t = tape::VirtualTape::open(f).ok()?;
    instmedia::savsys::recognize_tape(&mut t).ok().flatten()
}

fn iso_contents(dev: &Path) -> Option<Contents> {
    let f = File::open(dev).ok()?;
    let iso = optical::device::open_media(std::sync::Arc::new(f)).ok()?;
    instmedia::savsys::recognize_iso(&iso).ok().flatten()
}

/// What installing `c` over this system does, and whether it may.
fn action(store_release: Option<release::Release>, c: &Contents) -> (String, Option<String>) {
    let Some(have) = store_release else {
        return (format!("Fresh install of {}", c.release), None);
    };
    match release::check_upgrade(have, c.release) {
        Ok(InstallKind::Slip) => (format!("Slip install of {} over {have}", c.release), None),
        Ok(InstallKind::Upgrade { from, to }) => (format!("Upgrade from {from} to {to}"), None),
        Err(e) => (format!("Not allowed over {have}"), Some(e.to_string())),
    }
}

/// Every device with install media, scanned once per IPL (`rescan`
/// forgets the last scan). `store_release` is the release ASP 1 records,
/// `None` for none yet.
pub fn scan(store_release: Option<release::Release>, rescan: bool) -> Vec<Found> {
    let mut g = SCAN.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if rescan {
        *g = None;
    }
    if let Some(v) = g.as_ref() {
        return v.clone();
    }
    let mut out = Vec::new();
    if let Ok(f) = media::locate()
        && let Ok(c) = Contents::of(f.manifest.clone())
    {
        let (act, refused) = action(store_release, &c);
        out.push(Found {
            device: InstallDevice {
                name: "OPT01".into(),
                kind: DeviceKind::Optical,
                location: f.device.clone(),
                media: c.describe(),
                action: act,
                refused,
            },
            carrier: Carrier::Boot(f),
            contents: c,
        });
    }
    for (name, node) in tape_drives(Path::new("/sys")) {
        let Some(c) = open_drive(&node).ok().as_mut().and_then(probe) else {
            continue;
        };
        let (act, refused) = action(store_release, &c);
        out.push(Found {
            device: InstallDevice {
                name,
                kind: DeviceKind::Tape,
                location: node.display().to_string(),
                media: c.describe(),
                action: act,
                refused,
            },
            carrier: Carrier::Drive(node),
            contents: c,
        });
    }
    let boot = media::locate().ok().map(|f| f.disk);
    let (mut tapes, mut opts) = (0u8, 0u8);
    for d in diskimg::locate::whole_disks().unwrap_or_default() {
        if d.has_qs400 || boot.as_deref() == Some(d.name.as_str()) {
            continue;
        }
        let Some(h) = head(&d.dev, 8) else { continue };
        let (carrier, contents, name) = if h == tape::virt::format::MAGIC[..] {
            let Some(c) = tape_contents(&d.dev) else { continue };
            tapes += 1;
            (Carrier::Tape(d.dev.clone()), c, format!("TAPVRT{tapes:02}"))
        } else if let Some(c) = iso_contents(&d.dev) {
            opts += 1;
            (Carrier::Iso(d.dev.clone()), c, format!("OPTVRT{opts:02}"))
        } else {
            continue;
        };
        let (act, refused) = action(store_release, &contents);
        out.push(Found {
            device: InstallDevice {
                name,
                kind: DeviceKind::Virtual,
                location: d.dev.display().to_string(),
                media: contents.describe(),
                action: act,
                refused,
            },
            carrier,
            contents,
        });
    }
    *g = Some(out.clone());
    out
}

/// The disks that hold install media other than the boot media: not
/// load sources, not units.
pub fn media_disks() -> Vec<String> {
    let scanned = SCAN.lock().map_or(true, |g| g.is_some());
    if !scanned {
        scan(None, false);
    }
    let g = SCAN.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    g.as_ref()
        .map(|v| {
            v.iter()
                .filter_map(|f| match &f.carrier {
                    Carrier::Tape(p) | Carrier::Iso(p) => {
                        p.file_name().map(|n| n.to_string_lossy().into_owned())
                    }
                    Carrier::Boot(_) | Carrier::Drive(_) => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The device named `name`.
pub fn by_name(name: &str) -> Option<Found> {
    let g = SCAN.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    g.as_ref()?
        .iter()
        .find(|f| f.device.name == name)
        .cloned()
}

/// One payload from a device, checked against its manifest.
pub fn read(found: &Found, p: &Payload) -> Result<Vec<u8>, String> {
    let r = match &found.carrier {
        Carrier::Boot(f) => media::with_media(f, |at| {
            DirSource::open(at).and_then(|mut s| s.read(p))
        })?,
        Carrier::Tape(dev) => {
            let f = OpenOptions::new()
                .read(true)
                .write(true)
                .open(dev)
                .map_err(|e| format!("{}: {e}", dev.display()))?;
            let mut t = tape::VirtualTape::open(f).map_err(|e| e.to_string())?;
            TapeSource::open(&mut t).and_then(|mut s| s.read(p))
        }
        Carrier::Iso(dev) => {
            let f = File::open(dev).map_err(|e| format!("{}: {e}", dev.display()))?;
            let iso = optical::device::open_media(std::sync::Arc::new(f)).map_err(|e| e.to_string())?;
            IsoSource::open(iso).and_then(|mut s| s.read(p))
        }
        Carrier::Drive(node) => {
            let mut t = open_drive(node)?;
            TapeSource::open(&mut t).and_then(|mut s| s.read(p))
        }
    };
    r.map_err(|e| e.to_string())
}

/// The devices of one kind, for the panels.
pub fn devices(store_release: Option<release::Release>, kind: DeviceKind) -> Vec<InstallDevice> {
    scan(store_release, false)
        .into_iter()
        .filter(|f| f.device.kind == kind)
        .map(|f| f.device)
        .collect()
}

/// A `SAVSYS` medium, when one is attached: the recovery installs its
/// LIC and operating system.
pub fn savsys(store_release: Option<release::Release>) -> Option<Found> {
    scan(store_release, false)
        .into_iter()
        .find(|f| f.contents.savsys)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use instmedia::savsys::{Image, TapeWrite, write_tape};
    use tape::label::{Date, FileLabel, RecordFormat};
    use tape::volume::{Check, FileWriter, InzOptions, NoMoreVolumes, WriteOptions, inztap};
    use tape::{EndOpt, SeqNbr, VirtualTape, VolumeOptions};

    use super::*;

    fn today() -> Date {
        Date::from_ymd(2026, 10, 31).unwrap()
    }

    /// A labeled tape in memory holding a system save.
    fn saved() -> VirtualTape<Cursor<Vec<u8>>> {
        let mut t = VirtualTape::create(Cursor::new(Vec::new()), &VolumeOptions::imgsiz(4, "SYS001")).unwrap();
        inztap(
            &mut t,
            &InzOptions {
                new_vol: Some("SYS001".into()),
                check: Check::No,
                ..InzOptions::default()
            },
        )
        .unwrap();
        let img = Image::new(
            release::RELEASE,
            "SYS001",
            true,
            vec![
                (Payload::LicA, vec![1; 100]),
                (Payload::LicB, vec![2; 100]),
                (Payload::Product("QS00SS1".into()), vec![3; 1000]),
            ],
            None,
        );
        write_tape(&mut t, &img, &TapeWrite::new(today())).unwrap();
        t
    }

    fn append(t: &mut VirtualTape<Cursor<Vec<u8>>>, label: &str) {
        let opts = WriteOptions {
            vols: Vec::new(),
            seq: SeqNbr::End,
            label: FileLabel {
                file_id: label.into(),
                created: Some(today()),
                record_format: RecordFormat::Undefined,
                block_length: 512,
                record_length: 512,
                ..FileLabel::default()
            },
            unlabeled: false,
            today: Some(today()),
            allow_active: false,
            end_opt: EndOpt::Rewind,
        };
        let mut more = NoMoreVolumes;
        let mut w = FileWriter::open(t, &mut more, opts).unwrap();
        w.write_block(&[9; 512]).unwrap();
        w.finish().unwrap();
    }

    #[test]
    fn a_tape_of_savsys_media_is_found_with_its_user_data() {
        let mut t = saved();
        let c = probe(&mut t).unwrap();
        assert!(c.savsys && c.has_os());
        assert_eq!(user_count(&mut t), 0);
        append(&mut t, "PAYROLL");
        append(&mut t, "QSAVIFS");
        assert_eq!(user_count(&mut t), 2);
        // Still install media with the files after it.
        assert!(probe(&mut t).is_some());
        let (act, refused) = action(Some(release::RELEASE), &c);
        assert!(act.starts_with("Slip install") && refused.is_none());
    }

    #[test]
    fn a_tape_of_other_data_is_not_install_media() {
        let mut t = VirtualTape::create(Cursor::new(Vec::new()), &VolumeOptions::imgsiz(4, "DATA01")).unwrap();
        assert!(probe(&mut t).is_none());
        inztap(
            &mut t,
            &InzOptions {
                new_vol: Some("DATA01".into()),
                check: Check::No,
                ..InzOptions::default()
            },
        )
        .unwrap();
        append(&mut t, "PAYROLL");
        assert!(probe(&mut t).is_none());
    }

    #[test]
    fn drives_are_named_from_sysfs() {
        let root = std::env::temp_dir().join(format!("qs400-instdev-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let class = root.join("class/scsi_tape");
        for (node, hctl) in [("st1", "3:0:2:0"), ("st0", "2:0:0:0")] {
            let dev = root.join("devices").join(hctl);
            std::fs::create_dir_all(&dev).unwrap();
            std::fs::create_dir_all(class.join(node)).unwrap();
            std::os::unix::fs::symlink(&dev, class.join(node).join("device")).unwrap();
            // The no-rewind and mode nodes are not drives of their own.
            std::fs::create_dir_all(class.join(format!("n{node}"))).unwrap();
            std::fs::create_dir_all(class.join(format!("{node}a"))).unwrap();
        }
        let d = tape_drives(&root);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(
            d,
            vec![
                ("TAP01".to_string(), PathBuf::from("/dev/nst0")),
                ("TAP02".to_string(), PathBuf::from("/dev/nst1")),
            ]
        );
        assert!(tape_drives(Path::new("/nonexistent-sysfs")).is_empty());
    }
}
