// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The next volume of a virtual tape device: the entry the request
//! names (from the `VOL` list), or for `*MOUNTED` the next loaded entry
//! of the image catalog after the one in the drive. When none is
//! there the operator is asked; an answer other than retry ends the
//! operation.

use std::io::{Read, Seek, Write};

use tape::{TapeError, TransportError, VirtualTape, VolumeProvider, VolumeRequest};

/// Opens an entry's image.
pub type Opener<B> = Box<dyn FnMut(&str) -> std::io::Result<B>>;
/// Asks the operator for a volume; `true` to look again.
pub type Asker = Box<dyn FnMut(&VolumeRequest) -> bool>;

/// The loaded entries of a `*TAP` catalog, as volumes for a drive.
pub struct CatalogTapes<B> {
    /// `(volume, image path)` of each loaded entry, in index order.
    pub entries: Vec<(String, String)>,
    /// The volume in the drive.
    pub current: Option<String>,
    /// Every volume moved into the drive here, in order.
    pub mounted: Vec<String>,
    open: Opener<B>,
    ask: Asker,
}

impl<B> std::fmt::Debug for CatalogTapes<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CatalogTapes")
            .field("entries", &self.entries)
            .field("current", &self.current)
            .field("mounted", &self.mounted)
            .finish_non_exhaustive()
    }
}

impl<B> CatalogTapes<B> {
    /// Entries, the one mounted now, how to open an image and whom to ask.
    #[must_use]
    pub fn new(
        entries: Vec<(String, String)>,
        current: Option<String>,
        open: Opener<B>,
        ask: Asker,
    ) -> Self {
        Self {
            entries,
            current,
            mounted: Vec::new(),
            open,
            ask,
        }
    }

    /// The entry a request wants.
    fn pick(&self, req: &VolumeRequest) -> Option<(String, String)> {
        if let Some(v) = &req.expected {
            return self
                .entries
                .iter()
                .find(|(vol, _)| vol.eq_ignore_ascii_case(v))
                .cloned();
        }
        let after = self
            .current
            .as_ref()
            .and_then(|c| self.entries.iter().position(|(v, _)| v == c));
        let start = after.map_or(0, |i| i + 1);
        self.entries
            .get(start..)
            .and_then(|rest| rest.iter().find(|(v, _)| Some(v) != self.current.as_ref()))
            .cloned()
    }
}

impl<B: Read + Write + Seek> VolumeProvider<VirtualTape<B>> for CatalogTapes<B> {
    fn next_volume(
        &mut self,
        drive: &mut VirtualTape<B>,
        req: &VolumeRequest,
    ) -> Result<(), TapeError> {
        // One more look after the operator answers retry.
        for _ in 0..2 {
            if let Some((vol, path)) = self.pick(req) {
                let io = (self.open)(&path).map_err(TransportError::from)?;
                *drive = VirtualTape::open(io)?;
                self.current = Some(vol.clone());
                self.mounted.push(vol);
                return Ok(());
            }
            if !(self.ask)(req) {
                break;
            }
        }
        Err(TapeError::Cancelled)
    }
}
