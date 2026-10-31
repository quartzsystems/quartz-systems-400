// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! [`VirtualLibrary`]: virtual volumes in slots, import/export stations
//! and drives, moved by a [`Changer`]. Volume tags are the volumes' tags.
//!
//! As a [`TapeTransport`] the library is its selected drive, so a
//! [`crate::VolumeProvider`] handed the library can change the volume
//! the label layer is writing to.

use std::io::{Read, Seek, Write};

use super::VirtualTape;
use crate::transport::{
    Changer, ChangerError, Element, ElementKind, ReadOutcome, Status, TapeTransport,
    TransportError, Written,
};

/// Element address of the medium transport.
pub const TRANSPORT_ADDRESS: u16 = 0;
/// First drive element address.
pub const DRIVE_BASE: u16 = 0x100;
/// First import/export element address.
pub const STATION_BASE: u16 = 0x200;
/// First storage slot element address.
pub const SLOT_BASE: u16 = 0x400;

#[derive(Debug)]
struct Loaded<B> {
    tape: VirtualTape<B>,
    source: u16,
}

/// A virtual media library.
#[derive(Debug)]
pub struct VirtualLibrary<B> {
    slots: Vec<Option<Loaded<B>>>,
    stations: Vec<Option<Loaded<B>>>,
    drives: Vec<Option<Loaded<B>>>,
    current: usize,
}

/// Where an element address points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    Slot(usize),
    Station(usize),
    Drive(usize),
}

fn index(addr: u16, base: u16, count: usize) -> Option<usize> {
    let i = usize::from(addr.checked_sub(base)?);
    (i < count).then_some(i)
}

fn address(base: u16, i: usize) -> u16 {
    base + u16::try_from(i).expect("element index")
}

impl<B: Read + Write + Seek> VirtualLibrary<B> {
    /// An empty library.
    #[must_use]
    pub fn new(slots: usize, drives: usize, stations: usize) -> Self {
        let empty = |n| (0..n).map(|_| None).collect();
        Self {
            slots: empty(slots),
            stations: empty(stations),
            drives: empty(drives.max(1)),
            current: 0,
        }
    }

    /// Put a volume in a storage slot (0-based), as the catalog loads it.
    pub fn insert(&mut self, slot: usize, tape: VirtualTape<B>) -> Result<u16, ChangerError> {
        let addr = address(SLOT_BASE, slot);
        let cell = self
            .slots
            .get_mut(slot)
            .ok_or(ChangerError::NoElement(addr))?;
        if cell.is_some() {
            return Err(ChangerError::DestinationFull(addr));
        }
        *cell = Some(Loaded { tape, source: addr });
        Ok(addr)
    }

    /// Put a volume in an import/export station, as an operator does.
    pub fn import(&mut self, station: usize, tape: VirtualTape<B>) -> Result<u16, ChangerError> {
        let addr = address(STATION_BASE, station);
        let cell = self
            .stations
            .get_mut(station)
            .ok_or(ChangerError::NoElement(addr))?;
        if cell.is_some() {
            return Err(ChangerError::DestinationFull(addr));
        }
        *cell = Some(Loaded { tape, source: addr });
        Ok(addr)
    }

    /// Put a volume back at element `addr` as a library kept it: in a
    /// slot, a station or a drive (loaded), `source` its home slot.
    pub fn put(&mut self, addr: u16, mut tape: VirtualTape<B>, source: u16) -> Result<(), ChangerError> {
        let p = self.place(addr)?;
        if self.cell(p).is_some() {
            return Err(ChangerError::DestinationFull(addr));
        }
        if let Place::Drive(_) = p {
            tape.load();
        }
        *self.cell(p) = Some(Loaded { tape, source });
        Ok(())
    }

    /// Whether element `addr` is empty (and exists).
    #[must_use]
    pub fn is_empty_at(&self, addr: u16) -> bool {
        match self.place(addr) {
            Ok(Place::Slot(i)) => self.slots[i].is_none(),
            Ok(Place::Station(i)) => self.stations[i].is_none(),
            Ok(Place::Drive(i)) => self.drives[i].is_none(),
            Err(_) => false,
        }
    }

    /// The number of drives.
    #[must_use]
    pub fn drives(&self) -> usize {
        self.drives.len()
    }

    /// Take a volume out of an import/export station.
    pub fn take(&mut self, station: usize) -> Option<VirtualTape<B>> {
        self.stations.get_mut(station)?.take().map(|l| l.tape)
    }

    /// Select the drive (0-based) the library's transport is.
    pub fn select_drive(&mut self, drive: usize) -> Result<(), ChangerError> {
        if drive >= self.drives.len() {
            return Err(ChangerError::NoElement(address(DRIVE_BASE, drive)));
        }
        self.current = drive;
        Ok(())
    }

    /// The element address of drive `n`.
    #[must_use]
    pub fn drive_address(&self, n: usize) -> u16 {
        address(DRIVE_BASE, n)
    }

    /// The volume in drive `n`.
    pub fn drive(&mut self, n: usize) -> Option<&mut VirtualTape<B>> {
        self.drives.get_mut(n)?.as_mut().map(|l| &mut l.tape)
    }

    /// The element holding the volume tagged `tag`.
    #[must_use]
    pub fn find(&self, tag: &str) -> Option<u16> {
        let lists = [
            (SLOT_BASE, &self.slots),
            (STATION_BASE, &self.stations),
            (DRIVE_BASE, &self.drives),
        ];
        lists.into_iter().find_map(|(base, list)| {
            list.iter()
                .position(|c| c.as_ref().is_some_and(|l| l.tape.tag() == tag))
                .map(|i| address(base, i))
        })
    }

    fn place(&self, addr: u16) -> Result<Place, ChangerError> {
        index(addr, SLOT_BASE, self.slots.len())
            .map(Place::Slot)
            .or_else(|| index(addr, STATION_BASE, self.stations.len()).map(Place::Station))
            .or_else(|| index(addr, DRIVE_BASE, self.drives.len()).map(Place::Drive))
            .ok_or(ChangerError::NoElement(addr))
    }

    fn cell(&mut self, p: Place) -> &mut Option<Loaded<B>> {
        match p {
            Place::Slot(i) => &mut self.slots[i],
            Place::Station(i) => &mut self.stations[i],
            Place::Drive(i) => &mut self.drives[i],
        }
    }

    fn selected(&mut self) -> Result<&mut VirtualTape<B>, TransportError> {
        self.drives[self.current]
            .as_mut()
            .map(|l| &mut l.tape)
            .ok_or(TransportError::NotLoaded)
    }
}

impl<B: Read + Write + Seek> Changer for VirtualLibrary<B> {
    fn move_medium(&mut self, from: u16, to: u16) -> Result<(), ChangerError> {
        let (src, dst) = (self.place(from)?, self.place(to)?);
        if self.cell(src).is_none() {
            return Err(ChangerError::SourceEmpty(from));
        }
        if src != dst && self.cell(dst).is_some() {
            return Err(ChangerError::DestinationFull(to));
        }
        if let Place::Drive(_) = src
            && self.cell(src).as_ref().is_some_and(|l| l.tape.is_loaded())
        {
            // Medium removal prevented: the drive still holds it.
            return Err(ChangerError::Check {
                key: 0x5,
                asc: 0x53,
                ascq: 0x02,
            });
        }
        let mut item = self.cell(src).take().expect("checked");
        if !matches!(src, Place::Drive(_)) {
            item.source = from;
        }
        if let Place::Drive(_) = dst {
            item.tape.load();
        }
        *self.cell(dst) = Some(item);
        Ok(())
    }

    fn read_element_status(&mut self) -> Result<Vec<Element>, ChangerError> {
        let mut out = vec![Element {
            kind: ElementKind::Transport,
            address: TRANSPORT_ADDRESS,
            full: false,
            volume_tag: None,
            source: None,
        }];
        let lists = [
            (ElementKind::Drive, DRIVE_BASE, &self.drives),
            (ElementKind::ImportExport, STATION_BASE, &self.stations),
            (ElementKind::Storage, SLOT_BASE, &self.slots),
        ];
        for (kind, base, list) in lists {
            for (i, c) in list.iter().enumerate() {
                out.push(Element {
                    kind,
                    address: address(base, i),
                    full: c.is_some(),
                    volume_tag: c.as_ref().map(|l| l.tape.tag().to_string()),
                    source: c.as_ref().map(|l| l.source),
                });
            }
        }
        Ok(out)
    }
}

impl<B: Read + Write + Seek> TapeTransport for VirtualLibrary<B> {
    fn rewind(&mut self) -> Result<(), TransportError> {
        self.selected()?.rewind()
    }
    fn fsf(&mut self, n: u32) -> Result<(), TransportError> {
        self.selected()?.fsf(n)
    }
    fn bsf(&mut self, n: u32) -> Result<(), TransportError> {
        self.selected()?.bsf(n)
    }
    fn fsr(&mut self, n: u32) -> Result<(), TransportError> {
        self.selected()?.fsr(n)
    }
    fn bsr(&mut self, n: u32) -> Result<(), TransportError> {
        self.selected()?.bsr(n)
    }
    fn eom(&mut self) -> Result<(), TransportError> {
        self.selected()?.eom()
    }
    fn write_eof(&mut self, n: u32) -> Result<(), TransportError> {
        self.selected()?.write_eof(n)
    }
    fn erase(&mut self) -> Result<(), TransportError> {
        self.selected()?.erase()
    }
    fn unload(&mut self) -> Result<(), TransportError> {
        self.selected()?.unload()
    }
    fn set_block_size(&mut self, size: u32) -> Result<(), TransportError> {
        self.selected()?.set_block_size(size)
    }
    fn set_density(&mut self, density: &str) -> Result<(), TransportError> {
        self.selected()?.set_density(density)
    }
    fn status(&mut self) -> Result<Status, TransportError> {
        match self.selected() {
            Ok(t) => t.status(),
            Err(_) => Ok(Status::default()),
        }
    }
    fn read_block(&mut self, buf: &mut [u8]) -> Result<ReadOutcome, TransportError> {
        self.selected()?.read_block(buf)
    }
    fn write_block(&mut self, data: &[u8]) -> Result<Written, TransportError> {
        self.selected()?.write_block(data)
    }
}
