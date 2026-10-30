// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! DDM objects: a two-byte length (`LL`, header included), a two-byte
//! code point and the data. A length with the high bit on is extended:
//! its low bits less four give how many bytes after the code point hold
//! the data length. Commands, reply messages and a few parameters are
//! collections of further objects; the rest are scalars.

use crate::{Error, be16, cp};

/// The object header: `LL` and `CP`.
pub const OBJ_HEAD: usize = 4;

/// The contents of an object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    /// Scalar data (or a collection that did not parse).
    Data(Vec<u8>),
    /// Nested objects.
    Coll(Vec<Object>),
}

/// One object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Object {
    /// Its code point.
    pub cp: u16,
    /// Extended length bytes (0 for an ordinary length).
    pub ext: u8,
    /// Its contents.
    pub body: Body,
}

impl Object {
    /// A scalar.
    pub fn data(cp: u16, data: impl Into<Vec<u8>>) -> Self {
        Self {
            cp,
            ext: 0,
            body: Body::Data(data.into()),
        }
    }

    /// A collection.
    pub fn coll(cp: u16, items: Vec<Object>) -> Self {
        Self {
            cp,
            ext: 0,
            body: Body::Coll(items),
        }
    }

    /// A `u16` scalar.
    pub fn u16(cp: u16, v: u16) -> Self {
        Self::data(cp, v.to_be_bytes())
    }

    /// A `u32` scalar.
    pub fn u32(cp: u16, v: u32) -> Self {
        Self::data(cp, v.to_be_bytes())
    }

    /// A one-byte scalar.
    pub fn byte(cp: u16, v: u8) -> Self {
        Self::data(cp, vec![v])
    }

    /// An EBCDIC character scalar.
    pub fn chars(cp: u16, text: &str) -> Self {
        Self::data(cp, ebcdic(text))
    }

    /// An EBCDIC scalar padded with blanks to `len`.
    pub fn chars_fixed(cp: u16, text: &str, len: usize) -> Self {
        Self::data(cp, ebcdic_fixed(text, len))
    }

    /// The data of a scalar (empty for a collection).
    pub fn bytes(&self) -> &[u8] {
        match &self.body {
            Body::Data(d) => d,
            Body::Coll(_) => &[],
        }
    }

    /// The items of a collection (empty for a scalar).
    pub fn items(&self) -> &[Object] {
        match &self.body {
            Body::Coll(c) => c,
            Body::Data(_) => &[],
        }
    }

    /// The first item with code point `cp`.
    pub fn get(&self, cp: u16) -> Option<&Object> {
        self.items().iter().find(|o| o.cp == cp)
    }

    /// Every item with code point `cp`.
    pub fn all(&self, cp: u16) -> impl Iterator<Item = &Object> {
        self.items().iter().filter(move |o| o.cp == cp)
    }

    /// The `u16` value of item `cp`.
    pub fn get_u16(&self, cp: u16) -> Option<u16> {
        let b = self.get(cp)?.bytes();
        (b.len() >= 2).then(|| be16(b, 0))
    }

    /// The `u32` value of item `cp`.
    pub fn get_u32(&self, cp: u16) -> Option<u32> {
        let b = self.get(cp)?.bytes();
        (b.len() >= 4).then(|| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// Item `cp` as EBCDIC text, trailing blanks removed.
    pub fn get_chars(&self, cp: u16) -> Option<String> {
        Some(text(self.get(cp)?.bytes()))
    }

    /// The severity of a reply message (`SVRCOD`), 0 when absent.
    pub fn svrcod(&self) -> u16 {
        self.get_u16(cp::param::SVRCOD).unwrap_or(0)
    }

    /// The encoded length.
    pub fn len(&self) -> usize {
        let content = self.content_len();
        OBJ_HEAD + ext_of(self.ext, content) + content
    }

    /// Always false: an object has at least its header.
    pub fn is_empty(&self) -> bool {
        false
    }

    fn content_len(&self) -> usize {
        match &self.body {
            Body::Data(d) => d.len(),
            Body::Coll(c) => c.iter().map(Object::len).sum(),
        }
    }

    /// Append the encoding to `out`.
    pub fn encode_into(&self, out: &mut Vec<u8>) {
        let content = self.content_len();
        let ext = ext_of(self.ext, content);
        if ext == 0 {
            out.extend_from_slice(&u16::try_from(OBJ_HEAD + content).unwrap_or(0).to_be_bytes());
        } else {
            out.extend_from_slice(&(0x8000 | u16::try_from(ext + 4).unwrap_or(0)).to_be_bytes());
        }
        out.extend_from_slice(&self.cp.to_be_bytes());
        if ext > 0 {
            let n = (content as u64).to_be_bytes();
            out.extend_from_slice(&n[8 - ext.min(8)..]);
        }
        match &self.body {
            Body::Data(d) => out.extend_from_slice(d),
            Body::Coll(c) => {
                for o in c {
                    o.encode_into(out);
                }
            }
        }
    }

    /// The encoding.
    pub fn encode(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(self.len());
        self.encode_into(&mut v);
        v
    }

    /// Read one object from the front of `b`; returns it and its length.
    pub fn decode(b: &[u8]) -> Result<(Self, usize), Error> {
        if b.len() < OBJ_HEAD {
            return Err(Error::Short {
                what: "object header",
                need: OBJ_HEAD,
                have: b.len(),
            });
        }
        let ll = be16(b, 0);
        let code = be16(b, 2);
        let (ext, start, total) = if ll & 0x8000 != 0 {
            let ext = usize::from(ll & 0x7FFF).saturating_sub(4);
            if ext == 0 || ext > 8 || b.len() < OBJ_HEAD + ext {
                return Err(Error::Invalid(format!("extended length of {code:04X}")));
            }
            let mut n: u64 = 0;
            for &x in &b[OBJ_HEAD..OBJ_HEAD + ext] {
                n = (n << 8) | u64::from(x);
            }
            let n = usize::try_from(n).map_err(|_| Error::Invalid("object too long".into()))?;
            (ext, OBJ_HEAD + ext, OBJ_HEAD + ext + n)
        } else {
            let ll = usize::from(ll);
            if ll < OBJ_HEAD {
                return Err(Error::Invalid(format!("object {code:04X} length {ll}")));
            }
            (0, OBJ_HEAD, ll)
        };
        if b.len() < total {
            return Err(Error::Short {
                what: "object",
                need: total,
                have: b.len(),
            });
        }
        let data = &b[start..total];
        let body = if cp::is_collection(code) {
            match decode_all(data) {
                Ok(items) => Body::Coll(items),
                Err(_) => Body::Data(data.to_vec()),
            }
        } else {
            Body::Data(data.to_vec())
        };
        Ok((
            Self {
                cp: code,
                ext: u8::try_from(ext).unwrap_or(0),
                body,
            },
            total,
        ))
    }
}

/// The extended length bytes an object uses: as decoded, or four when
/// the content will not fit an ordinary length.
fn ext_of(ext: u8, content: usize) -> usize {
    if ext == 0 && OBJ_HEAD + content > 0x7FFF {
        4
    } else {
        usize::from(ext)
    }
}

/// Read objects until `b` ends exactly.
pub fn decode_all(mut b: &[u8]) -> Result<Vec<Object>, Error> {
    let mut out = Vec::new();
    while !b.is_empty() {
        let (o, n) = Object::decode(b)?;
        out.push(o);
        b = &b[n..];
    }
    Ok(out)
}

/// Text as CCSID 37, unmappable characters as `?`.
pub fn ebcdic(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| ccsid::byte_of(c).unwrap_or(0x6F))
        .collect()
}

/// Text as CCSID 37 padded with blanks (or cut) to `len`.
pub fn ebcdic_fixed(text: &str, len: usize) -> Vec<u8> {
    let mut v = ebcdic(text);
    v.resize(len, 0x40);
    v
}

/// CCSID 37 bytes as text, trailing blanks removed.
pub fn text(b: &[u8]) -> String {
    b.iter()
        .map(|&x| ccsid::char_of(x))
        .collect::<String>()
        .trim_end()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_round_trip() {
        let o = Object::coll(
            cp::cmd::EXCSAT,
            vec![
                Object::chars(cp::param::EXTNAM, "QS400"),
                Object::data(cp::param::MGRLVLLS, vec![0x14, 0x03, 0, 7]),
            ],
        );
        let b = o.encode();
        assert_eq!(be16(&b, 0) as usize, b.len());
        let (d, n) = Object::decode(&b).unwrap();
        assert_eq!(n, b.len());
        assert_eq!(d, o);
    }

    #[test]
    fn extended_length() {
        let o = Object::data(cp::obj::QRYDTA, vec![7u8; 40000]);
        let b = o.encode();
        assert_eq!(be16(&b, 0), 0x8008);
        let (d, _) = Object::decode(&b).unwrap();
        assert_eq!(d.bytes().len(), 40000);
        assert_eq!(d.encode(), b);
    }
}
