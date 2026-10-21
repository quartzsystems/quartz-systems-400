// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The codec: [`Wire`] for the primitives and containers, and the
//! macros for structs and enums.

mod impls;
mod macros;

use crate::WireError;

/// A value that can be put in a message and read back.
pub trait Wire: Sized {
    /// Append the encoding.
    fn put(&self, out: &mut Vec<u8>);
    /// Read one value.
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError>;

    /// Append a slice of values; bytes go in one copy.
    fn put_slice(items: &[Self], out: &mut Vec<u8>) {
        for i in items {
            i.put(out);
        }
    }
    /// Read `n` values.
    fn get_vec(d: &mut Dec<'_>, n: usize) -> Result<Vec<Self>, WireError> {
        let mut v = Vec::with_capacity(n.min(4096));
        for _ in 0..n {
            v.push(Self::get(d)?);
        }
        Ok(v)
    }
}

/// A cursor over an encoded message.
pub struct Dec<'a> {
    buf: &'a [u8],
    at: usize,
}

impl<'a> Dec<'a> {
    /// At the start of `buf`.
    #[must_use]
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, at: 0 }
    }

    /// The next `n` bytes.
    pub fn take(&mut self, n: usize) -> Result<&'a [u8], WireError> {
        let end = self
            .at
            .checked_add(n)
            .filter(|e| *e <= self.buf.len())
            .ok_or(WireError::Truncated { at: self.at })?;
        let s = &self.buf[self.at..end];
        self.at = end;
        Ok(s)
    }

    /// Bytes not read yet.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.buf.len() - self.at
    }
}

/// Encode a value.
#[must_use]
pub fn encode<T: Wire>(v: &T) -> Vec<u8> {
    let mut out = Vec::new();
    v.put(&mut out);
    out
}

/// Decode a value that fills `buf` exactly.
pub fn decode<T: Wire>(buf: &[u8]) -> Result<T, WireError> {
    let mut d = Dec::new(buf);
    let v = T::get(&mut d)?;
    match d.remaining() {
        0 => Ok(v),
        extra => Err(WireError::Trailing { extra }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct Point {
        x: i64,
        name: String,
        tags: Vec<String>,
        raw: Vec<u8>,
        next: Option<Box<Point>>,
    }
    crate::wire_struct!(Point { x, name, tags, raw, next });

    #[derive(Debug, Clone, PartialEq)]
    enum Shape {
        Empty,
        Dot { at: Point },
        Line { from: (u32, u32), to: (u32, u32), bold: bool },
    }
    crate::wire_enum!(Shape {
        0 => Empty,
        1 => Dot { at },
        2 => Line { from, to, bold },
    });

    #[test]
    fn round_trip() {
        let p = Point {
            x: -7,
            name: "QSYS".into(),
            tags: vec!["a".into(), String::new()],
            raw: vec![0, 1, 0xFF],
            next: Some(Box::new(Point {
                x: 1,
                name: "é".into(),
                tags: Vec::new(),
                raw: Vec::new(),
                next: None,
            })),
        };
        for s in [
            Shape::Empty,
            Shape::Dot { at: p.clone() },
            Shape::Line {
                from: (1, 2),
                to: (3, u32::MAX),
                bold: true,
            },
        ] {
            assert_eq!(decode::<Shape>(&encode(&s)).unwrap(), s);
        }
    }

    #[test]
    fn errors() {
        assert!(matches!(
            decode::<Shape>(&[9]),
            Err(WireError::Tag { tag: 9, .. })
        ));
        assert!(matches!(
            decode::<u32>(&[1, 2]),
            Err(WireError::Truncated { .. })
        ));
        assert!(matches!(
            decode::<u8>(&[1, 2]),
            Err(WireError::Trailing { extra: 1 })
        ));
        assert!(matches!(
            decode::<String>(&[1, 0, 0, 0, 0xFF]),
            Err(WireError::Utf8)
        ));
    }
}
