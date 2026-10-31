// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The QS/400 save stream (`docs/SAVE_FORMAT.md`): a media header, one
//! group of records per object, an index and a trailer. Medium
//! independent: the caller supplies a `Write` or a `Read`.

#![deny(missing_docs)]

mod be;
mod codec;
mod digest;
mod error;
pub mod header;
pub mod object;
pub mod outfile;
mod read;
pub mod record;
pub mod spec;
mod write;

pub use codec::Compression;
pub use digest::digest;
pub use error::{Error, Result};
pub use header::{MediaHeader, SaveKind};
pub use object::{
    AuthorityEntry, IndexEntry, ObjectDescriptor, ObjectEnd, Part, RefEntry, SignatureRecord,
    Trailer, TreeImage,
};
pub use read::{Reader, SavedObject, Scan, descriptor_at, read_index};
pub use record::{RecordHeader, RecordKind};
pub use write::{Summary, Writer};

/// A name field (CP037, blank padded) from text.
#[must_use]
pub fn name10(s: &str) -> [u8; 10] {
    fixed(s)
}

/// A fixed CP037 field from text; characters CP037 cannot hold become
/// blanks, longer text is cut.
#[must_use]
pub fn fixed<const N: usize>(s: &str) -> [u8; N] {
    let mut out = [0x40u8; N];
    for (o, ch) in out.iter_mut().zip(s.chars()) {
        *o = ccsid::byte_of(ch).unwrap_or(0x40);
    }
    out
}

/// Text of a fixed CP037 field, trailing blanks removed.
#[must_use]
pub fn text(field: &[u8]) -> String {
    ccsid::decode_trimmed(field)
}
