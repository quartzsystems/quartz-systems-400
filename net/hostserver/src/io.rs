// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The transport: frames read off a stream, handed to a [`Handler`],
//! its replies written back, until the client or the server ends.

use std::io::{self, Read, Write};

use hostsvr::frame::HEADER_LEN;

use crate::Handler;

/// The largest frame accepted (a blocked insert can be large).
pub const MAX_FRAME: usize = 64 << 20;

/// Read one whole frame; `None` at a clean end of stream.
pub fn read_frame(r: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut head = [0u8; HEADER_LEN];
    let mut got = 0;
    while got < HEADER_LEN {
        match r.read(&mut head[got..]) {
            Ok(0) if got == 0 => return Ok(None),
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    let len = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
    if !(HEADER_LEN..=MAX_FRAME).contains(&len) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame length out of range"));
    }
    let mut f = head.to_vec();
    f.resize(len, 0);
    r.read_exact(&mut f[HEADER_LEN..])?;
    Ok(Some(f))
}

/// Serve one connection until it ends.
pub fn serve<S: Read + Write>(stream: &mut S, h: &mut dyn Handler) -> io::Result<()> {
    let r = frames(stream, h);
    h.finish();
    r
}

fn frames<S: Read + Write>(stream: &mut S, h: &mut dyn Handler) -> io::Result<()> {
    while let Some(f) = read_frame(stream)? {
        let o = h.handle(&f);
        for r in &o.replies {
            stream.write_all(r)?;
        }
        stream.flush()?;
        if o.close {
            break;
        }
    }
    Ok(())
}

/// Serve one port mapper lookup.
pub fn serve_svrmap<S: Read + Write>(stream: &mut S) -> io::Result<()> {
    let mut buf = [0u8; 128];
    let n = stream.read(&mut buf)?;
    if n == 0 {
        return Ok(());
    }
    stream.write_all(&crate::svrmap::answer(&buf[..n]))?;
    stream.flush()
}
