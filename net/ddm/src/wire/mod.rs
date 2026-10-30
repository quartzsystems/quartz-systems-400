// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The datastream: DSS framing and DDM objects, and reading whole
//! chains from a byte stream.

pub mod dss;
pub mod object;

pub use dss::{Dss, DssType, Payload, chain, decode_chain, dss_len, encode_chain};

pub use object::{Body, Object, ebcdic, ebcdic_fixed, text};

use std::io::Read;

use crate::Error;

/// Read one DSS from a stream.
pub fn read_dss(r: &mut impl Read) -> Result<Option<Dss>, Error> {
    let mut buf = vec![0u8; dss::DSS_HEAD];
    match r.read_exact(&mut buf) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    if buf[2] != dss::MAGIC {
        return Err(Error::Dss(format!("byte 2 is {:02X}", buf[2])));
    }
    let first = crate::be16(&buf, 0);
    let ll = usize::from(first & 0x7FFF).max(dss::DSS_HEAD);
    buf.resize(ll, 0);
    r.read_exact(&mut buf[dss::DSS_HEAD..])?;
    let mut more = first & 0x8000 != 0;
    while more {
        let at = buf.len();
        buf.resize(at + 2, 0);
        r.read_exact(&mut buf[at..])?;
        let l = crate::be16(&buf, at);
        more = l & 0x8000 != 0;
        let l = usize::from(l & 0x7FFF).max(2);
        buf.resize(at + l, 0);
        r.read_exact(&mut buf[at + 2..])?;
    }
    Ok(Some(Dss::decode(&buf)?.0))
}

/// Read DSSes until one ends a chain (not chained).
pub fn read_chain(r: &mut impl Read) -> Result<Vec<Dss>, Error> {
    let mut out = Vec::new();
    loop {
        let Some(d) = read_dss(r)? else {
            if out.is_empty() {
                return Err(Error::Io("connection closed".into()));
            }
            return Ok(out);
        };
        let more = d.chained();
        out.push(d);
        if !more {
            return Ok(out);
        }
    }
}
