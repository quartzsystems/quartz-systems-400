// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Fixed records and their tails. A [`Rec`] reads and writes the fields
//! of a [`Layout`] by name: text is CP037, blank padded; integers are
//! little-endian. After the fixed part, a [`Tail`] holds tagged
//! variable-length items. `docs/SECURITY.md` has the tables
//! `tests/spec.rs` compares the layouts with.

pub use store::spec::{Field, Layout, field};

use crate::{Result, SecError};

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
    pub fn read(layout: &'l Layout, bytes: &[u8]) -> Result<Self> {
        let need = layout.size.unwrap_or(0);
        if bytes.len() < need {
            return Err(SecError::Corrupt(format!(
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
        text_of(self.bytes(name))
    }

    /// Set text, blank padded; characters CP037 lacks become blanks.
    pub fn set_text(&mut self, name: &str, s: &str) {
        let (o, n) = self.at(name);
        self.buf[o..o + n].copy_from_slice(&text_bytes(s, n));
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

    /// A signed integer of eight bytes.
    #[must_use]
    pub fn int(&self, name: &str) -> i64 {
        i64::from_le_bytes(self.uint(name).to_le_bytes())
    }

    /// Set a signed integer of eight bytes.
    pub fn set_int(&mut self, name: &str, v: i64) {
        self.set_uint(name, u64::from_le_bytes(v.to_le_bytes()));
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

/// Tagged items after a fixed part: a one-byte tag, a two-byte length
/// and the bytes, repeated; tag 0 ends it. Strings are UTF-8.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Tail {
    items: Vec<(u8, Vec<u8>)>,
}

impl Tail {
    /// Add an item.
    pub fn put(&mut self, tag: u8, bytes: &[u8]) {
        self.items.push((tag, bytes.to_vec()));
    }

    /// Add a string.
    pub fn put_str(&mut self, tag: u8, s: &str) {
        self.put(tag, s.as_bytes());
    }

    /// Every item with `tag`, in order.
    pub fn all(&self, tag: u8) -> impl Iterator<Item = &[u8]> {
        self.items
            .iter()
            .filter(move |(t, _)| *t == tag)
            .map(|(_, b)| b.as_slice())
    }

    /// The first string with `tag`, or empty.
    #[must_use]
    pub fn str(&self, tag: u8) -> String {
        self.all(tag)
            .next()
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .unwrap_or_default()
    }

    /// Every string with `tag`.
    #[must_use]
    pub fn strs(&self, tag: u8) -> Vec<String> {
        self.all(tag)
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .collect()
    }

    /// Encode, terminated.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for (t, b) in &self.items {
            let n = u16::try_from(b.len()).unwrap_or(u16::MAX);
            out.push(*t);
            out.extend_from_slice(&n.to_le_bytes());
            out.extend_from_slice(&b[..usize::from(n)]);
        }
        out.push(0);
        out
    }

    /// Decode what [`Tail::encode`] wrote.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let mut items = Vec::new();
        let mut at = 0;
        loop {
            let Some(&tag) = b.get(at) else {
                return Err(SecError::Corrupt("tail not terminated".into()));
            };
            if tag == 0 {
                return Ok(Self { items });
            }
            let n = b
                .get(at + 1..at + 3)
                .map(|l| usize::from(u16::from_le_bytes([l[0], l[1]])))
                .ok_or_else(|| SecError::Corrupt("tail truncated".into()))?;
            let v = b
                .get(at + 3..at + 3 + n)
                .ok_or_else(|| SecError::Corrupt("tail item truncated".into()))?;
            items.push((tag, v.to_vec()));
            at += 3 + n;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const L: Layout = Layout {
        name: "Test",
        size: Some(24),
        fields: &[
            field("Name", 0, 10),
            field("Count", 10, 2),
            field("Big", 16, 8),
        ],
    };

    #[test]
    fn fields_and_tail() {
        let mut r = Rec::new(&L);
        r.set_text("Name", "QSYS");
        r.set_uint("Count", 513);
        r.set_int("Big", -1);
        let back = Rec::read(&L, &r.buf).unwrap();
        assert_eq!(back.text("Name"), "QSYS");
        assert_eq!(back.uint("Count"), 513);
        assert_eq!(back.int("Big"), -1);
        let mut t = Tail::default();
        t.put_str(1, "a");
        t.put_str(1, "bc");
        t.put_str(2, "/home/x");
        let t2 = Tail::decode(&t.encode()).unwrap();
        assert_eq!(t2.strs(1), vec!["a", "bc"]);
        assert_eq!(t2.str(2), "/home/x");
        assert!(Tail::decode(&[5, 9]).is_err());
    }
}
