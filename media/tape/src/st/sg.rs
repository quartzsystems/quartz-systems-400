// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! [`SgChanger`]: a SCSI media changer (SMC-3) through `SG_IO`.
//!
//! MOVE MEDIUM (A5h) moves a cartridge with the medium transport the
//! last READ ELEMENT STATUS (B8h) reported. READ ELEMENT STATUS asks for
//! every element with primary volume tags; the reply is an 8-byte
//! header and one page per element type, each an 8-byte page header and
//! fixed-length element descriptors.

use super::ioctl::{SgDir, SgIoctl, SgReply};
use crate::transport::{Changer, ChangerError, Element, ElementKind};

/// MOVE MEDIUM.
pub const MOVE_MEDIUM: u8 = 0xa5;
/// READ ELEMENT STATUS.
pub const READ_ELEMENT_STATUS: u8 = 0xb8;
/// The allocation length READ ELEMENT STATUS asks for.
pub const STATUS_ALLOCATION: usize = 64 * 1024;
/// MOVE MEDIUM timeout: a robot move can take minutes.
pub const MOVE_TIMEOUT_MS: u32 = 300_000;
/// READ ELEMENT STATUS timeout.
pub const STATUS_TIMEOUT_MS: u32 = 120_000;

/// The MOVE MEDIUM command block.
#[must_use]
pub fn move_medium_cdb(transport: u16, from: u16, to: u16) -> [u8; 12] {
    let (t, f, d) = (
        transport.to_be_bytes(),
        from.to_be_bytes(),
        to.to_be_bytes(),
    );
    [
        MOVE_MEDIUM,
        0,
        t[0],
        t[1],
        f[0],
        f[1],
        d[0],
        d[1],
        0,
        0,
        0,
        0,
    ]
}

/// The READ ELEMENT STATUS command block: all element types, volume
/// tags, from address 0, up to 65535 elements.
#[must_use]
pub fn read_element_status_cdb(allocation: usize) -> [u8; 12] {
    let a = u32::try_from(allocation).expect("allocation").to_be_bytes();
    [
        READ_ELEMENT_STATUS,
        0x10,
        0,
        0,
        0xff,
        0xff,
        0,
        a[1],
        a[2],
        a[3],
        0,
        0,
    ]
}

fn be16(b: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([b[at], b[at + 1]])
}

fn be24(b: &[u8], at: usize) -> usize {
    (usize::from(b[at]) << 16) | (usize::from(b[at + 1]) << 8) | usize::from(b[at + 2])
}

/// Parse READ ELEMENT STATUS data.
pub fn parse_element_status(data: &[u8]) -> Result<Vec<Element>, ChangerError> {
    let bad = |why: &str| ChangerError::Format(why.to_string());
    if data.len() < 8 {
        return Err(bad("short header"));
    }
    let end = (8 + be24(data, 5)).min(data.len());
    let mut at = 8;
    let mut out = Vec::new();
    while at < end {
        if at + 8 > end {
            return Err(bad("short page header"));
        }
        let page = &data[at..];
        let kind =
            ElementKind::from_code(page[0] & 0x0f).ok_or_else(|| bad("unknown element type"))?;
        let pvoltag = page[1] & 0x80 != 0;
        let dlen = usize::from(be16(page, 2));
        let bytes = be24(page, 5);
        at += 8;
        if dlen < 12 || at + bytes > end || !bytes.is_multiple_of(dlen) {
            return Err(bad("descriptor lengths"));
        }
        for d in data[at..at + bytes].chunks(dlen) {
            let tag = (pvoltag && dlen >= 44)
                .then(|| {
                    String::from_utf8_lossy(&d[12..44])
                        .trim_end_matches([' ', '\0'])
                        .to_string()
                })
                .filter(|t| !t.is_empty());
            out.push(Element {
                kind,
                address: be16(d, 0),
                full: d[2] & 0x01 != 0,
                volume_tag: tag,
                source: (d[9] & 0x80 != 0).then(|| be16(d, 10)),
            });
        }
        at += bytes;
    }
    Ok(out)
}

/// Sense key, ASC and ASCQ from fixed (70h/71h) or descriptor (72h/73h)
/// sense data.
#[must_use]
pub fn sense_codes(sense: &[u8]) -> Option<(u8, u8, u8)> {
    match sense.first()? & 0x7f {
        0x70 | 0x71 if sense.len() >= 14 => Some((sense[2] & 0x0f, sense[12], sense[13])),
        0x72 | 0x73 if sense.len() >= 4 => Some((sense[1] & 0x0f, sense[2], sense[3])),
        _ => None,
    }
}

/// A media changer on an `sg` node.
#[derive(Debug)]
pub struct SgChanger<I> {
    io: I,
    transport: Option<u16>,
}

impl<I: SgIoctl> SgChanger<I> {
    /// Over an open `/dev/sgN`.
    pub fn new(io: I) -> Self {
        Self {
            io,
            transport: None,
        }
    }

    /// The ioctl backend.
    pub fn into_inner(self) -> I {
        self.io
    }

    fn check(reply: &SgReply, from: u16, to: u16) -> Result<(), ChangerError> {
        if reply.host_status != 0 || (reply.driver_status & 0x0f) != 0 {
            return Err(ChangerError::Io(std::io::Error::other(format!(
                "host status {:#x}, driver status {:#x}",
                reply.host_status, reply.driver_status
            ))));
        }
        if reply.status == 0 {
            Ok(())
        } else {
            let (key, asc, ascq) = sense_codes(&reply.sense).unwrap_or((0, 0, 0));
            Err(match (key, asc, ascq) {
                (0x5, 0x3b, 0x0e) => ChangerError::SourceEmpty(from),
                (0x5, 0x3b, 0x0d) => ChangerError::DestinationFull(to),
                (0x5, 0x21, 0x01) => ChangerError::NoElement(from),
                _ => ChangerError::Check { key, asc, ascq },
            })
        }
    }
}

impl<I: SgIoctl> Changer for SgChanger<I> {
    fn move_medium(&mut self, from: u16, to: u16) -> Result<(), ChangerError> {
        let transport = if let Some(t) = self.transport {
            t
        } else {
            self.read_element_status()?;
            self.transport.unwrap_or(0)
        };
        let cdb = move_medium_cdb(transport, from, to);
        let reply = self.io.sg_io(&cdb, SgDir::None, &mut [], MOVE_TIMEOUT_MS)?;
        Self::check(&reply, from, to)
    }

    fn read_element_status(&mut self) -> Result<Vec<Element>, ChangerError> {
        let mut buf = vec![0u8; STATUS_ALLOCATION];
        let cdb = read_element_status_cdb(buf.len());
        let reply = self
            .io
            .sg_io(&cdb, SgDir::FromDevice, &mut buf, STATUS_TIMEOUT_MS)?;
        Self::check(&reply, 0, 0)?;
        let got = buf.len() - usize::try_from(reply.resid.max(0)).unwrap_or(0);
        let elems = parse_element_status(&buf[..got])?;
        self.transport = elems
            .iter()
            .find(|e| e.kind == ElementKind::Transport)
            .map(|e| e.address)
            .or(self.transport);
        Ok(elems)
    }
}
