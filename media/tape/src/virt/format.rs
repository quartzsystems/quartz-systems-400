// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The virtual volume file: a 512-byte header, then frames. A frame is a
//! data block or a tape mark behind a 16-byte frame header. See
//! `docs/MEDIA.md` "Virtual volume format".

use crc::{CRC_32_ISCSI, Crc};

use crate::spec::{Field, Layout};
use crate::transport::{MAX_BLOCK, TransportError};

const CRC32C: Crc<u32> = Crc::<u32>::new(&CRC_32_ISCSI);

/// The header's magic.
pub const MAGIC: &[u8; 8] = b"QSTAPVOL";
/// The format version this code writes and reads.
pub const VERSION: u16 = 1;
/// Header length; the data area starts here.
pub const HEADER_LEN: usize = 512;
/// Frame header length.
pub const FRAME_LEN: usize = 16;
/// Header flag: the volume is write protected.
pub const FLAG_WRITE_PROTECT: u16 = 1;
/// Frame kind: a data block.
pub const KIND_BLOCK: u8 = 1;
/// Frame kind: a tape mark.
pub const KIND_MARK: u8 = 2;

const fn f(name: &'static str, offset: usize, size: usize) -> Field {
    Field { name, offset, size }
}

const H_MAGIC: Field = f("magic", 0, 8);
const H_VERSION: Field = f("version", 8, 2);
const H_FLAGS: Field = f("flags", 10, 2);
const H_LEN: Field = f("header_len", 12, 4);
const H_CAPACITY: Field = f("capacity", 16, 8);
const H_EW: Field = f("early_warning", 24, 8);
const H_DENSITY: Field = f("density", 32, 16);
const H_TAG: Field = f("volume_tag", 48, 6);
const H_CRC: Field = f("crc32c", 508, 4);

/// The volume header.
pub const HEADER: Layout = Layout {
    name: "volume header",
    size: HEADER_LEN,
    fields: &[
        H_MAGIC,
        H_VERSION,
        H_FLAGS,
        H_LEN,
        H_CAPACITY,
        H_EW,
        H_DENSITY,
        H_TAG,
        f("reserved", 54, 454),
        H_CRC,
    ],
};

const F_KIND: Field = f("kind", 0, 1);
const F_LEN: Field = f("length", 4, 4);
const F_EPOCH: Field = f("epoch", 8, 4);
const F_CRC: Field = f("crc32c", 12, 4);

/// A frame header.
pub const FRAME: Layout = Layout {
    name: "frame header",
    size: FRAME_LEN,
    fields: &[F_KIND, f("reserved", 1, 3), F_LEN, F_EPOCH, F_CRC],
};

/// The early warning point of a data area of `capacity` bytes: 1/32 of
/// it is reserved for trailer labels, at least four maximum blocks
/// (but never more than half).
#[must_use]
pub fn early_warning_for(capacity: u64) -> u64 {
    let max = (MAX_BLOCK as u64 + FRAME_LEN as u64) * 4;
    capacity - (capacity / 32).max(max).min(capacity / 2)
}

/// What the header holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// Format version.
    pub version: u16,
    /// Write protected.
    pub write_protect: bool,
    /// Bytes in the data area (frames included).
    pub capacity: u64,
    /// The data area offset past which writes report early warning.
    pub early_warning: u64,
    /// Density name, up to 16 ASCII characters.
    pub density: String,
    /// Volume tag (the cartridge's barcode), up to 6 ASCII characters.
    pub tag: String,
}

fn ascii(out: &mut [u8], s: &str) -> Result<(), TransportError> {
    if s.len() > out.len() || !s.bytes().all(|b| (0x21..0x7f).contains(&b)) {
        return Err(TransportError::Format(format!(
            "{s:?} does not fit the header"
        )));
    }
    out.fill(b' ');
    out[..s.len()].copy_from_slice(s.as_bytes());
    Ok(())
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).trim_end().to_string()
}

fn put<const N: usize>(buf: &mut [u8], f: Field, v: [u8; N]) {
    buf[f.offset..f.offset + f.size].copy_from_slice(&v);
}

fn get<const N: usize>(buf: &[u8], f: Field) -> [u8; N] {
    buf[f.offset..f.offset + N].try_into().expect("field size")
}

fn slice(buf: &mut [u8], f: Field) -> &mut [u8] {
    &mut buf[f.offset..f.offset + f.size]
}

impl Header {
    /// The 512 header bytes.
    pub fn encode(&self) -> Result<[u8; HEADER_LEN], TransportError> {
        let mut b = [0u8; HEADER_LEN];
        b[..8].copy_from_slice(MAGIC);
        put(&mut b, H_VERSION, self.version.to_le_bytes());
        let flags = if self.write_protect {
            FLAG_WRITE_PROTECT
        } else {
            0
        };
        put(&mut b, H_FLAGS, flags.to_le_bytes());
        put(
            &mut b,
            H_LEN,
            u32::try_from(HEADER_LEN)
                .expect("header length")
                .to_le_bytes(),
        );
        put(&mut b, H_CAPACITY, self.capacity.to_le_bytes());
        put(&mut b, H_EW, self.early_warning.to_le_bytes());
        ascii(slice(&mut b, H_DENSITY), &self.density)?;
        ascii(slice(&mut b, H_TAG), &self.tag)?;
        let crc = CRC32C.checksum(&b[..H_CRC.offset]);
        put(&mut b, H_CRC, crc.to_le_bytes());
        Ok(b)
    }

    /// Parse and check a header.
    pub fn decode(b: &[u8]) -> Result<Self, TransportError> {
        let bad = |why: &str| TransportError::Format(why.to_string());
        if b.len() < HEADER_LEN || &b[..8] != MAGIC {
            return Err(bad("no volume header"));
        }
        if u32::from_le_bytes(get(b, H_CRC)) != CRC32C.checksum(&b[..H_CRC.offset]) {
            return Err(bad("header checksum mismatch"));
        }
        let version = u16::from_le_bytes(get(b, H_VERSION));
        if version != VERSION {
            return Err(TransportError::Format(format!("format version {version}")));
        }
        if u32::from_le_bytes(get(b, H_LEN)) as usize != HEADER_LEN {
            return Err(bad("header length"));
        }
        let capacity = u64::from_le_bytes(get(b, H_CAPACITY));
        let early_warning = u64::from_le_bytes(get(b, H_EW));
        if early_warning > capacity {
            return Err(bad("early warning past capacity"));
        }
        Ok(Self {
            version,
            write_protect: u16::from_le_bytes(get(b, H_FLAGS)) & FLAG_WRITE_PROTECT != 0,
            capacity,
            early_warning,
            density: text(&b[H_DENSITY.offset..H_DENSITY.offset + H_DENSITY.size]),
            tag: text(&b[H_TAG.offset..H_TAG.offset + H_TAG.size]),
        })
    }
}

/// A frame header, decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameHead {
    /// [`KIND_BLOCK`] or [`KIND_MARK`].
    pub kind: u8,
    /// Payload length.
    pub len: u32,
    /// The write epoch.
    pub epoch: u32,
    /// Checksum over header bytes 0..12 and the payload.
    pub crc: u32,
}

/// The frame header for `payload`.
#[must_use]
pub fn encode_frame(kind: u8, epoch: u32, payload: &[u8]) -> [u8; FRAME_LEN] {
    let mut b = [0u8; FRAME_LEN];
    b[0] = kind;
    let len = u32::try_from(payload.len()).expect("block length");
    put(&mut b, F_LEN, len.to_le_bytes());
    put(&mut b, F_EPOCH, epoch.to_le_bytes());
    let crc = frame_crc(&b, payload);
    put(&mut b, F_CRC, crc.to_le_bytes());
    b
}

fn frame_crc(head: &[u8], payload: &[u8]) -> u32 {
    let mut d = CRC32C.digest();
    d.update(&head[..F_CRC.offset]);
    d.update(payload);
    d.finalize()
}

/// Decode a frame header; `None` when it cannot be one.
#[must_use]
pub fn decode_frame(b: &[u8; FRAME_LEN]) -> Option<FrameHead> {
    let kind = b[F_KIND.offset];
    let len = u32::from_le_bytes(get(b, F_LEN));
    let ok = match kind {
        KIND_BLOCK => len > 0 && len as usize <= MAX_BLOCK,
        KIND_MARK => len == 0,
        _ => false,
    };
    (ok && b[1..4] == [0, 0, 0]).then(|| FrameHead {
        kind,
        len,
        epoch: u32::from_le_bytes(get(b, F_EPOCH)),
        crc: u32::from_le_bytes(get(b, F_CRC)),
    })
}

/// Whether the payload matches the frame's checksum.
#[must_use]
pub fn frame_ok(raw: &[u8; FRAME_LEN], head: &FrameHead, payload: &[u8]) -> bool {
    frame_crc(raw, payload) == head.crc
}
