// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Kernel hotplug events as `NETLINK_KOBJECT_UEVENT` delivers them. The
//! socket itself is the service's (`lic/slic/src/hwsvc/`).

/// Kernel multicast group for uevents.
pub const UEVENT_GROUP: u32 = 1;

/// One parsed event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uevent {
    /// `add`, `remove`, `change`, ...
    pub action: String,
    /// sysfs path without `/sys`.
    pub devpath: String,
    /// `pci`, `block`, `net`, ...
    pub subsystem: String,
}

impl Uevent {
    /// Parse the kernel wire format: `action@devpath\0KEY=VALUE\0...`.
    /// `None` for udev's own messages and anything malformed.
    #[must_use]
    pub fn parse(raw: &[u8]) -> Option<Self> {
        let mut parts = raw.split(|b| *b == 0).filter(|p| !p.is_empty());
        let head = String::from_utf8_lossy(parts.next()?).into_owned();
        let (action, devpath) = head.split_once('@')?;
        let mut ev = Self {
            action: action.to_string(),
            devpath: devpath.to_string(),
            subsystem: String::new(),
        };
        for p in parts {
            let kv = String::from_utf8_lossy(p);
            if let Some(v) = kv.strip_prefix("SUBSYSTEM=") {
                ev.subsystem = v.to_string();
            }
        }
        Some(ev)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wire_format() {
        let raw = b"add@/devices/pci0000:00/0000:00:04.0\0ACTION=add\0DEVPATH=/devices/pci0000:00/0000:00:04.0\0SUBSYSTEM=pci\0SEQNUM=7\0";
        let ev = Uevent::parse(raw).unwrap();
        assert_eq!(ev.action, "add");
        assert_eq!(ev.devpath, "/devices/pci0000:00/0000:00:04.0");
        assert_eq!(ev.subsystem, "pci");
        assert!(Uevent::parse(b"libudev\0junk").is_none());
    }
}
