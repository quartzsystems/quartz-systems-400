// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Fixed records: a [`Layout`] names every field's offset and size, and
//! a [`Rec`] reads and writes the fields by name. Text is CP037, blank
//! padded; integers are little-endian. `docs/WORKMGMT.md` holds the
//! tables `tests/spec.rs` compares the layouts with.

pub use store::spec::{Field, Layout, field};

use crate::Error;

/// A record being built or read.
pub struct Rec<'l> {
    layout: &'l Layout,
    /// The bytes, the fixed part first.
    pub buf: Vec<u8>,
}

impl<'l> Rec<'l> {
    /// A record of zeros the layout's size.
    #[must_use]
    pub fn new(layout: &'l Layout) -> Self {
        Self {
            layout,
            buf: vec![0; layout.size.unwrap_or(0)],
        }
    }

    /// Over bytes read back; at least the layout's size, or an error.
    pub fn read(layout: &'l Layout, bytes: &[u8]) -> Result<Self, Error> {
        let need = layout.size.unwrap_or(0);
        if bytes.len() < need {
            return Err(Error::Corrupt(format!(
                "{}: {} bytes, {need} needed",
                layout.name,
                bytes.len()
            )));
        }
        Ok(Self {
            layout,
            buf: bytes.to_vec(),
        })
    }

    fn at(&self, name: &str) -> (usize, usize) {
        let f = self
            .layout
            .fields
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("{} has no field {name}", self.layout.name));
        (f.offset.expect("fixed"), f.size.expect("fixed"))
    }

    /// The field's bytes.
    #[must_use]
    pub fn bytes(&self, name: &str) -> &[u8] {
        let (o, n) = self.at(name);
        &self.buf[o..o + n]
    }

    /// Set the field's bytes; shorter is zero filled.
    pub fn set_bytes(&mut self, name: &str, b: &[u8]) {
        let (o, n) = self.at(name);
        let k = b.len().min(n);
        self.buf[o..o + k].copy_from_slice(&b[..k]);
        self.buf[o + k..o + n].fill(0);
    }

    /// Text, trailing blanks dropped.
    #[must_use]
    pub fn text(&self, name: &str) -> String {
        ccsid::from_ebcdic(self.bytes(name))
            .trim_end_matches([' ', '\0'])
            .to_string()
    }

    /// Set text, blank padded; characters CP037 lacks become blanks.
    pub fn set_text(&mut self, name: &str, s: &str) {
        let (o, n) = self.at(name);
        let mut b: Vec<u8> = s
            .chars()
            .map(|c| ccsid::to_ebcdic(&c.to_string()).map_or(0x40, |v| v[0]))
            .take(n)
            .collect();
        b.resize(n, 0x40);
        self.buf[o..o + n].copy_from_slice(&b);
    }

    /// An unsigned integer of the field's size.
    #[must_use]
    pub fn uint(&self, name: &str) -> u64 {
        let b = self.bytes(name);
        let mut v = [0u8; 8];
        v[..b.len().min(8)].copy_from_slice(&b[..b.len().min(8)]);
        u64::from_le_bytes(v)
    }

    /// Set an unsigned integer of the field's size.
    pub fn set_uint(&mut self, name: &str, v: u64) {
        let (_, n) = self.at(name);
        let b = v.to_le_bytes();
        self.set_bytes(name, &b[..n.min(8)]);
    }

    /// A one-byte flag.
    #[must_use]
    pub fn flag(&self, name: &str) -> bool {
        self.uint(name) != 0
    }

    /// Set a one-byte flag.
    pub fn set_flag(&mut self, name: &str, v: bool) {
        self.set_uint(name, u64::from(v));
    }

    /// The bytes after the fixed part.
    #[must_use]
    pub fn tail(&self) -> &[u8] {
        &self.buf[self.layout.size.unwrap_or(0)..]
    }
}

/// Text in a fixed field of `n` bytes, CP037, blank padded.
#[must_use]
pub fn text_bytes(s: &str, n: usize) -> Vec<u8> {
    let mut b: Vec<u8> = s
        .chars()
        .map(|c| ccsid::to_ebcdic(&c.to_string()).map_or(0x40, |v| v[0]))
        .take(n)
        .collect();
    b.resize(n, 0x40);
    b
}

/// Text from CP037 bytes, trailing blanks dropped.
#[must_use]
pub fn text_of(b: &[u8]) -> String {
    ccsid::from_ebcdic(b)
        .trim_end_matches([' ', '\0'])
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const L: Layout = Layout {
        name: "Test",
        size: Some(16),
        fields: &[
            field("Name", 0, 10),
            field("Count", 10, 2),
            field("Flag", 12, 1),
            field("Big", 13, 3),
        ],
    };

    #[test]
    fn fields() {
        let mut r = Rec::new(&L);
        r.set_text("Name", "QSYS");
        r.set_uint("Count", 513);
        r.set_flag("Flag", true);
        r.set_uint("Big", 0x0102_0304);
        r.buf.extend_from_slice(b"tail");
        let back = Rec::read(&L, &r.buf).unwrap();
        assert_eq!(back.text("Name"), "QSYS");
        assert_eq!(back.bytes("Name")[4], 0x40);
        assert_eq!(back.uint("Count"), 513);
        assert!(back.flag("Flag"));
        assert_eq!(back.uint("Big"), 0x02_0304);
        assert_eq!(back.tail(), b"tail");
        assert!(Rec::read(&L, &[0; 3]).is_err());
    }
}
