// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A small XDR writer and reader (RFC 4506) for the calls written by hand
//! here: NLM, NSM and NFS version 4. Big-endian, four-byte units, zero
//! padding.

use crate::Error;

/// Builds XDR bytes.
#[derive(Debug, Default, Clone)]
pub struct W(pub Vec<u8>);

impl W {
    /// An empty buffer.
    pub fn new() -> W {
        W(Vec::new())
    }

    /// An unsigned int.
    pub fn u32(&mut self, v: u32) -> &mut W {
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }

    /// A signed int.
    pub fn i32(&mut self, v: i32) -> &mut W {
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }

    /// An unsigned hyper.
    pub fn u64(&mut self, v: u64) -> &mut W {
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }

    /// A bool.
    pub fn bool(&mut self, v: bool) -> &mut W {
        self.u32(u32::from(v))
    }

    /// Fixed-length opaque data, padded.
    pub fn fixed(&mut self, v: &[u8]) -> &mut W {
        self.0.extend_from_slice(v);
        let pad = (4 - v.len() % 4) % 4;
        self.0.extend(std::iter::repeat_n(0, pad));
        self
    }

    /// Variable-length opaque data or a string.
    pub fn opaque(&mut self, v: &[u8]) -> &mut W {
        self.u32(u32::try_from(v.len()).expect("short"));
        self.fixed(v)
    }

    /// Bytes as they are.
    pub fn raw(&mut self, v: &[u8]) -> &mut W {
        self.0.extend_from_slice(v);
        self
    }

    /// The bytes.
    pub fn done(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.0)
    }
}

/// Reads XDR bytes.
#[derive(Debug, Clone)]
pub struct R<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> R<'a> {
    /// A reader of `b`.
    pub fn new(b: &'a [u8]) -> R<'a> {
        R { b, at: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let end = self
            .at
            .checked_add(n)
            .filter(|e| *e <= self.b.len())
            .ok_or_else(|| Error::Decode(format!("short by {n} bytes")))?;
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }

    /// An unsigned int.
    pub fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().expect("4")))
    }

    /// A signed int.
    pub fn i32(&mut self) -> Result<i32, Error> {
        Ok(i32::from_be_bytes(self.take(4)?.try_into().expect("4")))
    }

    /// An unsigned hyper.
    pub fn u64(&mut self) -> Result<u64, Error> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().expect("8")))
    }

    /// A bool (0 or 1 only).
    pub fn bool(&mut self) -> Result<bool, Error> {
        match self.u32()? {
            0 => Ok(false),
            1 => Ok(true),
            v => Err(Error::Decode(format!("bool {v}"))),
        }
    }

    /// Fixed-length opaque data of `n` bytes.
    pub fn fixed(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let v = self.take(n)?;
        let pad = (4 - n % 4) % 4;
        if self.take(pad)?.iter().any(|b| *b != 0) {
            return Err(Error::Decode("nonzero padding".into()));
        }
        Ok(v)
    }

    /// Variable-length opaque data of at most `max` bytes.
    pub fn opaque(&mut self, max: usize) -> Result<&'a [u8], Error> {
        let n = self.u32()? as usize;
        if n > max {
            return Err(Error::Decode(format!("length {n} over {max}")));
        }
        self.fixed(n)
    }

    /// A count of at most `max`.
    pub fn count(&mut self, max: usize) -> Result<usize, Error> {
        let n = self.u32()? as usize;
        if n > max || n > self.left() / 4 + 1 {
            return Err(Error::Decode(format!("count {n}")));
        }
        Ok(n)
    }

    /// Bytes not read yet.
    pub fn left(&self) -> usize {
        self.b.len() - self.at
    }

    /// The rest, as it is.
    pub fn rest(&mut self) -> &'a [u8] {
        let s = &self.b[self.at..];
        self.at = self.b.len();
        s
    }

    /// Fails when bytes are left.
    pub fn end(&self) -> Result<(), Error> {
        if self.left() == 0 {
            Ok(())
        } else {
            Err(Error::Decode(format!("{} bytes left", self.left())))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_with_padding() {
        let b = W::new()
            .u32(7)
            .opaque(b"abcde")
            .u64(1 << 40)
            .bool(true)
            .done();
        assert_eq!(b.len(), 4 + 4 + 8 + 8 + 4);
        let mut r = R::new(&b);
        assert_eq!(r.u32().unwrap(), 7);
        assert_eq!(r.opaque(8).unwrap(), b"abcde");
        assert_eq!(r.u64().unwrap(), 1 << 40);
        assert!(r.bool().unwrap());
        r.end().unwrap();
        assert!(R::new(&b[..6]).u64().is_err());
    }
}
