// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Stock volume providers: one that never has another volume, and one
//! that mounts the next volume through a changer.

use super::{VolumeProvider, VolumeRequest};
use crate::TapeError;
use crate::device::cartridge::cartridge_id;
use crate::transport::{Changer, ChangerError, ElementKind, TapeTransport, TransportError};

/// A provider with no further volumes: a multi-volume request fails
/// with [`TapeError::Cancelled`].
#[derive(Debug, Clone, Copy, Default)]
pub struct NoMoreVolumes;

impl<D: ?Sized> VolumeProvider<D> for NoMoreVolumes {
    fn next_volume(&mut self, _: &mut D, _: &VolumeRequest) -> Result<(), TapeError> {
        Err(TapeError::Cancelled)
    }
}

/// Mounts volumes from a library's slots into one of its drives: the
/// requested volume, or for VOL(*MOUNTED) the next of `spares`.
#[derive(Debug, Clone, Default)]
pub struct LibraryProvider {
    /// The drive's index among the library's drive elements.
    pub drive: usize,
    /// Volumes to use, in order, when a request names none.
    pub spares: Vec<String>,
    /// Every volume mounted, in order.
    pub mounted: Vec<String>,
}

impl<L: Changer + TapeTransport + ?Sized> VolumeProvider<L> for LibraryProvider {
    fn next_volume(&mut self, lib: &mut L, req: &VolumeRequest) -> Result<(), TapeError> {
        let want = match &req.expected {
            Some(v) => v.clone(),
            None if !self.spares.is_empty() => self.spares.remove(0),
            None => return Err(TapeError::Cancelled),
        };
        match lib.unload() {
            Ok(()) | Err(TransportError::NotLoaded) => {}
            Err(e) => return Err(e.into()),
        }
        let elems = lib.read_element_status()?;
        let drive = elems
            .iter()
            .filter(|e| e.kind == ElementKind::Drive)
            .nth(self.drive)
            .cloned()
            .ok_or(ChangerError::NoElement(u16::MAX))?;
        if drive.full {
            let home = drive
                .source
                .filter(|s| elems.iter().any(|e| e.address == *s && !e.full))
                .or_else(|| {
                    elems
                        .iter()
                        .find(|e| e.kind == ElementKind::Storage && !e.full)
                        .map(|e| e.address)
                })
                .ok_or(ChangerError::DestinationFull(drive.address))?;
            lib.move_medium(drive.address, home)?;
        }
        let from = elems
            .iter()
            .find(|e| {
                e.full
                    && e.kind != ElementKind::Drive
                    && e.volume_tag.as_deref().map(cartridge_id) == Some(want.as_str())
            })
            .map(|e| e.address)
            .ok_or_else(|| TapeError::WrongVolume {
                expected: want.clone(),
                found: "*NONE".into(),
            })?;
        lib.move_medium(from, drive.address)?;
        self.mounted.push(want);
        Ok(())
    }
}
