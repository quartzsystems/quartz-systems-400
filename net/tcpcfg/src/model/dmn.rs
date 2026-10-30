// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Domain information: `CHGTCPDMN`.

use std::net::IpAddr;

use super::Proto;
use crate::addr::bad;
use crate::error::{Error, Result};
use crate::msg;

/// `HOSTSCHPTY`: where a name is looked up first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchPriority {
    /// `*LOCAL`: the host table, then the name servers.
    #[default]
    Local,
    /// `*REMOTE`: the name servers, then the host table.
    Remote,
}

impl SearchPriority {
    /// The special value.
    pub fn name(self) -> &'static str {
        match self {
            Self::Local => "*LOCAL",
            Self::Remote => "*REMOTE",
        }
    }
}

/// `INLDMNSVR`: which name server is asked first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InitialServer {
    /// `*FIRST`.
    #[default]
    First,
    /// `*ROTATE`.
    Rotate,
}

impl InitialServer {
    /// The special value.
    pub fn name(self) -> &'static str {
        match self {
            Self::First => "*FIRST",
            Self::Rotate => "*ROTATE",
        }
    }
}

/// The domain information.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Domain {
    /// `HOSTNAME`: the local host name.
    pub host_name: String,
    /// `DMNNAME`: the local domain name.
    pub domain_name: String,
    /// `DMNSCHLIST`: the domains searched; empty is `*DFT` (the local
    /// domain).
    pub search_list: Vec<String>,
    /// `HOSTSCHPTY`.
    pub priority: SearchPriority,
    /// `INTNETADR`: up to three name servers.
    pub dns: Vec<IpAddr>,
    /// `PORT`.
    pub port: u16,
    /// `PROTOCOL`.
    pub protocol: Proto,
    /// `INLDMNSVR`.
    pub initial: InitialServer,
    /// `DMNSVRRTY`, first element: attempts per server.
    pub retry_attempts: u8,
    /// `DMNSVRRTY`, second element: seconds between attempts.
    pub retry_interval: u8,
    /// `DNSSEC`.
    pub dnssec: bool,
}

impl Default for Domain {
    fn default() -> Self {
        Self {
            host_name: String::new(),
            domain_name: String::new(),
            search_list: Vec::new(),
            priority: SearchPriority::Local,
            dns: Vec::new(),
            port: 53,
            protocol: Proto::Udp,
            initial: InitialServer::First,
            retry_attempts: 2,
            retry_interval: 2,
            dnssec: false,
        }
    }
}

impl Domain {
    /// The fully qualified local name (`HOSTNAME.DMNNAME`).
    pub fn fqdn(&self) -> String {
        match (self.host_name.is_empty(), self.domain_name.is_empty()) {
            (true, _) => String::new(),
            (false, true) => self.host_name.clone(),
            (false, false) => format!("{}.{}", self.host_name, self.domain_name),
        }
    }

    /// The search list in effect: the list, or the local domain.
    pub fn search(&self) -> Vec<String> {
        if self.search_list.is_empty() && !self.domain_name.is_empty() {
            vec![self.domain_name.clone()]
        } else {
            self.search_list.clone()
        }
    }

    /// Check the names.
    pub fn validate(&self) -> Result<()> {
        if !self.host_name.is_empty() {
            let h = super::host::check_name(&self.host_name)?;
            if h.contains('.') {
                return Err(bad("HOSTNAME", &self.host_name));
            }
        }
        if !self.domain_name.is_empty() && super::host::check_name(&self.domain_name).is_err() {
            return Err(Error::new(
                msg::BAD_DOMAIN,
                format!("{} is not a valid local domain name.", self.domain_name),
            ));
        }
        if self.host_name.len() + self.domain_name.len() + 1 > 255 {
            return Err(Error::new(
                msg::NAME_TOO_LONG,
                "The host name and domain name together are longer than 255 characters.",
            ));
        }
        if self.dns.len() > 3 {
            return Err(bad("INTNETADR", "more than three name servers"));
        }
        for d in &self.search_list {
            super::host::check_name(d).map_err(|_| bad("DMNSCHLIST", d))?;
        }
        if self.search_list.len() > 6 {
            return Err(bad("DMNSCHLIST", "more than six domains"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_rules() {
        let mut d = Domain {
            host_name: "QS400".into(),
            domain_name: "EXAMPLE.COM".into(),
            ..Domain::default()
        };
        d.validate().unwrap();
        assert_eq!(d.fqdn(), "QS400.EXAMPLE.COM");
        assert_eq!(d.search(), vec!["EXAMPLE.COM".to_string()]);
        d.host_name = "A.B".into();
        assert!(d.validate().is_err());
        d.host_name = "A".into();
        d.domain_name = "-X".into();
        assert_eq!(d.validate().unwrap_err().msgid, msg::BAD_DOMAIN);
    }
}
