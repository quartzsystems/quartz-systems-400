// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/etc/netgroup`: `name member member ...`, a member being a triple
//! `(host,user,domain)` or another group's name. Only the host field is
//! used: empty means any host, `-` none. `#` starts a comment and `\`
//! at the end of a line continues it. A group that contains itself,
//! directly or through others, is refused.

use std::collections::{BTreeMap, BTreeSet};

use super::error::NetgroupError;
use super::hosts::Client;
use super::lines;

/// How deep groups may nest.
pub const MAX_DEPTH: usize = 16;

/// One member of a group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Member {
    /// A triple's host field: `None` any host, `Some("-")` none.
    Host(Option<String>),
    /// Another group.
    Group(String),
}

/// The groups of `/etc/netgroup`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Netgroups {
    groups: BTreeMap<String, Vec<Member>>,
}

impl Netgroups {
    /// Parse the file's text.
    pub fn parse(text: &str) -> Result<Self, NetgroupError> {
        let mut groups: BTreeMap<String, Vec<Member>> = BTreeMap::new();
        for (line, logical) in lines::logical(text) {
            let mut rest = logical.trim();
            if rest.is_empty() {
                continue;
            }
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let name = rest[..end].to_string();
            if name.contains(['(', ')', ',']) {
                return Err(NetgroupError::Format { line });
            }
            rest = rest[end..].trim_start();
            let mut members = Vec::new();
            while !rest.is_empty() {
                if let Some(t) = rest.strip_prefix('(') {
                    let close = t.find(')').ok_or(NetgroupError::Format { line })?;
                    let fields: Vec<&str> = t[..close].split(',').map(str::trim).collect();
                    if fields.len() != 3 {
                        return Err(NetgroupError::Format { line });
                    }
                    let host = (!fields[0].is_empty()).then(|| fields[0].to_string());
                    members.push(Member::Host(host));
                    rest = t[close + 1..].trim_start();
                } else {
                    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                    let g = &rest[..end];
                    if g.contains(['(', ')', ',']) {
                        return Err(NetgroupError::Format { line });
                    }
                    members.push(Member::Group(g.to_string()));
                    rest = rest[end..].trim_start();
                }
            }
            groups.entry(name).or_default().extend(members);
        }
        let ng = Self { groups };
        for name in ng.groups.keys() {
            ng.hosts_of(name)?;
        }
        Ok(ng)
    }

    /// Whether a group of that name exists.
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.groups.contains_key(name)
    }

    /// The host fields of a group, its nested groups' included.
    pub fn hosts_of(&self, name: &str) -> Result<Vec<Option<String>>, NetgroupError> {
        let mut out = Vec::new();
        let mut path = BTreeSet::new();
        self.walk(name, &mut path, &mut out)?;
        Ok(out)
    }

    fn walk(
        &self,
        name: &str,
        path: &mut BTreeSet<String>,
        out: &mut Vec<Option<String>>,
    ) -> Result<(), NetgroupError> {
        if path.len() >= MAX_DEPTH || !path.insert(name.to_string()) {
            return Err(NetgroupError::Cycle(name.to_string()));
        }
        for m in self.groups.get(name).map_or(&[][..], Vec::as_slice) {
            match m {
                Member::Host(h) => out.push(h.clone()),
                // A group no line defines has no hosts.
                Member::Group(g) => self.walk(g, path, out)?,
            }
        }
        path.remove(name);
        Ok(())
    }

    /// Whether the client is a host of the group.
    #[must_use]
    pub fn contains(&self, name: &str, client: &Client<'_>) -> bool {
        self.hosts_of(name).is_ok_and(|hosts| {
            hosts.iter().any(|h| match h.as_deref() {
                None => true,
                Some("-") => false,
                Some(n) => client.is_named(n),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exports::hosts::{NoNames, Resolver};
    use std::net::{IpAddr, Ipv4Addr};

    struct One;
    impl Resolver for One {
        fn addresses(&self, name: &str) -> Vec<IpAddr> {
            if name == "gamma" {
                vec![IpAddr::V4(Ipv4Addr::new(10, 0, 0, 3))]
            } else {
                Vec::new()
            }
        }
        fn names(&self, _: IpAddr) -> Vec<String> {
            Vec::new()
        }
    }

    #[test]
    fn nested_groups() {
        let g = Netgroups::parse(
            "# lab machines\nlab (gamma,,) (delta, joe, dom) \\\n  servers\nservers (-,root,)\nall lab (,,)\n",
        )
        .unwrap();
        assert_eq!(
            g.hosts_of("lab").unwrap(),
            vec![Some("gamma".into()), Some("delta".into()), Some("-".into())]
        );
        let c = Client::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 3)), &One);
        assert!(g.contains("lab", &c));
        assert!(!g.contains("servers", &c));
        // An empty host field is any host.
        let other = Client::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), &NoNames);
        assert!(g.contains("all", &other));
        assert!(!g.contains("lab", &other));
    }

    #[test]
    fn cycles_and_format() {
        assert_eq!(
            Netgroups::parse("a b\nb a\n"),
            Err(NetgroupError::Cycle("a".into()))
        );
        assert!(Netgroups::parse("a (h,u)\n").is_err());
        assert!(Netgroups::parse("a (h,u,d\n").is_err());
        assert_eq!(
            Netgroups::parse("a (h,,)\nb (x,,y\n").unwrap_err().reason(),
            17
        );
        assert_eq!(Netgroups::parse("a a\n").unwrap_err().reason(), 8);
    }
}
