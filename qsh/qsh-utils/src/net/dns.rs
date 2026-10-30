// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What `nslookup`, `host` and `dig` share: asking the resolver,
//! reverse names, and the record types' display forms.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use qsh::sys::services::rtype;
use qsh::sys::{DnsAnswer, DnsQuery, DnsRecord, SysError, UtilCtx};

/// Ask the system's resolver.
pub fn ask(
    cx: &UtilCtx<'_>,
    name: &str,
    ty: u16,
    server: Option<&str>,
    recurse: bool,
) -> Result<DnsAnswer, SysError> {
    cx.sys.resolver()?.query(&DnsQuery {
        name: name.to_string(),
        rtype: ty,
        server: server.map(ToString::to_string),
        recurse,
    })
}

/// The address in a string, if it is one.
pub fn ip(s: &str) -> Option<IpAddr> {
    s.parse().ok()
}

/// The `in-addr.arpa` or `ip6.arpa` name of an address.
pub fn reverse(addr: IpAddr) -> String {
    match addr {
        IpAddr::V4(a) => v4_reverse(a),
        IpAddr::V6(a) => v6_reverse(a),
    }
}

fn v4_reverse(a: Ipv4Addr) -> String {
    let o = a.octets();
    format!("{}.{}.{}.{}.in-addr.arpa", o[3], o[2], o[1], o[0])
}

fn v6_reverse(a: Ipv6Addr) -> String {
    let mut nibbles = Vec::with_capacity(32);
    for b in a.octets().iter().rev() {
        nibbles.push(format!("{:x}", b & 0xf));
        nibbles.push(format!("{:x}", b >> 4));
    }
    format!("{}.ip6.arpa", nibbles.join("."))
}

/// A name with its final dot.
pub fn absolute(name: &str) -> String {
    if name.ends_with('.') {
        name.to_string()
    } else {
        format!("{name}.")
    }
}

/// A domain name in data without its final dot.
pub fn bare(name: &str) -> &str {
    name.strip_suffix('.').unwrap_or(name)
}

/// The type of a `-type=`/`-t` value, in any case.
pub fn parse_type(s: &str) -> Option<u16> {
    rtype::parse(s)
}

/// The SOA data's seven fields.
pub fn soa_fields(data: &str) -> Option<[&str; 7]> {
    let f: Vec<&str> = data.split_whitespace().collect();
    if f.len() != 7 {
        return None;
    }
    Some([f[0], f[1], f[2], f[3], f[4], f[5], f[6]])
}

/// A record in master-file form (`dig`): name, TTL, class, type, data.
pub fn master_line(r: &DnsRecord) -> String {
    format!(
        "{}\t{}\tIN\t{}\t{}",
        absolute(&r.name),
        r.ttl,
        rtype::name(r.rtype),
        r.data
    )
}

/// Records of one type.
pub fn of_type(rs: &[DnsRecord], ty: u16) -> impl Iterator<Item = &DnsRecord> {
    rs.iter().filter(move |r| r.rtype == ty)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverse_names() {
        assert_eq!(
            reverse(ip("93.184.216.34").unwrap()),
            "34.216.184.93.in-addr.arpa"
        );
        assert!(reverse(ip("2001:db8::1").unwrap()).starts_with("1.0.0.0.0.0.0.0."));
        assert!(reverse(ip("2001:db8::1").unwrap()).ends_with("8.b.d.0.1.0.0.2.ip6.arpa"));
        assert_eq!(absolute("a.b"), "a.b.");
        assert_eq!(bare("a.b."), "a.b");
    }
}
