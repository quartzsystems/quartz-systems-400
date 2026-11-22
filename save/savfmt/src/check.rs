// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Why data is not a save stream this release takes: bad magic, a
//! version it does not read, flags it does not know, or the first field
//! that does not match, by offset. Every command and server that refuses
//! a save file says this.

use std::io::Cursor;

use crate::be::{get16, get32};
use crate::error::Error;
use crate::header::{self, MediaHeader, OLDEST, VERSION};
use crate::object::TRAILER_LEN;
use crate::read::read_index;
use crate::record::{self, crc32c};

/// Why a stream is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Reject {
    /// Shorter than a media header.
    #[error("only {0} bytes, shorter than a save stream's 128-byte media header")]
    Short(usize),
    /// The first 8 bytes are not `QS400SAV`.
    #[error("bad magic: the data does not begin with QS400SAV")]
    Magic,
    /// A format version this release does not read.
    #[error("save format version {0} is not supported (this release reads {OLDEST} to {VERSION})")]
    Version(u16),
    /// Media header flags this release does not know.
    #[error("unknown media header flags X'{0:04X}'")]
    Flags(u16),
    /// A field that does not match.
    #[error("field mismatch at offset {offset}: {field}")]
    Field {
        /// Stream offset of the field.
        offset: u64,
        /// What it is and what is wrong.
        field: String,
    },
    /// Damage a reader found where no one field is to blame.
    #[error("the save stream is damaged: {0}")]
    Damaged(String),
}

fn field(offset: u64, field: impl Into<String>) -> Reject {
    Reject::Field {
        offset,
        field: field.into(),
    }
}

/// The media header of `b`, or why it is refused.
pub fn header(b: &[u8]) -> Result<MediaHeader, Reject> {
    if b.len() < header::LEN {
        return Err(if b.len() >= 8 && &b[..8] != header::MAGIC {
            Reject::Magic
        } else {
            Reject::Short(b.len())
        });
    }
    if &b[..8] != header::MAGIC {
        return Err(Reject::Magic);
    }
    let version = get16(b, 8);
    if !(OLDEST..=VERSION).contains(&version) {
        return Err(Reject::Version(version));
    }
    if crc32c(&b[..124]) != get32(b, 124) {
        return Err(field(124, "the media header's CRC does not match its bytes"));
    }
    let flags = get16(b, 10);
    if flags != 0 {
        return Err(Reject::Flags(flags));
    }
    if b[18] > 3 {
        return Err(field(18, format!("compression {} is not a known code", b[18])));
    }
    if b[19] > 9 {
        return Err(field(19, format!("kind {} is not a known code", b[19])));
    }
    if let Some(i) = (20..24).chain(86..124).find(|&i| b[i] != 0) {
        return Err(field(i as u64, "a reserved byte of the media header is not zero"));
    }
    MediaHeader::decode(b).map_err(|e| of_error(&e))
}

/// The media header of the whole stream `b`, checked through to its
/// trailer, or why it is refused.
pub fn stream(b: &[u8]) -> Result<MediaHeader, Reject> {
    let h = header(b)?;
    let tail = record::LEN + TRAILER_LEN;
    let (_, trailer, _) = read_index(&mut Cursor::new(b)).map_err(|e| match e {
        Error::Truncated | Error::Damaged { .. } if b.len() >= tail => field(
            (b.len() - tail) as u64,
            "no trailer record at the end: the stream is incomplete or has bytes after it",
        ),
        e => of_error(&e),
    })?;
    let at = (b.len() - tail) as u64;
    if trailer.stream_id != h.stream_id {
        return Err(field(
            at + record::LEN as u64 + 32,
            "the trailer's stream ID is not the media header's",
        ));
    }
    if trailer.bytes != at {
        return Err(field(
            at + record::LEN as u64 + 24,
            format!(
                "the trailer counts {} bytes before it, the stream has {at}",
                trailer.bytes
            ),
        ));
    }
    Ok(h)
}

/// A reader's error as a reason.
#[must_use]
pub fn of_error(e: &Error) -> Reject {
    match e {
        Error::NotSave => Reject::Magic,
        Error::Version(v) => Reject::Version(*v),
        Error::HeaderDamaged => Reject::Damaged("the media header is damaged".into()),
        Error::Damaged { offset } => field(*offset, "a record header is damaged or out of sequence"),
        Error::BadCrc { seq } => Reject::Damaged(format!("record {seq}'s payload does not match its CRC")),
        Error::Truncated => Reject::Damaged("the stream ends before its trailer".into()),
        Error::ObjectDamaged { index } => Reject::Damaged(format!("saved object {index} is damaged")),
        Error::Codec => Reject::Damaged("compressed data does not decompress to its length".into()),
        Error::Io(e) => Reject::Damaged(format!("the medium failed: {e}")),
        Error::Misuse(m) => Reject::Damaged(format!("the stream is misused: {m}")),
    }
}
