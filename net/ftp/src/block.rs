// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Block mode (`MODE B`, RFC 959 3.4.2): data in blocks of a descriptor
//! byte and a 16-bit count, the last block marked end of file.

/// The descriptor bit of the last block.
pub const EOF: u8 = 0x40;
/// The descriptor bit of a restart marker, which carries no data.
pub const MARKER: u8 = 0x10;

/// Data as blocks, ended by an empty end-of-file block.
pub fn encode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / 0xFFFF * 3 + 6);
    for piece in data.chunks(0xFFFF) {
        out.push(0);
        out.extend_from_slice(&u16::try_from(piece.len()).unwrap_or(0xFFFF).to_be_bytes());
        out.extend_from_slice(piece);
    }
    out.extend_from_slice(&[EOF, 0, 0]);
    out
}

/// Whether `raw` holds the end-of-file block.
pub fn complete(raw: &[u8]) -> bool {
    let mut i = 0;
    while i + 3 <= raw.len() {
        let len = usize::from(u16::from_be_bytes([raw[i + 1], raw[i + 2]]));
        if i + 3 + len > raw.len() {
            return false;
        }
        if raw[i] & EOF != 0 {
            return true;
        }
        i += 3 + len;
    }
    false
}

/// The data of blocks, to the end-of-file block.
pub fn decode(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i + 3 <= raw.len() {
        let desc = raw[i];
        let len = usize::from(u16::from_be_bytes([raw[i + 1], raw[i + 2]]));
        let end = (i + 3 + len).min(raw.len());
        if desc & MARKER == 0 {
            out.extend_from_slice(&raw[i + 3..end]);
        }
        i = end;
        if desc & EOF != 0 {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let data: Vec<u8> = (0..70_000u32).map(|i| u8::try_from(i % 251).unwrap()).collect();
        let b = encode(&data);
        assert!(complete(&b));
        assert!(!complete(&b[..b.len() - 1]));
        assert_eq!(decode(&b), data);
        assert_eq!(decode(&encode(&[])), Vec::<u8>::new());
    }
}
