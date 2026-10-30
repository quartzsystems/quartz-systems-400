// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Interfaces: `ADDTCPIFC`, `CHGTCPIFC`, `RMVTCPIFC`.

use std::net::{IpAddr, Ipv4Addr};

use super::{LineRef, Tos};
use crate::addr::{IfcAddr, bad, mask_prefix};
use crate::error::{Error, Result};
use crate::msg;

/// The most interfaces the table holds (IBM's limit is far higher; ours
/// keeps a runaway script from filling the file).
pub const MAX: usize = 8192;

/// One configured interface.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // the command's *YES/*NO parameters
pub struct Interface {
    /// `INTNETADR`.
    pub addr: IfcAddr,
    /// `LIND`, first element.
    pub line: LineRef,
    /// `LIND`, second element: the VLAN id.
    pub vlan: Option<u16>,
    /// `SUBNETMASK` (IPv4).
    pub mask: Option<Ipv4Addr>,
    /// `ADRPFXLEN` (IPv6).
    pub prefix_len: Option<u8>,
    /// `TOS`.
    pub tos: Tos,
    /// `MTU`; `None` is `*LIND`.
    pub mtu: Option<u32>,
    /// `AUTOSTART`.
    pub autostart: bool,
    /// `LCLIFC`: the associated local interface.
    pub local_ifc: Option<Ipv4Addr>,
    /// `ALIASNAME`.
    pub alias: Option<String>,
    /// `TEXT`.
    pub text: String,
    /// `DADMAXTRN` (IPv6).
    pub dad_max: u8,
    /// `PVYEXN`: privacy extensions (IPv6).
    pub privacy: bool,
    /// `IP6GENGLBA`: generate global addresses (`*IP6SAC`).
    pub gen_global: bool,
    /// `PREFIFC`: the preferred interface list (virtual IP).
    pub pref_ifc: Vec<String>,
    /// `DHCPDYNDNS`.
    pub dhcp_dyndns: bool,
    /// `DHCPCLTID`.
    pub dhcp_client_id: String,
}

impl Interface {
    /// An interface with the definition's defaults.
    pub fn new(addr: IfcAddr, line: LineRef) -> Self {
        Self {
            addr,
            line,
            vlan: None,
            mask: None,
            prefix_len: None,
            tos: Tos::Normal,
            mtu: None,
            autostart: true,
            local_ifc: None,
            alias: None,
            text: String::new(),
            dad_max: 1,
            privacy: false,
            gen_global: true,
            pref_ifc: Vec::new(),
            dhcp_dyndns: false,
            dhcp_client_id: "*ADPTADR".into(),
        }
    }

    /// The shipped loopback interface, as IBM ships it.
    pub fn loopback() -> Self {
        let mut i = Self::new(
            IfcAddr::Ip(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            LineRef::Loopback,
        );
        i.mask = Some(Ipv4Addr::new(255, 0, 0, 0));
        i.mtu = Some(576);
        i.alias = Some("LOCALHOST".into());
        i
    }

    /// The key the commands find it by: the address as typed.
    pub fn key(&self) -> String {
        self.addr.to_string()
    }

    /// The prefix length of the address: the mask's or `ADRPFXLEN`.
    pub fn prefix(&self) -> u8 {
        if self.addr.is_v6() {
            self.prefix_len.unwrap_or(64)
        } else {
            self.mask.and_then(mask_prefix).unwrap_or(32)
        }
    }

    /// The line type the panels show: `*NONE` for the loopback,
    /// `*VIRTUALIP` for a virtual address, `*ELAN` for Ethernet.
    pub fn line_type(&self) -> &'static str {
        match self.line {
            LineRef::Loopback => "*NONE",
            LineRef::VirtualIp => "*VIRTUALIP",
            LineRef::Opc => "*OPC",
            LineRef::Line(_) => "*ELAN",
        }
    }

    /// Check the values that do not depend on other entries.
    pub fn validate(&self) -> Result<()> {
        if let Some(m) = self.mtu
            && !(576..=16388).contains(&m)
        {
            return Err(bad("MTU", &m.to_string()));
        }
        if let Some(v) = self.vlan
            && !(1..=4094).contains(&v)
        {
            return Err(bad("LIND", &v.to_string()));
        }
        if self.addr.is_v6() {
            if let Some(p) = self.prefix_len
                && !(1..=128).contains(&p)
            {
                return Err(bad("ADRPFXLEN", &p.to_string()));
            }
        } else if matches!(self.addr, IfcAddr::Ip(_)) && self.mask.is_none() {
            return Err(Error::new(
                msg::BAD_VALUE,
                "SUBNETMASK is needed for an IPv4 interface.",
            ));
        }
        if let (IfcAddr::Ip(IpAddr::V4(ip)), Some(m)) = (self.addr, self.mask) {
            let host = u32::from(ip) & !u32::from(m);
            if m != Ipv4Addr::BROADCAST && (host == 0 || host == !u32::from(m)) {
                return Err(bad("INTNETADR", &ip.to_string()));
            }
        }
        if let Some(a) = &self.alias
            && (a.is_empty() || a.len() > 25)
        {
            return Err(bad("ALIASNAME", a));
        }
        if self.line == LineRef::Loopback
            && let IfcAddr::Ip(ip) = self.addr
            && !ip.is_loopback()
        {
            return Err(bad("INTNETADR", &ip.to_string()));
        }
        Ok(())
    }
}

/// The interface table, in the order entries were added.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Interfaces {
    /// The entries.
    pub list: Vec<Interface>,
}

impl Interfaces {
    /// The entry with this address.
    pub fn get(&self, addr: &IfcAddr) -> Option<&Interface> {
        self.list.iter().find(|i| &i.addr == addr)
    }

    /// The entry with this alias name.
    pub fn by_alias(&self, alias: &str) -> Option<&Interface> {
        self.list
            .iter()
            .find(|i| i.alias.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(alias)))
    }

    /// `ADDTCPIFC`.
    pub fn add(&mut self, ifc: Interface) -> Result<()> {
        ifc.validate()?;
        if self.get(&ifc.addr).is_some() {
            return Err(Error::new(
                msg::DUPLICATE,
                format!("An interface with address {} already exists.", ifc.addr),
            ));
        }
        if let Some(a) = &ifc.alias
            && self.by_alias(a).is_some()
        {
            return Err(Error::new(
                msg::DUPLICATE,
                format!("Alias name {a} is already used by another interface."),
            ));
        }
        if let Some(l) = ifc.local_ifc
            && self.get(&IfcAddr::Ip(IpAddr::V4(l))).is_none()
        {
            return Err(Error::new(
                msg::NO_IFC,
                format!("No interface has the address {l} named by LCLIFC."),
            ));
        }
        if self.list.len() >= MAX {
            return Err(Error::new(msg::FULL, "The interface table is full."));
        }
        self.list.push(ifc);
        Ok(())
    }

    /// `CHGTCPIFC`: replace the entry with `ifc` (same address).
    pub fn change(&mut self, ifc: Interface) -> Result<()> {
        ifc.validate()?;
        if let Some(a) = &ifc.alias
            && self.by_alias(a).is_some_and(|o| o.addr != ifc.addr)
        {
            return Err(Error::new(
                msg::DUPLICATE,
                format!("Alias name {a} is already used by another interface."),
            ));
        }
        let slot = self
            .list
            .iter_mut()
            .find(|i| i.addr == ifc.addr)
            .ok_or_else(|| not_found(&ifc.addr.to_string()))?;
        *slot = ifc;
        Ok(())
    }

    /// `RMVTCPIFC`: the entry removed.
    pub fn remove(&mut self, addr: &IfcAddr) -> Result<Interface> {
        let at = self
            .list
            .iter()
            .position(|i| &i.addr == addr)
            .ok_or_else(|| not_found(&addr.to_string()))?;
        let lcl = self.list[at].addr.ip();
        if let Some(user) = self
            .list
            .iter()
            .find(|i| i.local_ifc.map(IpAddr::V4) == lcl && lcl.is_some())
        {
            return Err(Error::new(
                msg::IN_USE,
                format!("Interface {} names {addr} as its local interface.", user.addr),
            ));
        }
        Ok(self.list.remove(at))
    }

    /// The entries in the panels' order: by address text.
    pub fn sorted(&self) -> Vec<&Interface> {
        let mut v: Vec<&Interface> = self.list.iter().collect();
        v.sort_by_key(|i| sort_key(&i.addr));
        v
    }
}

/// The panels' order: special values first, then IPv4 numerically,
/// then IPv6.
fn sort_key(a: &IfcAddr) -> (u8, u128) {
    match a {
        IfcAddr::Dhcp4 => (0, 0),
        IfcAddr::Slaac6 => (1, 0),
        IfcAddr::Ip(IpAddr::V4(v)) => (2, u128::from(u32::from(*v))),
        IfcAddr::Ip(IpAddr::V6(v)) => (3, u128::from(*v)),
    }
}

fn not_found(what: &str) -> Error {
    Error::new(msg::NOT_FOUND, format!("Interface {what} is not configured."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eth(a: &str) -> Interface {
        let mut i = Interface::new(IfcAddr::parse(a).unwrap(), LineRef::Line("ETHLINE".into()));
        i.mask = Some(Ipv4Addr::new(255, 255, 255, 0));
        i
    }

    #[test]
    fn add_change_remove() {
        let mut t = Interfaces::default();
        t.add(Interface::loopback()).unwrap();
        t.add(eth("10.0.2.16")).unwrap();
        assert_eq!(t.add(eth("10.0.2.16")).unwrap_err().msgid, msg::DUPLICATE);
        let mut c = eth("10.0.2.16");
        c.alias = Some("LOCALHOST".into());
        assert_eq!(t.change(c).unwrap_err().msgid, msg::DUPLICATE);
        let mut c = eth("10.0.2.16");
        c.mtu = Some(1500);
        t.change(c).unwrap();
        assert_eq!(t.list[1].mtu, Some(1500));
        assert_eq!(t.remove(&IfcAddr::parse("10.0.2.99").unwrap()).unwrap_err().msgid, msg::NOT_FOUND);
        t.remove(&IfcAddr::parse("10.0.2.16").unwrap()).unwrap();
        assert_eq!(t.list.len(), 1);
    }

    #[test]
    fn validation() {
        let mut i = eth("10.0.2.0");
        assert!(i.validate().is_err(), "a network address is not an interface");
        i = eth("10.0.2.5");
        i.mtu = Some(100);
        assert!(i.validate().is_err());
        let mut v4 = Interface::new(IfcAddr::parse("10.1.1.1").unwrap(), LineRef::VirtualIp);
        assert!(v4.validate().is_err(), "IPv4 needs a mask");
        v4.mask = Some(Ipv4Addr::BROADCAST);
        v4.validate().unwrap();
        let mut lo = Interface::new(IfcAddr::parse("10.1.1.1").unwrap(), LineRef::Loopback);
        lo.mask = Some(Ipv4Addr::new(255, 0, 0, 0));
        assert!(lo.validate().is_err());
    }

    #[test]
    fn local_interface_must_exist() {
        let mut t = Interfaces::default();
        let mut v = Interface::new(IfcAddr::parse("10.9.9.9").unwrap(), LineRef::VirtualIp);
        v.mask = Some(Ipv4Addr::BROADCAST);
        t.add(v).unwrap();
        let mut i = eth("10.0.2.16");
        i.local_ifc = Some("10.9.9.8".parse().unwrap());
        assert_eq!(t.add(i.clone()).unwrap_err().msgid, msg::NO_IFC);
        i.local_ifc = Some("10.9.9.9".parse().unwrap());
        t.add(i).unwrap();
        assert_eq!(
            t.remove(&IfcAddr::parse("10.9.9.9").unwrap()).unwrap_err().msgid,
            msg::IN_USE
        );
    }
}
