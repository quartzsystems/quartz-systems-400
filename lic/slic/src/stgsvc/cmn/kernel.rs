// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a vary does to the kernel, through netlink: a line's port (or
//! bond) brought up with its MTU and address, taken down again, the
//! 802.1Q sub-interfaces TCP/IP asks for, the bond the kernel makes at
//! boot removed, and the thread that turns link events into status.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use cmncfg::link::{BondMode, LinkPlan};
use netlink::route::{BondMode as NlBond, BondSpec, LinkEvents, Rtnl};

use super::Shared;
use crate::hwrsrc;
use crate::ipl::system::{System, now_unix};

/// The kernel's boot-time bond (`BONDING=y` makes one; the baked
/// command line cannot say `max_bonds=0`).
const BOOT_BOND: &str = "bond0";

fn nl(e: &netlink::Error) -> String {
    e.to_string()
}

/// The Ethernet ports: resource name and interface, read from sysfs
/// the first time.
pub(super) fn ports(system: &mut System) -> Vec<(String, String)> {
    if system.hw.resources.is_empty() {
        system.hw = hwrsrc::Table::coldplug();
    }
    system
        .hw
        .resources
        .iter()
        .filter(|r| r.kind == hwrsrc::Kind::Port && r.status == hwrsrc::Status::Operational)
        .filter_map(|r| r.port.as_ref().map(|p| (r.name.clone(), p.ifname.clone())))
        .collect()
}

/// The port's speed and duplex as `DSPLIND` shows them while the line
/// is up, `None` when the kernel does not say.
pub(super) fn current_speed(ifname: &str) -> Option<String> {
    let s = std::fs::read_to_string(format!("/sys/class/net/{ifname}/speed")).ok()?;
    let mbps: i64 = s.trim().parse().ok()?;
    Some(match mbps {
        10 => "10M".into(),
        100 => "100M".into(),
        1000 => "1G".into(),
        10_000 => "10G".into(),
        25_000 => "25G".into(),
        100_000 => "100G".into(),
        n if n > 0 => format!("{n}M"),
        _ => return None,
    })
}

/// The port's duplex (`*FULL`, `*HALF`), when the kernel says.
pub(super) fn current_duplex(ifname: &str) -> Option<String> {
    let s = std::fs::read_to_string(format!("/sys/class/net/{ifname}/duplex")).ok()?;
    match s.trim() {
        "full" => Some("*FULL".into()),
        "half" => Some("*HALF".into()),
        _ => None,
    }
}

fn bond_spec(mode: BondMode, xmit_hash: u8) -> BondSpec {
    let mut spec = BondSpec::new(match mode {
        BondMode::Xor => NlBond::Xor,
        BondMode::RoundRobin => NlBond::RoundRobin,
        BondMode::Lacp => NlBond::Lacp,
    });
    if mode != BondMode::RoundRobin {
        spec.xmit_hash = Some(xmit_hash);
    }
    spec
}

/// Bring the line's interface up as `plan` says: the bond made and its
/// ports enslaved first, the MTU and address set while it is down.
/// Waits up to `wait` for the carrier; whether it is there.
pub(super) fn bring_up(plan: &LinkPlan, wait: Duration) -> Result<bool, String> {
    let mut r = Rtnl::open().map_err(|e| nl(&e))?;
    if let Some(b) = &plan.bond {
        let bond = match r.link_by_name(&b.name) {
            Ok(l) => l,
            Err(e) if e.is_not_found() => r
                .create_bond(&b.name, &bond_spec(b.mode, b.xmit_hash))
                .map_err(|e| format!("bond {}: {e}", b.name))?,
            Err(e) => return Err(nl(&e)),
        };
        for s in &b.slaves {
            let l = r.link_by_name(s).map_err(|e| format!("port {s}: {e}"))?;
            if l.master != Some(bond.index) {
                r.enslave(l.index, bond.index)
                    .map_err(|e| format!("port {s} into {}: {e}", b.name))?;
            }
            r.set_up(l.index, true)
                .map_err(|e| format!("port {s}: {e}"))?;
        }
    }
    let link = r
        .link_by_name(&plan.ifname)
        .map_err(|e| format!("interface {}: {e}", plan.ifname))?;
    let mac_differs = plan.mac.is_some_and(|m| link.mac != m);
    if link.is_up() && (mac_differs || link.mtu != plan.mtu) {
        r.set_up(link.index, false).map_err(|e| nl(&e))?;
    }
    if link.mtu != plan.mtu {
        r.set_mtu(link.index, plan.mtu)
            .map_err(|e| format!("MTU {} on {}: {e}", plan.mtu, plan.ifname))?;
    }
    if let Some(mac) = plan.mac.filter(|_| mac_differs) {
        r.set_mac(link.index, &mac)
            .map_err(|e| format!("address on {}: {e}", plan.ifname))?;
    }
    r.set_up(link.index, true)
        .map_err(|e| format!("{} up: {e}", plan.ifname))?;
    let until = Instant::now() + wait;
    loop {
        let carrier = carrier(&mut r, link.index);
        if carrier || Instant::now() >= until {
            return Ok(carrier);
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn carrier(r: &mut Rtnl, index: u32) -> bool {
    r.link_by_index(index).is_ok_and(|l| l.has_carrier())
}

/// Whether `ifname` has its carrier now.
pub(super) fn has_carrier(ifname: &str) -> bool {
    Rtnl::open()
        .and_then(|mut r| r.link_by_name(ifname))
        .is_ok_and(|l| l.has_carrier())
}

/// Take a line's interface down; a bond of the line's own is taken
/// apart and removed.
pub(super) fn bring_down(ifname: &str, bond: bool) -> Result<(), String> {
    let mut r = Rtnl::open().map_err(|e| nl(&e))?;
    let link = match r.link_by_name(ifname) {
        Ok(l) => l,
        Err(e) if e.is_not_found() => return Ok(()),
        Err(e) => return Err(nl(&e)),
    };
    r.set_up(link.index, false)
        .map_err(|e| format!("{ifname} down: {e}"))?;
    if bond {
        let links = r.links().map_err(|e| nl(&e))?;
        for s in links.iter().filter(|l| l.master == Some(link.index)) {
            let _ = r.release(s.index);
            let _ = r.set_up(s.index, false);
        }
        r.delete_link(link.index)
            .map_err(|e| format!("bond {ifname}: {e}"))?;
    }
    Ok(())
}

/// The 802.1Q sub-interface `vlan_id` of `parent`, made when missing
/// (for TCP/IP interfaces that name a VLAN); its name.
#[allow(dead_code)] // MIGRATE(phase10): called by TCP/IP configuration
pub(crate) fn ensure_vlan(parent: &str, vlan_id: u16) -> Result<String, String> {
    let name = cmncfg::link::vlan_name(parent, vlan_id);
    let mut r = Rtnl::open().map_err(|e| nl(&e))?;
    match r.link_by_name(&name) {
        Ok(_) => {}
        Err(e) if e.is_not_found() => {
            let p = r
                .link_by_name(parent)
                .map_err(|e| format!("{parent}: {e}"))?;
            let v = r
                .create_vlan(&name, p.index, vlan_id)
                .map_err(|e| format!("VLAN {vlan_id} on {parent}: {e}"))?;
            r.set_up(v.index, true).map_err(|e| nl(&e))?;
        }
        Err(e) => return Err(nl(&e)),
    }
    Ok(name)
}

/// Remove the kernel's boot-time bond when nothing is enslaved to it:
/// lines make their own bonds. `dummy0` stays; nothing uses it.
pub(super) fn remove_boot_bond() {
    let Ok(mut r) = Rtnl::open() else {
        return;
    };
    let Ok(links) = r.links() else {
        return;
    };
    let Some(b) = links
        .iter()
        .find(|l| l.name == BOOT_BOND && l.kind.as_deref() == Some("bond"))
    else {
        return;
    };
    if links.iter().any(|l| l.master == Some(b.index)) {
        return;
    }
    if let Err(e) = r.delete_link(b.index) {
        eprintln!("qsstg: removing {BOOT_BOND}: {e}");
    }
}

/// Follow the kernel's link events in a thread of their own: each
/// carrier change or removal goes to the status machine, the messages
/// it answers wait in `pending` for the service to send.
pub(super) fn follow(shared: Arc<Mutex<Shared>>) {
    let mut events = match LinkEvents::open() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("qsstg: link events: {e}");
            return;
        }
    };
    let spawned = std::thread::Builder::new()
        .name("qsstg-links".into())
        .spawn(move || {
            loop {
                let Ok(batch) = events.wait() else {
                    std::thread::sleep(Duration::from_millis(200));
                    continue;
                };
                let mut sh = shared.lock().unwrap_or_else(PoisonError::into_inner);
                let now = now_unix();
                for ev in batch {
                    let l = &ev.link;
                    let effects = if ev.removed {
                        sh.state.link_removed(&l.name, now)
                    } else {
                        sh.state.carrier(&l.name, l.is_up() && l.has_carrier(), now)
                    };
                    sh.pending.extend(effects);
                }
            }
        });
    if let Err(e) = spawned {
        eprintln!("qsstg: link events thread: {e}");
    }
}
