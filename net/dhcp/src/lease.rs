// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A lease: the address and what came with it, when it was obtained
//! and its three times (RFC 2131 §4.4.5: T1 at half the lease, T2 at
//! seven eighths, unless the server says otherwise). Kept across IPLs
//! as `key=value` lines ([`Lease::to_text`]).

use std::fmt::Write as _;
use std::net::Ipv4Addr;

use crate::packet::{Kind, Message};

/// One lease.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lease {
    /// The address.
    pub addr: Ipv4Addr,
    /// Its mask.
    pub mask: Ipv4Addr,
    /// The first router.
    pub router: Option<Ipv4Addr>,
    /// The name servers.
    pub dns: Vec<Ipv4Addr>,
    /// The domain name.
    pub domain: String,
    /// The server that granted it.
    pub server: Ipv4Addr,
    /// The next server and boot file (PXE), when the server named them.
    pub boot: Option<(Ipv4Addr, String)>,
    /// Seconds since the epoch it was obtained or renewed.
    pub obtained: u64,
    /// Its length in seconds (`u32::MAX`: infinite).
    pub secs: u32,
    /// T1, seconds after `obtained`.
    pub t1: u32,
    /// T2, seconds after `obtained`.
    pub t2: u32,
}

/// The mask for a class-ful address, when the server sent none.
fn class_mask(a: Ipv4Addr) -> Ipv4Addr {
    match a.octets()[0] {
        0..=127 => Ipv4Addr::new(255, 0, 0, 0),
        128..=191 => Ipv4Addr::new(255, 255, 0, 0),
        _ => Ipv4Addr::new(255, 255, 255, 0),
    }
}

impl Lease {
    /// The lease an `ACK` grants at `now`; `None` for anything else.
    pub fn from_ack(m: &Message, now: u64) -> Option<Self> {
        if m.kind() != Some(Kind::Ack) || m.yiaddr.is_unspecified() {
            return None;
        }
        let o = &m.options;
        let secs = o.lease_secs.unwrap_or(86_400);
        let half = secs / 2;
        let t1 = o.renewal_secs.unwrap_or(half).min(secs);
        let t2 = o
            .rebinding_secs
            .unwrap_or_else(|| u32::try_from(u64::from(secs) * 7 / 8).unwrap_or(secs))
            .clamp(t1, secs);
        let file = o.bootfile.clone().unwrap_or_else(|| m.file.clone());
        Some(Self {
            addr: m.yiaddr,
            mask: o.subnet_mask.unwrap_or_else(|| class_mask(m.yiaddr)),
            router: o.routers.first().copied(),
            dns: o.dns.clone(),
            domain: o.domain.clone().unwrap_or_default(),
            server: o.server_id.unwrap_or(m.siaddr),
            boot: (!file.is_empty()).then_some((m.siaddr, file)),
            obtained: now,
            secs,
            t1,
            t2,
        })
    }

    /// The prefix length of the mask.
    pub fn prefix(&self) -> u8 {
        u8::try_from(u32::from(self.mask).count_ones()).unwrap_or(32)
    }

    fn at(&self, s: u32) -> u64 {
        if self.secs == u32::MAX {
            u64::MAX
        } else {
            self.obtained.saturating_add(u64::from(s))
        }
    }

    /// When to renew (T1).
    pub fn renew_at(&self) -> u64 {
        self.at(self.t1)
    }

    /// When to rebind (T2).
    pub fn rebind_at(&self) -> u64 {
        self.at(self.t2)
    }

    /// When it ends.
    pub fn expires_at(&self) -> u64 {
        self.at(self.secs)
    }

    /// Whether it has ended at `now`.
    pub fn expired(&self, now: u64) -> bool {
        now >= self.expires_at()
    }

    /// The text it is kept in.
    pub fn to_text(&self) -> String {
        let dns: Vec<String> = self.dns.iter().map(ToString::to_string).collect();
        let mut s = format!(
            "addr={}\nmask={}\nrouter={}\ndns={}\ndomain={}\nserver={}\nobtained={}\nsecs={}\nt1={}\nt2={}\n",
            self.addr,
            self.mask,
            self.router.map_or_else(String::new, |r| r.to_string()),
            dns.join(","),
            self.domain,
            self.server,
            self.obtained,
            self.secs,
            self.t1,
            self.t2
        );
        if let Some((next, file)) = &self.boot {
            let _ = write!(s, "next={next}\nfile={file}\n");
        }
        s
    }

    /// A lease from its text; `None` when a value is missing or wrong.
    pub fn from_text(t: &str) -> Option<Self> {
        let get = |k: &str| {
            t.lines()
                .filter_map(|l| l.split_once('='))
                .find(|(a, _)| a.trim() == k)
                .map(|(_, v)| v.trim().to_string())
        };
        let ip = |k: &str| get(k)?.parse::<Ipv4Addr>().ok();
        let num = |k: &str| get(k)?.parse::<u64>().ok();
        let n32 = |k: &str| u32::try_from(num(k)?).ok();
        let dns = get("dns")
            .unwrap_or_default()
            .split(',')
            .filter_map(|a| a.trim().parse().ok())
            .collect();
        Some(Self {
            addr: ip("addr")?,
            mask: ip("mask")?,
            router: ip("router"),
            dns,
            domain: get("domain").unwrap_or_default(),
            server: ip("server")?,
            boot: ip("next").zip(get("file")),
            obtained: num("obtained")?,
            secs: n32("secs")?,
            t1: n32("t1")?,
            t2: n32("t2")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ack() -> Message {
        let mut m = Message::request(Kind::Ack, 7, [2; 6]);
        m.op = crate::packet::BOOTREPLY;
        m.yiaddr = Ipv4Addr::new(10, 0, 2, 15);
        m.options.server_id = Some(Ipv4Addr::new(10, 0, 2, 2));
        m.options.subnet_mask = Some(Ipv4Addr::new(255, 255, 255, 0));
        m.options.routers = vec![Ipv4Addr::new(10, 0, 2, 2)];
        m.options.dns = vec![Ipv4Addr::new(10, 0, 2, 3)];
        m.options.lease_secs = Some(86_400);
        m
    }

    #[test]
    fn times_default_to_rfc_2131() {
        let l = Lease::from_ack(&ack(), 1000).unwrap();
        assert_eq!((l.t1, l.t2, l.secs), (43_200, 75_600, 86_400));
        assert_eq!(l.renew_at(), 44_200);
        assert_eq!(l.rebind_at(), 76_600);
        assert!(l.expired(87_400));
        assert!(!l.expired(87_399));
        assert_eq!(l.prefix(), 24);
        let mut m = ack();
        m.options.renewal_secs = Some(100);
        m.options.rebinding_secs = Some(50);
        let l = Lease::from_ack(&m, 0).unwrap();
        assert_eq!((l.t1, l.t2), (100, 100));
        let mut m = ack();
        m.options.kind = Some(Kind::Nak);
        assert!(Lease::from_ack(&m, 0).is_none());
    }

    #[test]
    fn text_round_trip() {
        let mut l = Lease::from_ack(&ack(), 1_793_750_000).unwrap();
        assert_eq!(Lease::from_text(&l.to_text()), Some(l.clone()));
        l.router = None;
        l.dns.clear();
        l.boot = Some((Ipv4Addr::new(10, 0, 2, 2), "qs400/BOOTX64.EFI".into()));
        assert_eq!(Lease::from_text(&l.to_text()), Some(l));
        assert_eq!(Lease::from_text("addr=x"), None);
    }

    #[test]
    fn infinite_lease() {
        let mut m = ack();
        m.options.lease_secs = Some(u32::MAX);
        let l = Lease::from_ack(&m, 5).unwrap();
        assert!(!l.expired(u64::MAX - 1));
    }
}
