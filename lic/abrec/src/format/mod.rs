// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One copy on disk, little-endian throughout.
//!
//! ```text
//! 0   magic                       8   the product's
//! 8   format_version   u32        4   the product's
//! 12  record_len       u32        4   bytes from offset 0 to the payload's end
//! 16  crc32c           u32        4   over [20, record_len)
//! 20  sequence         u64        8
//! 28  payload                         the product's
//! ```

pub mod slot;

use crc::{CRC_32_ISCSI, Crc};

use crate::error::AbError;
use crate::spec::Spec;

/// Bytes before the payload.
pub const HEAD_LEN: usize = 28;

const CRC_OFFSET: usize = 16;
const CRC_START: usize = 20;

/// The checksum of a copy: CRC-32C.
pub const CRC32C: Crc<u32> = Crc::<u32>::new(&CRC_32_ISCSI);

/// One decoded copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame<'a> {
    /// Monotonic; the newest copy has the highest.
    pub sequence: u64,
    /// The product's bytes.
    pub payload: &'a [u8],
}

/// One copy's bytes, with CRC. Fails if they do not fit a slot.
pub fn encode(spec: &Spec, sequence: u64, payload: &[u8]) -> Result<Vec<u8>, AbError> {
    let mut out = Vec::with_capacity(HEAD_LEN + payload.len());
    out.extend_from_slice(&spec.magic);
    out.extend_from_slice(&spec.version.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // record_len, patched below
    out.extend_from_slice(&0u32.to_le_bytes()); // crc, patched below
    out.extend_from_slice(&sequence.to_le_bytes());
    out.extend_from_slice(payload);

    let len = out.len();
    let too_large = AbError::TooLarge {
        len,
        max: spec.slot_bytes,
    };
    if len > spec.slot_bytes {
        return Err(too_large);
    }
    let Ok(len32) = u32::try_from(len) else {
        return Err(too_large);
    };
    out[12..16].copy_from_slice(&len32.to_le_bytes());
    let crc = CRC32C.checksum(&out[CRC_START..]);
    out[CRC_OFFSET..CRC_OFFSET + 4].copy_from_slice(&crc.to_le_bytes());
    Ok(out)
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().expect("4 bytes"))
}

/// Parse and verify one copy. Any defect is `Malformed`.
pub fn decode<'a>(spec: &Spec, bytes: &'a [u8]) -> Result<Frame<'a>, AbError> {
    let fixed = HEAD_LEN + spec.min_payload;
    if bytes.len() < fixed {
        return Err(AbError::Malformed("shorter than the fixed part"));
    }
    if bytes[..8] != spec.magic {
        return Err(AbError::Malformed("bad magic"));
    }
    if u32_at(bytes, 8) != spec.version {
        return Err(AbError::Malformed("unsupported format version"));
    }
    let len = usize::try_from(u32_at(bytes, 12)).map_err(|_| AbError::Malformed("record_len"))?;
    if len < fixed || len > bytes.len() || len > spec.slot_bytes {
        return Err(AbError::Malformed("record_len out of range"));
    }
    if CRC32C.checksum(&bytes[CRC_START..len]) != u32_at(bytes, CRC_OFFSET) {
        return Err(AbError::Malformed("crc mismatch"));
    }
    let sequence = u64::from_le_bytes(bytes[20..HEAD_LEN].try_into().expect("8 bytes"));
    Ok(Frame {
        sequence,
        payload: &bytes[HEAD_LEN..len],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: Spec = Spec {
        magic: *b"TESTABRC",
        version: 3,
        type_guid: uuid::Uuid::nil(),
        slot_bytes: 256,
        min_payload: 2,
    };

    #[test]
    fn layout_is_as_documented() {
        let bytes = encode(&SPEC, 0x0102_0304_0506_0708, b"payload").unwrap();
        assert_eq!(&bytes[..8], b"TESTABRC");
        assert_eq!(bytes[8..12], 3u32.to_le_bytes());
        assert_eq!(bytes[12..16], 35u32.to_le_bytes());
        assert_eq!(bytes[16..20], CRC32C.checksum(&bytes[20..]).to_le_bytes());
        assert_eq!(bytes[20..28], 0x0102_0304_0506_0708u64.to_le_bytes());
        assert_eq!(&bytes[28..], b"payload");
        let f = decode(&SPEC, &bytes).unwrap();
        assert_eq!(
            (f.sequence, f.payload),
            (0x0102_0304_0506_0708, &b"payload"[..])
        );
        // What follows the record in its slot is not read.
        let mut slot = bytes.clone();
        slot.resize(SPEC.slot_bytes, 0xEE);
        assert_eq!(decode(&SPEC, &slot).unwrap(), f);
    }

    fn why(bytes: &[u8]) -> &'static str {
        match decode(&SPEC, bytes) {
            Err(AbError::Malformed(why)) => why,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn defects_are_malformed() {
        let good = encode(&SPEC, 9, b"payload").unwrap();
        assert_eq!(why(&good[..29]), "shorter than the fixed part");
        let mut b = good.clone();
        b[0] ^= 1;
        assert_eq!(why(&b), "bad magic");
        let mut b = good.clone();
        b[8] = 4;
        assert_eq!(why(&b), "unsupported format version");
        assert_eq!(why(&good[..good.len() - 1]), "record_len out of range");
        let mut b = good.clone();
        b[12] = 29;
        assert_eq!(why(&b), "record_len out of range");
        for at in [16, 20, 30] {
            let mut b = good.clone();
            b[at] ^= 1;
            assert_eq!(why(&b), "crc mismatch", "byte {at}");
        }
        // Another product's record is not ours.
        let other = Spec {
            magic: *b"OTHERREC",
            ..SPEC
        };
        assert_eq!(why(&encode(&other, 9, b"payload").unwrap()), "bad magic");
    }

    #[test]
    fn a_record_past_its_slot_is_refused() {
        let fits = vec![7u8; SPEC.slot_bytes - HEAD_LEN];
        assert_eq!(encode(&SPEC, 1, &fits).unwrap().len(), SPEC.slot_bytes);
        let over = vec![7u8; SPEC.slot_bytes - HEAD_LEN + 1];
        assert!(matches!(
            encode(&SPEC, 1, &over),
            Err(AbError::TooLarge { len: 257, max: 256 })
        ));
    }
}
