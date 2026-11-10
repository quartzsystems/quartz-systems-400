// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The records both files are made of: an 8-byte magic, a 2-byte
//! version, a 2-byte record count, then records of a 2-byte tag, a
//! 4-byte length and the value (UTF-8 text). Big-endian throughout.

use super::{FORMAT_VERSION, FileError};

/// One record.
pub type Record = (u16, Vec<u8>);

/// The file's bytes.
pub fn write(magic: &[u8; 8], records: &[Record]) -> Result<Vec<u8>, FileError> {
    let n = u16::try_from(records.len()).map_err(|_| FileError::TooMany(records.len()))?;
    let mut out = Vec::with_capacity(12 + records.iter().map(|r| 6 + r.1.len()).sum::<usize>());
    out.extend_from_slice(magic);
    out.extend_from_slice(&FORMAT_VERSION.to_be_bytes());
    out.extend_from_slice(&n.to_be_bytes());
    for (tag, v) in records {
        let len = u32::try_from(v.len()).map_err(|_| FileError::TooLong(*tag))?;
        out.extend_from_slice(&tag.to_be_bytes());
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(v);
    }
    Ok(out)
}

/// The records of a file with magic `magic`.
pub fn read(magic: &[u8; 8], b: &[u8]) -> Result<Vec<Record>, FileError> {
    if b.len() < 12 || &b[..8] != magic {
        return Err(FileError::Magic);
    }
    let version = u16::from_be_bytes([b[8], b[9]]);
    if version == 0 || version > FORMAT_VERSION {
        return Err(FileError::Version(version));
    }
    let n = usize::from(u16::from_be_bytes([b[10], b[11]]));
    let mut at = 12;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let head = b.get(at..at + 6).ok_or(FileError::Truncated(at))?;
        let tag = u16::from_be_bytes([head[0], head[1]]);
        let len = u32::from_be_bytes([head[2], head[3], head[4], head[5]]);
        let len = usize::try_from(len).map_err(|_| FileError::Truncated(at))?;
        at += 6;
        let v = b.get(at..at + len).ok_or(FileError::Truncated(at))?;
        out.push((tag, v.to_vec()));
        at += len;
    }
    if at != b.len() {
        return Err(FileError::Trailing(b.len() - at));
    }
    Ok(out)
}

/// A record's value as text.
pub fn text(tag: u16, v: &[u8]) -> Result<String, FileError> {
    String::from_utf8(v.to_vec()).map_err(|_| FileError::Value(tag))
}

/// A record's value as a decimal integer.
pub fn int(tag: u16, v: &[u8]) -> Result<i32, FileError> {
    text(tag, v)?.parse().map_err(|_| FileError::Value(tag))
}

/// A record's value as `0` or `1`.
pub fn flag(tag: u16, v: &[u8]) -> Result<bool, FileError> {
    match v {
        b"0" => Ok(false),
        b"1" => Ok(true),
        _ => Err(FileError::Value(tag)),
    }
}

/// A text record.
#[must_use]
pub fn put(tag: u16, s: &str) -> Record {
    (tag, s.as_bytes().to_vec())
}

/// An integer record.
#[must_use]
pub fn put_int(tag: u16, v: i32) -> Record {
    (tag, v.to_string().into_bytes())
}

/// A flag record.
#[must_use]
pub fn put_flag(tag: u16, v: bool) -> Record {
    (tag, if v { b"1".to_vec() } else { b"0".to_vec() })
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: &[u8; 8] = b"QSZLSTST";

    #[test]
    fn records_round_trip() {
        let r = vec![
            put(1, "QS400"),
            put_int(2, -1),
            put_flag(3, true),
            (0x7777, vec![0, 1, 2]),
        ];
        let b = write(M, &r).expect("writes");
        assert_eq!(&b[..12], b"QSZLSTST\x00\x01\x00\x04");
        assert_eq!(read(M, &b).expect("reads"), r);
    }

    #[test]
    fn damage_is_reported() {
        let b = write(M, &[put(1, "abc")]).expect("writes");
        assert_eq!(read(b"QSZLSXXX", &b), Err(FileError::Magic));
        assert_eq!(read(M, &b[..b.len() - 1]), Err(FileError::Truncated(18)));
        let mut more = b.clone();
        more.push(0);
        assert_eq!(read(M, &more), Err(FileError::Trailing(1)));
        let mut newer = b;
        newer[9] = 2;
        assert_eq!(read(M, &newer), Err(FileError::Version(2)));
        assert_eq!(int(5, b"x"), Err(FileError::Value(5)));
        assert_eq!(flag(5, b"2"), Err(FileError::Value(5)));
    }
}
