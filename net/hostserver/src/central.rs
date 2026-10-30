// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The central server (8470): NLS exchange attributes (the job CCSID
//! and the CCSIDs the server converts), conversion tables from the
//! `ccsid` crate's tables, and license requests, every one granted.

use hostsvr::central::id;
use hostsvr::cp::central as cp;
use hostsvr::frame::{Frame, Param};
use regfac::ExitRequest;
use regfac::formats::host::{License, zscl0100, zscn0100};

use crate::exits::{Exits, who};
use crate::session::{Auth, Session};
use crate::{Handler, Outcome, SERVER_LEVEL, SERVER_VRM, reply, text};

/// The exit points.
pub const EXIT_LM: &str = "QIBM_QZSC_LM";
/// NLS.
pub const EXIT_NLS: &str = "QIBM_QZSC_NLS";

/// Primary return codes.
pub mod rc {
    /// Success.
    pub const OK: u16 = 0;
    /// A CCSID the server cannot convert.
    pub const CCSID: u16 = 0x0102;
    /// An exit program refused the request.
    pub const EXIT: u16 = 0x0105;
}

/// The server.
pub struct Central<A, X> {
    /// Sign-on state.
    pub session: Session,
    /// Signing on.
    pub auth: A,
    /// Exit programs.
    pub exits: X,
    /// The job's CCSID.
    pub job_ccsid: u32,
}

/// The CCSIDs the server converts: the job's, the code pages of the
/// `ccsid` crate, UTF-16 and UTF-8.
pub fn ccsids() -> Vec<u32> {
    let mut v: Vec<u32> = vec![37];
    for c in ccsid::conv::supported() {
        let c = u32::from(c);
        if !v.contains(&c) {
            v.push(c);
        }
    }
    for c in [1200, 13488, 1208] {
        if !v.contains(&c) {
            v.push(c);
        }
    }
    v
}

fn is_unicode(c: u32) -> bool {
    matches!(c, 1200 | 13488 | 61952)
}

fn is_ebcdic(c: u32) -> bool {
    c == 37
        || u16::try_from(c)
            .ok()
            .and_then(|c| ccsid::conv::Codec::of(c).ok().flatten())
            .is_some_and(|k| k.is_ebcdic())
}

/// Whether a CCSID converts here.
pub fn converts(c: u32) -> bool {
    is_unicode(c) || c == 1208 || ccsids().contains(&c)
}

/// The conversion of each code point of `from` into `to`: one char per
/// input unit (a byte, or a u16 for UTF-16 sources), u16 values for a
/// UTF-16 target, bytes otherwise; unmappable ones become the target's
/// substitution character.
pub fn table(from: u32, to: u32, points: &[u8]) -> Vec<u8> {
    let units: Vec<Vec<u8>> = if is_unicode(from) {
        points.chunks(2).map(<[u8]>::to_vec).collect()
    } else {
        points.iter().map(|b| vec![*b]).collect()
    };
    let mut out = Vec::new();
    for u in units {
        let s = text::decode(from, &u);
        let ch = s.chars().next().unwrap_or('\u{1A}');
        if is_unicode(to) {
            let mut buf = [0u16; 2];
            let x = ch.encode_utf16(&mut buf);
            out.extend(x[0].to_be_bytes());
        } else {
            let b = text::encode(to, &ch.to_string());
            out.push(match b.as_slice() {
                [one] => *one,
                _ if is_ebcdic(to) => 0x3F,
                _ => 0x1A,
            });
        }
    }
    out
}

impl<A: Auth, X: Exits> Central<A, X> {
    /// A server for one connection.
    pub fn new(auth: A, exits: X) -> Self {
        Self {
            session: Session::default(),
            auth,
            exits,
            job_ccsid: 37,
        }
    }

    fn user(&self) -> String {
        self.session.user.clone().unwrap_or_default()
    }

    fn exchange(&mut self, f: &Frame) -> Outcome {
        let mut t = Vec::new();
        t.extend(0u16.to_be_bytes());
        t.extend(rc::OK.to_be_bytes());
        t.extend(0u16.to_be_bytes());
        t.extend(SERVER_LEVEL.to_be_bytes());
        t.extend(self.job_ccsid.to_be_bytes());
        t.extend(SERVER_VRM.to_be_bytes());
        t.extend(1u16.to_be_bytes());
        t.extend(1u16.to_be_bytes());
        t.extend(1u16.to_be_bytes());
        let list = ccsids();
        let mut d = reply::len32(list.len()).to_be_bytes().to_vec();
        for c in list {
            d.extend(c.to_be_bytes());
        }
        Outcome::one(reply::frame(
            &f.header,
            id::EXCHANGE_ATTRIBUTES,
            t,
            vec![Param::new(cp::CCSID_LIST, d)],
        ))
    }

    fn conversion(&mut self, f: &Frame) -> Outcome {
        let from = reply::be32(&f.template, 2);
        let to = reply::be32(&f.template, 6);
        let map = reply::be16(&f.template, 10);
        let req = ExitRequest::host(
            "ZSCN0100",
            zscn0100(
                &who(&self.user(), regfac::formats::host::func::RTV_CONVERSION_MAP),
                i32::try_from(from).unwrap_or(0),
                i32::try_from(to).unwrap_or(0),
                i16::try_from(map).unwrap_or(0),
            ),
        );
        let head = |code: u16| {
            let mut t = 0u16.to_be_bytes().to_vec();
            t.extend(code.to_be_bytes());
            t.extend(0u16.to_be_bytes());
            t.extend(0u16.to_be_bytes());
            t
        };
        if !self.exits.allow(EXIT_NLS, &req) {
            return Outcome::one(reply::frame(&f.header, id::CONVERSION_TABLE, head(rc::EXIT), Vec::new()));
        }
        if !converts(from) || !converts(to) {
            return Outcome::one(reply::frame(&f.header, id::CONVERSION_TABLE, head(rc::CCSID), Vec::new()));
        }
        let points = f.param(cp::CODE_POINTS).map_or_else(
            || hostsvr::central::code_points(!is_unicode(from)).data,
            |p| p.data.clone(),
        );
        let data = table(from, to, &points);
        Outcome::one(reply::frame(
            &f.header,
            id::CONVERSION_TABLE,
            head(rc::OK),
            vec![Param::new(cp::CODE_POINTS, data)],
        ))
    }

    fn license(&mut self, f: &Frame) -> Outcome {
        let function = i32::from(f.header.req_rep_id);
        let req = ExitRequest::host(
            "ZSCL0100",
            zscl0100(&who(&self.user(), function), &License::default()),
        );
        let code = if self.exits.allow(EXIT_LM, &req) { rc::OK } else { rc::EXIT };
        let mut t = 0u16.to_be_bytes().to_vec();
        t.extend(code.to_be_bytes());
        t.extend(0u16.to_be_bytes());
        t.extend(0u16.to_be_bytes());
        Outcome::one(reply::frame(&f.header, f.header.req_rep_id, t, f.params.clone()))
    }
}

impl<A: Auth, X: Exits> Handler for Central<A, X> {
    fn handle(&mut self, frame: &[u8]) -> Outcome {
        let Ok(f) = Frame::decode(frame) else {
            return Outcome::close();
        };
        if let Some(o) = self.session.common(&f, &mut self.auth) {
            return o;
        }
        if !self.session.started() {
            return Outcome::close();
        }
        match f.header.req_rep_id {
            id::EXCHANGE_ATTRIBUTES => self.exchange(&f),
            id::CONVERSION_TABLE => self.conversion(&f),
            id::REQUEST_LICENSE | id::RELEASE_LICENSE | id::RETRIEVE_LICENSE_INFO => self.license(&f),
            _ => Outcome::close(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables() {
        let t = table(37, 13488, &[0xC1, 0x40]);
        assert_eq!(t, vec![0, 0x41, 0, 0x20]);
        let t = table(13488, 37, &[0, 0x41]);
        assert_eq!(t, vec![0xC1]);
    }
}
