// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Text conversion per host (`HOSTOPT`), as `NetServer`'s shares do it
//! (`net/netserver/src/ifs/text.rs`):
//!
//! - Data: with a data CCSID other than `*BINARY`, a file tagged with
//!   a single-byte CCSID other than the host's is converted byte for
//!   byte on read and write, so sizes, offsets and locks stay the
//!   file's own. A file in a multi-byte or mixed CCSID, in 65535, or
//!   already in the host's CCSID is served as stored. A double-byte
//!   data CCSID is refused when the export is made (`CPDA1CA`).
//! - Names: the host's path name CCSID on the wire, the IFS's names
//!   (UTF-8) inside. A single-byte path name CCSID or UTF-8 only.

use ccsid::conv::{Codec, Form, UTF8, convert};

use super::error::VfsError;

/// Whether `ccsid` holds one character per byte.
#[must_use]
pub fn single_byte(ccsid: u16) -> bool {
    matches!(
        Codec::of(ccsid),
        Ok(Some(Codec {
            form: Form::Sbcs { .. },
            ..
        }))
    )
}

/// Byte tables between a file's CCSID and the host's.
#[derive(Debug, Clone)]
pub struct Xlate {
    to_client: [u8; 256],
    to_file: [u8; 256],
}

fn table(from: u16, to: u16) -> Option<[u8; 256]> {
    let mut t = [0u8; 256];
    for (b, slot) in (0..=255u8).zip(t.iter_mut()) {
        match convert(from, to, &[b]).ok()?.as_slice() {
            [one] => *slot = *one,
            _ => return None,
        }
    }
    Some(t)
}

impl Xlate {
    /// The tables for a file in `file` served to a host in `host`
    /// (`None`: `*BINARY`); `None` when its data is served as stored.
    #[must_use]
    pub fn between(file: u16, host: Option<u16>) -> Option<Self> {
        let host = host?;
        if file == host || !single_byte(file) || !single_byte(host) {
            return None;
        }
        Some(Self {
            to_client: table(file, host)?,
            to_file: table(host, file)?,
        })
    }

    /// File data as the client reads it.
    pub fn to_client(&self, data: &mut [u8]) {
        for b in data {
            *b = self.to_client[usize::from(*b)];
        }
    }

    /// Client data as the file stores it.
    pub fn to_file(&self, data: &mut [u8]) {
        for b in data {
            *b = self.to_file[usize::from(*b)];
        }
    }
}

/// A name or path from the client's bytes in `ccsid`.
///
/// # Errors
///
/// [`VfsError::Inval`] when it is not valid in the CCSID, or the CCSID
/// is neither UTF-8 nor single-byte.
pub fn from_client(ccsid: u16, b: &[u8]) -> Result<String, VfsError> {
    if ccsid == UTF8 {
        return String::from_utf8(b.to_vec()).map_err(|_| VfsError::Inval);
    }
    if !single_byte(ccsid) {
        return Err(VfsError::Inval);
    }
    let u = convert(ccsid, UTF8, b).map_err(|_| VfsError::Inval)?;
    String::from_utf8(u).map_err(|_| VfsError::Inval)
}

/// A name or path as the client's bytes in `ccsid`.
///
/// # Errors
///
/// [`VfsError::Inval`] when the CCSID cannot be converted to.
pub fn to_client(ccsid: u16, s: &str) -> Result<Vec<u8>, VfsError> {
    if ccsid == UTF8 {
        return Ok(s.as_bytes().to_vec());
    }
    if !single_byte(ccsid) {
        return Err(VfsError::Inval);
    }
    convert(UTF8, ccsid, s.as_bytes()).map_err(|_| VfsError::Inval)
}

/// A name a client gave: one component, not empty, without `/` or NUL
/// (`Inval`), at most 255 bytes on the wire (`NameTooLong`).
///
/// # Errors
///
/// As above, and those of [`from_client`].
pub fn name(ccsid: u16, b: &[u8]) -> Result<String, VfsError> {
    if b.len() > 255 {
        return Err(VfsError::NameTooLong);
    }
    if b.is_empty() || b.contains(&0) {
        return Err(VfsError::Inval);
    }
    let s = from_client(ccsid, b)?;
    if s.contains('/') || s.contains('\0') {
        return Err(VfsError::Inval);
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_and_names() {
        let x = Xlate::between(37, Some(819)).unwrap();
        let mut d = vec![0xC8, 0x85, 0x93, 0x93, 0x96];
        x.to_client(&mut d);
        assert_eq!(d, b"Hello");
        assert!(Xlate::between(37, None).is_none());
        assert!(Xlate::between(1208, Some(819)).is_none());
        assert_eq!(name(819, b"caf\xe9").unwrap(), "caf\u{e9}");
        assert_eq!(to_client(819, "caf\u{e9}").unwrap(), b"caf\xe9");
        assert_eq!(name(UTF8, b"a/b"), Err(VfsError::Inval));
        assert_eq!(name(UTF8, &[b'a'; 256]), Err(VfsError::NameTooLong));
        assert_eq!(name(UTF8, b"a\0"), Err(VfsError::Inval));
        assert_eq!(name(930, b"a"), Err(VfsError::Inval));
    }
}
