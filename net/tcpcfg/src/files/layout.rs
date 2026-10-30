// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Record formats: the fields of each file and a record buffer that
//! reads and writes them. Character fields are CP037, binary fields
//! big-endian, graphic fields UCS-2.

/// A field's data type (DDS position 35).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ty {
    /// `A`: character; the length is in characters.
    A,
    /// `B`: binary; the length is in digits.
    B,
    /// `G`: graphic; the length is in double-byte characters.
    G,
}

/// One field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Field {
    /// The name.
    pub name: &'static str,
    /// The type.
    pub ty: Ty,
    /// Characters, digits or graphic characters.
    pub len: u32,
    /// The field's text (ours).
    pub text: &'static str,
}

impl Field {
    /// Bytes in the record.
    pub fn bytes(&self) -> usize {
        let n = self.len as usize;
        match self.ty {
            Ty::A => n,
            Ty::B if n <= 4 => 2,
            Ty::B if n <= 9 => 4,
            Ty::B => 8,
            Ty::G => n * 2,
        }
    }
}

/// A file's record format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    /// The file in `QUSRSYS`.
    pub file: &'static str,
    /// The record format name.
    pub format: &'static str,
    /// The file's text (ours).
    pub text: &'static str,
    /// Whether the format is IBM's (field names, order, lengths) or ours.
    pub ibm: bool,
    /// The fields in order.
    pub fields: &'static [Field],
}

impl Layout {
    /// The record length.
    pub fn record_len(&self) -> usize {
        self.fields.iter().map(Field::bytes).sum()
    }

    /// A field's 0-based offset and the field.
    pub fn find(&self, name: &str) -> Option<(usize, &Field)> {
        let mut at = 0;
        for f in self.fields {
            if f.name == name {
                return Some((at, f));
            }
            at += f.bytes();
        }
        None
    }

    /// A blank record.
    pub fn blank(&'static self) -> Rec {
        let mut bytes = vec![0u8; self.record_len()];
        let mut at = 0;
        for f in self.fields {
            let n = f.bytes();
            match f.ty {
                Ty::A => bytes[at..at + n].fill(ccsid::SPACE),
                Ty::B => {}
                Ty::G => {
                    for pair in bytes[at..at + n].chunks_mut(2) {
                        pair.copy_from_slice(&[0x00, 0x20]);
                    }
                }
            }
            at += n;
        }
        Rec { layout: self, bytes }
    }

    /// A record read from the file (shorter buffers are padded blank).
    pub fn rec(&'static self, data: &[u8]) -> Rec {
        let mut r = self.blank();
        let n = data.len().min(r.bytes.len());
        r.bytes[..n].copy_from_slice(&data[..n]);
        r
    }
}

/// One record of a layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rec {
    layout: &'static Layout,
    /// The record's bytes.
    pub bytes: Vec<u8>,
}

impl Rec {
    fn span(&self, name: &str) -> (usize, &'static Field) {
        let layout: &'static Layout = self.layout;
        layout
            .find(name)
            .unwrap_or_else(|| panic!("{} has no field {name}", layout.file))
    }

    /// The record's layout.
    pub fn layout(&self) -> &'static Layout {
        self.layout
    }

    /// Set a character field (characters CP037 has not become `?`; a
    /// longer value is cut).
    pub fn set(&mut self, name: &str, value: &str) -> &mut Self {
        let (at, f) = self.span(name);
        self.put_chars(at, f.bytes(), value);
        self
    }

    /// Set part of a character field: `len` bytes from `off`.
    pub fn set_at(&mut self, name: &str, off: usize, len: usize, value: &str) -> &mut Self {
        let (at, f) = self.span(name);
        let len = len.min(f.bytes().saturating_sub(off));
        self.put_chars(at + off, len, value);
        self
    }

    fn put_chars(&mut self, at: usize, len: usize, value: &str) {
        let slot = &mut self.bytes[at..at + len];
        slot.fill(ccsid::SPACE);
        for (b, ch) in slot.iter_mut().zip(value.chars()) {
            *b = ccsid::byte_of(ch).unwrap_or(0x6f);
        }
    }

    /// A character field, trailing blanks dropped.
    pub fn get(&self, name: &str) -> String {
        let (at, f) = self.span(name);
        ccsid::decode_trimmed(&self.bytes[at..at + f.bytes()])
    }

    /// Part of a character field, trailing blanks dropped.
    pub fn get_at(&self, name: &str, off: usize, len: usize) -> String {
        let (at, f) = self.span(name);
        let end = (off + len).min(f.bytes());
        ccsid::decode_trimmed(&self.bytes[at + off.min(end)..at + end])
    }

    /// Set a binary field.
    pub fn set_bin(&mut self, name: &str, v: i64) -> &mut Self {
        let (at, f) = self.span(name);
        match f.bytes() {
            2 => self.bytes[at..at + 2].copy_from_slice(&i16::try_from(v).unwrap_or(0).to_be_bytes()),
            4 => {
                // An address's 32 bits go in whole, as IBM's binary fields hold them.
                let b = i32::try_from(v)
                    .map(i32::to_be_bytes)
                    .or_else(|_| u32::try_from(v).map(u32::to_be_bytes))
                    .unwrap_or([0; 4]);
                self.bytes[at..at + 4].copy_from_slice(&b);
            }
            _ => self.bytes[at..at + 8].copy_from_slice(&v.to_be_bytes()),
        }
        self
    }

    /// A binary field.
    pub fn bin(&self, name: &str) -> i64 {
        let (at, f) = self.span(name);
        let b = &self.bytes[at..at + f.bytes()];
        match b.len() {
            2 => i64::from(i16::from_be_bytes([b[0], b[1]])),
            4 => i64::from(i32::from_be_bytes([b[0], b[1], b[2], b[3]])),
            _ => i64::from_be_bytes(b.try_into().unwrap_or([0; 8])),
        }
    }

    /// Set a graphic field (UCS-2, cut to fit).
    pub fn set_g(&mut self, name: &str, value: &str) -> &mut Self {
        let (at, f) = self.span(name);
        let slot = &mut self.bytes[at..at + f.bytes()];
        for pair in slot.chunks_mut(2) {
            pair.copy_from_slice(&[0x00, 0x20]);
        }
        let units: Vec<u16> = value.encode_utf16().collect();
        for (pair, u) in slot.chunks_mut(2).zip(units) {
            pair.copy_from_slice(&u.to_be_bytes());
        }
        self
    }

    /// A graphic field, trailing blanks dropped.
    pub fn get_g(&self, name: &str) -> String {
        let (at, f) = self.span(name);
        let units: Vec<u16> = self.bytes[at..at + f.bytes()]
            .chunks(2)
            .map(|p| u16::from_be_bytes([p[0], p[1]]))
            .collect();
        String::from_utf16_lossy(&units).trim_end().to_string()
    }
}

/// A field, shortened.
pub(crate) const fn a(name: &'static str, len: u32, text: &'static str) -> Field {
    Field {
        name,
        ty: Ty::A,
        len,
        text,
    }
}

/// A binary field.
pub(crate) const fn b(name: &'static str, digits: u32, text: &'static str) -> Field {
    Field {
        name,
        ty: Ty::B,
        len: digits,
        text,
    }
}

/// A graphic field.
pub(crate) const fn g(name: &'static str, len: u32, text: &'static str) -> Field {
    Field {
        name,
        ty: Ty::G,
        len,
        text,
    }
}
