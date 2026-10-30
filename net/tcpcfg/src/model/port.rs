// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Port restrictions: `ADDTCPPORT`, `RMVTCPPORT`. A port (or range) of a
//! protocol with entries may be bound only by the user profiles the
//! entries name; a port no entry covers is open to everyone. The sockets
//! layer asks [`PortRestrictions::allows`] at every `bind()`; the
//! nftables backstop gets [`PortRestrictions::ranges`].

use super::Proto;
use crate::addr::bad;
use crate::error::{Error, Result};
use crate::msg;

/// IBM's limit on port restriction entries.
pub const MAX: usize = 100;

/// One restriction entry.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PortRestriction {
    /// The lower port.
    pub lower: u16,
    /// The upper port; `None` is `*ONLY`.
    pub upper: Option<u16>,
    /// The protocol.
    pub proto: Proto,
    /// The user profile allowed.
    pub user: String,
}

impl PortRestriction {
    /// The last port covered.
    pub fn last(&self) -> u16 {
        self.upper.unwrap_or(self.lower)
    }

    /// Whether it covers `port` of `proto`.
    pub fn covers(&self, port: u16, proto: Proto) -> bool {
        self.proto == proto && (self.lower..=self.last()).contains(&port)
    }

    /// Check the values.
    pub fn validate(&self) -> Result<()> {
        if self.lower == 0 || self.last() < self.lower {
            return Err(bad("PORT", &format!("{} {}", self.lower, self.last())));
        }
        if self.user.is_empty() || self.user.len() > 10 {
            return Err(bad("USRPRF", &self.user));
        }
        Ok(())
    }
}

/// The port restriction table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PortRestrictions {
    /// The entries, in the order added.
    pub list: Vec<PortRestriction>,
}

impl PortRestrictions {
    /// `ADDTCPPORT`. A range overlapping another of the protocol must be
    /// the same range (more users for it).
    pub fn add(&mut self, mut r: PortRestriction) -> Result<()> {
        r.user = r.user.to_ascii_uppercase();
        if r.upper == Some(r.lower) {
            r.upper = None;
        }
        r.validate()?;
        if self.list.contains(&r) {
            return Err(Error::new(msg::DUP_PORT, "The same port restriction already exists."));
        }
        if let Some(o) = self.list.iter().find(|o| {
            o.proto == r.proto
                && o.lower <= r.last()
                && r.lower <= o.last()
                && (o.lower, o.last()) != (r.lower, r.last())
        }) {
            return Err(Error::new(
                msg::BAD_VALUE,
                format!(
                    "Ports {}-{} overlap the restricted range {}-{} of {}.",
                    r.lower,
                    r.last(),
                    o.lower,
                    o.last(),
                    r.proto.name()
                ),
            ));
        }
        if self.list.len() >= MAX {
            return Err(Error::new(msg::FULL, format!("At most {MAX} port restrictions can be added.")));
        }
        self.list.push(r);
        Ok(())
    }

    /// `RMVTCPPORT`: `lower` `None` is `*ALL` (every entry of the
    /// protocol and user); the entries removed.
    pub fn remove(&mut self, lower: Option<u16>, upper: Option<u16>, proto: Proto, user: &str) -> Result<usize> {
        let user = user.to_ascii_uppercase();
        let upper = match (lower, upper) {
            (Some(l), Some(u)) if u == l => None,
            (_, u) => u,
        };
        let before = self.list.len();
        self.list.retain(|r| {
            !(r.proto == proto
                && r.user == user
                && lower.is_none_or(|l| r.lower == l && r.upper == upper))
        });
        let n = before - self.list.len();
        if n == 0 {
            return Err(Error::new(msg::NOT_FOUND, "No port restriction matches."));
        }
        Ok(n)
    }

    /// Whether `user` may bind `port` of `proto`.
    pub fn allows(&self, port: u16, proto: Proto, user: &str) -> bool {
        let mut covered = false;
        for r in self.list.iter().filter(|r| r.covers(port, proto)) {
            covered = true;
            if r.user.eq_ignore_ascii_case(user) {
                return true;
            }
        }
        !covered
    }

    /// The distinct restricted ranges: what the backstop drops.
    pub fn ranges(&self) -> Vec<(Proto, u16, u16)> {
        let mut v: Vec<(Proto, u16, u16)> = self.list.iter().map(|r| (r.proto, r.lower, r.last())).collect();
        v.sort();
        v.dedup();
        v
    }

    /// The entries in the panel's order.
    pub fn sorted(&self) -> Vec<&PortRestriction> {
        let mut v: Vec<&PortRestriction> = self.list.iter().collect();
        v.sort_by_key(|r| (r.lower, r.last(), r.proto, r.user.clone()));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(lower: u16, upper: Option<u16>, proto: Proto, user: &str) -> PortRestriction {
        PortRestriction {
            lower,
            upper,
            proto,
            user: user.into(),
        }
    }

    #[test]
    fn evaluation() {
        let mut t = PortRestrictions::default();
        t.add(r(8000, Some(8100), Proto::Tcp, "web")).unwrap();
        t.add(r(8000, Some(8100), Proto::Tcp, "QTMHHTTP")).unwrap();
        assert_eq!(t.add(r(8000, Some(8100), Proto::Tcp, "WEB")).unwrap_err().msgid, msg::DUP_PORT);
        assert!(t.add(r(8050, None, Proto::Tcp, "X")).is_err(), "partial overlap");
        t.add(r(8050, None, Proto::Udp, "X")).unwrap();
        assert!(t.allows(8050, Proto::Tcp, "WEB"));
        assert!(t.allows(8050, Proto::Tcp, "qtmhhttp"));
        assert!(!t.allows(8050, Proto::Tcp, "ANNE"));
        assert!(t.allows(8101, Proto::Tcp, "ANNE"));
        assert!(!t.allows(8050, Proto::Udp, "WEB"));
        assert_eq!(t.ranges(), vec![(Proto::Tcp, 8000, 8100), (Proto::Udp, 8050, 8050)]);
        assert_eq!(t.remove(None, None, Proto::Tcp, "web").unwrap(), 1);
        assert!(!t.allows(8050, Proto::Tcp, "WEB"));
        assert!(t.remove(Some(1), None, Proto::Tcp, "WEB").is_err());
        assert_eq!(t.remove(Some(8050), Some(8050), Proto::Udp, "X").unwrap(), 1);
    }
}
