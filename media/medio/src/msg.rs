// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The messages the media commands send, as their catalogue entries in
//! `QCPFMSG` lay out the data: IBM's identifier for the meaning and
//! IBM's field formats (`tools/conformance/oracle/msgdata/fields.txt`),
//! so a program that monitors for one reads what it expects. The text
//! is the catalogue's; a cause the entry cannot hold is the `detail`.

use tape::TapeError;
use tape::device::LibraryError;

use crate::MediaError;

/// One field of a message's data.
#[derive(Debug, Clone, Copy)]
pub enum Field<'a> {
    /// `*CHAR n`: CP037, padded with blanks or cut to `n`.
    Char(&'a str, usize),
    /// `*BIN 2`.
    Bin2(i16),
    /// `*BIN 4`.
    Bin4(i32),
}

/// The data of `fields`, one after another.
#[must_use]
pub fn data(fields: &[Field<'_>]) -> Vec<u8> {
    let mut out = Vec::new();
    for f in fields {
        match *f {
            Field::Char(text, n) => {
                let mut b = ccsid::to_ebcdic(text).unwrap_or_default();
                b.resize(n, 0x40);
                out.extend_from_slice(&b);
            }
            Field::Bin2(v) => out.extend_from_slice(&v.to_be_bytes()),
            Field::Bin4(v) => out.extend_from_slice(&v.to_be_bytes()),
        }
    }
    out
}

/// A message: its identifier, its data, and a cause the entry has no
/// field for (empty when the message says it all).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Msg {
    /// The identifier, in `QCPFMSG`.
    pub id: &'static str,
    /// The data, laid out as the entry declares it.
    pub data: Vec<u8>,
    /// The cause underneath, for the job log before the message.
    pub detail: String,
}

impl Msg {
    /// Message `id` with `fields` and no detail.
    #[must_use]
    pub fn new(id: &'static str, fields: &[Field<'_>]) -> Self {
        Self {
            id,
            data: data(fields),
            detail: String::new(),
        }
    }

    /// With the cause underneath.
    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }
}

/// The escape for `e` on device `dev`, `vol` the volume in it (empty
/// when not known).
#[must_use]
pub fn escape(e: &MediaError, dev: &str, vol: &str) -> Msg {
    let id = e.msgid();
    match e {
        MediaError::Tape(t) => tape(id, t, dev, vol),
        MediaError::Library(l) => library(id, l, dev),
        MediaError::Optical(o) => optical(id, o, dev, vol),
        MediaError::NotFound(p) | MediaError::Exists(p) => {
            Msg::new(id, &[Field::Char(vol, 32), Field::Char(p, 256)])
        }
        MediaError::Continuation(why) => {
            Msg::new(id, &[Field::Char(vol, 32), Field::Char("", 256)]).with_detail(why.clone())
        }
        MediaError::State(why) => Msg::new(id, &[]).with_detail(why.clone()),
    }
}

fn tape(id: &'static str, e: &TapeError, dev: &str, vol: &str) -> Msg {
    let d = Field::Char(dev, 10);
    match e {
        TapeError::WrongVolume { expected, found } => Msg::new(
            id,
            &[d, Field::Char(found, 6), Field::Char(expected, 6)],
        ),
        TapeError::NotContinuation { volume, .. } => Msg::new(
            id,
            &[d, Field::Char(volume, 6), Field::Char("", 6)],
        )
        .with_detail(e.to_string()),
        TapeError::FileNotFound { .. } => {
            Msg::new(id, &[d, Field::Char(vol, 6)]).with_detail(e.to_string())
        }
        TapeError::ActiveFile { .. } => active_file_data(id, e, dev),
        _ => match id {
            // The generic escape: the cause is all it has.
            "CPF6708" => Msg::new(id, &[]).with_detail(e.to_string()),
            "CPF6760" | "CPF6768" | "CPF6722" => Msg::new(id, &[d]),
            _ => Msg::new(id, &[d]).with_detail(e.to_string()),
        },
    }
}

fn library(id: &'static str, e: &LibraryError, dev: &str) -> Msg {
    let d = Field::Char(dev, 10);
    let none = Field::Char("", 6);
    match e {
        LibraryError::NotFound(c) | LibraryError::InUse(c) => {
            Msg::new(id, &[d, Field::Char(c, 6)])
        }
        LibraryError::Category { id: c, category } => Msg::new(
            id,
            &[d, Field::Char(c, 6), none, Field::Char(category, 10)],
        ),
        LibraryError::BadCategory(category) => {
            Msg::new(id, &[d, none, none, Field::Char(category, 10)])
        }
        LibraryError::SysGen(_) | LibraryError::NoDrive(_) | LibraryError::NoSpace(_) => {
            Msg::new(id, &[d]).with_detail(e.to_string())
        }
    }
}

fn optical(id: &'static str, e: &optical::Error, dev: &str, vol: &str) -> Msg {
    use optical::Error as O;
    let v = Field::Char(vol, 32);
    match e {
        O::Io(_) | O::Malformed(_) => {
            Msg::new(id, &[Field::Char(dev, 10), v]).with_detail(e.to_string())
        }
        O::NotFound(p) | O::Exists(p) | O::NotDir(p) | O::IsDir(p) | O::NotEmpty(p) => {
            Msg::new(id, &[v, Field::Char(p, 256)])
        }
        O::BadName(n) => Msg::new(id, &[Field::Char(n, 256)]),
        O::BadVolumeId(n) => Msg::new(id, &[Field::Char(n, 32)]),
        O::VolumeFull { .. } => Msg::new(id, &[v]).with_detail(e.to_string()),
        O::WriteProtected | O::ReadOnly | O::ActiveFiles => Msg::new(id, &[v]),
        O::Invalid(why) => Msg::new(id, &[]).with_detail(why.clone()),
    }
}

/// `CPF6754` or `CPA6754`'s data: device, volume, (cartridge), file
/// label, expiration, sequence number.
fn active_file_data(id: &'static str, e: &TapeError, dev: &str) -> Msg {
    let TapeError::ActiveFile {
        volume,
        seq,
        label,
        expires,
    } = e
    else {
        return Msg::new(id, &[Field::Char(dev, 10)]);
    };
    let seq = seq.to_string();
    Msg::new(
        id,
        &[
            Field::Char(dev, 10),
            Field::Char(volume, 6),
            Field::Char("", 6),
            Field::Char(label, 17),
            Field::Char(expires, 10),
            Field::Char(&seq, 10),
        ],
    )
}

/// The inquiry a write sends before it overwrites an active file
/// (`CPA6754`, replies C and I); `None` when `e` is not that.
#[must_use]
pub fn active_file_inquiry(e: &MediaError, dev: &str) -> Option<Msg> {
    match e {
        MediaError::Tape(t @ TapeError::ActiveFile { .. }) => {
            Some(active_file_data(ACTIVE_FILE_INQUIRY, t, dev))
        }
        _ => None,
    }
}

/// An active file in the way of a write: the inquiry.
pub const ACTIVE_FILE_INQUIRY: &str = "CPA6754";
/// Load the next tape volume (C G); with the volume named.
pub const NEXT_VOLUME: &str = "CPA4088";
/// Load tape volume `&5` (C G).
pub const NAMED_VOLUME: &str = "CPA4089";

/// The inquiry for the next tape volume on `dev`: `CPA4089` with the
/// volume when one is expected, else `CPA4088`.
#[must_use]
pub fn next_volume(dev: &str, expected: Option<&str>) -> Msg {
    let blank = Field::Char("", 10);
    let d = Field::Char(dev, 10);
    match expected.filter(|v| !v.is_empty() && !v.starts_with('*')) {
        Some(v) => Msg::new(NAMED_VOLUME, &[blank, blank, blank, d, Field::Char(v, 6)]),
        None => Msg::new(NEXT_VOLUME, &[blank, blank, blank, d]),
    }
}

/// `INZTAP`'s completion: `CPC6773` (QS/400's own) for a labeled
/// volume, `CPC6776` for a nonlabeled one at `density`.
#[must_use]
pub fn tape_initialized(dev: &str, new_vol: Option<&str>, density: &str) -> Msg {
    match new_vol {
        Some(v) => Msg::new("CPC6773", &[Field::Char(dev, 10), Field::Char(v, 6)]),
        None => Msg::new(
            "CPC6776",
            &[
                Field::Char(dev, 10),
                Field::Char("", 0),
                Field::Char("", 0),
                Field::Char(density, 10),
            ],
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chars_are_cp037_and_padded() {
        let d = data(&[Field::Char("TAP01", 10), Field::Bin2(3), Field::Bin4(-1)]);
        assert_eq!(d.len(), 16);
        assert_eq!(&d[..5], &[0xe3, 0xc1, 0xd7, 0xf0, 0xf1]);
        assert_eq!(&d[5..10], &[0x40; 5]);
        assert_eq!(&d[10..12], &[0, 3]);
        assert_eq!(&d[12..], &[0xff; 4]);
        assert_eq!(data(&[Field::Char("ABCDEFGH", 6)]).len(), 6);
    }

    #[test]
    fn next_volume_names_the_volume_when_known() {
        assert_eq!(next_volume("TAP01", None).id, "CPA4088");
        assert_eq!(next_volume("TAP01", Some("*MOUNTED")).id, "CPA4088");
        let m = next_volume("TAP01", Some("VOL002"));
        assert_eq!((m.id, m.data.len()), ("CPA4089", 46));
    }
}
