// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What identifies a resource from one IPL to the next, per kind
//! (`docs/HARDWARE_NOTES.md`, "Names that last"):
//!
//! | Kind | Key |
//! |---|---|
//! | Ethernet port | `mac:` permanent MAC, else `pci:` address and port |
//! | network function | `mac:` its first port's permanent MAC, else `sn:`, else `pci:` |
//! | storage controller | `sn:` serial, else `pci:` address |
//! | disk, optical, tape, media changer | `sn:` serial (or WWID), else `dev:` device path |
//! | bridge, other PCI function | `pci:` address |
//! | serial port, keyboard, other input | `dev:` device path |
//! | processor, main storage | `cpu:` number, `mem:` |
//! | virtual channel | `chan:` port name |
//!
//! `mac:` and `sn:` are the hardware's own: they follow a card to
//! another slot. The others are where the device sits.

use std::collections::HashSet;

use crate::scan::pci::is_pci_address;
use crate::table::{Kind, Resource, Table};

/// Longest key kept; a longer one is cut here, the same at every IPL.
pub const KEY_MAX: usize = 200;

/// Whether a key is the hardware's own identity (MAC, serial).
#[must_use]
pub fn strong(key: &str) -> bool {
    key.starts_with("mac:") || key.starts_with("sn:")
}

/// Whether two keys can be one device: equal, or not both hardware
/// identities (a weak key says nothing against a strong one).
#[must_use]
pub fn same_device(a: &str, b: &str) -> bool {
    a == b || !(strong(a) && strong(b))
}

/// Set every resource's key. A key two resources share is no identity:
/// the later ones fall back to their device path.
pub fn identify(t: &mut Table) {
    let keys: Vec<String> = t.resources.iter().map(|r| key_of(t, r)).collect();
    let mut used: HashSet<(Kind, String)> = HashSet::new();
    for (r, k) in t.resources.iter_mut().zip(keys) {
        let k = if used.contains(&(r.kind, k.clone())) {
            place(r)
        } else {
            k
        };
        let k = cut(k);
        used.insert((r.kind, k.clone()));
        r.key = k;
    }
}

/// The key of one resource of the table.
#[must_use]
pub fn key_of(t: &Table, r: &Resource) -> String {
    match r.kind {
        Kind::Port => r
            .port
            .as_ref()
            .and_then(|p| mac_key(&p.perm_mac))
            .unwrap_or_else(|| place(r)),
        Kind::Lan => first_port_mac(t, &r.location)
            .or_else(|| serial_key(&r.serial))
            .unwrap_or_else(|| place(r)),
        Kind::Storage | Kind::Disk | Kind::Optical | Kind::Tape | Kind::TapeLibrary => {
            serial_key(&r.serial).unwrap_or_else(|| place(r))
        }
        Kind::Processor => format!("cpu:{}", r.location),
        Kind::Memory => "mem:".into(),
        Kind::Channel => format!("chan:{}", r.location),
        Kind::Bridge | Kind::Comm | Kind::Keyboard | Kind::Other => place(r),
    }
}

/// The permanent MAC of the lowest-numbered port of the function at
/// `addr`.
fn first_port_mac(t: &Table, addr: &str) -> Option<String> {
    t.resources
        .iter()
        .filter_map(|r| r.port.as_ref())
        .filter(|p| p.pci == addr)
        .min_by_key(|p| p.dev_port)
        .and_then(|p| mac_key(&p.perm_mac))
}

/// `mac:` and twelve upper-case hexadecimal digits; none for an empty,
/// malformed or all-zero address.
fn mac_key(mac: &str) -> Option<String> {
    let hex: String = mac
        .chars()
        .filter(|c| *c != ':')
        .collect::<String>()
        .to_ascii_uppercase();
    (hex.len() == 12 && hex.bytes().all(|b| b.is_ascii_hexdigit()) && hex != "000000000000")
        .then(|| format!("mac:{hex}"))
}

fn serial_key(serial: &str) -> Option<String> {
    let s = serial.trim();
    (!s.is_empty()).then(|| format!("sn:{s}"))
}

/// Where the device sits: its PCI address (a port's with its number),
/// else its device path. An input device's path drops the `inputN` the
/// kernel numbers anew at each boot.
fn place(r: &Resource) -> String {
    if is_pci_address(&r.location) {
        return format!("pci:{}", r.location);
    }
    if r.port.as_ref().is_some_and(|p| !p.pci.is_empty()) {
        return format!("pci:{}", r.location);
    }
    let path = r.devpath.as_str();
    let path = match path.rsplit_once("/input/") {
        Some((head, tail)) if tail.starts_with("input") && !tail.contains('/') => head,
        _ => path,
    };
    format!("dev:{path}")
}

fn cut(mut k: String) -> String {
    if k.len() > KEY_MAX {
        let mut end = KEY_MAX;
        while !k.is_char_boundary(end) {
            end -= 1;
        }
        k.truncate(end);
    }
    k
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_by_kind() {
        assert_eq!(
            mac_key("52:54:00:12:34:5a").as_deref(),
            Some("mac:52540012345A")
        );
        assert_eq!(mac_key("00:00:00:00:00:00"), None);
        assert_eq!(mac_key(""), None);
        let mut r = Resource::new(
            "KBD01".into(),
            Kind::Keyboard,
            "input3".into(),
            String::new(),
            "/devices/platform/i8042/serio0/input/input3".into(),
        );
        assert_eq!(place(&r), "dev:/devices/platform/i8042/serio0");
        r.location = "0000:00:1f.0".into();
        assert_eq!(place(&r), "pci:0000:00:1f.0");
        assert!(strong("mac:52540012345A") && strong("sn:X") && !strong("pci:0"));
        assert!(same_device("pci:0000:00:02.0", "mac:52540012345A"));
        assert!(!same_device("mac:52540012345A", "mac:52540012345B"));
        assert_eq!(cut("x".repeat(300)).len(), KEY_MAX);
    }
}
