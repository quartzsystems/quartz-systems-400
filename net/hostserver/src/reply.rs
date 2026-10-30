// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Reply frames: the request's header echoed (header ID, server,
//! correlation), a new reply ID, the template and the parameters.

use hostsvr::frame::{Frame, Header, Param};

/// A reply to `req` with ID `id`.
pub fn frame(req: &Header, id: u16, template: Vec<u8>, params: Vec<Param>) -> Vec<u8> {
    let mut f = Frame::new(req.server_id, id, template, params);
    f.header.header_id = req.header_id;
    f.header.correlation = req.correlation;
    f.encode()
}

/// A big-endian u16.
pub fn be16(b: &[u8], at: usize) -> u16 {
    b.get(at..at + 2)
        .map_or(0, |x| u16::from_be_bytes([x[0], x[1]]))
}

/// A big-endian u32.
pub fn be32(b: &[u8], at: usize) -> u32 {
    b.get(at..at + 4)
        .map_or(0, |x| u32::from_be_bytes([x[0], x[1], x[2], x[3]]))
}

/// Bytes `at..at+n`, short when the slice is.
pub fn slice(b: &[u8], at: usize, n: usize) -> &[u8] {
    let end = (at + n).min(b.len());
    b.get(at.min(end)..end).unwrap_or(&[])
}

/// A u32 that fits.
pub fn len32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// A u16 that fits.
pub fn len16(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}
