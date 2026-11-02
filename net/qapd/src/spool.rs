// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `ReadSpooledFile` over a file's data (docs/QAP.md, "Reading
//! pages"): the pages asked for, as the SCS spooled (after the state in
//! effect at the first, `spool::scs::range`), as text
//! (`spool::view::text`) or as PDF (`spool::pdf::render`), the views
//! `DSPSPLF` and Navigator read through. Only SCS files have them.

use qap_proto::ops::spool::{DeviceType, ReadFormat, ReadSpooledFile, SpoolStatus, SpooledData};

/// The message when a file is still open or being written.
pub const IN_USE: &str = "TCP1A57";
/// The message when a format cannot be made of a file.
pub const NO_FORMAT: &str = "TCP1A56";

/// Why a file cannot be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unreadable {
    /// Still `Open` or `Writing`: `InUse`.
    InUse,
    /// Its device type has no such format: `Error`.
    Format,
    /// The pages asked for are not the file's: `InvalidRequest`.
    Range,
}

/// The format's name in messages.
#[must_use]
pub fn format_name(f: ReadFormat) -> &'static str {
    match f {
        ReadFormat::Scs => "*SCS",
        ReadFormat::Text => "*TEXT",
        ReadFormat::Pdf => "*PDF",
    }
}

/// The device type's name in messages.
#[must_use]
pub fn device_name(d: DeviceType) -> &'static str {
    match d {
        DeviceType::Scs => "*SCS",
        DeviceType::Afp => "*AFPDS",
        DeviceType::Ascii => "*USERASCII",
        DeviceType::Line => "*LINE",
    }
}

/// Pages `req.first_page` to `req.last_page` of a file in `status` of
/// `device` whose data is `data`, in `req.format`.
///
/// # Errors
/// [`Unreadable`]: why not.
pub fn read(status: SpoolStatus, device: DeviceType, data: &[u8], req: &ReadSpooledFile) -> Result<SpooledData, Unreadable> {
    if matches!(status, SpoolStatus::Open | SpoolStatus::Writing) {
        return Err(Unreadable::InUse);
    }
    if device != DeviceType::Scs {
        return Err(Unreadable::Format);
    }
    let (pages, fmt) = spool::scs::decode(data);
    let n = pages.len();
    let first = usize::try_from(req.first_page).map_err(|_| Unreadable::Range)?;
    let total = u32::try_from(n).unwrap_or(u32::MAX);
    if first == 0 || first > n.max(1) {
        return Err(Unreadable::Range);
    }
    if n == 0 {
        return Ok(SpooledData {
            format: req.format,
            pages: 0,
            data: Vec::new(),
        });
    }
    let last = match usize::try_from(req.last_page).unwrap_or(usize::MAX) {
        0 => n,
        l => l.min(n),
    };
    if last < first {
        return Err(Unreadable::Range);
    }
    let data = match req.format {
        ReadFormat::Scs => spool::scs::range(data, first, last).ok_or(Unreadable::Range)?,
        // The whole file's pages and format, so a range reads as the
        // same pages do in the whole.
        ReadFormat::Text => spool::view::text(&pages[first - 1..last]).into_bytes(),
        ReadFormat::Pdf => spool::pdf::render(&pages[first - 1..last], fmt),
    };
    Ok(SpooledData {
        format: req.format,
        pages: total,
        data,
    })
}
