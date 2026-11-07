// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The machine's identity from the firmware's SMBIOS tables, as the
//! kernel publishes them under `/sys/class/dmi/id/`: vendor, product,
//! serial numbers and UUID. Firmware often fills these with placeholders
//! (`To Be Filled By O.E.M.`, `Default string`, zeros); [`genuine`] tells
//! a real value from those. `product_serial`, `chassis_serial` and
//! `product_uuid` are readable by root only.

use std::path::Path;

use crate::scan::read_trim;

/// Where the kernel publishes the DMI identity.
pub const DMI_DIR: &str = "/sys/class/dmi/id";

/// The DMI identity, each field trimmed as the firmware reports it
/// (empty when unreadable).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dmi {
    /// `sys_vendor` (`Dell Inc.`).
    pub vendor: String,
    /// `product_name` (`PowerEdge R240`).
    pub product: String,
    /// `product_serial` (a Dell service tag).
    pub product_serial: String,
    /// `chassis_serial`.
    pub chassis_serial: String,
    /// `product_uuid`.
    pub uuid: String,
}

impl Dmi {
    /// Read from [`DMI_DIR`].
    #[must_use]
    pub fn read() -> Self {
        Self::read_from(Path::new(DMI_DIR))
    }

    /// Read from a directory laid out like [`DMI_DIR`].
    #[must_use]
    pub fn read_from(dir: &Path) -> Self {
        let f = |n: &str| read_trim(&dir.join(n));
        Self {
            vendor: f("sys_vendor"),
            product: f("product_name"),
            product_serial: f("product_serial"),
            chassis_serial: f("chassis_serial"),
            uuid: f("product_uuid"),
        }
    }

    /// The machine's real serial number: the product serial, else the
    /// chassis serial, when genuine.
    #[must_use]
    pub fn serial(&self) -> Option<&str> {
        [&self.product_serial, &self.chassis_serial]
            .into_iter()
            .map(String::as_str)
            .find(|s| genuine(s))
    }

    /// Vendor and product (`Dell Inc. PowerEdge R240`), the genuine parts
    /// only; empty when the firmware names neither.
    #[must_use]
    pub fn model(&self) -> String {
        let vendor = Some(self.vendor.as_str()).filter(|s| genuine(s));
        let product = Some(self.product.as_str()).filter(|s| genuine(s));
        match (vendor, product) {
            (Some(v), Some(p)) if p.to_ascii_lowercase().starts_with(&v.to_ascii_lowercase()) => {
                p.into()
            }
            (Some(v), Some(p)) => format!("{v} {p}"),
            (Some(x), None) | (None, Some(x)) => x.into(),
            (None, None) => String::new(),
        }
    }

    /// What a derived serial number hashes: the UUID unless it is all
    /// zeros, else the first non-empty serial. QEMU publishes no UUID
    /// unless given one.
    #[must_use]
    pub fn identity(&self) -> Option<&str> {
        [&self.uuid, &self.product_serial, &self.chassis_serial]
            .into_iter()
            .map(String::as_str)
            .find(|s| !s.is_empty() && !s.starts_with("00000000-"))
    }
}

/// Text firmware leaves in a field it was never given (compared without
/// case).
const PLACEHOLDERS: &[&str] = &[
    "to be filled by o.e.m.",
    "to be filled by oem",
    "not specified",
    "not applicable",
    "not available",
    "default string",
    "system serial number",
    "chassis serial number",
    "system product name",
    "system manufacturer",
    "system name",
    "product name",
    "manufacturer",
    "serial",
    "none",
    "null",
    "n/a",
    "na",
    "oem",
    "o.e.m.",
    "unknown",
    "invalid",
    "empty",
    "default",
    "123456789",
    "0123456789",
];

/// Whether a DMI value is a real one rather than empty, a placeholder,
/// or one character repeated (`0000000`, `FFFFFFFF-FFFF-...`, `XXXX`).
#[must_use]
pub fn genuine(value: &str) -> bool {
    let v = value.trim();
    if v.is_empty() || v.chars().all(|c| !c.is_ascii_graphic()) {
        return false;
    }
    let lower = v.to_ascii_lowercase();
    if PLACEHOLDERS.contains(&lower.as_str()) || lower.contains("filled by") {
        return false;
    }
    let mut marks = lower
        .chars()
        .filter(|c| !matches!(c, '-' | ' ' | '.' | ':'));
    match marks.next() {
        None => false,
        Some(first) => !marks.all(|c| c == first),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dmi(vendor: &str, product: &str, ps: &str, cs: &str, uuid: &str) -> Dmi {
        Dmi {
            vendor: vendor.into(),
            product: product.into(),
            product_serial: ps.into(),
            chassis_serial: cs.into(),
            uuid: uuid.into(),
        }
    }

    #[test]
    fn placeholders() {
        for p in [
            "",
            "   ",
            "To Be Filled By O.E.M.",
            "To be filled by O.E.M.",
            "Not Specified",
            "Default string",
            "System Serial Number",
            "Chassis Serial Number",
            "System Product Name",
            "0",
            "None",
            "N/A",
            "0000000",
            "00000000-0000-0000-0000-000000000000",
            "FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF",
            "XXXXXXXX",
            "........",
        ] {
            assert!(!genuine(p), "{p:?}");
        }
        for g in [
            "BWMX6R0",
            "CZ2345678X",
            "J300ABCD",
            "Dell Inc.",
            "PowerEdge R240",
            "QEMU",
            "10-7A2B9C",
        ] {
            assert!(genuine(g), "{g:?}");
        }
    }

    #[test]
    fn dell() {
        let d = dmi(
            "Dell Inc.",
            "PowerEdge R240",
            "BWMX6R0",
            "BWMX6R0",
            "4c4c4544-0057-4d10-8058-c2c04f365230",
        );
        assert_eq!(d.serial(), Some("BWMX6R0"));
        assert_eq!(d.model(), "Dell Inc. PowerEdge R240");
        assert_eq!(d.identity(), Some("4c4c4544-0057-4d10-8058-c2c04f365230"));
    }

    #[test]
    fn whitebox() {
        let d = dmi(
            "System manufacturer",
            "System Product Name",
            "System Serial Number",
            "Default string",
            "03000200-0400-0500-0006-000700080009",
        );
        assert_eq!(d.serial(), None);
        assert_eq!(d.model(), "");
        let d = dmi(
            "To Be Filled By O.E.M.",
            "X570 Taichi",
            "To Be Filled By O.E.M.",
            "ABC1234",
            "",
        );
        assert_eq!(d.serial(), Some("ABC1234"));
        assert_eq!(d.model(), "X570 Taichi");
    }

    #[test]
    fn qemu() {
        // No -uuid and no serials: nothing genuine to show, nothing to hash.
        let d = dmi("QEMU", "Standard PC (Q35 + ICH9, 2009)", "", "", "");
        assert_eq!(d.serial(), None);
        assert_eq!(d.model(), "QEMU Standard PC (Q35 + ICH9, 2009)");
        assert_eq!(d.identity(), None);
        let d = dmi("QEMU", "", "", "", "00000000-0000-0000-0000-000000000000");
        assert_eq!(d.identity(), None);
    }

    #[test]
    fn vendor_in_product() {
        assert_eq!(
            dmi("LENOVO", "Lenovo ThinkSystem SR250", "", "", "").model(),
            "Lenovo ThinkSystem SR250"
        );
    }

    #[test]
    fn read_dir() {
        let dir = std::env::temp_dir().join(format!("hwrsrc-dmi-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (n, v) in [
            ("sys_vendor", "Dell Inc.\n"),
            ("product_name", "PowerEdge R240\n"),
            ("product_serial", "BWMX6R0\n"),
        ] {
            std::fs::write(dir.join(n), v).unwrap();
        }
        let d = Dmi::read_from(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(d.serial(), Some("BWMX6R0"));
        assert_eq!(d.model(), "Dell Inc. PowerEdge R240");
        assert_eq!(d.chassis_serial, "");
    }
}
