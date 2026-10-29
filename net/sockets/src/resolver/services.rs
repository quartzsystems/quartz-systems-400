// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The services table behind `getservbyname` and `getservbyport`. The
//! system keeps it as an object (`WRKSRVTBLE`); [`ServicesTable`] is
//! how the resolver reads it and [`DefaultServices`] is the shipped
//! content.

/// A service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service {
    /// Name, as the table spells it.
    pub name: String,
    /// Aliases.
    pub aliases: Vec<String>,
    /// Port.
    pub port: u16,
    /// `tcp` or `udp`.
    pub protocol: String,
}

/// The services table.
pub trait ServicesTable {
    /// A service by name (any case), for one protocol or any.
    fn by_name(&self, name: &str, protocol: Option<&str>) -> Option<Service>;
    /// A service by port, for one protocol or any.
    fn by_port(&self, port: u16, protocol: Option<&str>) -> Option<Service>;
}

/// `(name, port, protocols, aliases)`.
const SHIPPED: &[(&str, u16, &[&str], &[&str])] = &[
    ("echo", 7, &["tcp", "udp"], &[]),
    ("discard", 9, &["tcp", "udp"], &["sink", "null"]),
    ("daytime", 13, &["tcp", "udp"], &[]),
    ("ftp-data", 20, &["tcp"], &[]),
    ("ftp", 21, &["tcp"], &[]),
    ("ssh", 22, &["tcp"], &[]),
    ("telnet", 23, &["tcp"], &[]),
    ("smtp", 25, &["tcp"], &["mail"]),
    ("time", 37, &["tcp", "udp"], &["timserver"]),
    ("domain", 53, &["tcp", "udp"], &["nameserver"]),
    ("bootps", 67, &["udp"], &[]),
    ("bootpc", 68, &["udp"], &[]),
    ("tftp", 69, &["udp"], &[]),
    ("http", 80, &["tcp"], &["www", "www-http"]),
    ("pop3", 110, &["tcp"], &["pop-3"]),
    ("sunrpc", 111, &["tcp", "udp"], &["portmapper"]),
    ("ntp", 123, &["udp"], &[]),
    ("imap", 143, &["tcp"], &["imap2"]),
    ("snmp", 161, &["udp"], &[]),
    ("snmp-trap", 162, &["udp"], &["snmptrap"]),
    ("ldap", 389, &["tcp", "udp"], &[]),
    ("https", 443, &["tcp"], &[]),
    ("ddm", 446, &["tcp"], &["drda"]),
    ("ddm-ssl", 448, &["tcp"], &[]),
    ("as-svrmap", 449, &["tcp", "udp"], &[]),
    ("lpd", 515, &["tcp"], &["printer"]),
    ("ldaps", 636, &["tcp"], &[]),
    ("ftps", 990, &["tcp"], &[]),
    ("telnets", 992, &["tcp"], &["telnet-ssl"]),
    ("imaps", 993, &["tcp"], &[]),
    ("pop3s", 995, &["tcp"], &[]),
    ("as-central", 8470, &["tcp"], &[]),
    ("as-database", 8471, &["tcp"], &[]),
    ("as-dtaq", 8472, &["tcp"], &[]),
    ("as-file", 8473, &["tcp"], &[]),
    ("as-netprt", 8474, &["tcp"], &[]),
    ("as-rmtcmd", 8475, &["tcp"], &[]),
    ("as-signon", 8476, &["tcp"], &[]),
    ("as-central-s", 9470, &["tcp"], &[]),
    ("as-database-s", 9471, &["tcp"], &[]),
    ("as-dtaq-s", 9472, &["tcp"], &[]),
    ("as-file-s", 9473, &["tcp"], &[]),
    ("as-netprt-s", 9474, &["tcp"], &[]),
    ("as-rmtcmd-s", 9475, &["tcp"], &[]),
    ("as-signon-s", 9476, &["tcp"], &[]),
];

/// The shipped services.
#[derive(Debug, Clone, Default)]
pub struct DefaultServices;

fn entries() -> impl Iterator<Item = Service> {
    SHIPPED.iter().flat_map(|(name, port, protos, aliases)| {
        protos.iter().map(move |p| Service {
            name: (*name).to_string(),
            aliases: aliases.iter().map(|a| (*a).to_string()).collect(),
            port: *port,
            protocol: (*p).to_string(),
        })
    })
}

fn proto_ok(s: &Service, protocol: Option<&str>) -> bool {
    protocol.is_none_or(|p| p.is_empty() || s.protocol.eq_ignore_ascii_case(p))
}

impl ServicesTable for DefaultServices {
    fn by_name(&self, name: &str, protocol: Option<&str>) -> Option<Service> {
        entries().find(|s| {
            proto_ok(s, protocol)
                && (s.name.eq_ignore_ascii_case(name)
                    || s.aliases.iter().any(|a| a.eq_ignore_ascii_case(name)))
        })
    }

    fn by_port(&self, port: u16, protocol: Option<&str>) -> Option<Service> {
        entries().find(|s| s.port == port && proto_ok(s, protocol))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_lookups() {
        let t = DefaultServices;
        let s = t.by_name("WWW", Some("tcp")).unwrap();
        assert_eq!((s.name.as_str(), s.port), ("http", 80));
        assert!(t.by_name("http", Some("udp")).is_none());
        assert_eq!(t.by_name("domain", None).unwrap().protocol, "tcp");
        assert_eq!(t.by_port(53, Some("UDP")).unwrap().protocol, "udp");
        assert_eq!(t.by_port(8476, None).unwrap().name, "as-signon");
        assert!(t.by_port(1, None).is_none());
    }
}
