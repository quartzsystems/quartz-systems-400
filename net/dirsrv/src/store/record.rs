// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The records the trees hold (docs/DIRSRV.md §5.2): `EntryRec`, the
//! index definitions of entry 0, `ChangeRec`. Frozen at V1R2M0.

use store::spec::{Layout, field};

use super::error::{DirError, Result};
use super::format::{IndexKind, MAX_RECORD, le};

/// Longest attribute description.
pub const MAX_ATTR_NAME: usize = 1024;
/// Longest attribute name in an index definition.
pub const MAX_INDEX_NAME: usize = 255;

const ENTRY_HEADER: usize = 24;
const CHANGE_HEADER: usize = 32;

/// The spec layout of an `EntryRec`'s header.
pub static ENTRY_HEADER_LAYOUT: Layout = Layout {
    name: "EntryRecHeader",
    size: Some(ENTRY_HEADER),
    fields: &[
        field("version", 0, 1),
        field("flags", 1, 1),
        field("attr_count", 2, 2),
        field("rdn_len", 4, 2),
        field("rdn_key_len", 6, 2),
        field("parent", 8, 8),
        field("total_len", 16, 4),
        field("reserved", 20, 4),
    ],
};

/// The spec layout of an attribute's header.
pub static ATTR_HEADER_LAYOUT: Layout = Layout {
    name: "AttrHeader",
    size: Some(8),
    fields: &[
        field("name_len", 0, 2),
        field("reserved", 2, 2),
        field("value_count", 4, 4),
    ],
};

/// The spec layout of a value's header.
pub static VALUE_HEADER_LAYOUT: Layout = Layout {
    name: "ValueHeader",
    size: Some(4),
    fields: &[field("len", 0, 4)],
};

/// The spec layout of entry 0's header.
pub static DEFS_HEADER_LAYOUT: Layout = Layout {
    name: "IndexDefsHeader",
    size: Some(4),
    fields: &[
        field("version", 0, 1),
        field("reserved", 1, 1),
        field("count", 2, 2),
    ],
};

/// The spec layout of an index definition.
pub static DEF_LAYOUT: Layout = Layout {
    name: "IndexDef",
    size: Some(4),
    fields: &[
        field("number", 0, 2),
        field("kinds", 2, 1),
        field("name_len", 3, 1),
    ],
};

/// The spec layout of a `ChangeRec`'s header.
pub static CHANGE_HEADER_LAYOUT: Layout = Layout {
    name: "ChangeRecHeader",
    size: Some(CHANGE_HEADER),
    fields: &[
        field("version", 0, 1),
        field("kind", 1, 1),
        field("flags", 2, 1),
        field("reserved", 3, 1),
        field("total_len", 4, 4),
        field("time", 8, 8),
        field("entry", 16, 8),
        field("reserved2", 24, 8),
    ],
};

/// One attribute of an entry: its description and values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attr {
    /// The description as stored (type and options).
    pub name: String,
    /// The values.
    pub values: Vec<Vec<u8>>,
}

impl Attr {
    /// An attribute of text values.
    #[must_use]
    pub fn text(name: &str, values: &[&str]) -> Self {
        Self {
            name: name.into(),
            values: values.iter().map(|v| v.as_bytes().to_vec()).collect(),
        }
    }
}

/// An entry as the store keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Entry {
    /// The parent's entry ID, 0 for a suffix root (the store sets it).
    pub parent: u64,
    /// Whether this is a suffix's root (the store sets it).
    pub suffix: bool,
    /// The RDN as the client wrote it; a suffix root's whole DN.
    pub rdn: String,
    /// The normalized RDN, or a suffix key (the store sets it).
    pub rdn_key: Vec<u8>,
    /// The attributes, operational ones included.
    pub attrs: Vec<Attr>,
}

impl Entry {
    /// An entry named `rdn` with `attrs`.
    #[must_use]
    pub fn new(rdn: &str, attrs: Vec<Attr>) -> Self {
        Self {
            rdn: rdn.into(),
            attrs,
            ..Self::default()
        }
    }

    /// The first attribute whose description is `name`, ASCII case
    /// ignored.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Attr> {
        self.attrs
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(name))
    }

    /// The `EntryRec` bytes.
    ///
    /// # Errors
    ///
    /// A field past its limit.
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut b = vec![0u8; ENTRY_HEADER];
        b[0] = 1;
        b[1] = u8::from(self.suffix);
        le::put_u16(&mut b, 2, count16(self.attrs.len(), "attributes")?);
        le::put_u16(&mut b, 4, count16(self.rdn.len(), "RDN")?);
        le::put_u16(&mut b, 6, count16(self.rdn_key.len(), "normalized RDN")?);
        le::put_u64(&mut b, 8, self.parent);
        b.extend_from_slice(self.rdn.as_bytes());
        b.extend_from_slice(&self.rdn_key);
        for a in &self.attrs {
            if a.name.is_empty() || a.name.len() > MAX_ATTR_NAME {
                return Err(DirError::Limit(format!(
                    "attribute description of {} bytes",
                    a.name.len()
                )));
            }
            let mut h = [0u8; 8];
            le::put_u16(&mut h, 0, u16::try_from(a.name.len()).expect("checked"));
            le::put_u32(&mut h, 4, count32(a.values.len())?);
            b.extend_from_slice(&h);
            b.extend_from_slice(a.name.as_bytes());
            for v in &a.values {
                b.extend_from_slice(&count32(v.len())?.to_le_bytes());
                b.extend_from_slice(v);
                if b.len() > MAX_RECORD {
                    return Err(too_long());
                }
            }
        }
        finish(&mut b, 16)?;
        Ok(b)
    }

    /// Read an `EntryRec`.
    ///
    /// # Errors
    ///
    /// Bytes that are not a well-formed version 1 record.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let mut r = Reader::new(b, "entry record");
        let h = r.take(ENTRY_HEADER)?;
        if h[0] != 1 || le::u32_at(h, 16) as usize != b.len() {
            return Err(r.bad("header"));
        }
        let attr_count = le::u16_at(h, 2);
        let rdn = r.text(usize::from(le::u16_at(h, 4)))?;
        let rdn_key = r.take(usize::from(le::u16_at(h, 6)))?.to_vec();
        let mut attrs = Vec::with_capacity(usize::from(attr_count));
        for _ in 0..attr_count {
            let ah = r.take(8)?;
            let (name_len, n) = (usize::from(le::u16_at(ah, 0)), le::u32_at(ah, 4));
            let name = r.text(name_len)?;
            let mut values = Vec::new();
            for _ in 0..n {
                let len = le::u32_at(r.take(4)?, 0) as usize;
                values.push(r.take(len)?.to_vec());
            }
            attrs.push(Attr { name, values });
        }
        r.end()?;
        Ok(Self {
            parent: le::u64_at(h, 8),
            suffix: h[1] & 1 != 0,
            rdn,
            rdn_key,
            attrs,
        })
    }
}

/// An index definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexDef {
    /// The number in its keys.
    pub number: u16,
    /// The attribute, as `Normalizer::attr_id` names it.
    pub attr: String,
    /// `IndexKind::bit`s.
    pub kinds: u8,
    /// Shipped with the store and never removed.
    pub always: bool,
}

impl IndexDef {
    /// Whether it keeps `kind` keys.
    #[must_use]
    pub fn has(&self, kind: IndexKind) -> bool {
        self.kinds & kind.bit() != 0
    }
}

const ALWAYS: u8 = 0x80;

/// Entry 0's bytes for `defs`.
///
/// # Errors
///
/// A name past its limit, or too many definitions.
pub fn encode_defs(defs: &[IndexDef]) -> Result<Vec<u8>> {
    let mut b = vec![1, 0];
    b.extend_from_slice(&count16(defs.len(), "indexes")?.to_le_bytes());
    for d in defs {
        if d.attr.is_empty() || d.attr.len() > MAX_INDEX_NAME {
            return Err(DirError::Limit(format!(
                "index attribute of {} bytes",
                d.attr.len()
            )));
        }
        b.extend_from_slice(&d.number.to_le_bytes());
        b.push((d.kinds & 0x07) | if d.always { ALWAYS } else { 0 });
        b.push(u8::try_from(d.attr.len()).expect("checked"));
        b.extend_from_slice(d.attr.as_bytes());
    }
    Ok(b)
}

/// Read entry 0.
///
/// # Errors
///
/// Bytes that are not well formed.
pub fn decode_defs(b: &[u8]) -> Result<Vec<IndexDef>> {
    let mut r = Reader::new(b, "index definitions");
    let h = r.take(4)?;
    if h[0] != 1 {
        return Err(r.bad("version"));
    }
    let mut out = Vec::new();
    for _ in 0..le::u16_at(h, 2) {
        let d = r.take(4)?;
        let attr = r.text(usize::from(d[3]))?;
        out.push(IndexDef {
            number: le::u16_at(d, 0),
            attr,
            kinds: d[2] & 0x07,
            always: d[2] & ALWAYS != 0,
        });
    }
    r.end()?;
    Ok(out)
}

/// What a change did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// An entry added.
    Add = 1,
    /// An entry deleted.
    Delete = 2,
    /// An entry modified.
    Modify = 3,
    /// An entry renamed or moved.
    ModDn = 4,
}

/// One change log record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeRec {
    /// What changed.
    pub kind: ChangeKind,
    /// When, Unix milliseconds.
    pub time: u64,
    /// The entry.
    pub entry: u64,
    /// The entry's DN before the change.
    pub target_dn: String,
    /// A modify DN's new RDN.
    pub new_rdn: Option<String>,
    /// A modify DN's `deleteOldRDN`.
    pub delete_old_rdn: bool,
    /// A modify DN's new superior.
    pub new_superior: Option<String>,
    /// Who changed it.
    pub modifier: String,
    /// LDIF of the added entry or the modifications.
    pub changes: Vec<u8>,
}

impl ChangeRec {
    /// The `ChangeRec` bytes.
    ///
    /// # Errors
    ///
    /// A record past the limit.
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut b = vec![0u8; CHANGE_HEADER];
        b[0] = 1;
        b[1] = self.kind as u8;
        b[2] = u8::from(self.delete_old_rdn)
            | (u8::from(self.new_rdn.is_some()) << 1)
            | (u8::from(self.new_superior.is_some()) << 2);
        le::put_u64(&mut b, 8, self.time);
        le::put_u64(&mut b, 16, self.entry);
        let fields: [&[u8]; 5] = [
            self.target_dn.as_bytes(),
            self.new_rdn.as_deref().unwrap_or("").as_bytes(),
            self.new_superior.as_deref().unwrap_or("").as_bytes(),
            self.modifier.as_bytes(),
            &self.changes,
        ];
        for f in fields {
            b.extend_from_slice(&count32(f.len())?.to_le_bytes());
            b.extend_from_slice(f);
        }
        finish(&mut b, 4)?;
        Ok(b)
    }

    /// Read a `ChangeRec`.
    ///
    /// # Errors
    ///
    /// Bytes that are not a well-formed version 1 record.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let mut r = Reader::new(b, "change record");
        let h = r.take(CHANGE_HEADER)?;
        if h[0] != 1 || le::u32_at(h, 4) as usize != b.len() {
            return Err(r.bad("header"));
        }
        let kind = match h[1] {
            1 => ChangeKind::Add,
            2 => ChangeKind::Delete,
            3 => ChangeKind::Modify,
            4 => ChangeKind::ModDn,
            _ => return Err(r.bad("kind")),
        };
        let field = |r: &mut Reader<'_>| -> Result<Vec<u8>> {
            let len = le::u32_at(r.take(4)?, 0) as usize;
            Ok(r.take(len)?.to_vec())
        };
        let target = field(&mut r)?;
        let new_rdn = field(&mut r)?;
        let new_sup = field(&mut r)?;
        let modifier = field(&mut r)?;
        let changes = field(&mut r)?;
        r.end()?;
        let text = |v: Vec<u8>| String::from_utf8(v).map_err(|_| corrupt("change record text"));
        Ok(Self {
            kind,
            time: le::u64_at(h, 8),
            entry: le::u64_at(h, 16),
            target_dn: text(target)?,
            new_rdn: (h[2] & 2 != 0).then(|| text(new_rdn)).transpose()?,
            delete_old_rdn: h[2] & 1 != 0,
            new_superior: (h[2] & 4 != 0).then(|| text(new_sup)).transpose()?,
            modifier: text(modifier)?,
            changes,
        })
    }
}

fn too_long() -> DirError {
    DirError::Limit(format!("record longer than {MAX_RECORD} bytes"))
}

fn count16(n: usize, what: &str) -> Result<u16> {
    u16::try_from(n).map_err(|_| DirError::Limit(format!("{n} bytes or items of {what}")))
}

fn count32(n: usize) -> Result<u32> {
    if n > MAX_RECORD {
        return Err(too_long());
    }
    Ok(u32::try_from(n).expect("below the limit"))
}

/// Check the length and write it at `at`.
fn finish(b: &mut [u8], at: usize) -> Result<()> {
    if b.len() > MAX_RECORD {
        return Err(too_long());
    }
    let n = u32::try_from(b.len()).expect("below the limit");
    le::put_u32(b, at, n);
    Ok(())
}

fn corrupt(what: &str) -> DirError {
    DirError::Corrupt(what.into())
}

/// A bounds-checked cursor over a record.
struct Reader<'a> {
    b: &'a [u8],
    at: usize,
    what: &'static str,
}

impl<'a> Reader<'a> {
    fn new(b: &'a [u8], what: &'static str) -> Self {
        Self { b, at: 0, what }
    }

    fn bad(&self, part: &str) -> DirError {
        DirError::Corrupt(format!("{}: {part} at byte {}", self.what, self.at))
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.b.len());
        let Some(end) = end else {
            return Err(self.bad("short"));
        };
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }

    fn text(&mut self, n: usize) -> Result<String> {
        let b = self.take(n)?;
        String::from_utf8(b.to_vec()).map_err(|_| self.bad("text"))
    }

    fn end(&self) -> Result<()> {
        if self.at == self.b.len() {
            Ok(())
        } else {
            Err(self.bad("trailing bytes"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_round_trip() {
        let e = Entry {
            parent: 4,
            suffix: false,
            rdn: "cn=Bob".into(),
            rdn_key: b"cn=bob".to_vec(),
            attrs: vec![
                Attr::text("objectClass", &["top", "person"]),
                Attr::text("cn", &["Bob"]),
                Attr {
                    name: "jpegPhoto".into(),
                    values: vec![vec![0xFF; 5000]],
                },
            ],
        };
        let b = e.encode().unwrap();
        assert_eq!(Entry::decode(&b).unwrap(), e);
        assert!(Entry::decode(&b[..b.len() - 1]).is_err());
        let defs = vec![IndexDef {
            number: 1,
            attr: "cn".into(),
            kinds: 7,
            always: true,
        }];
        assert_eq!(decode_defs(&encode_defs(&defs).unwrap()).unwrap(), defs);
        let c = ChangeRec {
            kind: ChangeKind::ModDn,
            time: 9,
            entry: 4,
            target_dn: "cn=Bob,o=x".into(),
            new_rdn: Some("cn=Rob".into()),
            delete_old_rdn: true,
            new_superior: None,
            modifier: "cn=Administrator".into(),
            changes: Vec::new(),
        };
        assert_eq!(ChangeRec::decode(&c.encode().unwrap()).unwrap(), c);
    }
}
