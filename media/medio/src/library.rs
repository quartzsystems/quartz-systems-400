// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A virtual media library over a `*TAP` image catalog: the catalog's
//! loaded entries are its cartridges, in [`VirtualLibrary`] slots,
//! stations and drives, with the `tape::device::MediaLibrary` model's
//! categories and moves. Between commands the library is kept as text
//! ([`LibraryState`], the catalog directory's `QS400MLB.INF`) and built
//! again from the entries and that state.

use std::fmt::Write as _;
use std::io::{Read, Seek, Write};

use tape::device::{
    Allocation, Cartridge, Category, DriveResource, Location, MediaLibrary, Move, cartridge_id,
    execute,
};
use tape::virt::{SLOT_BASE, STATION_BASE};
use tape::volume::LibraryProvider;
use tape::{
    Changer, TapeError, TapeTransport, TransportError, VirtualLibrary, VirtualTape, VolumeProvider,
    VolumeRequest,
};

use crate::catalog::{Asker, NextVolume};
use crate::{MediaError, Result};

/// Drives of a virtual library.
pub const DRIVES: usize = 2;
/// Import/export stations of a virtual library.
pub const STATIONS: usize = 4;
/// Storage slots: one per catalog index.
pub const SLOTS: usize = 256;
/// The state file in the catalog's directory.
pub const STATE_FILE: &str = "QS400MLB.INF";

const MAGIC: &str = "QS400-MLB 1";

/// What a library keeps between commands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LibraryState {
    /// Every cartridge the library knows, ejected ones included.
    pub cartridges: Vec<Cartridge>,
    /// Each drive's allocation, in drive order.
    pub allocation: Vec<Allocation>,
}

fn location_text(l: Location) -> String {
    match l {
        Location::Slot(a) => format!("slot:{a}"),
        Location::Drive(a) => format!("drive:{a}"),
        Location::Station(a) => format!("station:{a}"),
        Location::Outside => "outside".into(),
    }
}

fn parse_location(s: &str) -> Option<Location> {
    if s == "outside" {
        return Some(Location::Outside);
    }
    let (k, a) = s.split_once(':')?;
    let a: u16 = a.parse().ok()?;
    Some(match k {
        "slot" => Location::Slot(a),
        "drive" => Location::Drive(a),
        "station" => Location::Station(a),
        _ => return None,
    })
}

fn parse_allocation(s: &str) -> Option<Allocation> {
    Some(match s {
        "*ALLOCATED" => Allocation::Allocated,
        "*UNPROTECTED" => Allocation::Unprotected,
        "*DEALLOCATED" => Allocation::Deallocated,
        _ => return None,
    })
}

impl LibraryState {
    /// The state as text: the magic line, one `drive` line per drive,
    /// one `ctg` line per cartridge.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut s = format!("{MAGIC}\n");
        for (i, a) in self.allocation.iter().enumerate() {
            let _ = writeln!(s, "drive\t{}\t{}", i + 1, a.name());
        }
        for c in &self.cartridges {
            let home = c.home.map_or_else(|| "-".to_string(), |h| h.to_string());
            let _ = writeln!(
                s,
                "ctg\t{}\t{}\t{}\t{home}",
                c.id,
                c.category.name(),
                location_text(c.location)
            );
        }
        s
    }

    /// Read the text back; lines it does not know are skipped.
    pub fn parse(text: &str) -> Result<Self> {
        let mut lines = text.lines();
        if lines.next().map(str::trim_end) != Some(MAGIC) {
            return Err(MediaError::State("not a media library state file".into()));
        }
        let mut st = Self::default();
        for l in lines {
            let f: Vec<&str> = l.split('\t').collect();
            match f.as_slice() {
                ["drive", _, a] => {
                    st.allocation.push(parse_allocation(a).unwrap_or_default());
                }
                ["ctg", id, cgy, loc, home] => {
                    let bad = || MediaError::State(format!("bad cartridge line {l:?}"));
                    st.cartridges.push(Cartridge {
                        id: (*id).to_string(),
                        category: Category::parse(cgy).ok_or_else(bad)?,
                        location: parse_location(loc).ok_or_else(bad)?,
                        home: home.parse().ok(),
                    });
                }
                _ => {}
            }
        }
        Ok(st)
    }
}

/// The drive resource names of a virtual library over resource `rsrc`:
/// `TAPVRT03D1`, `TAPVRT03D2`.
#[must_use]
pub fn drive_names(rsrc: &str) -> Vec<String> {
    (1..=DRIVES).map(|n| format!("{rsrc}D{n}")).collect()
}

/// A virtual library ready for one command.
pub struct CatalogLibrary<B> {
    /// The changer and drives, holding the volumes.
    pub changer: VirtualLibrary<B>,
    /// Categories, locations and allocation.
    pub model: MediaLibrary,
}

impl<B> std::fmt::Debug for CatalogLibrary<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CatalogLibrary")
            .field("model", &self.model)
            .finish_non_exhaustive()
    }
}

fn slot_address(i: usize) -> u16 {
    SLOT_BASE + u16::try_from(i).unwrap_or(u16::MAX - SLOT_BASE)
}

impl<B: Read + Write + Seek> CatalogLibrary<B> {
    /// Library `name` over resource `rsrc`: the catalog's loaded entries
    /// as `(index, volume)` and the state kept last time. Each volume
    /// goes back where the state had it, else to its index's slot.
    pub fn open(
        name: &str,
        rsrc: &str,
        tapes: Vec<(u16, VirtualTape<B>)>,
        state: &LibraryState,
    ) -> Result<Self> {
        let mut changer = VirtualLibrary::new(SLOTS, DRIVES, STATIONS);
        let mut later = Vec::new();
        for (index, tape) in tapes {
            let id = cartridge_id(tape.tag()).to_string();
            let kept = state.cartridges.iter().find(|c| c.id == id);
            let at = kept.and_then(|c| match c.location {
                Location::Slot(a) | Location::Drive(a) | Location::Station(a) => Some(a),
                Location::Outside => None,
            });
            match at.filter(|a| changer.is_empty_at(*a)) {
                Some(a) => {
                    let home = kept.and_then(|c| c.home).unwrap_or(a);
                    changer.put(a, tape, home).map_err(TapeError::from)?;
                }
                None => later.push((index, tape)),
            }
        }
        for (index, tape) in later {
            let want = slot_address(usize::from(index.max(1)) - 1);
            let a = if changer.is_empty_at(want) {
                want
            } else {
                (0..SLOTS)
                    .map(slot_address)
                    .find(|a| changer.is_empty_at(*a))
                    .ok_or(TapeError::Changer(tape::ChangerError::DestinationFull(want)))?
            };
            changer.put(a, tape, a).map_err(TapeError::from)?;
        }
        let drives = drive_names(rsrc)
            .into_iter()
            .enumerate()
            .map(|(i, n)| DriveResource {
                name: n,
                address: changer.drive_address(i),
                allocation: state.allocation.get(i).copied().unwrap_or_default(),
                cartridge: None,
            })
            .collect();
        let mut model = MediaLibrary::new(name, drives, false);
        for c in &state.cartridges {
            model.restore(c.clone());
        }
        let mut lib = Self { changer, model };
        lib.sync()?;
        Ok(lib)
    }

    /// The model in line with the elements.
    pub fn sync(&mut self) -> Result<()> {
        let e = self.changer.read_element_status().map_err(TapeError::from)?;
        self.model.sync(&e);
        Ok(())
    }

    /// What to keep.
    #[must_use]
    pub fn state(&self) -> LibraryState {
        LibraryState {
            cartridges: self.model.cartridges().cloned().collect(),
            allocation: self.model.drives.iter().map(|d| d.allocation).collect(),
        }
    }

    fn run(&mut self, moves: &[Move]) -> Result<()> {
        execute(&mut self.changer, moves).map_err(TapeError::from)?;
        self.sync()
    }

    /// ADDTAPCTG.
    pub fn add(&mut self, id: &str, to: Category) -> Result<()> {
        Ok(self.model.add(id, to)?)
    }

    /// CHGTAPCTG.
    pub fn change(&mut self, id: &str, to: Category) -> Result<()> {
        Ok(self.model.change(id, to)?)
    }

    /// RMVTAPCTG: to *EJECT and a station; the moves made.
    pub fn remove(&mut self, id: &str) -> Result<Vec<Move>> {
        let moves = self.model.remove(id)?;
        self.run(&moves)?;
        // The model already says *EJECT; sync keeps it.
        Ok(moves)
    }

    /// Take a cartridge in a station out of the library (the operator
    /// empties the station): it is dropped from the elements.
    pub fn take_out(&mut self, id: &str) -> Option<VirtualTape<B>> {
        let a = self.changer.find(id)?;
        let i = usize::from(a.checked_sub(STATION_BASE)?);
        if i >= STATIONS {
            return None;
        }
        let t = self.changer.take(i);
        let _ = self.sync();
        t
    }

    /// Set a drive's allocation.
    pub fn allocate(&mut self, drive: &str, a: Allocation) -> Result<()> {
        let d = self
            .model
            .drives
            .iter_mut()
            .find(|d| d.name.eq_ignore_ascii_case(drive))
            .ok_or_else(|| tape::device::LibraryError::NoDrive(drive.to_string()))?;
        d.allocation = a;
        Ok(())
    }

    /// The drive holding a cartridge (the first, for VOL(*MOUNTED)):
    /// its index and the cartridge.
    #[must_use]
    pub fn mounted(&self) -> Option<(usize, String)> {
        self.model
            .drives
            .iter()
            .enumerate()
            .filter(|(_, d)| d.allocation != Allocation::Deallocated)
            .find_map(|(i, d)| d.cartridge.clone().map(|c| (i, c)))
    }

    fn drive_index(&self, name: &str) -> Option<usize> {
        self.model.drives.iter().position(|d| d.name == name)
    }

    /// VOL(volid): the cartridge into a drive (the drive's cartridge sent
    /// home first); the drive's index, now the transport.
    pub fn mount(&mut self, vol: &str) -> Result<usize> {
        let sel = self.model.select(vol)?;
        let i = self
            .drive_index(&sel.drive)
            .ok_or_else(|| tape::device::LibraryError::NoDrive(sel.drive.clone()))?;
        self.changer.select_drive(i).map_err(TapeError::from)?;
        if !sel.moves.is_empty() {
            unload_quietly(&mut self.changer)?;
        }
        self.run(&sel.moves)?;
        Ok(i)
    }

    /// Send the cartridge in drive `i` home (or out, for *CNV); whether
    /// one was there.
    pub fn demount(&mut self, i: usize) -> Result<bool> {
        let Some(name) = self.model.drives.get(i).map(|d| d.name.clone()) else {
            return Ok(false);
        };
        self.changer.select_drive(i).map_err(TapeError::from)?;
        unload_quietly(&mut self.changer)?;
        // A *CNV cartridge goes out and becomes *EJECT in the model.
        let moves = self.model.release(&name)?;
        if moves.is_empty() {
            return Ok(false);
        }
        self.run(&moves)?;
        Ok(true)
    }

    /// After a command on drive `i`: a cartridge the command unloaded
    /// (ENDOPT(*UNLOAD)) goes home; others stay mounted. Whether it went.
    pub fn after(&mut self, i: usize) -> Result<bool> {
        self.sync()?;
        let loaded = self.changer.drive(i).is_some_and(|t| t.is_loaded());
        if loaded || self.model.drives.get(i).and_then(|d| d.cartridge.as_ref()).is_none() {
            return Ok(false);
        }
        self.demount(i)
    }

    /// The cartridges a VOL(*MOUNTED) operation may go on to, in slot
    /// order: usable categories, not in a drive.
    #[must_use]
    pub fn spares(&self) -> Vec<String> {
        let mut v: Vec<&Cartridge> = self
            .model
            .cartridges()
            .filter(|c| c.category.usable() && matches!(c.location, Location::Slot(_)))
            .collect();
        v.sort_by_key(|c| match c.location {
            Location::Slot(a) => a,
            _ => u16::MAX,
        });
        v.into_iter().map(|c| c.id.clone()).collect()
    }

    /// Whether cartridge `id` may be mounted (a usable category).
    pub fn check_usable(&self, id: &str) -> Result<()> {
        let c = self
            .model
            .cartridge(id)
            .ok_or_else(|| tape::device::LibraryError::NotFound(id.to_string()))?;
        if c.category.usable() && c.location != Location::Outside {
            Ok(())
        } else {
            Err(tape::device::LibraryError::Category {
                id: id.to_string(),
                category: c.category.name().to_string(),
            }
            .into())
        }
    }
}

fn unload_quietly<D: TapeTransport + ?Sized>(d: &mut D) -> Result<()> {
    match d.unload() {
        Ok(()) | Err(TransportError::NotLoaded) => Ok(()),
        Err(e) => Err(TapeError::from(e).into()),
    }
}

/// The further volumes of a library operation: a volume the `VOL` list
/// names is moved in; for `*MOUNTED` the operator is asked, and a retry
/// reply takes the next spare cartridge.
pub struct LibraryVolumes<'a> {
    inner: LibraryProvider,
    pick: NextVolume,
    ask: Asker<'a>,
}

impl<'a> LibraryVolumes<'a> {
    /// For drive `drive`, the spare cartridges, the one mounted now and
    /// whom to ask.
    #[must_use]
    pub fn new(drive: usize, spares: Vec<String>, current: Option<String>, ask: Asker<'a>) -> Self {
        Self {
            inner: LibraryProvider {
                drive,
                ..LibraryProvider::default()
            },
            pick: NextVolume::new(spares, current, false),
            ask,
        }
    }

    /// Every volume moved into the drive, in order.
    #[must_use]
    pub fn mounted(&self) -> &[String] {
        &self.inner.mounted
    }
}

impl std::fmt::Debug for LibraryVolumes<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LibraryVolumes")
            .field("inner", &self.inner)
            .field("pick", &self.pick)
            .finish_non_exhaustive()
    }
}

impl<L: Changer + TapeTransport + ?Sized> VolumeProvider<L> for LibraryVolumes<'_> {
    fn next_volume(&mut self, lib: &mut L, req: &VolumeRequest) -> std::result::Result<(), TapeError> {
        if req.expected.is_some() {
            self.inner.next_volume(lib, req)?;
            self.pick.used(&req.expected.clone().unwrap_or_default());
            return Ok(());
        }
        // No volume named: the operator decides.
        if !(self.ask)(req) {
            return Err(TapeError::Cancelled);
        }
        let Some(v) = self.pick.pick(None, true) else {
            return Err(TapeError::Cancelled);
        };
        let named = VolumeRequest {
            expected: Some(v.clone()),
            ..req.clone()
        };
        self.inner.next_volume(lib, &named)?;
        self.pick.used(&v);
        Ok(())
    }
}
