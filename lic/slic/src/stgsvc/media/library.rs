// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Virtual media libraries: a `*TAPMLB` device made `RSRCNAME(*VRT)`
//! is the `*TAP` image catalog loaded on it, its loaded entries the
//! cartridges (`medio::CatalogLibrary`). The cartridges' categories,
//! places and the drives' allocation are kept in the catalog directory's
//! `QS400MLB.INF` between commands. A real library's changer is not
//! driven in this release.

use std::fs::File;

use medio::library::{CatalogLibrary, LibraryState, STATE_FILE};
use tape::device::{Category, Location};
use tape::virt::{DRIVE_BASE, SLOT_BASE, STATION_BASE};
use tape::{TapeTransport, VirtualTape};
use wm::obj::devmed::MediaKind;

use super::catalogs::{host_path, open_image};
use super::drive::{self, NOT_READY, Target};
use crate::ipl::system::System;

/// A refusal of the library layer as `msgid text`.
pub(crate) fn refused(e: &medio::MediaError) -> String {
    format!("{} {e}", e.msgid())
}

/// The library device `dev`, varied on and virtual, and its catalog.
fn library_target(system: &mut System, dev: &str) -> Result<(Target, imgclg::Catalog), String> {
    let t = drive::target(system, dev)?;
    if t.kind != MediaKind::TapeLibrary {
        return Err(format!("CPF6745 device {} is not a tape media library", t.dev));
    }
    if !t.virtual_ {
        return Err(format!(
            "CPF6745 device {}: the changer of a real media library is not driven in this release",
            t.dev
        ));
    }
    let c = super::catalog_on(&t.dev)
        .ok_or_else(|| format!("{NOT_READY} device {} not ready: no image catalog is loaded", t.dev))?;
    if c.kind != imgclg::ImgType::Tap {
        return Err(format!("{NOT_READY} device {} not ready: image catalog {} is not a tape catalog", t.dev, c.name));
    }
    Ok((t, c))
}

fn state_path(c: &imgclg::Catalog) -> std::path::PathBuf {
    host_path(&imgclg::fs::join(&c.dir, STATE_FILE))
}

/// The library over the catalog's loaded entries, as last kept.
fn open(t: &Target, c: &imgclg::Catalog) -> Result<CatalogLibrary<File>, String> {
    let mut tapes = Vec::new();
    for e in c.entries.iter().filter(|e| e.status != imgclg::EntryStatus::Available) {
        let path = c.path_of(e);
        let f = open_image(&path).map_err(|x| format!("CPF6708 {path}: {x}"))?;
        let mut v = VirtualTape::open(f).map_err(|x| format!("CPF6772 {path}: {x}"))?;
        if v.header().write_protect != e.write_protect {
            let _ = v.set_write_protect(e.write_protect);
        }
        tapes.push((e.index, v));
    }
    let state = match std::fs::read_to_string(state_path(c)) {
        Ok(text) => LibraryState::parse(&text).map_err(|e| refused(&e))?,
        Err(_) => LibraryState::default(),
    };
    CatalogLibrary::open(&t.dev, &t.rsrc, tapes, &state).map_err(|e| refused(&e))
}

fn keep(c: &imgclg::Catalog, lib: &CatalogLibrary<File>) {
    if let Err(e) = std::fs::write(state_path(c), lib.state().to_text()) {
        eprintln!("qsstg: media library {}: {e}", c.name);
    }
}

/// Run `f` on library `dev`; the library is kept afterwards whatever
/// `f` gave.
pub(crate) fn with_library<T>(
    system: &mut System,
    dev: &str,
    f: impl FnOnce(&mut System, &Target, &mut CatalogLibrary<File>) -> Result<T, String>,
) -> Result<T, String> {
    let (t, c) = library_target(system, dev)?;
    let mut lib = open(&t, &c)?;
    let r = f(system, &t, &mut lib);
    keep(&c, &lib);
    r
}

/// One cartridge as `WRKTAPCTG` and `DSPTAPCTG` show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CtgRow {
    /// The cartridge identifier.
    pub id: String,
    /// The volume identifier its label carries (`*NL`, `*BLANK`).
    pub volume: String,
    /// The category.
    pub category: String,
    /// Inserted, Available, Mounted or Ejected.
    pub status: String,
    /// The slot, drive or station; `*NONE` outside.
    pub location: String,
    /// The home slot.
    pub home: String,
}

fn slot_no(a: u16) -> u16 {
    a - SLOT_BASE + 1
}

/// Where a cartridge is, in words.
fn place(lib: &CatalogLibrary<File>, l: Location) -> String {
    match l {
        Location::Slot(a) => format!("Slot {}", slot_no(a)),
        Location::Station(a) => format!("Station {}", a - STATION_BASE + 1),
        Location::Drive(a) => lib
            .model
            .drives
            .iter()
            .find(|d| d.address == a)
            .map_or_else(|| format!("Drive {}", a - DRIVE_BASE + 1), |d| d.name.clone()),
        Location::Outside => "*NONE".into(),
    }
}

/// The cartridges of library `dev` that `ids` (generic, empty for all)
/// and `cgy` (`*ALL` for any) select.
pub(crate) fn rows(system: &mut System, dev: &str, ids: &[String], cgy: &str) -> Result<Vec<CtgRow>, String> {
    let (_, c) = library_target(system, dev)?;
    let volumes = labels(&c);
    with_library(system, dev, |_, _, lib| {
        let mut out = Vec::new();
        let carts: Vec<_> = lib.model.cartridges().cloned().collect();
        for k in carts {
            let pick = ids.is_empty()
                || ids.iter().any(|g| g == "*ALL" || *g == k.id || g.strip_suffix('*').is_some_and(|p| k.id.starts_with(p)));
            if !pick || !(cgy.is_empty() || cgy == "*ALL" || cgy == k.category.name()) {
                continue;
            }
            let status = match (&k.category, k.location) {
                (Category::Eject, _) | (_, Location::Outside) => "Ejected",
                (Category::Insert, _) => "Inserted",
                (_, Location::Drive(_)) => "Mounted",
                _ => "Available",
            };
            out.push(CtgRow {
                volume: volumes
                    .iter()
                    .find(|(t, _)| *t == k.id)
                    .map_or_else(|| "*UNKNOWN".to_string(), |(_, v)| v.clone()),
                category: k.category.name().to_string(),
                status: status.into(),
                location: place(lib, k.location),
                home: k.home.map_or_else(|| "*NONE".to_string(), |h| slot_no(h).to_string()),
                id: k.id,
            });
        }
        Ok(out)
    })
}

/// Each loaded entry's cartridge identifier and label volume.
fn labels(c: &imgclg::Catalog) -> Vec<(String, String)> {
    let mut v = Vec::new();
    for e in c.entries.iter().filter(|e| e.status != imgclg::EntryStatus::Available) {
        let Ok(f) = open_image(&c.path_of(e)) else { continue };
        let Ok(mut t) = VirtualTape::open(f) else { continue };
        let tag = tape::device::cartridge_id(t.tag()).to_string();
        let vol = tape::volume::identify(&mut t).map_or_else(|_| "*UNKNOWN".to_string(), |k| k.id());
        let _ = t.rewind();
        v.push((tag, vol));
    }
    v
}

/// The cartridge's label volume, for `ADDTAPCTG CHECK(*YES)`.
pub(crate) fn label_of(system: &mut System, dev: &str, id: &str) -> Result<Option<String>, String> {
    let (_, c) = library_target(system, dev)?;
    Ok(labels(&c).into_iter().find(|(t, _)| t == id).map(|(_, v)| v))
}

/// One drive of a library as `WRKMLBSTS` shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DriveRow {
    /// The resource.
    pub name: String,
    /// `*ALLOCATED`, `*UNPROTECTED`, `*DEALLOCATED`.
    pub allocation: String,
    /// The cartridge in it, or empty.
    pub cartridge: String,
}

/// The drives of library `dev`.
pub(crate) fn drives(system: &mut System, dev: &str) -> Result<Vec<DriveRow>, String> {
    with_library(system, dev, |_, _, lib| {
        Ok(lib
            .model
            .drives
            .iter()
            .map(|d| DriveRow {
                name: d.name.clone(),
                allocation: d.allocation.name().to_string(),
                cartridge: d.cartridge.clone().unwrap_or_default(),
            })
            .collect())
    })
}
