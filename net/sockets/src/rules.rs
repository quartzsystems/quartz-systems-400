// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Port restrictions (`ADDTCPPORT`): who may bind a port. [`Sockets`]
//! asks on every `bind` of a TCP or UDP socket to a nonzero port and
//! answers `EACCES` on a refusal. TCP/IP configuration implements
//! [`PortRules`] over its restriction list; [`Restrictions`] is the
//! same rule over a plain list, and [`AllowAll`] has none.
//!
//! [`Sockets`]: crate::Sockets

use crate::consts::{IPPROTO_TCP, IPPROTO_UDP};

/// The restriction check.
pub trait PortRules {
    /// Whether `user` may bind `port` for `protocol` (`IPPROTO_TCP` or
    /// `IPPROTO_UDP`).
    fn may_bind(&self, protocol: i32, port: u16, user: &str) -> bool;
}

/// No restrictions.
#[derive(Debug, Clone, Copy, Default)]
pub struct AllowAll;

impl PortRules for AllowAll {
    fn may_bind(&self, _protocol: i32, _port: u16, _user: &str) -> bool {
        true
    }
}

/// One `ADDTCPPORT` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Restriction {
    /// `IPPROTO_TCP` or `IPPROTO_UDP`.
    pub protocol: i32,
    /// Lowest port covered.
    pub low: u16,
    /// Highest port covered.
    pub high: u16,
    /// The profile allowed; `None` is `USRPRF(*NONE)`, nobody.
    pub user: Option<String>,
}

/// IBM's rule over a list: a port no entry covers is free; a covered
/// port is open only to the profiles of the entries covering it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Restrictions(pub Vec<Restriction>);

impl PortRules for Restrictions {
    fn may_bind(&self, protocol: i32, port: u16, user: &str) -> bool {
        let mut covered = self
            .0
            .iter()
            .filter(|r| r.protocol == protocol && (r.low..=r.high).contains(&port))
            .peekable();
        if covered.peek().is_none() {
            return true;
        }
        covered.any(|r| r.user.as_deref().is_some_and(|u| u.eq_ignore_ascii_case(user)))
    }
}

/// The protocol a socket's restrictions are kept under, if any: TCP for
/// a stream socket, UDP for a datagram one.
#[must_use]
pub fn protocol_of(socktype: i32, protocol: i32) -> Option<i32> {
    match (socktype, protocol) {
        (crate::consts::SOCK_STREAM, 0 | IPPROTO_TCP) => Some(IPPROTO_TCP),
        (crate::consts::SOCK_DGRAM, 0 | IPPROTO_UDP) => Some(IPPROTO_UDP),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(protocol: i32, low: u16, high: u16, user: Option<&str>) -> Restriction {
        Restriction {
            protocol,
            low,
            high,
            user: user.map(str::to_string),
        }
    }

    #[test]
    fn covered_ports_take_the_listed_profiles_only() {
        let rules = Restrictions(vec![
            r(IPPROTO_TCP, 8000, 8100, Some("WEBSRV")),
            r(IPPROTO_TCP, 8050, 8050, Some("QTCP")),
            r(IPPROTO_UDP, 5000, 5000, None),
        ]);
        assert!(rules.may_bind(IPPROTO_TCP, 7999, "ANYONE"));
        assert!(rules.may_bind(IPPROTO_TCP, 8000, "websrv"), "profiles match in any case");
        assert!(!rules.may_bind(IPPROTO_TCP, 8100, "QTCP"));
        assert!(rules.may_bind(IPPROTO_TCP, 8050, "QTCP"), "a second entry adds a profile");
        assert!(rules.may_bind(IPPROTO_TCP, 8050, "WEBSRV"));
        assert!(!rules.may_bind(IPPROTO_UDP, 5000, "QSECOFR"), "*NONE is nobody");
        assert!(rules.may_bind(IPPROTO_TCP, 5000, "X"), "per protocol");
        assert!(AllowAll.may_bind(IPPROTO_UDP, 5000, "X"));
    }

    #[test]
    fn protocols_of_sockets() {
        use crate::consts::{SOCK_DGRAM, SOCK_RAW, SOCK_STREAM};
        assert_eq!(protocol_of(SOCK_STREAM, 0), Some(IPPROTO_TCP));
        assert_eq!(protocol_of(SOCK_DGRAM, IPPROTO_UDP), Some(IPPROTO_UDP));
        assert_eq!(protocol_of(SOCK_RAW, 1), None);
    }
}
