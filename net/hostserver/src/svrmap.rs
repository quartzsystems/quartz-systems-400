// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The port mapper (`as-svrmap`, 449): a service name in, `+` and the
//! port out; one lookup per connection.

use hostsvr::server::{ALL, ServerId};
use hostsvr::svrmap::{Reply, Request};

/// What an unknown service gets.
pub const UNKNOWN: u8 = b'-';

/// The port of a service name: the plain port, or with `-s` the TLS port.
pub fn port_of(service: &str) -> Option<u16> {
    let s = service.trim().to_ascii_lowercase();
    let (base, tls) = match s.strip_suffix("-s") {
        Some(b) => (b.to_string(), true),
        None => (s, false),
    };
    if base == "as-svrmap" {
        return (!tls).then_some(hostsvr::server::SVRMAP_PORT);
    }
    if base == "drda" || base == "ddm" {
        return Some(if tls { 448 } else { 446 });
    }
    ALL.into_iter()
        .find(|s: &ServerId| s.service() == base)
        .map(|s| if tls { s.tls_port() } else { s.port() })
}

/// The answer to a request's bytes.
pub fn answer(request: &[u8]) -> Vec<u8> {
    match Request::decode(request)
        .ok()
        .and_then(|r| port_of(&r.service))
    {
        Some(p) => Reply::Port(u32::from(p)).encode(),
        None => vec![UNKNOWN],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports() {
        assert_eq!(port_of("as-signon"), Some(8476));
        assert_eq!(port_of("as-database-s"), Some(9471));
        assert_eq!(port_of("as-nothing"), None);
        assert_eq!(answer(b"as-rmtcmd"), vec![0x2b, 0, 0, 0x21, 0x1b]);
    }
}
