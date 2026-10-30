// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Servers: each standard server's attributes ([`ServerAttrs`]: the
//! `AUTOSTART` its `CHGxxxA` command sets and a bag of keyword values
//! the server's own code defines), and the user-defined servers of
//! `ADDTCPSVR`/`CHGTCPSVR`/`RMVTCPSVR` (`QATOCSTART`). `STRTCP
//! STRSVR(*YES)` starts every server whose autostart is on.

use crate::addr::bad;
use crate::error::{Error, Result};
use crate::msg;

/// The standard servers and whether a new system starts each with
/// TCP/IP.
pub const STANDARD: &[(&str, bool)] = &[
    ("*TELNET", true),
    ("*FTP", true),
    ("*SMTP", false),
    ("*LPD", false),
    ("*HTTP", false),
    ("*SNMP", false),
    ("*ROUTED", false),
    ("*BOOTP", false),
    ("*TFTP", false),
    ("*REXEC", false),
    ("*DHCP", false),
    ("*DNS", false),
    ("*POP", false),
    ("*DDM", false),
    ("*NTP", false),
    ("*SSHD", false),
    ("*HOSTSVR", false),
];

/// One server's attributes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerAttrs {
    /// `*TELNET`, `*FTP`, ...
    pub server: String,
    /// Started by `STRTCP`.
    pub autostart: bool,
    /// The server's own attributes: `(keyword, value)`, in order.
    pub bag: Vec<(String, String)>,
}

impl ServerAttrs {
    /// A value from the bag.
    pub fn get(&self, kwd: &str) -> Option<&str> {
        self.bag
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(kwd))
            .map(|(_, v)| v.as_str())
    }

    /// Set a bag value (replacing one already there).
    pub fn set(&mut self, kwd: &str, value: &str) {
        let k = kwd.to_ascii_uppercase();
        if let Some(slot) = self.bag.iter_mut().find(|(x, _)| *x == k) {
            slot.1 = value.to_string();
        } else {
            self.bag.push((k, value.to_string()));
        }
    }
}

/// A user-defined server (`ADDTCPSVR`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserServer {
    /// `SVRSPCVAL`.
    pub special: String,
    /// `PGM` as `(name, library)`.
    pub pgm: (String, String),
    /// `SVRNAME`.
    pub name: String,
    /// `SVRTYPE`.
    pub kind: String,
    /// `AUTOSTART`.
    pub autostart: bool,
    /// `TEXT`.
    pub text: String,
}

/// Every server's attributes and the user-defined servers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Servers {
    /// The standard servers, in [`STANDARD`]'s order.
    pub attrs: Vec<ServerAttrs>,
    /// The user-defined ones.
    pub user: Vec<UserServer>,
}

impl Default for Servers {
    fn default() -> Self {
        Self {
            attrs: STANDARD
                .iter()
                .map(|(s, a)| ServerAttrs {
                    server: (*s).to_string(),
                    autostart: *a,
                    bag: Vec::new(),
                })
                .collect(),
            user: Vec::new(),
        }
    }
}

impl Servers {
    /// A standard server's attributes.
    pub fn get(&self, server: &str) -> Option<&ServerAttrs> {
        self.attrs.iter().find(|a| a.server.eq_ignore_ascii_case(server))
    }

    /// A standard server's attributes, to change; added when a server
    /// agent names one the table does not know yet.
    pub fn get_mut(&mut self, server: &str) -> &mut ServerAttrs {
        let name = server.to_ascii_uppercase();
        if let Some(i) = self.attrs.iter().position(|a| a.server == name) {
            return &mut self.attrs[i];
        }
        self.attrs.push(ServerAttrs {
            server: name,
            autostart: false,
            bag: Vec::new(),
        });
        self.attrs.last_mut().expect("pushed")
    }

    /// The servers `STRTCP` starts: the standard ones with autostart
    /// on, then the user-defined ones.
    pub fn autostart(&self) -> Vec<String> {
        self.attrs
            .iter()
            .filter(|a| a.autostart)
            .map(|a| a.server.clone())
            .chain(self.user.iter().filter(|u| u.autostart).map(|u| u.special.clone()))
            .collect()
    }

    /// `ADDTCPSVR`.
    pub fn add_user(&mut self, s: UserServer) -> Result<()> {
        if !s.special.starts_with('*') || s.special.len() > 10 {
            return Err(bad("SVRSPCVAL", &s.special));
        }
        if self.get(&s.special).is_some() || self.user.iter().any(|u| u.special.eq_ignore_ascii_case(&s.special)) {
            return Err(Error::new(msg::EXISTS, format!("Server {} already exists.", s.special)));
        }
        self.user.push(s);
        Ok(())
    }

    /// `CHGTCPSVR`: change a user-defined server in place.
    pub fn change_user(&mut self, special: &str, f: impl FnOnce(&mut UserServer)) -> Result<()> {
        let u = self
            .user
            .iter_mut()
            .find(|u| u.special.eq_ignore_ascii_case(special))
            .ok_or_else(|| Error::new(msg::NOT_FOUND, format!("Server {special} is not defined.")))?;
        f(u);
        Ok(())
    }

    /// `RMVTCPSVR`.
    pub fn remove_user(&mut self, special: &str) -> Result<UserServer> {
        let at = self
            .user
            .iter()
            .position(|u| u.special.eq_ignore_ascii_case(special))
            .ok_or_else(|| Error::new(msg::NOT_FOUND, format!("Server {special} is not defined.")))?;
        Ok(self.user.remove(at))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autostart_list() {
        let mut s = Servers::default();
        assert_eq!(s.autostart(), vec!["*TELNET".to_string(), "*FTP".to_string()]);
        s.get_mut("*ftp").autostart = false;
        s.get_mut("*NEWSVR").set("port", "9999");
        assert_eq!(s.get("*NEWSVR").unwrap().get("PORT"), Some("9999"));
        s.add_user(UserServer {
            special: "*MYSVR".into(),
            pgm: ("MYPGM".into(), "QGPL".into()),
            name: "my server".into(),
            kind: "TEST".into(),
            autostart: true,
            text: String::new(),
        })
        .unwrap();
        assert_eq!(s.autostart(), vec!["*TELNET".to_string(), "*MYSVR".to_string()]);
        assert!(s.add_user(s.user[0].clone()).is_err());
        s.change_user("*mysvr", |u| u.autostart = false).unwrap();
        assert_eq!(s.autostart().len(), 1);
        s.remove_user("*MYSVR").unwrap();
        assert!(s.remove_user("*MYSVR").is_err());
    }
}
