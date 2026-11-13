// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Whether a client is one of a host list, and what an export lets it
//! do. Names are resolved through a [`Resolver`] the caller supplies
//! (the host table and the name servers); nothing here does I/O.

use std::cell::OnceCell;
use std::net::IpAddr;

use super::model::{Export, HostOpt, HostSpec};
use super::netgroup::Netgroups;

/// Names to addresses and back, as the caller resolves them.
pub trait Resolver {
    /// The addresses of a host name; empty when it has none.
    fn addresses(&self, name: &str) -> Vec<IpAddr>;
    /// The names of an address; empty when it has none.
    fn names(&self, addr: IpAddr) -> Vec<String>;
}

/// A resolver that knows nothing: only addresses and networks match.
pub struct NoNames;

impl Resolver for NoNames {
    fn addresses(&self, _: &str) -> Vec<IpAddr> {
        Vec::new()
    }
    fn names(&self, _: IpAddr) -> Vec<String> {
        Vec::new()
    }
}

/// A client being matched: its address, and its names once asked for.
pub struct Client<'a> {
    addr: IpAddr,
    resolver: &'a dyn Resolver,
    names: OnceCell<Vec<String>>,
}

impl<'a> Client<'a> {
    /// A client at `addr`; an IPv4 address mapped into IPv6 is taken as
    /// IPv4.
    #[must_use]
    pub fn new(addr: IpAddr, resolver: &'a dyn Resolver) -> Self {
        let addr = match addr {
            IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(addr, IpAddr::V4),
            v4 @ IpAddr::V4(_) => v4,
        };
        Self {
            addr,
            resolver,
            names: OnceCell::new(),
        }
    }

    /// The client's address.
    #[must_use]
    pub fn addr(&self) -> IpAddr {
        self.addr
    }

    fn names(&self) -> &[String] {
        self.names.get_or_init(|| self.resolver.names(self.addr))
    }

    /// Whether `name` is this client: one of its names (a short name
    /// matches the full name it starts), or a name whose addresses
    /// include the client's.
    #[must_use]
    pub fn is_named(&self, name: &str) -> bool {
        let same = |a: &str, b: &str| a.eq_ignore_ascii_case(b) || short_of(a, b) || short_of(b, a);
        self.names().iter().any(|n| same(n, name))
            || self.resolver.addresses(name).contains(&self.addr)
    }
}

/// `short` is the first label of `full`.
fn short_of(short: &str, full: &str) -> bool {
    !short.contains('.')
        && full.len() > short.len()
        && full.as_bytes()[short.len()] == b'.'
        && full[..short.len()].eq_ignore_ascii_case(short)
}

/// The network of `addr` with a prefix of `len` bits (the host bits
/// cleared).
#[must_use]
pub fn network(addr: IpAddr, len: u8) -> IpAddr {
    match addr {
        IpAddr::V4(a) => {
            let m = u32::MAX
                .checked_shl(32 - u32::from(len.min(32)))
                .unwrap_or(0);
            IpAddr::V4((u32::from(a) & m).into())
        }
        IpAddr::V6(a) => {
            let m = u128::MAX
                .checked_shl(128 - u32::from(len.min(128)))
                .unwrap_or(0);
            IpAddr::V6((u128::from(a) & m).into())
        }
    }
}

/// Whether the client is `spec`: a netgroup is looked up first, then
/// the name as a host.
#[must_use]
pub fn matches(spec: &HostSpec, client: &Client<'_>, groups: &Netgroups) -> bool {
    match spec {
        HostSpec::Addr(a) => Client::new(*a, &NoNames).addr == client.addr,
        HostSpec::Net { addr, len } => match (addr, client.addr) {
            (IpAddr::V4(_), IpAddr::V4(_)) | (IpAddr::V6(_), IpAddr::V6(_)) => {
                network(client.addr, *len) == network(*addr, *len)
            }
            _ => false,
        },
        HostSpec::Name(n) => {
            if groups.has(n) {
                groups.contains(n, client)
            } else {
                client.is_named(n)
            }
        }
    }
}

/// Whether the client is any of `list`.
#[must_use]
pub fn any(list: &[HostSpec], client: &Client<'_>, groups: &Netgroups) -> bool {
    list.iter().any(|h| matches(h, client, groups))
}

/// What an export lets a client do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Access {
    /// The client may mount and use the export.
    pub allowed: bool,
    /// It may write.
    pub write: bool,
    /// Its UID 0 is `QSECOFR`.
    pub root: bool,
}

impl Export {
    /// What this export lets `client` do: `access=` lists the hosts that
    /// may use it; `ro` makes it read only but for the hosts of `rw=`;
    /// without `ro`, `rw=hosts` lets those write and the rest read.
    #[must_use]
    pub fn access_for(&self, client: &Client<'_>, groups: &Netgroups) -> Access {
        let o = &self.options;
        let listed_rw =
            o.rw.as_ref()
                .is_some_and(|l| !l.is_empty() && any(l, client, groups));
        let allowed = o.access.as_ref().is_none_or(|l| any(l, client, groups))
            // A host of rw= or root= may use the export too.
            || listed_rw
            || any(&o.root_hosts, client, groups);
        let write = match &o.rw {
            _ if o.ro => listed_rw,
            Some(l) if !l.is_empty() => listed_rw,
            _ => true,
        };
        Access {
            allowed,
            write: allowed && write,
            root: allowed && any(&o.root_hosts, client, groups),
        }
    }

    /// The host options that apply to `client`: the first entry that
    /// names it.
    #[must_use]
    pub fn host_opt(&self, client: &Client<'_>, groups: &Netgroups) -> Option<&HostOpt> {
        self.hosts.iter().find(|h| matches(&h.host, client, groups))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exports::options;
    use std::net::Ipv4Addr;

    struct Table;

    impl Resolver for Table {
        fn addresses(&self, name: &str) -> Vec<IpAddr> {
            match name.to_ascii_lowercase().as_str() {
                "alpha" | "alpha.lab.example" => vec![v4(10, 0, 0, 1)],
                "beta.lab.example" => vec![v4(10, 0, 0, 2)],
                _ => Vec::new(),
            }
        }
        fn names(&self, addr: IpAddr) -> Vec<String> {
            if addr == v4(10, 0, 0, 2) {
                vec!["beta.lab.example".into()]
            } else {
                Vec::new()
            }
        }
    }

    fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    fn export(opts: &str) -> Export {
        let mut e = Export::new("/home");
        e.options = options::parse(opts).unwrap();
        e
    }

    #[test]
    fn names_addresses_networks() {
        let g = Netgroups::default();
        let alpha = Client::new(v4(10, 0, 0, 1), &Table);
        let beta = Client::new(v4(10, 0, 0, 2), &Table);
        let far = Client::new(v4(192, 168, 9, 9), &Table);
        let name = |n: &str| HostSpec::Name(n.into());
        // By forward lookup, and by reverse lookup with a short name.
        assert!(matches(&name("ALPHA"), &alpha, &g));
        assert!(matches(&name("beta"), &beta, &g));
        assert!(!matches(&name("beta"), &alpha, &g));
        let net = options::host("@10.0.0.0/24").unwrap();
        assert!(matches(&net, &alpha, &g) && !matches(&net, &far, &g));
        // An IPv4 client over IPv6.
        let mapped = Client::new("::ffff:10.0.0.1".parse().unwrap(), &Table);
        assert!(matches(&HostSpec::Addr(v4(10, 0, 0, 1)), &mapped, &g));
    }

    #[test]
    fn access_rules() {
        let g = Netgroups::default();
        let alpha = Client::new(v4(10, 0, 0, 1), &Table);
        let beta = Client::new(v4(10, 0, 0, 2), &Table);
        let all = export("");
        assert_eq!(
            all.access_for(&beta, &g),
            Access {
                allowed: true,
                write: true,
                root: false
            }
        );
        let ro = export("ro,rw=alpha,root=alpha");
        assert!(ro.access_for(&alpha, &g).write);
        assert!(ro.access_for(&alpha, &g).root);
        assert!(!ro.access_for(&beta, &g).write);
        let only = export("access=alpha");
        assert!(only.access_for(&alpha, &g).allowed);
        assert!(!only.access_for(&beta, &g).allowed);
        assert!(!only.access_for(&beta, &g).write);
        let some_rw = export("rw=beta");
        assert!(some_rw.access_for(&beta, &g).write);
        assert!(!some_rw.access_for(&alpha, &g).write);
        assert!(some_rw.access_for(&alpha, &g).allowed);
    }
}
