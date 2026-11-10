// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A file share's text conversion (`QZLSADFS`'s text CCSID, enable text
//! conversion and file extension table): the data of a file whose
//! extension is in the share's list is converted from the file's CCSID
//! to the share's on read and back on write, as IBM i `NetServer` does.
//!
//! Both CCSIDs are single-byte (`QZLSADFS` refuses a multi-byte share
//! CCSID and mixed data), so every byte maps to one byte: sizes, offsets,
//! byte-range locks and end of file stay the file's own. A file in a
//! multi-byte or mixed CCSID, in 65535, or already in the share's CCSID
//! is served as stored. Whether a file converts is decided when it is
//! opened; a file created through the share takes the share's CCSID.

use ccsid::conv::{Codec, Form, convert};

/// The share's CCSID when its text CCSID is 0: the server's, the ASCII
/// equivalent of the system's CCSID 37.
pub const SERVER_CCSID: u16 = 819;

/// What a share converts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextConversion {
    /// The share's (the client's) CCSID.
    pub ccsid: u16,
    /// The extensions converted, without the dot; `*` is every file.
    pub extensions: Vec<String>,
}

impl TextConversion {
    /// From the share's attributes: `ccsid` 0 is [`SERVER_CCSID`].
    #[must_use]
    pub fn new(ccsid: i32, extensions: Vec<String>) -> Self {
        let ccsid = u16::try_from(ccsid).ok().filter(|c| *c != 0).unwrap_or(SERVER_CCSID);
        Self { ccsid, extensions }
    }

    /// Whether a file named `name` (its last component) is converted:
    /// its extension is in the list, without regard to case, or the list
    /// has `*`.
    #[must_use]
    pub fn applies(&self, name: &str) -> bool {
        let ext = name.rsplit_once('.').map(|(_, e)| e);
        self.extensions.iter().any(|e| e == "*" || ext.is_some_and(|x| !x.is_empty() && x.eq_ignore_ascii_case(e)))
    }
}

/// Whether `ccsid` holds one character per byte.
#[must_use]
pub fn single_byte(ccsid: u16) -> bool {
    matches!(Codec::of(ccsid), Ok(Some(Codec { form: Form::Sbcs { .. }, .. })))
}

/// Byte tables between a file's CCSID and its share's.
#[derive(Debug)]
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
    /// The tables for a file in `file` served in `share`; `None` when its
    /// data is served as stored.
    #[must_use]
    pub fn between(file: u16, share: u16) -> Option<Self> {
        if file == share || !single_byte(file) || !single_byte(share) {
            return None;
        }
        Some(Self { to_client: table(file, share)?, to_file: table(share, file)? })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_and_the_asterisk() {
        let t = TextConversion::new(0, vec!["TXT".into(), "MBR".into()]);
        assert_eq!(t.ccsid, SERVER_CCSID);
        assert!(t.applies("notes.txt") && t.applies("A.B.Txt") && t.applies("X.MBR"));
        assert!(!t.applies("notes.txt2") && !t.applies("README") && !t.applies("trail."));
        let all = TextConversion::new(1252, vec!["*".into()]);
        assert_eq!(all.ccsid, 1252);
        assert!(all.applies("README") && all.applies("a.bin"));
    }

    #[test]
    fn single_byte_tables_keep_sizes() {
        let x = Xlate::between(37, 819).unwrap();
        let mut d = vec![0xC8, 0x85, 0x93, 0x93, 0x96, 0x40, 0x5A];
        x.to_client(&mut d);
        assert_eq!(d, b"Hello !");
        x.to_file(&mut d);
        assert_eq!(d, [0xC8, 0x85, 0x93, 0x93, 0x96, 0x40, 0x5A]);
        assert!(Xlate::between(819, 819).is_none());
        assert!(Xlate::between(1208, 819).is_none());
        assert!(Xlate::between(65535, 819).is_none());
        assert!(Xlate::between(37, 930).is_none());
        assert!(single_byte(1252) && !single_byte(1208) && !single_byte(930));
    }
}
