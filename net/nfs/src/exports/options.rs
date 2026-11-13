// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The export options of `EXPORTFS -O` and `/etc/exports`
//! (docs/NFS.md section 6.1): parsed into [`ExportOptions`] and printed
//! back in one canonical order.

use std::fmt::Write as _;
use std::net::IpAddr;

use super::error::OptError;
use super::list::{self, Item};
use super::model::{ExportOptions, Flavor, HostSpec, Versions};

/// The longest host name.
pub const HOST_MAX: usize = 255;

/// Parse an export option list. `*DFT` and the empty list are the
/// defaults.
pub fn parse(text: &str) -> Result<ExportOptions, OptError> {
    let mut o = ExportOptions::default();
    if text.trim().eq_ignore_ascii_case("*DFT") {
        return Ok(o);
    }
    for it in list::split(text)? {
        let bad = || OptError::BadValue {
            name: it.name.clone(),
            value: it.value.unwrap_or("").to_string(),
        };
        match it.name.as_str() {
            "ro" => o.ro = flag(&it)?,
            "nosuid" => o.nosuid = flag(&it)?,
            "public" => o.public = flag(&it)?,
            "rw" => {
                o.rw = Some(match it.value {
                    Some(v) => hosts(v).ok_or_else(bad)?,
                    None => Vec::new(),
                });
            }
            "access" => o.access = Some(hosts(need(&it)?).ok_or_else(bad)?),
            // `root` alone makes the export the NFSv4 pseudo root.
            "root" => match it.value {
                None => o.pseudo_root = true,
                Some(v) => o.root_hosts = hosts(v).ok_or_else(bad)?,
            },
            "anon" => {
                let v: i64 = need(&it)?.parse().map_err(|_| bad())?;
                if !(-1..=i64::from(u32::MAX) - 1).contains(&v) {
                    return Err(bad());
                }
                o.anon = Some(v);
            }
            "sec" => {
                let mut out = Vec::new();
                for f in need(&it)?.split(':') {
                    // Only AUTH_SYS: Kerberos and the others are refused
                    // (docs/NFS.md section 10).
                    if !f.trim().eq_ignore_ascii_case("sys") || out.contains(&Flavor::Sys) {
                        return Err(bad());
                    }
                    out.push(Flavor::Sys);
                }
                o.sec = out;
            }
            "vers" => {
                let mut v = Versions {
                    v3: false,
                    v4: false,
                };
                for n in need(&it)?.split(':') {
                    match n.trim() {
                        "3" if !v.v3 => v.v3 = true,
                        "4" if !v.v4 => v.v4 = true,
                        // Version 2 is not served.
                        _ => return Err(bad()),
                    }
                }
                o.vers = Some(v);
            }
            "nfsroot" => o.nfsroot = Some(abs_path(need(&it)?).ok_or_else(bad)?),
            "exname" => o.exname = Some(abs_path(need(&it)?).ok_or_else(bad)?),
            _ => return Err(OptError::Unknown { pos: it.pos }),
        }
    }
    Ok(o)
}

/// An option that takes no value.
fn flag(it: &Item<'_>) -> Result<bool, OptError> {
    match it.value {
        None => Ok(true),
        Some(v) => Err(OptError::BadValue {
            name: it.name.clone(),
            value: v.to_string(),
        }),
    }
}

/// An option's value, which it must have.
fn need<'a>(it: &Item<'a>) -> Result<&'a str, OptError> {
    match it.value {
        Some(v) if !v.is_empty() => Ok(v),
        _ => Err(OptError::BadValue {
            name: it.name.clone(),
            value: String::new(),
        }),
    }
}

fn abs_path(v: &str) -> Option<String> {
    let v = v.trim();
    (v.starts_with('/') && !v.contains('\0')).then(|| v.to_string())
}

/// A host list, `host:host...`; an IPv6 address goes in brackets
/// (`[fd00::1]`, `@[fd00::]/64`).
pub fn hosts(text: &str) -> Option<Vec<HostSpec>> {
    let mut out = Vec::new();
    let mut rest = text.trim();
    while !rest.is_empty() {
        let (item, next) = if rest.starts_with('[') || rest.starts_with("@[") {
            let close = rest.find(']')?;
            let tail = &rest[close + 1..];
            let end = tail.find(':').map_or(rest.len(), |i| close + 1 + i);
            (&rest[..end], rest.get(end + 1..).unwrap_or(""))
        } else {
            match rest.split_once(':') {
                Some((a, b)) => (a, b),
                None => (rest, ""),
            }
        };
        out.push(host(item.trim())?);
        rest = next.trim();
    }
    (!out.is_empty()).then_some(out)
}

/// One host: `@net/len`, `@net/mask`, an address, or a name.
pub fn host(text: &str) -> Option<HostSpec> {
    if let Some(net) = text.strip_prefix('@') {
        let (a, l) = net.split_once('/')?;
        let addr = ip(a)?;
        let max: u8 = if addr.is_ipv4() { 32 } else { 128 };
        let len = match l.parse::<u8>() {
            Ok(n) if n <= max => n,
            Ok(_) => return None,
            Err(_) => match ip(l)? {
                IpAddr::V4(m) if addr.is_ipv4() => mask_len(u32::from(m))?,
                _ => return None,
            },
        };
        return Some(HostSpec::Net {
            addr: super::hosts::network(addr, len),
            len,
        });
    }
    if let Some(a) = ip(text) {
        return Some(HostSpec::Addr(a));
    }
    let ok = !text.is_empty()
        && text.len() <= HOST_MAX
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'));
    ok.then(|| HostSpec::Name(text.to_string()))
}

fn ip(text: &str) -> Option<IpAddr> {
    let t = text.strip_prefix('[').and_then(|t| t.strip_suffix(']'));
    match t {
        Some(v6) => v6.parse::<std::net::Ipv6Addr>().ok().map(IpAddr::V6),
        None => text.parse::<std::net::Ipv4Addr>().ok().map(IpAddr::V4),
    }
}

/// A netmask's prefix length; the bits must be contiguous.
fn mask_len(m: u32) -> Option<u8> {
    let len = m.leading_ones();
    if m.checked_shl(len).unwrap_or(0) != 0 {
        return None;
    }
    u8::try_from(len).ok()
}

/// A host list as the options write it.
#[must_use]
pub fn print_hosts(list: &[HostSpec]) -> String {
    list.iter()
        .map(|h| match h {
            HostSpec::Addr(IpAddr::V6(a)) => format!("[{a}]"),
            HostSpec::Net {
                addr: IpAddr::V6(a),
                len,
            } => format!("@[{a}]/{len}"),
            other => other.to_string(),
        })
        .collect::<Vec<_>>()
        .join(":")
}

/// The options as a list, in one order: `ro`, `rw`, `access`, `root`,
/// `anon`, `nosuid`, `sec`, `vers`, `public`, the pseudo root, `nfsroot`,
/// `exname`. The defaults are left out; all defaults print as nothing.
#[must_use]
pub fn print(o: &ExportOptions) -> String {
    let mut v: Vec<String> = Vec::new();
    if o.ro {
        v.push("ro".into());
    }
    match &o.rw {
        Some(h) if h.is_empty() => v.push("rw".into()),
        Some(h) => v.push(format!("rw={}", print_hosts(h))),
        None => {}
    }
    if let Some(h) = &o.access {
        v.push(format!("access={}", print_hosts(h)));
    }
    if !o.root_hosts.is_empty() {
        v.push(format!("root={}", print_hosts(&o.root_hosts)));
    }
    if let Some(a) = o.anon {
        v.push(format!("anon={a}"));
    }
    if o.nosuid {
        v.push("nosuid".into());
    }
    if !o.sec.is_empty() {
        v.push(format!(
            "sec={}",
            o.sec.iter().map(|_| "sys").collect::<Vec<_>>().join(":")
        ));
    }
    if let Some(vs) = o.vers {
        let mut s = String::from("vers=");
        let mut parts = Vec::new();
        if vs.v3 {
            parts.push("3");
        }
        if vs.v4 {
            parts.push("4");
        }
        let _ = write!(s, "{}", parts.join(":"));
        v.push(s);
    }
    if o.public {
        v.push("public".into());
    }
    if o.pseudo_root {
        v.push("root".into());
    }
    if let Some(p) = &o.nfsroot {
        v.push(format!("nfsroot={p}"));
    }
    if let Some(p) = &o.exname {
        v.push(format!("exname={p}"));
    }
    v.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn every_option() {
        let o = parse("ro, rw=alpha:10.1.2.3, access=@10.0.0.0/8:grp, root=alpha, anon=-1, nosuid, sec=sys, vers=3:4, public, nfsroot=/home, exname=/data").unwrap();
        assert!(o.ro && o.nosuid && o.public);
        assert_eq!(
            o.rw,
            Some(vec![
                HostSpec::Name("alpha".into()),
                HostSpec::Addr(IpAddr::V4(Ipv4Addr::new(10, 1, 2, 3)))
            ])
        );
        assert_eq!(
            o.access.as_ref().unwrap()[0],
            HostSpec::Net {
                addr: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 0)),
                len: 8
            }
        );
        assert_eq!(o.anon, Some(-1));
        assert_eq!(o.sec, vec![Flavor::Sys]);
        assert_eq!(o.nfsroot.as_deref(), Some("/home"));
        // Printed and read back, the same.
        assert_eq!(parse(&print(&o)).unwrap(), o);
    }

    #[test]
    fn defaults_and_flags() {
        assert_eq!(parse("*DFT").unwrap(), ExportOptions::default());
        assert_eq!(print(&ExportOptions::default()), "");
        let o = parse("root,rw").unwrap();
        assert!(o.pseudo_root);
        assert_eq!(o.rw, Some(vec![]));
        assert_eq!(print(&o), "rw,root");
    }

    #[test]
    fn netmask_and_ipv6() {
        let o = parse("access=@192.168.1.7/255.255.255.0:[fd00::1]:@[fd00::]/64").unwrap();
        let a = o.access.unwrap();
        assert_eq!(
            a[0],
            HostSpec::Net {
                addr: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 0)),
                len: 24
            }
        );
        assert!(matches!(a[1], HostSpec::Addr(IpAddr::V6(_))));
        assert!(matches!(a[2], HostSpec::Net { len: 64, .. }));
        assert_eq!(print_hosts(&a), "@192.168.1.0/24:[fd00::1]:@[fd00::]/64");
    }

    #[test]
    fn refusals() {
        let bad = |s: &str| parse(s).unwrap_err().msgid();
        assert_eq!(bad("ro,bogus"), "CPFA1B0");
        assert_eq!(parse("ro,bogus"), Err(OptError::Unknown { pos: 4 }));
        assert_eq!(bad("ro,RO"), "CPFA1B1");
        assert_eq!(bad("vers=2"), "CPFA1BB");
        assert_eq!(bad("vers=3:3"), "CPFA1BB");
        assert_eq!(bad("sec=krb5"), "CPFA1BB");
        assert_eq!(bad("anon=x"), "CPFA1BB");
        assert_eq!(bad("anon=-2"), "CPFA1BB");
        assert_eq!(bad("access="), "CPFA1BB");
        assert_eq!(bad("access=a b"), "CPFA1BB");
        assert_eq!(bad("ro=1"), "CPFA1BB");
        assert_eq!(bad("nfsroot=rel"), "CPFA1BB");
        assert_eq!(bad("access=@10.0.0.0/33"), "CPFA1BB");
        assert_eq!(bad("access=@10.0.0.0/255.0.255.0"), "CPFA1BB");
        assert_eq!(
            parse("vers=2"),
            Err(OptError::BadValue {
                name: "vers".into(),
                value: "2".into()
            })
        );
    }
}
