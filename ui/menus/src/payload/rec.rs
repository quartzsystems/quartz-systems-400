// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A fixed record over a [`Layout`], as `work/wm/src/rec.rs` keeps the
//! work-management payloads: text is CP037, blank padded; integers are
//! little-endian. `tests/spec.rs` compares the layout with the table
//! in `docs/MENUS.md`.

pub use uim::spec::{Field, Layout, field};

use crate::Error;

/// A record being built or read.
pub struct Rec<'l> {
    layout: &'l Layout,
    /// The bytes.
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
            .map(|c| ccsid::byte_of(c).unwrap_or(0x40))
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
        let k = b.len().min(8);
        v[..k].copy_from_slice(&b[..k]);
        u64::from_le_bytes(v)
    }

    /// Set an unsigned integer of the field's size.
    pub fn set_uint(&mut self, name: &str, v: u64) {
        let (_, n) = self.at(name);
        let b = v.to_le_bytes();
        self.set_bytes(name, &b[..n.min(8)]);
    }
}
