// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Chunk compression: `DTACPR` to a codec.

use std::io::Read;

use crate::error::{Error, Result};

/// `DTACPR`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    /// `*NO`.
    None,
    /// `*LOW`: LZ4.
    Low,
    /// `*MEDIUM`: zstd.
    Medium,
    /// `*HIGH`: zstd.
    High,
}

/// Codec codes in a record header.
pub const STORED: u8 = 0;
/// LZ4 block with its length prepended.
pub const LZ4: u8 = 1;
/// A zstd frame.
pub const ZSTD: u8 = 2;

impl Compression {
    /// The header code.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Low => 1,
            Self::Medium => 2,
            Self::High => 3,
        }
    }

    /// From a header code.
    #[must_use]
    pub fn from_code(c: u8) -> Option<Self> {
        Some(match c {
            0 => Self::None,
            1 => Self::Low,
            2 => Self::Medium,
            3 => Self::High,
            _ => return None,
        })
    }

    /// From the `DTACPR` value (`*NO`, `*YES` is `*LOW` as on IBM i's
    /// software compression, `*LOW`, `*MEDIUM`, `*HIGH`, `*DEV` none).
    #[must_use]
    pub fn from_param(s: &str) -> Option<Self> {
        Some(match s.to_ascii_uppercase().as_str() {
            "*NO" | "*DEV" => Self::None,
            "*YES" | "*LOW" => Self::Low,
            "*MEDIUM" => Self::Medium,
            "*HIGH" => Self::High,
            _ => return None,
        })
    }

    /// Compress `raw`: the codec used and the bytes. Data that does not
    /// shrink is stored.
    #[must_use]
    pub fn pack(self, raw: &[u8]) -> (u8, Vec<u8>) {
        let packed = match self {
            Self::None => return (STORED, raw.to_vec()),
            Self::Low => (LZ4, lz4_flex::block::compress(raw)),
            Self::Medium | Self::High => (
                ZSTD,
                ruzstd::encoding::compress_to_vec(raw, ruzstd::encoding::CompressionLevel::Fastest),
            ),
        };
        if packed.1.len() < raw.len() {
            packed
        } else {
            (STORED, raw.to_vec())
        }
    }
}

/// Decompress `stored` written with `codec` to exactly `raw_len` bytes.
pub fn unpack(codec: u8, stored: &[u8], raw_len: usize) -> Result<Vec<u8>> {
    let out = match codec {
        STORED => stored.to_vec(),
        LZ4 => lz4_flex::block::decompress(stored, raw_len).map_err(|_| Error::Codec)?,
        ZSTD => {
            let mut d = ruzstd::decoding::StreamingDecoder::new(stored).map_err(|_| Error::Codec)?;
            let mut out = Vec::with_capacity(raw_len);
            // Read at most one byte past the length to catch an overlong frame.
            (&mut d)
                .take(raw_len as u64 + 1)
                .read_to_end(&mut out)
                .map_err(|_| Error::Codec)?;
            out
        }
        _ => return Err(Error::Codec),
    };
    if out.len() == raw_len {
        Ok(out)
    } else {
        Err(Error::Codec)
    }
}
