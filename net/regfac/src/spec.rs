// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every fixed layout of the crate, by the name its table has in
//! `docs/HOSTSVR.md` (§ Exit points): the `*EXITRG` payload records, the
//! `EXTI` receivers and the exit point formats. `tests/spec.rs`
//! compares them with the tables. Encoders write through these so code
//! and tables change together.

/// One field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Field {
    /// Name, as the table gives it.
    pub name: &'static str,
    /// Offset from the start.
    pub offset: usize,
    /// Size in bytes.
    pub size: usize,
}

/// A layout: the fixed part of a record or structure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    /// Name, as the table gives it.
    pub name: &'static str,
    /// Size of the fixed part.
    pub size: usize,
    /// Fields.
    pub fields: &'static [Field],
}

/// A field.
#[must_use]
pub const fn field(name: &'static str, offset: usize, size: usize) -> Field {
    Field { name, offset, size }
}

impl Layout {
    /// A field by name.
    ///
    /// # Panics
    ///
    /// When the layout has no such field: a coding error.
    #[must_use]
    pub fn get(&self, name: &str) -> Field {
        *self
            .fields
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("{} has no field {name}", self.name))
    }

    /// A zeroed buffer of the fixed size.
    #[must_use]
    pub fn zeroed(&self) -> Rec {
        Rec {
            layout: *self,
            buf: vec![0; self.size],
        }
    }

    /// A buffer of the fixed size, character fields blank.
    #[must_use]
    pub fn blank(&self) -> Rec {
        Rec {
            layout: *self,
            buf: vec![0x40; self.size],
        }
    }
}

/// A record being written through its layout.
#[derive(Clone, Debug)]
pub struct Rec {
    layout: Layout,
    /// The bytes.
    pub buf: Vec<u8>,
}

impl Rec {
    fn slot(&mut self, name: &str) -> &mut [u8] {
        let f = self.layout.get(name);
        &mut self.buf[f.offset..f.offset + f.size]
    }

    /// A CP037 character field, blank padded.
    pub fn text(&mut self, name: &str, s: &str) -> &mut Self {
        let n = self.layout.get(name).size;
        self.slot(name).copy_from_slice(&crate::text::fixed(s, n));
        self
    }

    /// Raw bytes, zero filled past them.
    pub fn bytes(&mut self, name: &str, b: &[u8]) -> &mut Self {
        let slot = self.slot(name);
        slot.fill(0);
        let n = b.len().min(slot.len());
        slot[..n].copy_from_slice(&b[..n]);
        self
    }

    /// Raw bytes, blank filled past them.
    pub fn bytes_blank(&mut self, name: &str, b: &[u8]) -> &mut Self {
        let slot = self.slot(name);
        slot.fill(0x40);
        let n = b.len().min(slot.len());
        slot[..n].copy_from_slice(&b[..n]);
        self
    }

    /// A big-endian `i32` (`BINARY(4)`).
    pub fn bin4(&mut self, name: &str, v: i32) -> &mut Self {
        self.slot(name).copy_from_slice(&v.to_be_bytes());
        self
    }

    /// A big-endian `i16` (`BINARY(2)`).
    pub fn bin2(&mut self, name: &str, v: i16) -> &mut Self {
        self.slot(name).copy_from_slice(&v.to_be_bytes());
        self
    }

    /// A little-endian `u32` (the payload's counts).
    pub fn le4(&mut self, name: &str, v: u32) -> &mut Self {
        self.slot(name).copy_from_slice(&v.to_le_bytes());
        self
    }

    /// One CP037 character.
    pub fn ch(&mut self, name: &str, c: char) -> &mut Self {
        self.slot(name)[0] = ccsid::byte_of(c).unwrap_or(0x40);
        self
    }

    /// The bytes.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }
}

/// A field of `b` through `layout`.
#[must_use]
pub fn read<'a>(layout: &Layout, b: &'a [u8], name: &str) -> &'a [u8] {
    let f = layout.get(name);
    b.get(f.offset..f.offset + f.size).unwrap_or(&[])
}

/// Every layout the tables give.
#[must_use]
pub fn layouts() -> Vec<&'static Layout> {
    let mut v: Vec<&'static Layout> = Vec::new();
    v.extend(crate::payload::LAYOUTS);
    v.extend(crate::exti::LAYOUTS);
    v.extend(crate::formats::LAYOUTS);
    v
}
