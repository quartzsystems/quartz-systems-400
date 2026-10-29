// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The 20-byte header every host server frame starts with.

use crate::Error;

/// The header's size.
pub const HEADER_LEN: usize = 20;

/// Byte offsets of the header fields.
pub mod off {
    /// Total length of the frame, header included (u32).
    pub const LENGTH: usize = 0;
    /// Header ID (u16): client attributes in requests, zero in most replies.
    pub const HEADER_ID: usize = 4;
    /// Server ID (u16).
    pub const SERVER_ID: usize = 6;
    /// CS instance (u32).
    pub const CS_INSTANCE: usize = 8;
    /// Correlation ID (u32).
    pub const CORRELATION: usize = 12;
    /// Template length (u16).
    pub const TEMPLATE_LEN: usize = 16;
    /// Request or reply ID (u16).
    pub const REQ_REP_ID: usize = 18;
}

/// A decoded header.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Header {
    /// Total length, header included.
    pub length: u32,
    /// Header ID.
    pub header_id: u16,
    /// Server ID.
    pub server_id: u16,
    /// CS instance.
    pub cs_instance: u32,
    /// Correlation ID.
    pub correlation: u32,
    /// Template length.
    pub template_len: u16,
    /// Request or reply ID.
    pub req_rep_id: u16,
}

impl Header {
    /// Decode the first 20 bytes.
    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        if b.len() < HEADER_LEN {
            return Err(Error::Short {
                what: "header",
                need: HEADER_LEN,
                have: b.len(),
            });
        }
        Ok(Self {
            length: be32(b, off::LENGTH),
            header_id: be16(b, off::HEADER_ID),
            server_id: be16(b, off::SERVER_ID),
            cs_instance: be32(b, off::CS_INSTANCE),
            correlation: be32(b, off::CORRELATION),
            template_len: be16(b, off::TEMPLATE_LEN),
            req_rep_id: be16(b, off::REQ_REP_ID),
        })
    }

    /// Append the 20 bytes.
    pub fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.length.to_be_bytes());
        out.extend_from_slice(&self.header_id.to_be_bytes());
        out.extend_from_slice(&self.server_id.to_be_bytes());
        out.extend_from_slice(&self.cs_instance.to_be_bytes());
        out.extend_from_slice(&self.correlation.to_be_bytes());
        out.extend_from_slice(&self.template_len.to_be_bytes());
        out.extend_from_slice(&self.req_rep_id.to_be_bytes());
    }

    /// A header for a frame with the given body sizes.
    pub fn new(server_id: u16, req_rep_id: u16, template_len: u16, body_len: usize) -> Self {
        Self {
            length: u32::try_from(HEADER_LEN + body_len).unwrap_or(u32::MAX),
            header_id: 0,
            server_id,
            cs_instance: 0,
            correlation: 0,
            template_len,
            req_rep_id,
        }
    }
}

/// A big-endian u16 at `at`; the caller checked the length.
pub(crate) fn be16(b: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([b[at], b[at + 1]])
}

/// A big-endian u32 at `at`; the caller checked the length.
pub(crate) fn be32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// How many bytes of `buf` the frame at its start takes, once the header
/// is in; `None` while fewer than 20 bytes have arrived or the length is
/// below 20 (not a framed stream).
pub fn frame_len(buf: &[u8]) -> Option<usize> {
    if buf.len() < HEADER_LEN {
        return None;
    }
    let n = be32(buf, 0) as usize;
    (n >= HEADER_LEN).then_some(n)
}
