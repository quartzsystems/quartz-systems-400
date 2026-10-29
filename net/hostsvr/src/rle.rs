// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The run-length encoding the remote command server (program
//! parameters) and the database server (compressed replies, `0x3832`)
//! use. An escape byte (0x1B) starts a record: escape, escape is one
//! literal escape byte; escape, a two-byte pattern and a u16 count is the
//! pattern repeated count times. Everything else is literal.

use crate::Error;

/// The escape byte.
pub const ESCAPE: u8 = 0x1B;

/// Pairs repeated at least this many times become a record.
const MIN_PAIRS: usize = 5;

/// Expand `b`.
pub fn decompress(b: &[u8]) -> Result<Vec<u8>, Error> {
    let mut out = Vec::with_capacity(b.len() * 2);
    let mut i = 0;
    while i < b.len() {
        if b[i] != ESCAPE {
            out.push(b[i]);
            i += 1;
        } else if b.get(i + 1) == Some(&ESCAPE) {
            out.push(ESCAPE);
            i += 2;
        } else if i + 5 <= b.len() {
            let pair = [b[i + 1], b[i + 2]];
            let n = u16::from_be_bytes([b[i + 3], b[i + 4]]);
            for _ in 0..n {
                out.extend_from_slice(&pair);
            }
            i += 5;
        } else {
            return Err(Error::Invalid("RLE record cut short".into()));
        }
    }
    Ok(out)
}

/// Compress `b`; `None` when that would not make it smaller.
pub fn compress(b: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == ESCAPE {
            out.extend_from_slice(&[ESCAPE, ESCAPE]);
            i += 1;
            continue;
        }
        if i + 1 < b.len() && b[i + 1] != ESCAPE {
            let pair = [b[i], b[i + 1]];
            let mut n = 1;
            while i + 2 * n + 1 < b.len()
                && b[i + 2 * n] == pair[0]
                && b[i + 2 * n + 1] == pair[1]
                && n < usize::from(u16::MAX)
            {
                n += 1;
            }
            if n >= MIN_PAIRS {
                out.push(ESCAPE);
                out.extend_from_slice(&pair);
                out.extend_from_slice(&u16::try_from(n).unwrap_or(u16::MAX).to_be_bytes());
                i += 2 * n;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    (out.len() < b.len()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let mut data = vec![1, 2, ESCAPE, 3];
        data.extend(std::iter::repeat_n([0x40, 0x40], 50).flatten());
        data.extend([ESCAPE, 9, 9]);
        let c = compress(&data).unwrap();
        assert!(c.len() < data.len());
        assert_eq!(decompress(&c).unwrap(), data);
    }

    #[test]
    fn records() {
        assert_eq!(
            decompress(&[ESCAPE, 0, 0, 0, 3, ESCAPE, ESCAPE]).unwrap(),
            vec![0, 0, 0, 0, 0, 0, ESCAPE]
        );
    }
}
