// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The TCP/IP attributes: `CHGTCPA`. Every parameter of the command is
//! kept as its text (`120`, `*BSD`, `*YES 10` for a group), checked
//! against [`SPEC`]; [`Attributes::sysctls`] turns the ones with a kernel
//! equivalent into `/proc/sys` writes (`docs/TCPIP.md` has the table).

use std::collections::BTreeMap;

use crate::addr::bad;
use crate::error::Result;

/// What one parameter (or one element of a group) takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A whole number in a range.
    Int(i64, i64),
    /// A number with one decimal position, in tenths.
    Tenths(i64, i64),
    /// One of a set of special values.
    Choice(&'static [&'static str]),
    /// A number in a range, or one of the special values.
    IntOr(i64, i64, &'static [&'static str]),
    /// Any text.
    Text,
}

/// One `CHGTCPA` parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParamSpec {
    /// The keyword.
    pub kwd: &'static str,
    /// The value a new system has.
    pub default: &'static str,
    /// The elements (one for a simple parameter).
    pub elems: &'static [Kind],
    /// Single values that stand for the whole group.
    pub single: &'static [&'static str],
}

const YN: &[&str] = &["*YES", "*NO"];

/// Every `CHGTCPA` parameter, in the definition's order.
pub const SPEC: &[ParamSpec] = &[
    p("TCPKEEPALV", "120", &[Kind::Int(1, 40320)]),
    p("TCPURGPTR", "*BSD", &[Kind::Choice(&["*BSD", "*RFC"])]),
    p("TCPRCVBUF", "65535", &[Kind::Int(512, 8_388_608)]),
    p("TCPSNDBUF", "65535", &[Kind::Int(512, 8_388_608)]),
    p("TCPR1CNT", "3", &[Kind::Int(1, 15)]),
    p("TCPR2CNT", "16", &[Kind::Int(2, 16)]),
    p("TCPMINRTM", "250", &[Kind::Int(100, 1000)]),
    p("TCPCLOTIMO", "120", &[Kind::Int(0, 14400)]),
    p("TCPCNNMSG", "*THRESHOLD", &[Kind::Choice(&["*THRESHOLD", "*ALL", "*NONE"])]),
    p("UDPCKS", "*YES", &[Kind::Choice(YN)]),
    g(
        "IPPATHMTU",
        "*YES 10",
        &[Kind::Choice(&["*YES"]), Kind::IntOr(5, 40320, &["*ONCE"])],
        &["*NO"],
    ),
    p("IPDTGFWD", "*NO", &[Kind::Choice(YN)]),
    p("IPSRCRTG", "*YES", &[Kind::Choice(YN)]),
    p("IPRSBTIMO", "10", &[Kind::Int(5, 120)]),
    p("IPTTL", "64", &[Kind::Int(1, 255)]),
    p("IPQOSENB", "*NO", &[Kind::Choice(&["*TOS", "*YES", "*NO"])]),
    p("IPQOSBCH", "*NORMAL", &[Kind::Choice(&["*NORMAL", "*MINDELAY"])]),
    p("IPQOSTMR", "100", &[Kind::Int(5, 5000)]),
    g(
        "IPDEADGATE",
        "*YES 2",
        &[Kind::Choice(&["*YES", "*LINE"]), Kind::Int(1, 60)],
        &["*NO"],
    ),
    p("ARPTIMO", "15", &[Kind::Int(1, 1440)]),
    p("ECN", "*NO", &[Kind::Choice(YN)]),
    g(
        "NFC",
        "*YES 300 10",
        &[
            Kind::Choice(YN),
            Kind::IntOr(30, 604_800, &["*NOMAX"]),
            Kind::Int(10, 100_000),
        ],
        &["*CLEAR"],
    ),
    p("LOGPCLERR", "*NO", &[Kind::Choice(YN)]),
    g(
        "DHCPUNQID",
        "*GEN *PENDING",
        &[Kind::Choice(&["*DUID", "*GEN"]), Kind::Text],
        &[],
    ),
    p("IP6ERRLMT", "10", &[Kind::Int(1, 255)]),
    p("IP6ERRRATE", "10", &[Kind::Int(1, 255)]),
    p("IP6HOPLMT", "64", &[Kind::Int(1, 255)]),
    p("IP6DUPRTLS", "*DFT", &[Kind::Choice(&["*DFT", "*NONE", "*RTR"])]),
    p("IP6TMPAVL", "7.0", &[Kind::Tenths(1, 300)]),
    p("IP6TMPAPL", "1.0", &[Kind::Tenths(1, 300)]),
    p("IP6TMPAXP", "", &[Kind::Text]),
];

const fn p(kwd: &'static str, default: &'static str, elems: &'static [Kind]) -> ParamSpec {
    ParamSpec {
        kwd,
        default,
        elems,
        single: &[],
    }
}

const fn g(
    kwd: &'static str,
    default: &'static str,
    elems: &'static [Kind],
    single: &'static [&'static str],
) -> ParamSpec {
    ParamSpec {
        kwd,
        default,
        elems,
        single,
    }
}

/// The spec of a keyword.
pub fn spec(kwd: &str) -> Option<&'static ParamSpec> {
    SPEC.iter().find(|s| s.kwd.eq_ignore_ascii_case(kwd))
}

/// The attributes, as set; a keyword not set has its default.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Attributes {
    values: BTreeMap<String, String>,
}

impl Attributes {
    /// The value of a keyword (its default when unset).
    pub fn get(&self, kwd: &str) -> String {
        let k = kwd.to_ascii_uppercase();
        if let Some(v) = self.values.get(&k) {
            return v.clone();
        }
        spec(&k).map(|s| s.default.to_string()).unwrap_or_default()
    }

    /// The numeric value of a simple parameter, or of a group's element.
    pub fn int(&self, kwd: &str, elem: usize) -> Option<i64> {
        self.get(kwd).split_whitespace().nth(elem)?.parse().ok()
    }

    /// Whether a parameter (or a group's first element) is `*YES`.
    pub fn yes(&self, kwd: &str) -> bool {
        self.get(kwd).split_whitespace().next() == Some("*YES")
    }

    /// Set a keyword: `*DFT` restores the default; groups take their
    /// elements separated by blanks. The value is checked.
    pub fn set(&mut self, kwd: &str, value: &str) -> Result<()> {
        let s = spec(kwd).ok_or_else(|| bad("CHGTCPA", kwd))?;
        let v = normalize(value);
        if v.eq_ignore_ascii_case("*DFT") {
            self.values.remove(s.kwd);
            return Ok(());
        }
        check(s, &v)?;
        self.values.insert(s.kwd.to_string(), v);
        Ok(())
    }

    /// Keyword and value of every parameter set away from its default.
    pub fn changed(&self) -> impl Iterator<Item = (&str, &str)> {
        self.values.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// The `/proc/sys/net` writes the attributes make: `(key, value)`.
    /// `read` gives a key's current value, for the `min default max`
    /// triples of the buffer sizes.
    pub fn sysctls(&self, read: &mut dyn FnMut(&str) -> Option<String>) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for m in netlink::sysctl::CHGTCPA {
            let v = self.get(m.param);
            let Some(n) = ibm_number(m.param, &v) else {
                continue;
            };
            for key in m.paths {
                let text = match m.unit {
                    netlink::sysctl::Unit::Same => n.to_string(),
                    netlink::sysctl::Unit::MinutesToSeconds => (n * 60).to_string(),
                    netlink::sysctl::Unit::SecondsToMillis => (n * 1000).to_string(),
                    netlink::sysctl::Unit::YesNo => u64::from(n != 0).to_string(),
                    netlink::sysctl::Unit::TripleDefault => {
                        let Some(cur) = read(key) else { continue };
                        let f: Vec<&str> = cur.split_whitespace().collect();
                        if f.len() != 3 {
                            continue;
                        }
                        format!("{}\t{n}\t{}", f[0], f[2])
                    }
                };
                out.push(((*key).to_string(), text));
            }
        }
        // The parameters netlink's table leaves out.
        let pmtu = u64::from(!self.yes("IPPATHMTU"));
        out.push(("ipv4/ip_no_pmtu_disc".into(), pmtu.to_string()));
        if let Some(h) = self.int("IP6HOPLMT", 0) {
            out.push(("ipv6/conf/default/hop_limit".into(), h.to_string()));
            out.push(("ipv6/conf/all/hop_limit".into(), h.to_string()));
        }
        for (kwd, key) in [
            ("IP6TMPAVL", "ipv6/conf/default/temp_valid_lft"),
            ("IP6TMPAPL", "ipv6/conf/default/temp_prefered_lft"),
        ] {
            if let Some(t) = tenths(&self.get(kwd)) {
                out.push((key.into(), (t * 86_400 / 10).to_string()));
            }
        }
        out
    }
}

/// The parameters stored only: no kernel equivalent.
pub fn stored_only() -> Vec<&'static str> {
    let mapped: Vec<&str> = netlink::sysctl::CHGTCPA
        .iter()
        .map(|m| m.param)
        .chain(["IPPATHMTU", "IP6HOPLMT", "IP6TMPAVL", "IP6TMPAPL"])
        .collect();
    SPEC.iter().map(|s| s.kwd).filter(|k| !mapped.contains(k)).collect()
}

/// The number netlink's mapping converts for a parameter's value.
fn ibm_number(kwd: &str, v: &str) -> Option<u64> {
    let first = v.split_whitespace().next()?;
    match first {
        "*YES" | "*RFC" => Some(1),
        "*NO" | "*BSD" => Some(0),
        _ => {
            let _ = kwd;
            first.parse().ok()
        }
    }
}

/// `7.0` or `7,0` in tenths.
fn tenths(v: &str) -> Option<i64> {
    let t = v.trim().replace(',', ".");
    let (whole, frac) = t.split_once('.').unwrap_or((&t, "0"));
    let w: i64 = if whole.is_empty() { 0 } else { whole.parse().ok()? };
    let f: i64 = frac.chars().next().map_or(Some(0), |c| c.to_digit(10).map(i64::from))?;
    Some(w * 10 + f)
}

/// Upper-case special values, single blanks, no quotes.
fn normalize(v: &str) -> String {
    v.split_whitespace()
        .map(|w| {
            let w = w.trim_matches('\'');
            if w.starts_with('*') { w.to_ascii_uppercase() } else { w.to_string() }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn check(s: &ParamSpec, v: &str) -> Result<()> {
    if s.single.iter().any(|x| x.eq_ignore_ascii_case(v)) {
        return Ok(());
    }
    let parts: Vec<&str> = v.split_whitespace().collect();
    if s.elems == [Kind::Text] {
        return Ok(());
    }
    if parts.len() != s.elems.len() {
        return Err(bad(s.kwd, v));
    }
    for (k, part) in s.elems.iter().zip(&parts) {
        let ok = match *k {
            Kind::Int(lo, hi) => part.parse::<i64>().is_ok_and(|n| (lo..=hi).contains(&n)),
            Kind::Tenths(lo, hi) => tenths(part).is_some_and(|n| (lo..=hi).contains(&n)),
            Kind::Choice(set) => set.contains(part),
            Kind::IntOr(lo, hi, set) => {
                set.contains(part) || part.parse::<i64>().is_ok_and(|n| (lo..=hi).contains(&n))
            }
            Kind::Text => true,
        };
        if !ok {
            return Err(bad(s.kwd, v));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_set_and_check() {
        let mut a = Attributes::default();
        assert_eq!(a.get("TCPKEEPALV"), "120");
        a.set("tcpkeepalv", "1").unwrap();
        assert_eq!(a.int("TCPKEEPALV", 0), Some(1));
        assert!(a.set("TCPKEEPALV", "0").is_err());
        assert!(a.set("TCPURGPTR", "*XYZ").is_err());
        a.set("IPPATHMTU", "*no").unwrap();
        assert!(!a.yes("IPPATHMTU"));
        a.set("IPPATHMTU", "*YES *ONCE").unwrap();
        assert!(a.set("IPPATHMTU", "*YES").is_err());
        a.set("IP6TMPAVL", "2,5").unwrap();
        a.set("TCPKEEPALV", "*DFT").unwrap();
        assert_eq!(a.get("TCPKEEPALV"), "120");
        assert_eq!(a.changed().count(), 2);
        for s in SPEC {
            let mut b = Attributes::default();
            if !s.default.is_empty() {
                b.set(s.kwd, s.default).unwrap_or_else(|e| panic!("{}: {e}", s.kwd));
            }
        }
    }

    #[test]
    fn sysctl_writes() {
        let mut a = Attributes::default();
        a.set("TCPKEEPALV", "2").unwrap();
        a.set("TCPRCVBUF", "131072").unwrap();
        a.set("IPDTGFWD", "*YES").unwrap();
        a.set("TCPURGPTR", "*RFC").unwrap();
        let w = a.sysctls(&mut |k| (k == "ipv4/tcp_rmem" || k == "ipv4/tcp_wmem").then(|| "4096 131072 6291456".into()));
        let get = |k: &str| w.iter().find(|(x, _)| x == k).map(|(_, v)| v.clone());
        assert_eq!(get("ipv4/tcp_keepalive_time").as_deref(), Some("120"));
        assert_eq!(get("ipv4/tcp_rmem").as_deref(), Some("4096\t131072\t6291456"));
        assert_eq!(get("ipv4/tcp_wmem").as_deref(), Some("4096\t65535\t6291456"));
        assert_eq!(get("ipv6/conf/all/forwarding").as_deref(), Some("1"));
        assert_eq!(get("ipv4/tcp_stdurg").as_deref(), Some("1"));
        assert_eq!(get("ipv4/ip_no_pmtu_disc").as_deref(), Some("0"));
        assert_eq!(get("ipv6/conf/default/temp_valid_lft").as_deref(), Some("604800"));
        assert!(stored_only().contains(&"TCPMINRTM"));
        assert!(!stored_only().contains(&"ARPTIMO"));
    }
}
