// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Ethernet ports from sysfs: one `CMNnn` resource per port of a NIC,
//! with a QS/400 type and model per driver family (docs/TCPIP.md,
//! "Hardware resources").

use std::fs;
use std::path::Path;

use crate::scan::{pci, read_trim};

/// A driver family and the type it reports as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Family {
    /// Kernel driver name.
    pub driver: &'static str,
    /// Four-character type, in the Quartz `QNxx` range.
    pub type_code: &'static str,
    /// Three-character model.
    pub model: &'static str,
    /// Port description.
    pub text: &'static str,
}

/// The families QS/400 ships drivers for.
pub const FAMILIES: &[Family] = &[
    Family {
        driver: "virtio_net",
        type_code: "QN01",
        model: "001",
        text: "Virtio Ethernet port",
    },
    Family {
        driver: "e1000",
        type_code: "QN02",
        model: "001",
        text: "Gigabit Ethernet port",
    },
    Family {
        driver: "e1000e",
        type_code: "QN03",
        model: "001",
        text: "Gigabit Ethernet port",
    },
    Family {
        driver: "igb",
        type_code: "QN04",
        model: "001",
        text: "Gigabit Ethernet port",
    },
    Family {
        driver: "ixgbe",
        type_code: "QN05",
        model: "001",
        text: "10 Gigabit Ethernet port",
    },
    Family {
        driver: "i40e",
        type_code: "QN06",
        model: "001",
        text: "40 Gigabit Ethernet port",
    },
    Family {
        driver: "ice",
        type_code: "QN07",
        model: "001",
        text: "100 Gigabit Ethernet port",
    },
    Family {
        driver: "mlx5_core",
        type_code: "QN08",
        model: "001",
        text: "ConnectX Ethernet port",
    },
];

/// Any other driver.
pub const OTHER: Family = Family {
    driver: "",
    type_code: "QN00",
    model: "001",
    text: "Ethernet port",
};

/// The family of a driver.
#[must_use]
pub fn family(driver: &str) -> Family {
    FAMILIES
        .iter()
        .find(|f| f.driver == driver)
        .copied()
        .unwrap_or(OTHER)
}

/// One port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetPort {
    /// Interface name (`eth0`, `enp1s0`).
    pub ifname: String,
    /// Driver family.
    pub family: Family,
    /// Kernel driver name.
    pub driver: String,
    /// PCI address of the function, empty if not on PCI.
    pub pci: String,
    /// PCI vendor:device, e.g. `1af4:1041`.
    pub ids: String,
    /// Port number on the function (`dev_port`).
    pub dev_port: u32,
    /// MAC address as sysfs prints it.
    pub mac: String,
    /// Carrier; `None` while the interface is down.
    pub carrier: Option<bool>,
    /// Speed in Mb/s, when the link reports one.
    pub speed: Option<u32>,
    /// sysfs path of the interface without the root.
    pub devpath: String,
}

impl NetPort {
    /// Description for the resource table.
    #[must_use]
    pub fn description(&self) -> String {
        let link = match (self.carrier, self.speed) {
            (Some(true), Some(s)) => format!("link up {s} Mb/s"),
            (Some(true), None) => "link up".to_string(),
            (Some(false), _) => "no link".to_string(),
            (None, _) => "interface down".to_string(),
        };
        format!(
            "{} {} {} {}, {link}",
            self.family.text, self.ifname, self.mac, self.driver
        )
    }

    /// Re-read carrier and speed.
    pub fn refresh(&mut self, root: &Path) {
        let net = root.join("class/net").join(&self.ifname);
        self.carrier = carrier(&net);
        self.speed = speed(&net);
    }
}

fn carrier(net: &Path) -> Option<bool> {
    // Reading `carrier` fails with EINVAL while the interface is down.
    match fs::read_to_string(net.join("carrier")).ok()?.trim() {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

fn speed(net: &Path) -> Option<u32> {
    fs::read_to_string(net.join("speed"))
        .ok()?
        .trim()
        .parse::<i64>()
        .ok()
        .and_then(|s| u32::try_from(s).ok())
        .filter(|s| *s > 0)
}

/// One interface under `root` (`/sys`). `None` for virtual interfaces
/// (no `device`) and non-Ethernet links.
#[must_use]
pub fn scan_one(root: &Path, ifname: &str) -> Option<NetPort> {
    let net = root.join("class/net").join(ifname);
    let dev = fs::canonicalize(net.join("device")).ok()?;
    if read_trim(&net.join("type")) != "1" {
        return None; // not ARPHRD_ETHER
    }
    let driver = fs::read_link(dev.join("driver"))
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default();
    let real_root = fs::canonicalize(root).ok()?;
    let rel = dev.strip_prefix(&real_root).ok()?;
    let devpath = format!("/{}", rel.to_string_lossy());
    let addr = pci::address_in(&devpath).unwrap_or_default();
    let ids = if addr.is_empty() {
        String::new()
    } else {
        let p = root.join("bus/pci/devices").join(&addr);
        let id = |f: &str| read_trim(&p.join(f)).trim_start_matches("0x").to_string();
        format!("{}:{}", id("vendor"), id("device"))
    };
    let netpath = fs::canonicalize(&net).ok()?;
    let netrel = netpath.strip_prefix(&real_root).ok()?;
    Some(NetPort {
        ifname: ifname.to_string(),
        family: family(&driver),
        driver,
        pci: addr,
        ids,
        dev_port: read_trim(&net.join("dev_port")).parse().unwrap_or(0),
        mac: read_trim(&net.join("address")),
        carrier: carrier(&net),
        speed: speed(&net),
        devpath: format!("/{}", netrel.to_string_lossy()),
    })
}

/// Every Ethernet port under `root`, in PCI address then port order,
/// so names stay the same from boot to boot whatever the kernel calls
/// the interfaces.
#[must_use]
pub fn scan(root: &Path) -> Vec<NetPort> {
    let mut names: Vec<String> = fs::read_dir(root.join("class/net"))
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    let mut ports: Vec<NetPort> = names.iter().filter_map(|n| scan_one(root, n)).collect();
    sort(&mut ports);
    ports
}

/// The stable order: PCI address, then port number, then name. Ports not
/// on PCI sort last.
pub fn sort(ports: &mut [NetPort]) {
    ports.sort_by(|a, b| {
        (a.pci.is_empty(), &a.pci, a.dev_port, &a.ifname).cmp(&(
            b.pci.is_empty(),
            &b.pci,
            b.dev_port,
            &b.ifname,
        ))
    });
}

#[cfg(test)]
/// A fake sysfs for the table's tests too.
pub mod tests {
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;

    use super::*;

    /// A sysfs-shaped tree: `(pci address, driver, ifname, dev_port, sub)`
    /// where `sub` puts the interface under a bus device (virtio).
    pub fn fake_sysfs(tag: &str, nics: &[(&str, &str, &str, u32, Option<&str>)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("qs400-sysfs-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("class/net")).unwrap();
        fs::create_dir_all(root.join("bus/pci/devices")).unwrap();
        for (i, (addr, driver, ifname, dev_port, sub)) in nics.iter().enumerate() {
            let func = root.join("devices/pci0000:00").join(addr);
            fs::create_dir_all(&func).unwrap();
            fs::write(func.join("vendor"), "0x8086\n").unwrap();
            fs::write(func.join("device"), format!("0x15{i:02x}\n")).unwrap();
            fs::write(func.join("class"), "0x020000\n").unwrap();
            let pci_link = root.join("bus/pci/devices").join(addr);
            if !pci_link.exists() {
                symlink(&func, pci_link).unwrap();
            }
            let dev = sub.map_or(func.clone(), |s| func.join(s));
            fs::create_dir_all(&dev).unwrap();
            let drv = root.join("bus/pci/drivers").join(driver);
            fs::create_dir_all(&drv).unwrap();
            let _ = symlink(&drv, dev.join("driver"));
            let net = dev.join("net").join(ifname);
            fs::create_dir_all(&net).unwrap();
            fs::write(net.join("type"), "1\n").unwrap();
            fs::write(net.join("address"), format!("52:54:00:00:00:{i:02x}\n")).unwrap();
            fs::write(net.join("dev_port"), format!("{dev_port}\n")).unwrap();
            fs::write(net.join("carrier"), "1\n").unwrap();
            fs::write(net.join("speed"), "1000\n").unwrap();
            symlink(&dev, net.join("device")).unwrap();
            symlink(&net, root.join("class/net").join(ifname)).unwrap();
        }
        // A virtual interface: no device link.
        let lo = root.join("devices/virtual/net/lo");
        fs::create_dir_all(&lo).unwrap();
        fs::write(lo.join("type"), "772\n").unwrap();
        symlink(&lo, root.join("class/net/lo")).unwrap();
        root
    }

    #[test]
    fn ports_sort_by_pci_address_not_name() {
        let root = fake_sysfs(
            "order",
            &[
                ("0000:03:00.0", "ixgbe", "eth0", 0, None),
                ("0000:01:00.0", "virtio_net", "eth1", 0, Some("virtio0")),
                ("0000:02:00.0", "igb", "enp2s0f1", 1, None),
                ("0000:02:00.1", "igb", "enp2s0f0", 0, None),
            ],
        );
        let ports = scan(&root);
        let names: Vec<&str> = ports.iter().map(|p| p.ifname.as_str()).collect();
        assert_eq!(names, ["eth1", "enp2s0f1", "enp2s0f0", "eth0"]);
        assert_eq!(ports[0].family.type_code, "QN01");
        assert_eq!(ports[0].driver, "virtio_net");
        assert_eq!(ports[0].pci, "0000:01:00.0");
        assert_eq!(ports[3].family.type_code, "QN05");
        assert_eq!(ports[1].mac, "52:54:00:00:00:02");
        assert_eq!(ports[1].carrier, Some(true));
        assert_eq!(ports[1].speed, Some(1000));
        assert!(ports[0].devpath.ends_with("/virtio0/net/eth1"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn every_shipped_driver_has_a_distinct_type() {
        let mut codes: Vec<&str> = FAMILIES.iter().map(|f| f.type_code).collect();
        codes.push(OTHER.type_code);
        let n = codes.len();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), n);
        assert!(codes.iter().all(|c| c.len() == 4 && c.starts_with("QN")));
        assert_eq!(family("mlx5_core").type_code, "QN08");
        assert_eq!(family("r8169").type_code, "QN00");
    }

    #[test]
    fn describes_link_state() {
        let mut p = NetPort {
            ifname: "eth0".into(),
            family: family("e1000e"),
            driver: "e1000e".into(),
            pci: "0000:00:02.0".into(),
            ids: "8086:10d3".into(),
            dev_port: 0,
            mac: "52:54:00:12:34:56".into(),
            carrier: Some(true),
            speed: Some(1000),
            devpath: String::new(),
        };
        assert_eq!(
            p.description(),
            "Gigabit Ethernet port eth0 52:54:00:12:34:56 e1000e, link up 1000 Mb/s"
        );
        p.carrier = None;
        assert!(p.description().ends_with("interface down"));
    }
}
