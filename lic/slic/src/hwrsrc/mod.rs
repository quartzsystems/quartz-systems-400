// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Hardware resources: a coldplug walk of sysfs plus hotplug events, with
//! IBM-style resource names assigned in a stable order. WRKHDWRSC and
//! DSPHDWRSC show it.

pub mod nic;
pub mod pci;
pub mod uevent;

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use uevent::Uevent;

/// Resource class. Decides the name prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// PCI bridge or root: `CMBnn`.
    Bridge,
    /// Network function: `LINnn`.
    Lan,
    /// Storage controller: `DCnn`.
    Storage,
    /// Disk unit: `DDnnn`.
    Disk,
    /// Optical unit: `OPTnn`.
    Optical,
    /// Serial or console port: `CMNnn`.
    Comm,
    /// Ethernet port of a network function: `CMNnn`, numbered with the
    /// serial ports.
    Port,
    /// Keyboard: `KBDnn`.
    Keyboard,
    /// Anything else: `OTHnn`.
    Other,
}

impl Kind {
    fn prefix(self) -> &'static str {
        match self {
            Kind::Bridge => "CMB",
            Kind::Lan => "LIN",
            Kind::Storage => "DC",
            Kind::Disk => "DD",
            Kind::Optical => "OPT",
            Kind::Comm | Kind::Port => "CMN",
            Kind::Keyboard => "KBD",
            Kind::Other => "OTH",
        }
    }

    fn width(self) -> usize {
        match self {
            Kind::Disk => 3,
            _ => 2,
        }
    }

    /// Index of the name counter; ports and serial ports share one.
    fn counter(self) -> usize {
        match self {
            Kind::Bridge => 0,
            Kind::Lan => 1,
            Kind::Storage => 2,
            Kind::Disk => 3,
            Kind::Optical => 4,
            Kind::Comm | Kind::Port => 5,
            Kind::Keyboard => 6,
            Kind::Other => 7,
        }
    }
}

/// Whether the device is currently present.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Present.
    Operational,
    /// Was present, removed by hotplug.
    NotDetected,
}

/// One row of the table.
#[derive(Debug)]
pub struct Resource {
    /// Resource name, e.g. `LIN01`.
    pub name: String,
    /// Class.
    pub kind: Kind,
    /// PCI address or device name.
    pub location: String,
    /// Human description.
    pub description: String,
    /// sysfs device path without `/sys`, used to match hotplug events.
    pub devpath: String,
    /// Presence.
    pub status: Status,
    /// Type and model, for network adapters and ports.
    pub type_model: Option<(&'static str, &'static str)>,
    /// The port, for `Kind::Port`.
    pub port: Option<nic::NetPort>,
}

/// Storage controllers that provide device parity protection. Nothing
/// reports any in this release: hardware parity is a Phase 10+ item
/// behind this trait, software RAID is out of scope.
pub trait ParityCapable {
    /// Resource names of parity-capable controllers.
    fn parity_capable(&self) -> Vec<String>;
}

impl ParityCapable for Table {
    fn parity_capable(&self) -> Vec<String> {
        Vec::new()
    }
}

/// The resource table.
#[derive(Default)]
pub struct Table {
    /// Rows in assignment order.
    pub resources: Vec<Resource>,
    next: [u32; 8],
}

impl Table {
    /// Walk sysfs once. PCI first, in address order, so names are stable.
    pub fn coldplug() -> Self {
        let mut t = Self::default();
        for dev in pci::scan() {
            t.add(dev.kind, dev.address.clone(), dev.description, dev.devpath);
        }
        for name in sorted_dir("/sys/class/block") {
            t.add_block(&name);
        }
        t.add_ports(Path::new("/sys"));
        for name in sorted_dir("/sys/class/net") {
            t.attach_net(&name);
        }
        for name in sorted_dir("/sys/class/tty") {
            t.add_tty(&name);
        }
        for name in sorted_dir("/sys/class/input") {
            t.add_input(&name);
        }
        t
    }

    /// Apply a hotplug event. Returns a line describing what changed.
    pub fn apply_uevent(&mut self, ev: &Uevent) -> Option<String> {
        match ev.action.as_str() {
            "add" => {
                if self.resources.iter().any(|r| r.devpath == ev.devpath) {
                    return None;
                }
                let name = ev.devpath.rsplit('/').next().unwrap_or("").to_string();
                let added = match ev.subsystem.as_str() {
                    "pci" => pci::scan_one(
                        Path::new("/sys")
                            .join(ev.devpath.trim_start_matches('/'))
                            .as_path(),
                    )
                    .map(|d| {
                        self.add(d.kind, d.address.clone(), d.description, d.devpath)
                            .name
                            .clone()
                    }),
                    "block" => self.add_block(&name),
                    "net" => {
                        let port = self.add_port(Path::new("/sys"), &name);
                        self.attach_net(&name);
                        port
                    }
                    "tty" => self.add_tty(&name),
                    "input" => self.add_input(&name),
                    _ => None,
                };
                added.map(|n| format!("hwrsrc: added {n} ({})", ev.devpath))
            }
            "remove" => {
                let r = self
                    .resources
                    .iter_mut()
                    .find(|r| r.devpath == ev.devpath)?;
                r.status = Status::NotDetected;
                Some(format!("hwrsrc: {} not detected ({})", r.name, ev.devpath))
            }
            _ => None,
        }
    }

    /// The table as text.
    pub fn dump(&self) -> String {
        let mut s =
            String::from("Resource  Type      Status        Location          Description\n");
        for r in &self.resources {
            let _ = writeln!(
                s,
                "{:<9} {:<9} {:<13} {:<17} {}",
                r.name,
                r.type_model
                    .map_or_else(|| format!("{:?}", r.kind), |(t, m)| format!("{t}-{m}")),
                format!("{:?}", r.status),
                r.location,
                r.description
            );
        }
        s
    }

    fn add(
        &mut self,
        kind: Kind,
        location: String,
        description: String,
        devpath: String,
    ) -> &Resource {
        let i = kind.counter();
        self.next[i] += 1;
        let name = format!(
            "{}{:0width$}",
            kind.prefix(),
            self.next[i],
            width = kind.width()
        );
        self.resources.push(Resource {
            name,
            kind,
            location,
            description,
            devpath,
            status: Status::Operational,
            type_model: None,
            port: None,
        });
        self.resources.last().expect("just pushed")
    }

    /// Whole disks and optical drives. Partitions and virtual devices are skipped.
    fn add_block(&mut self, name: &str) -> Option<String> {
        let sys = Path::new("/sys/class/block").join(name);
        if sys.join("partition").exists()
            || name.starts_with("loop")
            || name.starts_with("ram")
            || name.starts_with("zram")
        {
            return None;
        }
        let devpath = devpath_of(&sys)?;
        let sectors: u64 = read_trim(&sys.join("size")).parse().unwrap_or(0);
        let mib = sectors * 512 / (1024 * 1024);
        let kind = if read_trim(&sys.join("device/type")) == "5" || name.starts_with("sr") {
            Kind::Optical
        } else {
            Kind::Disk
        };
        let on = pci::address_in(&devpath).map_or(String::new(), |a| format!(" on {a}"));
        let desc = format!("{name} {mib} MiB{on}");
        Some(self.add(kind, name.to_string(), desc, devpath).name.clone())
    }

    /// Every Ethernet port under `root`, as `CMNnn` in PCI order.
    pub fn add_ports(&mut self, root: &Path) {
        for p in nic::scan(root) {
            self.push_port(p);
        }
    }

    /// One hot-added port.
    fn add_port(&mut self, root: &Path, ifname: &str) -> Option<String> {
        let p = nic::scan_one(root, ifname)?;
        if self.resources.iter().any(|r| r.devpath == p.devpath) {
            return None;
        }
        Some(self.push_port(p))
    }

    fn push_port(&mut self, p: nic::NetPort) -> String {
        let tm = (p.family.type_code, p.family.model);
        if let Some(lan) = self
            .resources
            .iter_mut()
            .find(|r| r.kind == Kind::Lan && r.location == p.pci)
        {
            lan.type_model = Some(tm);
        }
        let location = if p.dev_port > 0 {
            format!("{}/{}", p.pci, p.dev_port)
        } else {
            p.pci.clone()
        };
        let name = self
            .add(Kind::Port, location, p.description(), p.devpath.clone())
            .name
            .clone();
        let last = self.resources.last_mut().expect("just pushed");
        last.type_model = Some(tm);
        last.port = Some(p);
        name
    }

    /// Re-read carrier and speed of every port.
    pub fn refresh_ports(&mut self, root: &Path) {
        for r in &mut self.resources {
            if let Some(p) = &mut r.port {
                p.refresh(root);
                r.description = p.description();
            }
        }
    }

    /// A network interface is a port of its PCI function; note it there.
    fn attach_net(&mut self, name: &str) -> Option<String> {
        let sys = Path::new("/sys/class/net").join(name);
        let devpath = devpath_of(&sys)?;
        let addr = pci::address_in(&devpath)?;
        let lan = self
            .resources
            .iter_mut()
            .find(|r| r.kind == Kind::Lan && r.location == addr)?;
        let _ = write!(lan.description, ", port {name}");
        Some(lan.name.clone())
    }

    /// Serial ports with real hardware behind them.
    fn add_tty(&mut self, name: &str) -> Option<String> {
        let sys = Path::new("/sys/class/tty").join(name);
        if !name.starts_with("ttyS")
            || !sys.join("device").exists()
            || read_trim(&sys.join("type")) == "0"
        {
            return None;
        }
        let devpath = devpath_of(&sys)?;
        let desc = format!("serial port {name}");
        Some(
            self.add(Kind::Comm, name.to_string(), desc, devpath)
                .name
                .clone(),
        )
    }

    /// Input devices that report keys.
    fn add_input(&mut self, name: &str) -> Option<String> {
        if !name.starts_with("input") {
            return None;
        }
        let sys = Path::new("/sys/class/input").join(name);
        let ev = read_trim(&sys.join("capabilities/ev"));
        let bits = u64::from_str_radix(&ev, 16).unwrap_or(0);
        if bits & 0b10 == 0 {
            return None; // no EV_KEY
        }
        let devpath = devpath_of(&sys)?;
        let desc = read_trim(&sys.join("name"));
        let kind = if desc.to_ascii_lowercase().contains("keyboard") {
            Kind::Keyboard
        } else {
            Kind::Other
        };
        Some(self.add(kind, name.to_string(), desc, devpath).name.clone())
    }
}

/// Entries of a sysfs directory, sorted. Empty if it does not exist.
pub fn sorted_dir(dir: &str) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// A sysfs attribute, trimmed. Empty if unreadable.
pub fn read_trim(path: &Path) -> String {
    fs::read_to_string(path)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Canonical sysfs path without the `/sys` prefix, as uevents report it.
pub fn devpath_of(sys: &Path) -> Option<String> {
    let real = fs::canonicalize(sys).ok()?;
    let s = real.to_string_lossy();
    s.strip_prefix("/sys").map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports_are_cmn_in_pci_order_before_serial_ports() {
        let root = nic::tests::fake_sysfs(
            "table",
            &[
                ("0000:02:00.0", "e1000e", "eth0", 0, None),
                ("0000:01:00.0", "virtio_net", "eth1", 0, Some("virtio0")),
            ],
        );
        let mut t = Table::default();
        t.add(
            Kind::Lan,
            "0000:01:00.0".into(),
            "virtio network".into(),
            "/devices/pci0000:00/0000:01:00.0".into(),
        );
        t.add_ports(&root);
        t.add(
            Kind::Comm,
            "ttyS0".into(),
            "serial port ttyS0".into(),
            "/devices/platform/serial8250/tty/ttyS0".into(),
        );
        let names: Vec<(&str, &str)> = t
            .resources
            .iter()
            .map(|r| (r.name.as_str(), r.location.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                ("LIN01", "0000:01:00.0"),
                ("CMN01", "0000:01:00.0"),
                ("CMN02", "0000:02:00.0"),
                ("CMN03", "ttyS0"),
            ]
        );
        assert_eq!(t.resources[0].type_model, Some(("QN01", "001")));
        assert_eq!(t.resources[2].type_model, Some(("QN03", "001")));
        assert_eq!(t.resources[1].kind, Kind::Port);
        let dump = t.dump();
        assert!(dump.contains("CMN01     QN01-001"), "{dump}");
        assert!(dump.contains("Virtio Ethernet port eth1"), "{dump}");

        // Carrier changes show after a refresh; a known port is not re-added.
        fs::write(root.join("class/net/eth1/carrier"), "0\n").unwrap();
        t.refresh_ports(&root);
        assert!(t.resources[1].description.ends_with("no link"));
        assert!(t.add_port(&root, "eth1").is_none());
        assert!(t.add_port(&root, "lo").is_none());
        fs::remove_dir_all(root).unwrap();
    }
}
