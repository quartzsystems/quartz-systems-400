// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The options the client sends and reads (RFC 2132); the others are
//! kept as they came.

use std::net::Ipv4Addr;

use super::Kind;
use crate::Error;

/// Option codes.
#[allow(missing_docs)] // RFC 2132's names
pub mod code {
    pub const PAD: u8 = 0;
    pub const SUBNET_MASK: u8 = 1;
    pub const ROUTER: u8 = 3;
    pub const DNS: u8 = 6;
    pub const HOST_NAME: u8 = 12;
    pub const DOMAIN: u8 = 15;
    pub const REQUESTED_IP: u8 = 50;
    pub const LEASE_TIME: u8 = 51;
    pub const OVERLOAD: u8 = 52;
    pub const MESSAGE_TYPE: u8 = 53;
    pub const SERVER_ID: u8 = 54;
    pub const PARAMS: u8 = 55;
    pub const MESSAGE: u8 = 56;
    pub const MAX_SIZE: u8 = 57;
    pub const RENEWAL: u8 = 58;
    pub const REBINDING: u8 = 59;
    pub const CLIENT_ID: u8 = 61;
    pub const BOOTFILE: u8 = 67;
    pub const END: u8 = 255;
}

/// The options of one message.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// 53.
    pub kind: Option<Kind>,
    /// 54.
    pub server_id: Option<Ipv4Addr>,
    /// 50.
    pub requested_ip: Option<Ipv4Addr>,
    /// 51, seconds.
    pub lease_secs: Option<u32>,
    /// 58 (T1), seconds.
    pub renewal_secs: Option<u32>,
    /// 59 (T2), seconds.
    pub rebinding_secs: Option<u32>,
    /// 1.
    pub subnet_mask: Option<Ipv4Addr>,
    /// 3.
    pub routers: Vec<Ipv4Addr>,
    /// 6.
    pub dns: Vec<Ipv4Addr>,
    /// 15.
    pub domain: Option<String>,
    /// 12.
    pub host_name: Option<String>,
    /// 61.
    pub client_id: Option<Vec<u8>>,
    /// 55.
    pub params: Vec<u8>,
    /// 57.
    pub max_size: Option<u16>,
    /// 56.
    pub message: Option<String>,
    /// 52.
    pub overload: Option<u8>,
    /// 67.
    pub bootfile: Option<String>,
    /// Any other option, as it came.
    pub other: Vec<(u8, Vec<u8>)>,
}

fn addrs(v: &[u8]) -> Vec<Ipv4Addr> {
    v.as_chunks::<4>()
        .0
        .iter()
        .map(|c| Ipv4Addr::from(*c))
        .collect()
}

fn addr(v: &[u8]) -> Option<Ipv4Addr> {
    addrs(v).first().copied()
}

fn u32_of(v: &[u8]) -> Option<u32> {
    Some(u32::from_be_bytes(v.get(..4)?.try_into().ok()?))
}

fn text(v: &[u8]) -> String {
    String::from_utf8_lossy(v)
        .trim_end_matches('\0')
        .to_string()
}

/// One option: its code, length (at most 255 bytes, longer data is
/// split as RFC 3396 says) and data.
fn put(out: &mut Vec<u8>, c: u8, data: &[u8]) {
    for chunk in data.chunks(255) {
        out.push(c);
        out.push(u8::try_from(chunk.len()).unwrap_or(255));
        out.extend_from_slice(chunk);
    }
    if data.is_empty() {
        out.extend([c, 0]);
    }
}

impl Options {
    /// Append the options and the end marker.
    pub fn encode(&self, out: &mut Vec<u8>) {
        if let Some(k) = self.kind {
            put(out, code::MESSAGE_TYPE, &[k as u8]);
        }
        if let Some(id) = &self.client_id {
            put(out, code::CLIENT_ID, id);
        }
        if let Some(a) = self.requested_ip {
            put(out, code::REQUESTED_IP, &a.octets());
        }
        if let Some(a) = self.server_id {
            put(out, code::SERVER_ID, &a.octets());
        }
        if let Some(n) = self.max_size {
            put(out, code::MAX_SIZE, &n.to_be_bytes());
        }
        if let Some(h) = &self.host_name {
            put(out, code::HOST_NAME, h.as_bytes());
        }
        if !self.params.is_empty() {
            put(out, code::PARAMS, &self.params);
        }
        if let Some(m) = self.subnet_mask {
            put(out, code::SUBNET_MASK, &m.octets());
        }
        if !self.routers.is_empty() {
            put(
                out,
                code::ROUTER,
                &self
                    .routers
                    .iter()
                    .flat_map(Ipv4Addr::octets)
                    .collect::<Vec<_>>(),
            );
        }
        if !self.dns.is_empty() {
            put(
                out,
                code::DNS,
                &self
                    .dns
                    .iter()
                    .flat_map(Ipv4Addr::octets)
                    .collect::<Vec<_>>(),
            );
        }
        if let Some(d) = &self.domain {
            put(out, code::DOMAIN, d.as_bytes());
        }
        for (c, n) in [
            (code::LEASE_TIME, self.lease_secs),
            (code::RENEWAL, self.renewal_secs),
            (code::REBINDING, self.rebinding_secs),
        ] {
            if let Some(n) = n {
                put(out, c, &n.to_be_bytes());
            }
        }
        if let Some(m) = &self.message {
            put(out, code::MESSAGE, m.as_bytes());
        }
        if let Some(o) = self.overload {
            put(out, code::OVERLOAD, &[o]);
        }
        if let Some(f) = &self.bootfile {
            put(out, code::BOOTFILE, f.as_bytes());
        }
        for (c, v) in &self.other {
            put(out, *c, v);
        }
        out.push(code::END);
    }

    /// The options in `b` (after the cookie, or an overloaded field).
    /// An option that repeats is concatenated (RFC 3396).
    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        let mut raw: Vec<(u8, Vec<u8>)> = Vec::new();
        let mut i = 0;
        while i < b.len() {
            let c = b[i];
            if c == code::PAD {
                i += 1;
                continue;
            }
            if c == code::END {
                break;
            }
            let len = usize::from(
                *b.get(i + 1)
                    .ok_or(Error::Malformed("an option without a length"))?,
            );
            let data = b
                .get(i + 2..i + 2 + len)
                .ok_or(Error::Malformed("an option past the end"))?;
            match raw.iter_mut().find(|(x, _)| *x == c) {
                Some((_, v)) => v.extend_from_slice(data),
                None => raw.push((c, data.to_vec())),
            }
            i += 2 + len;
        }
        let mut o = Self::default();
        for (c, v) in raw {
            match c {
                code::MESSAGE_TYPE => o.kind = v.first().copied().and_then(Kind::from_u8),
                code::SERVER_ID => o.server_id = addr(&v),
                code::REQUESTED_IP => o.requested_ip = addr(&v),
                code::LEASE_TIME => o.lease_secs = u32_of(&v),
                code::RENEWAL => o.renewal_secs = u32_of(&v),
                code::REBINDING => o.rebinding_secs = u32_of(&v),
                code::SUBNET_MASK => o.subnet_mask = addr(&v),
                code::ROUTER => o.routers = addrs(&v),
                code::DNS => o.dns = addrs(&v),
                code::DOMAIN => o.domain = Some(text(&v)),
                code::HOST_NAME => o.host_name = Some(text(&v)),
                code::CLIENT_ID => o.client_id = Some(v),
                code::PARAMS => o.params = v,
                code::MAX_SIZE => o.max_size = v.get(..2).map(|x| u16::from_be_bytes([x[0], x[1]])),
                code::MESSAGE => o.message = Some(text(&v)),
                code::OVERLOAD => o.overload = v.first().copied(),
                code::BOOTFILE => o.bootfile = Some(text(&v)),
                _ => o.other.push((c, v)),
            }
        }
        Ok(o)
    }

    /// Fill what `self` lacks from `more` (an overloaded field's options).
    pub fn merge(&mut self, more: Self) {
        macro_rules! fill {
            ($($f:ident),*) => { $( if self.$f.is_none() { self.$f = more.$f; } )* };
        }
        fill!(
            kind,
            server_id,
            requested_ip,
            lease_secs,
            renewal_secs,
            rebinding_secs,
            subnet_mask,
            domain,
            host_name,
            client_id,
            max_size,
            message,
            bootfile
        );
        if self.routers.is_empty() {
            self.routers = more.routers;
        }
        if self.dns.is_empty() {
            self.dns = more.dns;
        }
        if self.params.is_empty() {
            self.params = more.params;
        }
        self.other.extend(more.other);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_option_round_trips() {
        let o = Options {
            kind: Some(Kind::Ack),
            server_id: Some(Ipv4Addr::new(10, 0, 2, 2)),
            requested_ip: Some(Ipv4Addr::new(10, 0, 2, 15)),
            lease_secs: Some(86_400),
            renewal_secs: Some(43_200),
            rebinding_secs: Some(75_600),
            subnet_mask: Some(Ipv4Addr::new(255, 255, 255, 0)),
            routers: vec![Ipv4Addr::new(10, 0, 2, 2)],
            dns: vec![Ipv4Addr::new(10, 0, 2, 3), Ipv4Addr::new(1, 1, 1, 1)],
            domain: Some("example.com".into()),
            host_name: Some("qs400".into()),
            client_id: Some(vec![1, 2, 3]),
            params: vec![1, 3, 6],
            max_size: Some(1500),
            message: Some("ok".into()),
            overload: None,
            bootfile: Some("qs400.efi".into()),
            other: vec![(66, b"10.0.2.2".to_vec())],
        };
        let mut b = Vec::new();
        o.encode(&mut b);
        assert_eq!(*b.last().unwrap(), code::END);
        assert_eq!(Options::decode(&b).unwrap(), o);
    }

    #[test]
    fn long_and_repeated_options() {
        let long = "x".repeat(300);
        let o = Options {
            domain: Some(long.clone()),
            ..Options::default()
        };
        let mut b = Vec::new();
        o.encode(&mut b);
        assert_eq!(
            Options::decode(&b).unwrap().domain.as_deref(),
            Some(long.as_str())
        );
        assert!(Options::decode(&[51, 4, 0]).is_err());
        assert_eq!(
            Options::decode(&[0, 0, 255, 51]).unwrap(),
            Options::default()
        );
    }
}
