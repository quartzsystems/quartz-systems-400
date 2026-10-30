// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a line description means to the kernel: the interface a vary
//! on brings up, its MTU and address, and the bond an aggregated line
//! is. Pure: the storage service turns a [`LinkPlan`] into netlink
//! requests.

use crate::Lind;

/// The bonding mode of an aggregated line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BondMode {
    /// `*ETHCHL`: a static channel, `balance-xor`.
    Xor,
    /// `*ETHCHL` with `*RNDRBN`: `balance-rr`.
    RoundRobin,
    /// `*LNKAGG`: LACP, `802.3ad`.
    Lacp,
}

/// The bond an aggregated line becomes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BondPlan {
    /// The bond's interface name.
    pub name: String,
    /// Mode.
    pub mode: BondMode,
    /// `xmit_hash_policy`: 0 layer 2, 1 layer 3+4.
    pub xmit_hash: u8,
    /// The ports' interface names, `AGGRSCL` order.
    pub slaves: Vec<String>,
}

/// What to do to the kernel at vary on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkPlan {
    /// The interface the line is: a port's, or the bond's.
    pub ifname: String,
    /// The MTU (`MAXFRAME` plus the four bytes IBM leaves out).
    pub mtu: u32,
    /// The hardware address to set, `None` for `*ADPT`.
    pub mac: Option<[u8; 6]>,
    /// The bond to make first, for `RSRCNAME(*AGG)`.
    pub bond: Option<BondPlan>,
}

/// The MTU of a line: `MAXFRAME(1496)` is the 1500-byte Ethernet.
#[must_use]
pub fn mtu(maxframe: u32) -> u32 {
    maxframe + 4
}

/// `ADPTADR` as bytes, `None` for `*ADPT` or anything that is not
/// twelve hexadecimal digits.
#[must_use]
pub fn mac(adptadr: &str) -> Option<[u8; 6]> {
    if adptadr.len() != 12 {
        return None;
    }
    let mut out = [0u8; 6];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&adptadr[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}

/// The bond interface of an aggregated line: `agg` and the line name
/// in lower case, within the kernel's fifteen characters.
#[must_use]
pub fn bond_name(line: &str) -> String {
    let mut s = format!("agg{}", line.to_ascii_lowercase());
    s.truncate(15);
    s
}

/// The 802.1Q sub-interface of `ifname` for `vlan_id`: `eth0.100`,
/// the parent cut to fit fifteen characters.
#[must_use]
pub fn vlan_name(ifname: &str, vlan_id: u16) -> String {
    let suffix = format!(".{vlan_id}");
    let keep = 15 - suffix.len();
    let parent: String = ifname.chars().take(keep).collect();
    format!("{parent}{suffix}")
}

/// The plan for `lind` (line `line_name`); `port` maps a `CMNnn` resource to its
/// interface name, `None` when the resource is not an Ethernet port.
pub fn plan(
    lind: &Lind,
    line_name: &str,
    port: impl Fn(&str) -> Option<String>,
) -> Result<LinkPlan, String> {
    let mtu = mtu(lind.maxframe);
    let mac = mac(&lind.adptadr);
    if lind.is_aggregate() {
        if lind.aggrscl.is_empty() {
            return Err("CPD0084 RSRCNAME(*AGG) needs AGGRSCL".into());
        }
        let slaves = lind
            .aggrscl
            .iter()
            .map(|r| port(r).ok_or_else(|| format!("CPD26D4 resource {r} is not an Ethernet port")))
            .collect::<Result<Vec<_>, _>>()?;
        let (mode, xmit_hash) = match (lind.aggpcy.0.as_str(), lind.aggpcy.1.as_str()) {
            ("*LNKAGG", "*DFT") => (BondMode::Lacp, 0),
            ("*LNKAGG", _) => (BondMode::Lacp, 1),
            (_, "*RNDRBN") => (BondMode::RoundRobin, 0),
            (_, "*DFT") => (BondMode::Xor, 0),
            _ => (BondMode::Xor, 1),
        };
        let name = bond_name(line_name);
        return Ok(LinkPlan {
            ifname: name.clone(),
            mtu,
            mac,
            bond: Some(BondPlan {
                name,
                mode,
                xmit_hash,
                slaves,
            }),
        });
    }
    if lind.rsrcname.starts_with('*') {
        return Err(format!(
            "CPD26D4 RSRCNAME({}) is not supported in this release",
            lind.rsrcname
        ));
    }
    let ifname = port(&lind.rsrcname)
        .ok_or_else(|| format!("CPD26D4 resource {} is not an Ethernet port", lind.rsrcname))?;
    Ok(LinkPlan {
        ifname,
        mtu,
        mac,
        bond: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ports(r: &str) -> Option<String> {
        match r {
            "CMN01" => Some("eth0".into()),
            "CMN02" => Some("eth1".into()),
            _ => None,
        }
    }

    #[test]
    fn a_port_line() {
        let mut l = Lind::default();
        l.set("RSRCNAME", "CMN01").unwrap();
        l.set("ADPTADR", "020000000001").unwrap();
        l.set("MAXFRAME", "8996").unwrap();
        let p = plan(&l, "ETHLINE", ports).unwrap();
        assert_eq!(p.ifname, "eth0");
        assert_eq!(p.mtu, 9000);
        assert_eq!(p.mac, Some([2, 0, 0, 0, 0, 1]));
        assert!(p.bond.is_none());
        l.set("RSRCNAME", "CMN09").unwrap();
        assert!(
            plan(&l, "ETHLINE", ports)
                .unwrap_err()
                .starts_with("CPD26D4")
        );
    }

    #[test]
    fn an_aggregated_line() {
        let mut l = Lind::default();
        l.set("RSRCNAME", "*AGG").unwrap();
        assert!(plan(&l, "AGGLINE", ports).is_err());
        l.set("AGGRSCL", "CMN01 CMN02").unwrap();
        l.set("AGGPCY", "(*LNKAGG *SRCDESTP)").unwrap();
        let p = plan(&l, "AGGLINE", ports).unwrap();
        assert_eq!(p.ifname, "aggaggline");
        let b = p.bond.unwrap();
        assert_eq!(b.mode, BondMode::Lacp);
        assert_eq!(b.xmit_hash, 1);
        assert_eq!(b.slaves, ["eth0", "eth1"]);
        l.set("AGGPCY", "(*ETHCHL *RNDRBN)").unwrap();
        assert_eq!(
            plan(&l, "AGGLINE", ports).unwrap().bond.unwrap().mode,
            BondMode::RoundRobin
        );
    }

    #[test]
    fn names_fit_the_kernel() {
        assert_eq!(bond_name("ABCDEFGHIJ"), "aggabcdefghij");
        assert_eq!(vlan_name("enp0s31f6", 4094), "enp0s31f6.4094");
        assert_eq!(vlan_name("aggabcdefghij", 100), "aggabcdefgh.100");
        assert!(vlan_name("aggabcdefghij", 100).len() <= 15);
    }
}
