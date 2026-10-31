// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The *TAPMLB model behind WRKTAPCTG: cartridges in categories, the
//! library's drive resources and their allocation, ADDTAPCTG, RMVTAPCTG
//! and CHGTAPCTG, and choosing a drive for VOL(volid) with the changer
//! moves that implies.

use std::collections::BTreeMap;

use crate::transport::{Changer, ChangerError, Element, ElementKind};

/// A cartridge category.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    /// *INSERT: in the library, not yet added.
    Insert,
    /// *EJECT: removed; in (or bound for) an import/export station.
    Eject,
    /// *SHARE400: available to every system sharing the library.
    Share400,
    /// *NOSHARE: available to this system only.
    NoShare,
    /// *CNV: convenience; ejected after one use.
    Cnv,
    /// *IPL: an alternate IPL volume.
    Ipl,
    /// *NL: unlabeled volumes.
    Nl,
    /// *SYSGEN: the library runs without categories.
    SysGen,
    /// A user-defined category (CRTTAPCGY).
    User(String),
}

impl Category {
    /// The parameter value.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Insert => "*INSERT",
            Self::Eject => "*EJECT",
            Self::Share400 => "*SHARE400",
            Self::NoShare => "*NOSHARE",
            Self::Cnv => "*CNV",
            Self::Ipl => "*IPL",
            Self::Nl => "*NL",
            Self::SysGen => "*SYSGEN",
            Self::User(n) => n,
        }
    }

    /// From a parameter value; anything without `*` is a user category.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "*INSERT" => Self::Insert,
            "*EJECT" => Self::Eject,
            "*SHARE400" => Self::Share400,
            "*NOSHARE" => Self::NoShare,
            "*CNV" => Self::Cnv,
            "*IPL" => Self::Ipl,
            "*NL" => Self::Nl,
            "*SYSGEN" => Self::SysGen,
            n if !n.is_empty() && !n.starts_with('*') && n.len() <= 10 => Self::User(n.into()),
            _ => return None,
        })
    }

    /// Whether a cartridge in this category can be mounted by VOL.
    #[must_use]
    pub fn usable(&self) -> bool {
        !matches!(self, Self::Insert | Self::Eject)
    }

    /// Whether ADDTAPCTG or CHGTAPCTG may put a cartridge here.
    #[must_use]
    pub fn assignable(&self) -> bool {
        !matches!(self, Self::Insert | Self::Eject | Self::SysGen)
    }
}

/// A drive resource's allocation in the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Allocation {
    /// *ALLOCATED: this system holds the drive.
    #[default]
    Allocated,
    /// *UNPROTECTED: used when needed, not held.
    Unprotected,
    /// *DEALLOCATED: not used by this system.
    Deallocated,
}

impl Allocation {
    /// The parameter value.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Allocated => "*ALLOCATED",
            Self::Unprotected => "*UNPROTECTED",
            Self::Deallocated => "*DEALLOCATED",
        }
    }
}

/// Where a cartridge is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    /// A storage slot.
    Slot(u16),
    /// A drive.
    Drive(u16),
    /// An import/export station.
    Station(u16),
    /// Out of the library (ejected and taken out).
    Outside,
}

/// One cartridge, as WRKTAPCTG lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cartridge {
    /// Cartridge identifier (the volume tag).
    pub id: String,
    /// Category.
    pub category: Category,
    /// Where it is.
    pub location: Location,
    /// The slot it returns to.
    pub home: Option<u16>,
}

/// A drive in the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveResource {
    /// `TAPnn`.
    pub name: String,
    /// Its element address.
    pub address: u16,
    /// Allocation.
    pub allocation: Allocation,
    /// The cartridge in it.
    pub cartridge: Option<String>,
}

/// A changer move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Move {
    /// Source element.
    pub from: u16,
    /// Destination element.
    pub to: u16,
}

/// The drive VOL(volid) selected and how to get the cartridge there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// The drive's resource name.
    pub drive: String,
    /// Moves, in order.
    pub moves: Vec<Move>,
}

/// A cartridge operation was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LibraryError {
    /// No such cartridge in the library.
    #[error("cartridge {0} is not in the library")]
    NotFound(String),
    /// The category does not allow the operation.
    #[error("cartridge {id} is in category {category}")]
    Category {
        /// The cartridge.
        id: String,
        /// Its category.
        category: String,
    },
    /// The category cannot be assigned.
    #[error("category {0} cannot be assigned")]
    BadCategory(String),
    /// The library runs without categories.
    #[error("library {0} uses category *SYSGEN")]
    SysGen(String),
    /// The cartridge is mounted.
    #[error("cartridge {0} is mounted")]
    InUse(String),
    /// No drive is allocated, or all are busy.
    #[error("no drive of library {0} is available")]
    NoDrive(String),
    /// No free element to put a cartridge in.
    #[error("no empty {0} element")]
    NoSpace(&'static str),
}

impl LibraryError {
    /// The IBM message identifier for the error.
    #[must_use]
    pub fn message_id(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "CPF6745",
            Self::Category { .. } | Self::BadCategory(_) | Self::SysGen(_) => "CPF67D2",
            Self::InUse(_) => "CPF6747",
            Self::NoDrive(_) | Self::NoSpace(_) => "CPF6749",
        }
    }
}

/// A *TAPMLB device's cartridges and drives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaLibrary {
    /// `TAPMLBnn`.
    pub name: String,
    /// The library runs without categories (every cartridge *SYSGEN).
    pub sysgen: bool,
    /// The drives.
    pub drives: Vec<DriveResource>,
    cartridges: BTreeMap<String, Cartridge>,
    stations: Vec<(u16, bool)>,
    slots: Vec<(u16, bool)>,
}

impl MediaLibrary {
    /// A library with these drives and no cartridges yet.
    #[must_use]
    pub fn new(name: &str, drives: Vec<DriveResource>, sysgen: bool) -> Self {
        Self {
            name: name.to_string(),
            sysgen,
            drives,
            cartridges: BTreeMap::new(),
            stations: Vec::new(),
            slots: Vec::new(),
        }
    }

    /// Every cartridge, by identifier.
    pub fn cartridges(&self) -> impl Iterator<Item = &Cartridge> {
        self.cartridges.values()
    }

    /// One cartridge.
    #[must_use]
    pub fn cartridge(&self, id: &str) -> Option<&Cartridge> {
        self.cartridges.get(id)
    }

    /// Take in the changer's element status: new tags arrive in
    /// *INSERT (or *SYSGEN), locations follow the elements, and
    /// cartridges no longer present are dropped unless *EJECT.
    pub fn sync(&mut self, elems: &[Element]) {
        self.slots.clear();
        self.stations.clear();
        for d in &mut self.drives {
            d.cartridge = None;
        }
        let mut seen = Vec::new();
        for e in elems {
            match e.kind {
                ElementKind::Storage => self.slots.push((e.address, e.full)),
                ElementKind::ImportExport => self.stations.push((e.address, e.full)),
                ElementKind::Drive | ElementKind::Transport => {}
            }
            let Some(tag) = e.volume_tag.as_deref().filter(|_| e.full) else {
                continue;
            };
            let tag = &cartridge_id(tag).to_string();
            let location = match e.kind {
                ElementKind::Storage => Location::Slot(e.address),
                ElementKind::Drive => Location::Drive(e.address),
                ElementKind::ImportExport => Location::Station(e.address),
                ElementKind::Transport => continue,
            };
            if let Some(d) = self.drives.iter_mut().find(|d| d.address == e.address) {
                d.cartridge = Some(tag.clone());
            }
            let default = if self.sysgen {
                Category::SysGen
            } else {
                Category::Insert
            };
            let c = self.cartridges.entry(tag.clone()).or_insert(Cartridge {
                id: tag.clone(),
                category: default,
                location,
                home: None,
            });
            c.location = location;
            match location {
                Location::Slot(a) => c.home = Some(a),
                Location::Drive(_) if c.home.is_none() => c.home = e.source,
                _ => {}
            }
            seen.push(tag.clone());
        }
        self.cartridges.retain(|id, c| {
            if seen.contains(id) {
                return true;
            }
            c.location = Location::Outside;
            c.category == Category::Eject
        });
    }

    fn get(&mut self, id: &str) -> Result<&mut Cartridge, LibraryError> {
        self.cartridges
            .get_mut(id)
            .ok_or_else(|| LibraryError::NotFound(id.to_string()))
    }

    fn assignable(&self, to: &Category) -> Result<(), LibraryError> {
        if self.sysgen {
            return Err(LibraryError::SysGen(self.name.clone()));
        }
        if !to.assignable() {
            return Err(LibraryError::BadCategory(to.name().to_string()));
        }
        Ok(())
    }

    /// ADDTAPCTG: a cartridge in *INSERT joins `to`.
    pub fn add(&mut self, id: &str, to: Category) -> Result<(), LibraryError> {
        self.assignable(&to)?;
        let c = self.get(id)?;
        if c.category != Category::Insert {
            return Err(LibraryError::Category {
                id: id.to_string(),
                category: c.category.name().to_string(),
            });
        }
        c.category = to;
        Ok(())
    }

    /// CHGTAPCTG: move a cartridge between assignable categories.
    pub fn change(&mut self, id: &str, to: Category) -> Result<(), LibraryError> {
        self.assignable(&to)?;
        let c = self.get(id)?;
        if !c.category.assignable() {
            return Err(LibraryError::Category {
                id: id.to_string(),
                category: c.category.name().to_string(),
            });
        }
        c.category = to;
        Ok(())
    }

    /// RMVTAPCTG: the cartridge goes to *EJECT; the move takes it to an
    /// empty import/export station (none when the library has none: it
    /// stays in its slot until the operator opens the library).
    pub fn remove(&mut self, id: &str) -> Result<Vec<Move>, LibraryError> {
        let station = self.stations.iter().find(|(_, full)| !full).map(|s| s.0);
        let c = self.get(id)?;
        let from = match c.location {
            Location::Drive(_) => return Err(LibraryError::InUse(id.to_string())),
            Location::Slot(a) | Location::Station(a) => a,
            Location::Outside => return Err(LibraryError::NotFound(id.to_string())),
        };
        c.category = Category::Eject;
        let moves = match (c.location, station) {
            (Location::Slot(_), Some(to)) => {
                c.location = Location::Station(to);
                vec![Move { from, to }]
            }
            _ => Vec::new(),
        };
        if let Some(s) = self.stations.iter_mut().find(|(a, _)| Some(*a) == station)
            && !moves.is_empty()
        {
            s.1 = true;
        }
        if !moves.is_empty()
            && let Some(s) = self.slots.iter_mut().find(|(a, _)| *a == from)
        {
            s.1 = false;
        }
        Ok(moves)
    }

    /// VOL(volid): the drive to use and the moves that mount the
    /// cartridge there. A drive already holding it is used as it is;
    /// otherwise the first available drive, empty ones first, has its
    /// cartridge returned home.
    pub fn select(&self, vol: &str) -> Result<Selection, LibraryError> {
        let c = self
            .cartridges
            .get(vol)
            .ok_or_else(|| LibraryError::NotFound(vol.to_string()))?;
        if !c.category.usable() {
            return Err(LibraryError::Category {
                id: vol.to_string(),
                category: c.category.name().to_string(),
            });
        }
        let usable = |d: &&DriveResource| d.allocation != Allocation::Deallocated;
        if let Location::Drive(a) = c.location {
            let d = self
                .drives
                .iter()
                .filter(usable)
                .find(|d| d.address == a)
                .ok_or_else(|| LibraryError::InUse(vol.to_string()))?;
            return Ok(Selection {
                drive: d.name.clone(),
                moves: Vec::new(),
            });
        }
        let from = match c.location {
            Location::Slot(a) | Location::Station(a) => a,
            Location::Drive(_) | Location::Outside => {
                return Err(LibraryError::NotFound(vol.to_string()));
            }
        };
        let drive = self
            .drives
            .iter()
            .filter(usable)
            .min_by_key(|d| (d.cartridge.is_some(), d.allocation != Allocation::Allocated))
            .ok_or_else(|| LibraryError::NoDrive(self.name.clone()))?;
        let mut moves = Vec::new();
        if let Some(other) = &drive.cartridge {
            let home = self
                .cartridges
                .get(other)
                .and_then(|o| o.home)
                .filter(|h| *h != from && self.slots.iter().any(|(a, full)| a == h && !full))
                .or_else(|| self.slots.iter().find(|(_, full)| !full).map(|s| s.0))
                .ok_or(LibraryError::NoSpace("storage"))?;
            moves.push(Move {
                from: drive.address,
                to: home,
            });
        }
        moves.push(Move {
            from,
            to: drive.address,
        });
        Ok(Selection {
            drive: drive.name.clone(),
            moves,
        })
    }

    /// The moves after a cartridge in drive `drive` is unloaded: home to
    /// its slot, or out to a station (and *EJECT) for *CNV.
    pub fn release(&mut self, drive: &str) -> Result<Vec<Move>, LibraryError> {
        let d = self
            .drives
            .iter()
            .find(|d| d.name == drive)
            .ok_or_else(|| LibraryError::NoDrive(drive.to_string()))?;
        let Some(id) = d.cartridge.clone() else {
            return Ok(Vec::new());
        };
        let from = d.address;
        let station = self.stations.iter().find(|(_, full)| !full).map(|s| s.0);
        let free_slot = self.slots.iter().find(|(_, full)| !full).map(|s| s.0);
        let c = self.get(&id)?;
        let to = if let (Category::Cnv, Some(s)) = (&c.category, station) {
            c.category = Category::Eject;
            s
        } else {
            c.home
                .or(free_slot)
                .ok_or(LibraryError::NoSpace("storage"))?
        };
        Ok(vec![Move { from, to }])
    }
}

/// The cartridge identifier for a volume tag: its first six characters
/// (an LTO bar code `E01001L5` is cartridge `E01001`).
#[must_use]
pub fn cartridge_id(tag: &str) -> &str {
    tag.char_indices().nth(6).map_or(tag, |(i, _)| &tag[..i])
}

/// Run moves on a changer.
pub fn execute<C: Changer + ?Sized>(changer: &mut C, moves: &[Move]) -> Result<(), ChangerError> {
    for m in moves {
        changer.move_medium(m.from, m.to)?;
    }
    Ok(())
}
