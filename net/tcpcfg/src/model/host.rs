// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The host table: `ADDTCPHTE`, `CHGTCPHTE`, `RMVTCPHTE`, `RNMTCPHTE`
//! and `MRGTCPHT`. Names are kept upper case, as the system shows them;
//! lookups ignore case.

use std::net::IpAddr;

use crate::addr::bad;
use crate::error::{Error, Result};
use crate::msg;

/// The most names one address holds (`HOSTNAME` takes 65).
pub const MAX_NAMES: usize = 65;
/// The most entries the table holds.
pub const MAX: usize = 65_535;

/// One host table entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostEntry {
    /// `INTNETADR`.
    pub addr: IpAddr,
    /// `HOSTNAME`, upper case, in order.
    pub names: Vec<String>,
    /// `TEXT`.
    pub text: String,
}

impl HostEntry {
    /// An entry; the names are checked and folded.
    pub fn new(addr: IpAddr, names: &[String], text: &str) -> Result<Self> {
        let mut out: Vec<String> = Vec::new();
        for n in names {
            let n = check_name(n)?;
            if !out.contains(&n) {
                out.push(n);
            }
        }
        if out.is_empty() {
            return Err(Error::new(msg::BAD_VALUE, "At least one host name is needed."));
        }
        if out.len() > MAX_NAMES {
            return Err(Error::new(msg::FULL, format!("An address holds at most {MAX_NAMES} host names.")));
        }
        if text.chars().count() > 64 {
            return Err(bad("TEXT", text));
        }
        Ok(Self {
            addr,
            names: out,
            text: text.to_string(),
        })
    }
}

/// A host name, checked and folded: letters, digits, `-`, `_` and `.`,
/// starting with a letter or digit, at most 255 characters.
pub fn check_name(s: &str) -> Result<String> {
    let n = crate::addr::unquote(s).to_ascii_uppercase();
    let ok = !n.is_empty()
        && n.len() <= 255
        && n.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && n.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !n.contains("..");
    if ok { Ok(n) } else { Err(bad("HOSTNAME", s.trim())) }
}

/// What `CHGTCPHTE` does with one position of the name list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NameChange {
    /// `*SAME`: keep the name at this position.
    Same,
    /// `*BLANK`: drop it.
    Blank,
    /// A new name.
    Name(String),
}

/// What a merge did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Merged {
    /// Entries added.
    pub added: usize,
    /// Entries that gained names.
    pub changed: usize,
}

/// The host table, in the order entries were added.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostTable {
    /// The entries.
    pub list: Vec<HostEntry>,
}

impl HostTable {
    /// The shipped table: the IPv4 and IPv6 loopback names.
    pub fn shipped() -> Self {
        let e = |a: &str, n: &[&str]| HostEntry {
            addr: a.parse().expect("loopback"),
            names: n.iter().map(|s| (*s).to_string()).collect(),
            text: String::new(),
        };
        Self {
            list: vec![
                e("127.0.0.1", &["LOOPBACK", "LOCALHOST"]),
                e("::1", &["IPV6-LOOPBACK", "IPV6-LOCALHOST"]),
            ],
        }
    }

    /// The entry for an address.
    pub fn get(&self, addr: IpAddr) -> Option<&HostEntry> {
        self.list.iter().find(|e| e.addr == addr)
    }

    /// Every address a name has, in table order.
    pub fn lookup(&self, name: &str) -> Vec<IpAddr> {
        self.list
            .iter()
            .filter(|e| e.names.iter().any(|n| n.eq_ignore_ascii_case(name.trim_end_matches('.'))))
            .map(|e| e.addr)
            .collect()
    }

    /// The first name of an address.
    pub fn name_of(&self, addr: IpAddr) -> Option<&str> {
        self.get(addr).and_then(|e| e.names.first()).map(String::as_str)
    }

    /// `ADDTCPHTE`.
    pub fn add(&mut self, e: HostEntry) -> Result<()> {
        if self.get(e.addr).is_some() {
            return Err(Error::new(
                msg::EXISTS,
                format!("The host table already has an entry for {}.", e.addr),
            ));
        }
        if self.list.len() >= MAX {
            return Err(Error::new(msg::FULL, "The host table is full."));
        }
        self.list.push(e);
        Ok(())
    }

    /// `CHGTCPHTE`: `names` (when given) rebuild the list position by
    /// position; `text` replaces the text.
    pub fn change(&mut self, addr: IpAddr, names: Option<&[NameChange]>, text: Option<&str>) -> Result<()> {
        let e = self
            .list
            .iter_mut()
            .find(|e| e.addr == addr)
            .ok_or_else(|| not_found(addr))?;
        if let Some(changes) = names {
            let mut out: Vec<String> = Vec::new();
            for (i, c) in changes.iter().enumerate() {
                let n = match c {
                    NameChange::Same => e.names.get(i).cloned(),
                    NameChange::Blank => None,
                    NameChange::Name(n) => Some(check_name(n)?),
                };
                if let Some(n) = n
                    && !out.contains(&n)
                {
                    out.push(n);
                }
            }
            if out.is_empty() {
                return Err(Error::new(msg::BAD_VALUE, "At least one host name is needed."));
            }
            e.names = out;
        }
        if let Some(t) = text {
            if t.chars().count() > 64 {
                return Err(bad("TEXT", t));
            }
            e.text = t.to_string();
        }
        Ok(())
    }

    /// `RMVTCPHTE`.
    pub fn remove(&mut self, addr: IpAddr) -> Result<HostEntry> {
        let at = self
            .list
            .iter()
            .position(|e| e.addr == addr)
            .ok_or_else(|| not_found(addr))?;
        Ok(self.list.remove(at))
    }

    /// `RNMTCPHTE`: the entry moves to a new address.
    pub fn rename(&mut self, from: IpAddr, to: IpAddr) -> Result<()> {
        if self.get(to).is_some() {
            return Err(Error::new(
                msg::EXISTS,
                format!("The host table already has an entry for {to}."),
            ));
        }
        let e = self
            .list
            .iter_mut()
            .find(|e| e.addr == from)
            .ok_or_else(|| not_found(from))?;
        e.addr = to;
        Ok(())
    }

    /// `MRGTCPHT`: with `replace` the table becomes `from`; otherwise an
    /// address already there gains the names it lacks (its text kept, or
    /// taken when it had none) and a new address is added.
    pub fn merge(&mut self, from: Vec<HostEntry>, replace: bool) -> Result<Merged> {
        let mut m = Merged::default();
        if replace {
            self.list.clear();
        }
        for e in from {
            if let Some(cur) = self.list.iter_mut().find(|c| c.addr == e.addr) {
                let before = cur.names.len();
                for n in e.names {
                    if !cur.names.contains(&n) && cur.names.len() < MAX_NAMES {
                        cur.names.push(n);
                    }
                }
                if cur.text.is_empty() && !e.text.is_empty() {
                    cur.text = e.text;
                }
                if cur.names.len() != before {
                    m.changed += 1;
                }
            } else {
                if self.list.len() >= MAX {
                    return Err(Error::new(msg::FULL, "The host table is full."));
                }
                self.list.push(e);
                m.added += 1;
            }
        }
        Ok(m)
    }

    /// The entries in the panel's order: IPv6 addresses, then IPv4,
    /// each in numeric order.
    pub fn sorted(&self) -> Vec<&HostEntry> {
        let mut v: Vec<&HostEntry> = self.list.iter().collect();
        v.sort_by_key(|e| (e.addr.is_ipv4(), e.addr));
        v
    }
}

fn not_found(addr: IpAddr) -> Error {
    Error::new(msg::NOT_FOUND, format!("The host table has no entry for {addr}."))
}

/// Entries from a host file in the `*AIX` format: `address name...`,
/// `#` starting a comment (its text becomes the entry's).
pub fn parse_aix(text: &str) -> Vec<HostEntry> {
    let mut out = Vec::new();
    for line in text.lines() {
        let (body, comment) = line.split_once('#').unwrap_or((line, ""));
        let mut words = body.split_whitespace();
        let Some(addr) = words.next().and_then(|a| crate::addr::parse_ip(a).ok()) else {
            continue;
        };
        let names: Vec<String> = words.map(str::to_string).collect();
        let text: String = comment.trim().chars().take(64).collect();
        if let Ok(e) = HostEntry::new(addr, &names, &text) {
            out.push(e);
        }
    }
    out
}

/// Entries from a host file in the `*NIC` format:
/// `HOST : address : name, name :`.
pub fn parse_nic(text: &str) -> Vec<HostEntry> {
    let mut out = Vec::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split(':').map(str::trim).collect();
        if f.len() < 3 || !f[0].eq_ignore_ascii_case("HOST") {
            continue;
        }
        let Ok(addr) = crate::addr::parse_ip(f[1]) else {
            continue;
        };
        let names: Vec<String> = f[2]
            .split(',')
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .collect();
        if let Ok(e) = HostEntry::new(addr, &names, "") {
            out.push(e);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn add_change_remove_rename() {
        let mut t = HostTable::shipped();
        t.add(HostEntry::new(ip("10.0.2.15"), &names(&["qs400", "qs400.example.com"]), "me").unwrap())
            .unwrap();
        assert_eq!(t.lookup("QS400.example.COM"), vec![ip("10.0.2.15")]);
        assert_eq!(
            t.add(HostEntry::new(ip("10.0.2.15"), &names(&["x"]), "").unwrap()).unwrap_err().msgid,
            msg::EXISTS
        );
        t.change(
            ip("10.0.2.15"),
            Some(&[NameChange::Same, NameChange::Blank, NameChange::Name("alias".into())]),
            Some("changed"),
        )
        .unwrap();
        assert_eq!(t.get(ip("10.0.2.15")).unwrap().names, names(&["QS400", "ALIAS"]));
        t.rename(ip("10.0.2.15"), ip("10.0.2.16")).unwrap();
        assert!(t.rename(ip("10.0.2.16"), ip("127.0.0.1")).is_err());
        t.remove(ip("10.0.2.16")).unwrap();
        assert_eq!(t.remove(ip("10.0.2.16")).unwrap_err().msgid, msg::NOT_FOUND);
        assert!(check_name("-bad").is_err());
        assert!(check_name("a..b").is_err());
        assert!(HostEntry::new(ip("1.2.3.4"), &[], "").is_err());
    }

    #[test]
    fn merge_rules() {
        let mut t = HostTable::shipped();
        let from = parse_aix(
            "127.0.0.1 localhost loopback lo   # the loopback\n\
             10.1.1.1 alpha alpha.example.com\n\
             garbage line\n",
        );
        let m = t.merge(from.clone(), false).unwrap();
        assert_eq!(m, Merged { added: 1, changed: 1 });
        assert_eq!(t.get(ip("127.0.0.1")).unwrap().names, names(&["LOOPBACK", "LOCALHOST", "LO"]));
        assert_eq!(t.get(ip("127.0.0.1")).unwrap().text, "the loopback");
        let m = t.merge(from, true).unwrap();
        assert_eq!(m.added, 2);
        assert_eq!(t.list.len(), 2);
        let nic = parse_nic("HOST : 10.2.2.2 : beta, beta.example.com :\nNET : 10.0.0.0 : x :\n");
        assert_eq!(nic.len(), 1);
        assert_eq!(nic[0].names, names(&["BETA", "BETA.EXAMPLE.COM"]));
    }
}
