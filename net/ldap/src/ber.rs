// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The BER subset LDAP uses (RFC 4511 §5.1): definite lengths only,
//! depth and length limits on input (docs/DIRSRV.md §3).
//!
//! Tags are single octets (LDAP never needs the high tag form). The
//! reader refuses indefinite lengths, constructed strings, lengths and
//! integers not in their shortest form, and anything over its limits.

use std::fmt;

/// The nesting a [`Reader`] allows by default.
pub const MAX_DEPTH: usize = 32;
/// The longest element a [`Reader`] allows by default (16 MiB).
pub const MAX_LEN: usize = 16 << 20;

/// Tag octets.
pub mod tag {
    /// BOOLEAN.
    pub const BOOLEAN: u8 = 0x01;
    /// INTEGER.
    pub const INTEGER: u8 = 0x02;
    /// OCTET STRING.
    pub const OCTET_STRING: u8 = 0x04;
    /// NULL.
    pub const NULL: u8 = 0x05;
    /// ENUMERATED.
    pub const ENUMERATED: u8 = 0x0A;
    /// SEQUENCE (constructed).
    pub const SEQUENCE: u8 = 0x30;
    /// SET (constructed).
    pub const SET: u8 = 0x31;
    /// The constructed bit.
    pub const CONSTRUCTED: u8 = 0x20;

    /// `[n]` context-specific, primitive.
    #[must_use]
    pub const fn context(n: u8) -> u8 {
        0x80 | n
    }

    /// `[n]` context-specific, constructed.
    #[must_use]
    pub const fn context_constructed(n: u8) -> u8 {
        0xA0 | n
    }

    /// `[APPLICATION n]`, primitive.
    #[must_use]
    pub const fn application(n: u8) -> u8 {
        0x40 | n
    }

    /// `[APPLICATION n]`, constructed.
    #[must_use]
    pub const fn application_constructed(n: u8) -> u8 {
        0x60 | n
    }
}

/// What is wrong with BER input.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BerError {
    /// The input ends inside an element.
    #[error("BER element is truncated")]
    Truncated,
    /// An indefinite length.
    #[error("BER indefinite lengths are not allowed")]
    Indefinite,
    /// A length not in its shortest form.
    #[error("BER length is not in its shortest form")]
    NonMinimalLength,
    /// More than four length octets.
    #[error("BER length is too large")]
    LengthTooBig,
    /// An element over the reader's length limit.
    #[error("BER element of {len} bytes is over the limit of {limit}")]
    TooLong {
        /// The element's length.
        len: usize,
        /// The limit.
        limit: usize,
    },
    /// Nesting over the reader's depth limit.
    #[error("BER nesting is too deep")]
    TooDeep,
    /// A tag in the high tag number form.
    #[error("BER high tag numbers are not used by LDAP")]
    HighTag,
    /// Another tag than the one expected.
    #[error("expected BER tag {expected:#04x}, found {found:#04x}")]
    Tag {
        /// The expected tag.
        expected: u8,
        /// The tag found.
        found: u8,
    },
    /// The constructed form of a string type.
    #[error("constructed BER strings are not allowed")]
    ConstructedString,
    /// An element expected and none left.
    #[error("BER element expected, none left")]
    Missing,
    /// A bad INTEGER or ENUMERATED.
    #[error("BER integer is empty, too long or not in its shortest form")]
    Integer,
    /// A BOOLEAN that is not one octet.
    #[error("BER boolean is not one octet")]
    Boolean,
    /// A NULL with contents.
    #[error("BER null has contents")]
    Null,
    /// Bytes after the last element.
    #[error("bytes after the BER element")]
    Trailing,
}

/// The result type of this module.
pub type Result<T> = std::result::Result<T, BerError>;

/// Reads one element's header: tag, header length, content length.
/// None if `buf` does not hold the whole header yet.
///
/// # Errors
///
/// A [`BerError`] for a header LDAP does not allow.
pub fn header(buf: &[u8]) -> Result<Option<(u8, usize, usize)>> {
    let Some(&t) = buf.first() else {
        return Ok(None);
    };
    if t & 0x1F == 0x1F {
        return Err(BerError::HighTag);
    }
    let Some(&l) = buf.get(1) else {
        return Ok(None);
    };
    if l < 0x80 {
        return Ok(Some((t, 2, usize::from(l))));
    }
    if l == 0x80 {
        return Err(BerError::Indefinite);
    }
    let n = usize::from(l & 0x7F);
    if n > 4 {
        return Err(BerError::LengthTooBig);
    }
    let Some(octets) = buf.get(2..2 + n) else {
        return Ok(None);
    };
    if octets[0] == 0 {
        return Err(BerError::NonMinimalLength);
    }
    let len = octets
        .iter()
        .fold(0usize, |a, &b| (a << 8) | usize::from(b));
    if len < 0x80 {
        return Err(BerError::NonMinimalLength);
    }
    Ok(Some((t, 2 + n, len)))
}

/// How many bytes the first element of `buf` takes: none while the
/// buffer holds less than its header and contents.
///
/// # Errors
///
/// A [`BerError`] for a bad header or an element over `max_len`.
pub fn frame_len(buf: &[u8], max_len: usize) -> Result<Option<usize>> {
    match header(buf)? {
        None => Ok(None),
        Some((_, h, len)) => {
            if len > max_len {
                return Err(BerError::TooLong {
                    len,
                    limit: max_len,
                });
            }
            Ok((buf.len() >= h + len).then_some(h + len))
        }
    }
}

/// A reader over one level of elements.
#[derive(Clone)]
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    depth: usize,
    max_depth: usize,
    max_len: usize,
}

impl fmt::Debug for Reader<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Reader")
            .field("pos", &self.pos)
            .field("len", &self.data.len())
            .field("depth", &self.depth)
            .finish()
    }
}

impl<'a> Reader<'a> {
    /// A reader with the default limits.
    #[must_use]
    pub const fn new(data: &'a [u8]) -> Self {
        Self::with_limits(data, MAX_LEN, MAX_DEPTH)
    }

    /// A reader with the caller's limits.
    #[must_use]
    pub const fn with_limits(data: &'a [u8], max_len: usize, max_depth: usize) -> Self {
        Self {
            data,
            pos: 0,
            depth: 0,
            max_depth,
            max_len,
        }
    }

    /// Allows `extra` more levels below this one (a filter's nesting has
    /// a limit of its own).
    pub const fn allow_depth(&mut self, extra: usize) {
        self.max_depth = self.depth + extra;
    }

    /// The nesting of this reader (0 at the top).
    #[must_use]
    pub const fn depth(&self) -> usize {
        self.depth
    }

    /// Whether every element has been read.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.pos >= self.data.len()
    }

    /// The bytes not read yet.
    #[must_use]
    pub fn rest(&self) -> &'a [u8] {
        &self.data[self.pos..]
    }

    /// The next element's tag.
    #[must_use]
    pub fn peek_tag(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    /// Whether the next element has tag `t`.
    #[must_use]
    pub fn at(&self, t: u8) -> bool {
        self.peek_tag() == Some(t)
    }

    /// Reads any element: its tag, its contents and its whole encoding.
    ///
    /// # Errors
    ///
    /// A [`BerError`].
    pub fn element(&mut self) -> Result<(u8, &'a [u8], &'a [u8])> {
        let buf = &self.data[self.pos..];
        if buf.is_empty() {
            return Err(BerError::Missing);
        }
        let (t, h, len) = header(buf)?.ok_or(BerError::Truncated)?;
        if len > self.max_len {
            return Err(BerError::TooLong {
                len,
                limit: self.max_len,
            });
        }
        let whole = buf.get(..h + len).ok_or(BerError::Truncated)?;
        self.pos += h + len;
        Ok((t, &whole[h..], whole))
    }

    /// Reads an element of tag `t` and returns its contents.
    ///
    /// # Errors
    ///
    /// [`BerError::Tag`] for another tag, [`BerError::ConstructedString`]
    /// for the constructed form of a primitive `t`.
    pub fn expect(&mut self, t: u8) -> Result<&'a [u8]> {
        match self.peek_tag() {
            None => return Err(BerError::Missing),
            Some(f) if f != t => {
                if t & tag::CONSTRUCTED == 0 && f == t | tag::CONSTRUCTED {
                    return Err(BerError::ConstructedString);
                }
                return Err(BerError::Tag {
                    expected: t,
                    found: f,
                });
            }
            Some(_) => {}
        }
        Ok(self.element()?.1)
    }

    /// Reads a constructed element of tag `t` and returns a reader over
    /// its contents, one level deeper.
    ///
    /// # Errors
    ///
    /// A [`BerError`], [`BerError::TooDeep`] past the depth limit.
    pub fn sequence(&mut self, t: u8) -> Result<Self> {
        if self.depth >= self.max_depth {
            return Err(BerError::TooDeep);
        }
        let c = self.expect(t)?;
        Ok(Self {
            data: c,
            pos: 0,
            depth: self.depth + 1,
            max_depth: self.max_depth,
            max_len: self.max_len,
        })
    }

    /// Reads an OCTET STRING (or another primitive string tag).
    ///
    /// # Errors
    ///
    /// A [`BerError`].
    pub fn octets(&mut self, t: u8) -> Result<&'a [u8]> {
        self.expect(t)
    }

    /// Reads an optional primitive string of tag `t`.
    ///
    /// # Errors
    ///
    /// A [`BerError`].
    pub fn opt_octets(&mut self, t: u8) -> Result<Option<&'a [u8]>> {
        if self.at(t) || self.at(t | tag::CONSTRUCTED) {
            self.octets(t).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Reads a BOOLEAN (any non-zero octet is true).
    ///
    /// # Errors
    ///
    /// A [`BerError`].
    pub fn boolean(&mut self, t: u8) -> Result<bool> {
        match self.expect(t)? {
            [b] => Ok(*b != 0),
            _ => Err(BerError::Boolean),
        }
    }

    /// Reads an INTEGER of up to eight octets.
    ///
    /// # Errors
    ///
    /// A [`BerError`].
    pub fn integer(&mut self, t: u8) -> Result<i64> {
        decode_int(self.expect(t)?)
    }

    /// Reads an INTEGER that must fit an `i32`.
    ///
    /// # Errors
    ///
    /// [`BerError::Integer`] out of range.
    pub fn int32(&mut self, t: u8) -> Result<i32> {
        i32::try_from(self.integer(t)?).map_err(|_| BerError::Integer)
    }

    /// Reads an ENUMERATED.
    ///
    /// # Errors
    ///
    /// A [`BerError`].
    pub fn enumerated(&mut self, t: u8) -> Result<i64> {
        self.integer(t)
    }

    /// Reads a NULL.
    ///
    /// # Errors
    ///
    /// A [`BerError`].
    pub fn null(&mut self, t: u8) -> Result<()> {
        if self.expect(t)?.is_empty() {
            Ok(())
        } else {
            Err(BerError::Null)
        }
    }

    /// Checks that nothing is left.
    ///
    /// # Errors
    ///
    /// [`BerError::Trailing`].
    pub const fn finish(&self) -> Result<()> {
        if self.is_empty() {
            Ok(())
        } else {
            Err(BerError::Trailing)
        }
    }
}

fn decode_int(c: &[u8]) -> Result<i64> {
    if c.is_empty() || c.len() > 8 {
        return Err(BerError::Integer);
    }
    if c.len() > 1 && ((c[0] == 0 && c[1] & 0x80 == 0) || (c[0] == 0xFF && c[1] & 0x80 != 0)) {
        return Err(BerError::Integer);
    }
    let mut v: i64 = if c[0] & 0x80 != 0 { -1 } else { 0 };
    for &b in c {
        v = (v << 8) | i64::from(b);
    }
    Ok(v)
}

/// A writer of BER elements.
#[derive(Debug, Clone, Default)]
pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    /// An empty writer.
    #[must_use]
    pub const fn new() -> Self {
        Self { buf: Vec::new() }
    }

    /// The bytes written.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    /// The bytes written so far.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf
    }

    fn len(&mut self, n: usize) {
        if n < 0x80 {
            self.buf.push(u8::try_from(n).unwrap_or(0));
        } else {
            let bytes = n.to_be_bytes();
            let skip = bytes.iter().take_while(|&&b| b == 0).count();
            let octets = &bytes[skip..];
            self.buf
                .push(0x80 | u8::try_from(octets.len()).unwrap_or(0));
            self.buf.extend_from_slice(octets);
        }
    }

    /// Writes a primitive element with contents `c`.
    pub fn octets(&mut self, t: u8, c: &[u8]) {
        self.buf.push(t);
        self.len(c.len());
        self.buf.extend_from_slice(c);
    }

    /// Writes a string as an OCTET STRING (or under tag `t`).
    pub fn string(&mut self, t: u8, s: &str) {
        self.octets(t, s.as_bytes());
    }

    /// Writes a constructed element whose contents `f` writes.
    pub fn constructed(&mut self, t: u8, f: impl FnOnce(&mut Self)) {
        let mut inner = Self::new();
        f(&mut inner);
        self.octets(t, &inner.buf);
    }

    /// Writes an INTEGER in its shortest form.
    pub fn integer(&mut self, t: u8, v: i64) {
        let bytes = v.to_be_bytes();
        let mut skip = 0;
        while skip < 7 {
            let (a, b) = (bytes[skip], bytes[skip + 1]);
            if (a == 0 && b & 0x80 == 0) || (a == 0xFF && b & 0x80 != 0) {
                skip += 1;
            } else {
                break;
            }
        }
        self.octets(t, &bytes[skip..]);
    }

    /// Writes an ENUMERATED.
    pub fn enumerated(&mut self, t: u8, v: i64) {
        self.integer(t, v);
    }

    /// Writes a BOOLEAN (`FF` for true).
    pub fn boolean(&mut self, t: u8, v: bool) {
        self.octets(t, &[if v { 0xFF } else { 0 }]);
    }

    /// Writes a NULL.
    pub fn null(&mut self, t: u8) {
        self.octets(t, &[]);
    }

    /// Appends bytes already encoded.
    pub fn raw(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }
}

#[cfg(test)]
mod tests {
    use super::{BerError, Reader, Writer, frame_len, tag};

    #[test]
    fn integers() {
        for (v, enc) in [
            (0i64, &[0u8][..]),
            (127, &[0x7F]),
            (128, &[0x00, 0x80]),
            (256, &[0x01, 0x00]),
            (-1, &[0xFF]),
            (-128, &[0x80]),
            (-129, &[0xFF, 0x7F]),
            (i64::from(i32::MAX), &[0x7F, 0xFF, 0xFF, 0xFF]),
            (i64::MIN, &[0x80, 0, 0, 0, 0, 0, 0, 0]),
        ] {
            let mut w = Writer::new();
            w.integer(tag::INTEGER, v);
            let b = w.into_bytes();
            assert_eq!(&b[2..], enc, "{v}");
            assert_eq!(Reader::new(&b).integer(tag::INTEGER).unwrap(), v);
        }
        for bad in [
            &[0x02u8, 0x02, 0x00, 0x01][..],
            &[0x02, 0x02, 0xFF, 0x80],
            &[0x02, 0x00],
        ] {
            assert_eq!(
                Reader::new(bad).integer(tag::INTEGER),
                Err(BerError::Integer)
            );
        }
    }

    #[test]
    fn lengths() {
        for n in [0usize, 1, 127, 128, 255, 256, 65_535, 65_536, 1 << 20] {
            let mut w = Writer::new();
            w.octets(tag::OCTET_STRING, &vec![7; n]);
            let b = w.into_bytes();
            assert_eq!(frame_len(&b, usize::MAX).unwrap(), Some(b.len()));
            assert_eq!(frame_len(&b[..b.len() - 1], usize::MAX).unwrap(), None);
            assert_eq!(Reader::new(&b).octets(tag::OCTET_STRING).unwrap().len(), n);
        }
        // 0x81 0x05: long form for a short length.
        assert_eq!(
            Reader::new(&[0x04, 0x81, 0x05, 1, 2, 3, 4, 5]).octets(4),
            Err(BerError::NonMinimalLength)
        );
        assert_eq!(
            Reader::new(&[0x04, 0x82, 0x00, 0x80]).octets(4),
            Err(BerError::NonMinimalLength)
        );
        assert_eq!(
            Reader::new(&[0x30, 0x80, 0, 0]).sequence(0x30).unwrap_err(),
            BerError::Indefinite
        );
        assert_eq!(
            Reader::new(&[0x04, 0x85, 1, 1, 1, 1, 1]).octets(4),
            Err(BerError::LengthTooBig)
        );
        assert_eq!(
            Reader::new(&[0x04, 0x03, 1]).octets(4),
            Err(BerError::Truncated)
        );
        assert_eq!(
            Reader::with_limits(&[0x04, 0x03, 1, 2, 3], 2, 32).octets(4),
            Err(BerError::TooLong { len: 3, limit: 2 })
        );
        assert_eq!(
            frame_len(&[0x04, 0x84, 0x7F, 0xFF, 0xFF, 0xFF], 100),
            Err(BerError::TooLong {
                len: 0x7FFF_FFFF,
                limit: 100
            })
        );
    }

    #[test]
    fn strings_and_tags() {
        assert_eq!(
            Reader::new(&[0x24, 0x03, 0x04, 0x01, 0x41]).octets(4),
            Err(BerError::ConstructedString)
        );
        assert_eq!(
            Reader::new(&[0x1F, 0x01, 0]).element().unwrap_err(),
            BerError::HighTag
        );
        assert_eq!(
            Reader::new(&[0x04, 0]).integer(2),
            Err(BerError::Tag {
                expected: 2,
                found: 4
            })
        );
        assert_eq!(
            Reader::new(&[0x01, 0x02, 0, 0]).boolean(1),
            Err(BerError::Boolean)
        );
        assert!(Reader::new(&[0x01, 0x01, 0x01]).boolean(1).unwrap());
        assert_eq!(Reader::new(&[0x05, 0x01, 0]).null(5), Err(BerError::Null));
        assert_eq!(Reader::new(&[]).null(5), Err(BerError::Missing));
    }

    #[test]
    fn depth() {
        fn nest(w: &mut Writer, n: usize) {
            if n == 0 {
                w.null(tag::NULL);
            } else {
                w.constructed(tag::SEQUENCE, |w| nest(w, n - 1));
            }
        }
        let mut w = Writer::new();
        nest(&mut w, 33);
        let b = w.into_bytes();
        let mut r = Reader::new(&b);
        let mut err = None;
        for _ in 0..33 {
            match r.sequence(tag::SEQUENCE) {
                Ok(n) => r = n,
                Err(e) => {
                    err = Some(e);
                    break;
                }
            }
        }
        assert_eq!(err, Some(BerError::TooDeep));
        assert_eq!(r.depth(), 32);
    }

    #[test]
    fn writer_round_trip() {
        let mut w = Writer::new();
        w.constructed(tag::SEQUENCE, |w| {
            w.boolean(tag::BOOLEAN, true);
            w.enumerated(tag::ENUMERATED, 3);
            w.string(tag::context(0), "x");
            w.null(tag::NULL);
        });
        let b = w.into_bytes();
        assert_eq!(
            b,
            [
                0x30, 0x0B, 0x01, 0x01, 0xFF, 0x0A, 0x01, 0x03, 0x80, 0x01, b'x', 0x05, 0x00
            ]
        );
        let mut top = Reader::new(&b);
        let mut r = top.sequence(tag::SEQUENCE).unwrap();
        top.finish().unwrap();
        assert!(r.boolean(tag::BOOLEAN).unwrap());
        assert_eq!(r.enumerated(tag::ENUMERATED).unwrap(), 3);
        assert_eq!(r.opt_octets(tag::context(1)).unwrap(), None);
        assert_eq!(r.opt_octets(tag::context(0)).unwrap(), Some(&b"x"[..]));
        r.null(tag::NULL).unwrap();
        r.finish().unwrap();
    }
}
