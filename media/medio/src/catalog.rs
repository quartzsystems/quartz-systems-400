// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The next volume of a virtual device: the entry the request names
//! (from the `VOL` list), or for `*MOUNTED` the next loaded entry of
//! the image catalog after the one in the drive. When none is there
//! the operator is asked; a retry reply looks again over every loaded
//! entry not used yet in the operation, any other reply ends it.

use std::io::{Read, Seek, Write};

use tape::{TapeError, TransportError, VirtualTape, VolumeProvider, VolumeRequest};

/// Opens an entry's image.
pub type Opener<'a, B> = Box<dyn FnMut(&str) -> std::io::Result<B> + 'a>;
/// Asks the operator for a volume; `true` to look again.
pub type Asker<'a> = Box<dyn FnMut(&VolumeRequest) -> bool + 'a>;

/// Which volume comes next, by volume identifier.
#[derive(Debug, Clone, Default)]
pub struct NextVolume {
    /// The candidates, in catalog order.
    pub volumes: Vec<String>,
    /// The volume in the device.
    pub current: Option<String>,
    /// Volumes used in this operation, the first one included.
    pub used: Vec<String>,
    /// For `*MOUNTED`, the volumes after the current one come without
    /// asking.
    pub follow: bool,
}

impl NextVolume {
    /// Candidates and the one in the device; `follow` as the field.
    #[must_use]
    pub fn new(volumes: Vec<String>, current: Option<String>, follow: bool) -> Self {
        Self {
            used: current.iter().cloned().collect(),
            volumes,
            current,
            follow,
        }
    }

    /// The volume `expected` names, or when it names none: the next
    /// after the current one (`wide` false, only when following) or any
    /// not used yet (`wide`).
    #[must_use]
    pub fn pick(&self, expected: Option<&str>, wide: bool) -> Option<String> {
        if let Some(v) = expected {
            return self.volumes.iter().find(|x| x.eq_ignore_ascii_case(v)).cloned();
        }
        let fresh = |v: &&String| !self.used.iter().any(|u| u.eq_ignore_ascii_case(v));
        if wide {
            return self.volumes.iter().find(fresh).cloned();
        }
        if !self.follow {
            return None;
        }
        let after = self
            .current
            .as_ref()
            .and_then(|c| self.volumes.iter().position(|v| v.eq_ignore_ascii_case(c)));
        let start = after.map_or(0, |i| i + 1);
        self.volumes.get(start..)?.iter().find(fresh).cloned()
    }

    /// `v` is now in the device.
    pub fn used(&mut self, v: &str) {
        self.current = Some(v.to_string());
        if !self.used.iter().any(|u| u.eq_ignore_ascii_case(v)) {
            self.used.push(v.to_string());
        }
    }

    /// The next volume for `req`, asking the operator when there is
    /// none: one retry reply widens the look. `None` ends the operation.
    pub fn next(&mut self, req: &VolumeRequest, ask: &mut Asker<'_>) -> Option<String> {
        let mut wide = false;
        for _ in 0..2 {
            if let Some(v) = self.pick(req.expected.as_deref(), wide) {
                return Some(v);
            }
            if !ask(req) {
                return None;
            }
            wide = true;
        }
        None
    }
}

/// The loaded entries of a `*TAP` catalog, as volumes for a drive.
pub struct CatalogTapes<'a, B> {
    /// `(volume, image path)` of each loaded entry, in index order.
    pub entries: Vec<(String, String)>,
    /// The volume in the drive.
    pub current: Option<String>,
    /// Every volume moved into the drive here, in order.
    pub mounted: Vec<String>,
    pick: NextVolume,
    open: Opener<'a, B>,
    ask: Asker<'a>,
}

impl<B> std::fmt::Debug for CatalogTapes<'_, B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CatalogTapes")
            .field("entries", &self.entries)
            .field("current", &self.current)
            .field("mounted", &self.mounted)
            .finish_non_exhaustive()
    }
}

impl<'a, B> CatalogTapes<'a, B> {
    /// Entries, the one mounted now, how to open an image and whom to ask.
    #[must_use]
    pub fn new(
        entries: Vec<(String, String)>,
        current: Option<String>,
        open: Opener<'a, B>,
        ask: Asker<'a>,
    ) -> Self {
        let vols = entries.iter().map(|(v, _)| v.clone()).collect();
        Self {
            pick: NextVolume::new(vols, current.clone(), true),
            entries,
            current,
            mounted: Vec::new(),
            open,
            ask,
        }
    }
}

impl<B: Read + Write + Seek> VolumeProvider<VirtualTape<B>> for CatalogTapes<'_, B> {
    fn next_volume(
        &mut self,
        drive: &mut VirtualTape<B>,
        req: &VolumeRequest,
    ) -> Result<(), TapeError> {
        let vol = self
            .pick
            .next(req, &mut self.ask)
            .ok_or(TapeError::Cancelled)?;
        let path = self
            .entries
            .iter()
            .find(|(v, _)| *v == vol)
            .map(|(_, p)| p.clone())
            .ok_or(TapeError::Cancelled)?;
        let io = (self.open)(&path).map_err(TransportError::from)?;
        *drive = VirtualTape::open(io)?;
        self.pick.used(&vol);
        self.current = Some(vol.clone());
        self.mounted.push(vol);
        Ok(())
    }
}
