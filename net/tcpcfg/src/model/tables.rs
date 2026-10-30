// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The related tables: services (`ADDSRVTBLE`, `RMVSRVTBLE`), protocols
//! (`ADDPCLTBLE`, `RMVPCLTBLE`) and networks (`ADDNETTBLE`,
//! `RMVNETTBLE`). The services table answers `getservbyname` and
//! `getservbyport`.

use std::net::Ipv4Addr;

use crate::addr::bad;
use crate::error::{Error, Result};
use crate::msg;

/// One service table entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceEntry {
    /// `SERVICE`, as added (case kept, compared without it).
    pub name: String,
    /// `PORT`.
    pub port: u16,
    /// `PROTOCOL`, as added (`tcp`, `udp`).
    pub proto: String,
    /// `ALIASES`, up to four.
    pub aliases: Vec<String>,
    /// `TEXT`.
    pub text: String,
}

/// One protocol table entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolEntry {
    /// `PROTOCOL`.
    pub name: String,
    /// `PCLNBR`.
    pub number: u8,
    /// `ALIASES`.
    pub aliases: Vec<String>,
    /// `TEXT`.
    pub text: String,
}

/// One network table entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkEntry {
    /// `NETWORK`.
    pub name: String,
    /// `INTNETADR`.
    pub addr: Ipv4Addr,
    /// `ALIASES`.
    pub aliases: Vec<String>,
    /// `TEXT`.
    pub text: String,
}

/// The services table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServicesTable {
    /// The entries.
    pub list: Vec<ServiceEntry>,
}

/// The services a new system has: the well-known ones and the host
/// servers' (`as-*`), as `getservbyname` callers expect them.
const SHIPPED: &[(&str, u16, &str)] = &[
    ("are-http", 12401, "TCP"),
    ("are-https", 12402, "TCP"),
    ("are-ias", 12405, "TCP"),
    ("are-ias-admin", 12403, "TCP"),
    ("are-ias-ssl", 12404, "TCP"),
    ("as-admin-http", 2001, "tcp"),
    ("as-admin-http", 2001, "udp"),
    ("as-admin-https", 2010, "tcp"),
    ("as-admin-https", 2010, "udp"),
    ("as-admin1-http", 2002, "tcp"),
    ("as-admin1-http", 2002, "udp"),
    ("as-admin1-http", 2015, "tcp"),
    ("as-admin1-http", 2015, "udp"),
    ("as-admin2-http", 2004, "tcp"),
    ("as-admin2-http", 2004, "udp"),
    ("as-admin2-http", 2016, "tcp"),
    ("as-admin2-http", 2016, "udp"),
    ("as-admin2-https", 2005, "tcp"),
    ("as-admin2-https", 2005, "udp"),
    ("as-admin3-http", 2006, "tcp"),
    ("as-admin3-http", 2006, "udp"),
    ("as-admin3-http", 2017, "tcp"),
    ("as-admin3-http", 2017, "udp"),
    ("as-central", 8470, "tcp"),
    ("as-central-s", 9470, "tcp"),
    ("as-database", 8471, "tcp"),
    ("as-database-s", 9471, "tcp"),
    ("as-dtaq", 8472, "tcp"),
    ("as-dtaq-s", 9472, "tcp"),
    ("as-file", 8473, "tcp"),
    ("as-file-s", 9473, "tcp"),
    ("as-netprt", 8474, "tcp"),
    ("as-netprt-s", 9474, "tcp"),
    ("as-rmtcmd", 8475, "tcp"),
    ("as-rmtcmd-s", 9475, "tcp"),
    ("as-signon", 8476, "tcp"),
    ("as-signon-s", 9476, "tcp"),
    ("as-svrmap", 449, "tcp"),
    ("as-svrmap", 449, "udp"),
    ("bootps", 67, "udp"),
    ("bootpc", 68, "udp"),
    ("ddm", 447, "tcp"),
    ("ddm-ssl", 448, "tcp"),
    ("domain", 53, "tcp"),
    ("domain", 53, "udp"),
    ("drda", 446, "tcp"),
    ("echo", 7, "tcp"),
    ("echo", 7, "udp"),
    ("finger", 79, "tcp"),
    ("ftp", 21, "tcp"),
    ("ftp-data", 20, "tcp"),
    ("ftps", 990, "tcp"),
    ("http", 80, "tcp"),
    ("https", 443, "tcp"),
    ("imap", 143, "tcp"),
    ("ldap", 389, "tcp"),
    ("ldaps", 636, "tcp"),
    ("lpd", 515, "tcp"),
    ("ntp", 123, "udp"),
    ("pop3", 110, "tcp"),
    ("printer", 515, "tcp"),
    ("rexec", 512, "tcp"),
    ("smtp", 25, "tcp"),
    ("snmp", 161, "udp"),
    ("snmp-trap", 162, "udp"),
    ("ssh", 22, "tcp"),
    ("submission", 587, "tcp"),
    ("telnet", 23, "tcp"),
    ("telnet-ssl", 992, "tcp"),
    ("tftp", 69, "udp"),
    ("time", 37, "tcp"),
    ("time", 37, "udp"),
];

impl ServicesTable {
    /// The shipped table.
    pub fn shipped() -> Self {
        Self {
            list: SHIPPED
                .iter()
                .map(|(n, p, pr)| ServiceEntry {
                    name: (*n).to_string(),
                    port: *p,
                    proto: (*pr).to_string(),
                    aliases: Vec::new(),
                    text: String::new(),
                })
                .collect(),
        }
    }

    /// `getservbyname`: the port of a service (or alias) for a protocol
    /// (any protocol when `None`).
    pub fn by_name(&self, name: &str, proto: Option<&str>) -> Option<&ServiceEntry> {
        self.sorted().into_iter().find(|e| {
            (e.name.eq_ignore_ascii_case(name) || e.aliases.iter().any(|a| a.eq_ignore_ascii_case(name)))
                && proto.is_none_or(|p| e.proto.eq_ignore_ascii_case(p))
        })
    }

    /// `getservbyport`.
    pub fn by_port(&self, port: u16, proto: Option<&str>) -> Option<&ServiceEntry> {
        self.sorted()
            .into_iter()
            .find(|e| e.port == port && proto.is_none_or(|p| e.proto.eq_ignore_ascii_case(p)))
    }

    /// `ADDSRVTBLE`.
    pub fn add(&mut self, e: ServiceEntry) -> Result<()> {
        if e.name.is_empty() || e.name.len() > 32 || e.port == 0 {
            return Err(bad("SERVICE", &e.name));
        }
        if self.list.iter().any(|x| {
            x.name.eq_ignore_ascii_case(&e.name) && x.port == e.port && x.proto.eq_ignore_ascii_case(&e.proto)
        }) {
            return Err(Error::new(msg::EXISTS, format!("Service {} {} {} already exists.", e.name, e.port, e.proto)));
        }
        self.list.push(e);
        Ok(())
    }

    /// `RMVSRVTBLE`: the entries removed.
    pub fn remove(&mut self, name: &str, port: u16, proto: &str) -> Result<usize> {
        let before = self.list.len();
        self.list.retain(|x| {
            !(x.name.eq_ignore_ascii_case(name) && x.port == port && x.proto.eq_ignore_ascii_case(proto))
        });
        match before - self.list.len() {
            0 => Err(Error::new(msg::NOT_FOUND, format!("Service {name} {port} {proto} is not in the table."))),
            n => Ok(n),
        }
    }

    /// The panel's order: by name (case ignored), port, protocol.
    pub fn sorted(&self) -> Vec<&ServiceEntry> {
        let mut v: Vec<&ServiceEntry> = self.list.iter().collect();
        v.sort_by_key(|e| (e.name.to_ascii_lowercase(), e.port, e.proto.to_ascii_lowercase()));
        v
    }
}

/// The protocols a new system has.
pub fn shipped_protocols() -> Vec<ProtocolEntry> {
    [("icmp", 1), ("ip", 0), ("tcp", 6), ("udp", 17)]
        .iter()
        .map(|(n, p)| ProtocolEntry {
            name: (*n).to_string(),
            number: *p,
            aliases: vec![n.to_ascii_uppercase()],
            text: String::new(),
        })
        .collect()
}

/// `ADDPCLTBLE`.
pub fn add_protocol(list: &mut Vec<ProtocolEntry>, e: ProtocolEntry) -> Result<()> {
    if e.name.is_empty() || e.name.len() > 32 {
        return Err(bad("PROTOCOL", &e.name));
    }
    if list.iter().any(|x| x.name.eq_ignore_ascii_case(&e.name)) {
        return Err(Error::new(msg::EXISTS, format!("Protocol {} already exists.", e.name)));
    }
    list.push(e);
    Ok(())
}

/// `ADDNETTBLE`.
pub fn add_network(list: &mut Vec<NetworkEntry>, e: NetworkEntry) -> Result<()> {
    if e.name.is_empty() || e.name.len() > 255 {
        return Err(bad("NETWORK", &e.name));
    }
    if list.iter().any(|x| x.name.eq_ignore_ascii_case(&e.name)) {
        return Err(Error::new(msg::EXISTS, format!("Network {} already exists.", e.name)));
    }
    list.push(e);
    Ok(())
}

/// Remove by name from a protocol or network list.
pub fn remove_named<T>(list: &mut Vec<T>, name: &str, of: impl Fn(&T) -> &str, what: &str) -> Result<()> {
    let before = list.len();
    list.retain(|x| !of(x).eq_ignore_ascii_case(name));
    if before == list.len() {
        return Err(Error::new(msg::NOT_FOUND, format!("{what} {name} is not in the table.")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn services() {
        let mut t = ServicesTable::shipped();
        assert_eq!(t.by_name("TELNET", Some("tcp")).unwrap().port, 23);
        assert_eq!(t.by_name("as-signon", None).unwrap().port, 8476);
        assert_eq!(t.by_port(21, Some("tcp")).unwrap().name, "ftp");
        assert_eq!(t.sorted()[0].name, "are-http");
        t.add(ServiceEntry {
            name: "myapp".into(),
            port: 7777,
            proto: "tcp".into(),
            aliases: vec!["mine".into()],
            text: String::new(),
        })
        .unwrap();
        assert_eq!(t.by_name("MINE", Some("TCP")).unwrap().port, 7777);
        assert_eq!(t.remove("myapp", 7777, "TCP").unwrap(), 1);
        assert!(t.remove("myapp", 7777, "tcp").is_err());
        let mut p = shipped_protocols();
        assert!(add_protocol(&mut p, ProtocolEntry { name: "TCP".into(), number: 6, aliases: vec![], text: String::new() }).is_err());
        remove_named(&mut p, "icmp", |e| &e.name, "Protocol").unwrap();
        assert_eq!(p.len(), 3);
    }
}
