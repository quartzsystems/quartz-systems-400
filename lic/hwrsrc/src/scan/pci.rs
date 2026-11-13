// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! PCI devices from sysfs, classified by class code.

use std::path::Path;

use crate::scan::{devpath_in, read_trim, sorted_dir, storage};
use crate::table::Kind;

/// One PCI function.
pub struct PciDevice {
    /// `dddd:bb:dd.f`.
    pub address: String,
    /// Resource class from the PCI class code.
    pub kind: Kind,
    /// Vendor, device and class as text.
    pub description: String,
    /// sysfs device path without the sysfs root.
    pub devpath: String,
    /// Type and model, for storage controllers.
    pub type_model: Option<(&'static str, &'static str)>,
}

/// Every PCI function under a sysfs root, in address order.
#[must_use]
pub fn scan_in(root: &Path) -> Vec<PciDevice> {
    let dir = root.join("bus/pci/devices");
    sorted_dir(&dir)
        .iter()
        .filter_map(|addr| scan_one_in(root, &dir.join(addr)))
        .collect()
}

/// One PCI function from its directory under a sysfs root. Storage
/// controllers are named from the ID table, board before chip.
#[must_use]
pub fn scan_one_in(root: &Path, sys: &Path) -> Option<PciDevice> {
    let devpath = devpath_in(root, sys)?;
    let address = sys.file_name()?.to_string_lossy().into_owned();
    let class =
        u32::from_str_radix(read_trim(&sys.join("class")).trim_start_matches("0x"), 16).ok()?;
    let id = |attr: &str| {
        read_trim(&sys.join(attr))
            .trim_start_matches("0x")
            .to_string()
    };
    let (vendor, device) = (id("vendor"), id("device"));
    let kind = match class >> 16 {
        0x01 => Kind::Storage,
        0x02 => Kind::Lan,
        0x06 => Kind::Bridge,
        0x07 => Kind::Comm,
        _ => Kind::Other,
    };
    let (name, type_model) = if kind == Kind::Storage {
        let subsys = (id("subsystem_vendor"), id("subsystem_device"));
        let ctl = storage::controller(&vendor, &device, (&subsys.0, &subsys.1), class);
        (ctl.name, Some((ctl.type_code, ctl.model)))
    } else {
        (
            describe(&vendor, &device, class),
            own_type(&vendor, &device),
        )
    };
    Some(PciDevice {
        address,
        kind,
        description: format!("{vendor}:{device} {name}"),
        devpath,
        type_model,
    })
}

/// The PCI address embedded in a sysfs device path, if any (the last one,
/// which is the function closest to the device).
#[must_use]
pub fn address_in(devpath: &str) -> Option<String> {
    devpath
        .split('/')
        .rfind(|seg| is_pci_address(seg))
        .map(str::to_string)
}

/// Whether `s` is a PCI address, `dddd:bb:dd.f`.
#[must_use]
pub fn is_pci_address(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 12
        && b[4] == b':'
        && b[7] == b':'
        && b[10] == b'.'
        && [0, 1, 2, 3, 5, 6, 8, 9, 11]
            .iter()
            .all(|&i| b[i].is_ascii_hexdigit())
}

/// The type and model of a device that is no storage controller and has
/// a type of its own; the others take their kind's.
fn own_type(vendor: &str, device: &str) -> Option<(&'static str, &'static str)> {
    match (vendor, device) {
        // The virtio entropy source, legacy and modern.
        ("1af4", "1005" | "1044") => Some((RNG_TYPE, "001")),
        _ => None,
    }
}

/// The type of the virtio entropy source.
pub const RNG_TYPE: &str = "QX01";

/// Names for the devices QS/400 is likely to meet. Falls back to the class.
fn describe(vendor: &str, device: &str, class: u32) -> &'static str {
    match (vendor, device) {
        ("1af4", "1000" | "1041") => "virtio network",
        ("1af4", "1003" | "1043") => "virtio console",
        ("1af4", "1005" | "1044") => "virtio entropy source",
        ("1af4", "1050") => "virtio GPU",
        ("1af4", "1052") => "virtio input",
        ("8086", "100e") => "Intel e1000",
        ("8086", "10d3") => "Intel e1000e",
        ("8086", "29c0") => "Q35 host bridge",
        ("8086", "2918") => "ICH9 LPC bridge",
        ("8086", "2930") => "ICH9 SMBus",
        ("8086", "293e") => "ICH9 HD audio",
        ("1b36", "000c") => "PCIe root port",
        ("1234", "1111") => "standard VGA",
        _ => class_name(class),
    }
}

fn class_name(class: u32) -> &'static str {
    match class >> 8 {
        0x0200 => "Ethernet controller",
        0x0300 => "VGA controller",
        0x0380 => "display controller",
        0x0403 => "audio device",
        0x0600 => "host bridge",
        0x0601 => "ISA bridge",
        0x0604 => "PCI bridge",
        0x0680 => "bridge",
        0x0700 => "serial controller",
        0x0780 => "communication controller",
        0x0c03 => "USB controller",
        0x0c05 => "SMBus",
        _ => "PCI device",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_last_pci_address_in_a_devpath() {
        let p = "/devices/pci0000:00/0000:00:02.0/0000:01:00.0/virtio2/block/vda";
        assert_eq!(address_in(p).as_deref(), Some("0000:01:00.0"));
        assert_eq!(address_in("/devices/platform/serial8250/tty/ttyS0"), None);
    }

    #[test]
    fn the_entropy_source_has_a_type_of_its_own() {
        for device in ["1005", "1044"] {
            assert_eq!(own_type("1af4", device), Some((RNG_TYPE, "001")));
            assert_eq!(describe("1af4", device, 0x00_ff00), "virtio entropy source");
        }
        assert_eq!(own_type("1af4", "1052"), None);
        assert_eq!(own_type("8086", "1044"), None);
    }
}
