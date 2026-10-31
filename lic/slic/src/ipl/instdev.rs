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
//! - **Tape**: `BLOCKED(phase11)`: physical drives come with the device
//!   layer; none are listed yet.
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
                    Carrier::Boot(_) => None,
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
