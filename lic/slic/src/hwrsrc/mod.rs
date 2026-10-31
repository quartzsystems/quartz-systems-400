// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Hardware resources: a coldplug walk of sysfs plus hotplug events, with
//! IBM-style resource names assigned in a stable order. WRKHDWRSC and
//! DSPHDWRSC show it.

pub mod nic;
pub mod pci;
pub mod storage;
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
    /// Tape drive: `TAPnn`, named by the tape crate.
    Tape,
    /// Tape media changer: `TAPMLBnn`, named by the tape crate.
    TapeLibrary,
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
            Kind::Tape => "TAP",
            Kind::TapeLibrary => "TAPMLB",
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
            Kind::Tape => 8,
            Kind::TapeLibrary => 9,
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
    next: [u32; 10],
}

impl Table {
    /// Walk sysfs once. PCI first, in address order, so names are stable.
    pub fn coldplug() -> Self {
        let mut t = Self::default();
        let sys = Path::new("/sys");
        t.add_pci(sys);
        t.add_blocks(sys);
        t.add_ports(sys);
        for name in sorted_dir("/sys/class/net") {
            t.attach_net(&name);
        }
        for name in sorted_dir("/sys/class/tty") {
            t.add_tty(&name);
        }
        for name in sorted_dir("/sys/class/input") {
            t.add_input(&name);
        }
        t.add_tapes(Path::new("/sys"));
        t
    }

    /// Tape drives (`/sys/class/scsi_tape`) and changers (SCSI type 8
    /// under `/sys/class/scsi_generic`), named `TAPnn` and `TAPMLBnn`
    /// by `tape::device::naming` in SCSI address order.
    pub fn add_tapes(&mut self, root: &Path) {
        use tape::device::naming::{ResourceKind, Scanned, assign, parse_hctl, tape_nodes};
        let class = |c: &str| root.join("class").join(c);
        let hctl_of = |dev: &Path| {
            fs::canonicalize(dev)
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                .and_then(|n| parse_hctl(&n))
        };
        let serial = |dev: &Path| {
            let s = read_trim(&dev.join("wwid"));
            (!s.is_empty()).then_some(s)
        };
        let mut scanned = Vec::new();
        let entries = sorted_dir(&class("scsi_tape").to_string_lossy());
        for node in tape_nodes(&entries) {
            let dev = class("scsi_tape").join(&node).join("device");
            if let Some(hctl) = hctl_of(&dev) {
                scanned.push(Scanned { kind: ResourceKind::Tape, node, hctl, serial: serial(&dev) });
            }
        }
        for node in sorted_dir(&class("scsi_generic").to_string_lossy()) {
            let dev = class("scsi_generic").join(&node).join("device");
            if read_trim(&dev.join("type")) != "8" {
                continue;
            }
            if let Some(hctl) = hctl_of(&dev) {
                scanned.push(Scanned { kind: ResourceKind::Library, node, hctl, serial: serial(&dev) });
            }
        }
        for r in assign(&scanned, &[]) {
            let kind = match r.device.kind {
                ResourceKind::Tape => Kind::Tape,
                ResourceKind::Library => Kind::TapeLibrary,
            };
            let sub = if kind == Kind::Tape { "scsi_tape" } else { "scsi_generic" };
            let devpath = devpath_of(&class(sub).join(&r.device.node)).unwrap_or_default();
            let [h, c, t, l] = r.device.hctl;
            let what = if kind == Kind::Tape { "tape drive" } else { "tape media changer" };
            self.resources.push(Resource {
                name: r.name,
                kind,
                location: r.device.node.clone(),
                description: format!("{what} {} at {h}:{c}:{t}:{l}", r.device.node),
                devpath,
                status: Status::Operational,
                type_model: None,
                port: None,
            });
        }
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
                    .map(|d| self.add_function(d)),
                    "block" => self.add_block(Path::new("/sys"), &name),
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

    /// Every PCI function under `root`, in address order.
    pub fn add_pci(&mut self, root: &Path) {
        for dev in pci::scan_in(root) {
            self.add_function(dev);
        }
    }

    /// One PCI function; a storage controller keeps its type and model.
    fn add_function(&mut self, dev: pci::PciDevice) -> String {
        let tm = dev.type_model;
        let name = self
            .add(dev.kind, dev.address, dev.description, dev.devpath)
            .name
            .clone();
        self.resources.last_mut().expect("just pushed").type_model = tm;
        name
    }

    /// Whole disks and optical drives under `root`, named in the order
    /// of `storage::disk_key`: by controller, then SCSI address, so a
    /// disk keeps its `DDnnn` from boot to boot whatever the kernel
    /// calls it.
    pub fn add_blocks(&mut self, root: &Path) {
        let dir = root.join("class/block");
        let mut names: Vec<(_, String)> = sorted_dir(&dir.to_string_lossy())
            .into_iter()
            .map(|n| {
                let path = devpath_in(root, &dir.join(&n)).unwrap_or_default();
                (storage::disk_key(&path, &n), n)
            })
            .collect();
        names.sort();
        for (_, name) in names {
            self.add_block(root, &name);
        }
    }

    /// One whole disk or optical drive. Partitions and virtual devices
    /// are skipped.
    fn add_block(&mut self, root: &Path, name: &str) -> Option<String> {
        let sys = root.join("class/block").join(name);
        if sys.join("partition").exists()
            || name.starts_with("loop")
            || name.starts_with("ram")
            || name.starts_with("zram")
        {
            return None;
        }
        let devpath = devpath_in(root, &sys)?;
        let sectors: u64 = read_trim(&sys.join("size")).parse().unwrap_or(0);
        let mib = sectors * 512 / (1024 * 1024);
        let kind = if read_trim(&sys.join("device/type")) == "5" || name.starts_with("sr") {
            Kind::Optical
        } else {
            Kind::Disk
        };
        let at = storage::hctl(&devpath).map_or(String::new(), |[h, c, t, l]| {
            format!(" at {h}:{c}:{t}:{l}")
        });
        let on = pci::address_in(&devpath).map_or(String::new(), |a| {
            let ctl = self.controller_at(&a).map_or(a, |r| r.name.clone());
            format!(" on {ctl}")
        });
        let desc = format!("{name} {mib} MiB{at}{on}");
        Some(self.add(kind, name.to_string(), desc, devpath).name.clone())
    }

    /// The storage controller at a PCI address.
    fn controller_at(&self, address: &str) -> Option<&Resource> {
        self.resources
            .iter()
            .find(|r| r.kind == Kind::Storage && r.location == address)
    }

    /// The storage controller a disk (`sda`) is attached through.
    pub fn controller_of(&self, disk: &str) -> Option<&Resource> {
        let d = self
            .resources
            .iter()
            .find(|r| matches!(r.kind, Kind::Disk | Kind::Optical) && r.location == disk)?;
        self.controller_at(&pci::address_in(&d.devpath)?)
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
    devpath_in(Path::new("/sys"), sys)
}

/// Canonical path under a sysfs root, without the root.
pub fn devpath_in(root: &Path, sys: &Path) -> Option<String> {
    let real = fs::canonicalize(sys).ok()?;
    let root = fs::canonicalize(root).ok()?;
    let rest = real.strip_prefix(&root).ok()?;
    Some(format!("/{}", rest.to_string_lossy()))
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use super::*;

    /// A PCI function under `parent`, linked from `bus/pci/devices`.
    fn function(root: &Path, parent: &str, addr: &str, ids: [&str; 5]) -> std::path::PathBuf {
        let dir = root.join(parent).join(addr);
        fs::create_dir_all(&dir).unwrap();
        let attrs = ["vendor", "device", "class", "subsystem_vendor", "subsystem_device"];
        for (attr, v) in attrs.iter().zip(ids) {
            fs::write(dir.join(attr), format!("0x{v}\n")).unwrap();
        }
        symlink(&dir, root.join("bus/pci/devices").join(addr)).unwrap();
        dir
    }

    /// A disk at `under/<hctl path>/block/<name>`, linked from `class/block`.
    fn disk(root: &Path, under: &Path, scsi: &str, name: &str, mib: u64) -> std::path::PathBuf {
        let dir = under.join(scsi).join("block").join(name);
        fs::create_dir_all(dir.join("device")).unwrap();
        fs::write(dir.join("size"), format!("{}\n", mib * 2048)).unwrap();
        fs::write(dir.join("device/type"), "0\n").unwrap();
        symlink(&dir, root.join("class/block").join(name)).unwrap();
        dir
    }

    #[test]
    fn disks_behind_a_perc_are_named_by_scsi_address_under_their_controller() {
        let root = std::env::temp_dir().join(format!("qs400-sysfs-perc-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("bus/pci/devices")).unwrap();
        fs::create_dir_all(root.join("class/block")).unwrap();
        let top = "devices/pci0000:00";
        function(&root, top, "0000:00:01.0", ["8086", "6f02", "060400", "8086", "0000"]);
        let ahci = function(&root, top, "0000:00:1f.2", ["8086", "2922", "010601", "1af4", "1100"]);
        let perc = function(
            &root,
            &format!("{top}/0000:00:01.0"),
            "0000:02:00.0",
            ["1000", "005f", "010400", "1028", "1f4b"],
        );
        // The kernel's names in another order than the SCSI addresses.
        for (t, name) in [(3, "sda"), (1, "sdb"), (0, "sdc"), (2, "sdd")] {
            disk(
                &root,
                &perc,
                &format!("host0/target0:0:{t}/0:0:{t}:0"),
                name,
                953_869,
            );
        }
        let sda = fs::canonicalize(root.join("class/block/sda")).unwrap();
        fs::create_dir_all(sda.join("sda1")).unwrap();
        fs::write(sda.join("sda1/partition"), "1\n").unwrap();
        symlink(sda.join("sda1"), root.join("class/block/sda1")).unwrap();
        disk(&root, &ahci, "ata1/host1/target1:0:0/1:0:0:0", "sde", 4096);

        let mut t = Table::default();
        t.add_pci(&root);
        t.add_blocks(&root);
        let names: Vec<(&str, &str)> = t
            .resources
            .iter()
            .map(|r| (r.name.as_str(), r.location.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                ("CMB01", "0000:00:01.0"),
                ("DC01", "0000:00:1f.2"),
                ("DC02", "0000:02:00.0"),
                ("DD001", "sde"),
                ("DD002", "sdc"),
                ("DD003", "sdb"),
                ("DD004", "sdd"),
                ("DD005", "sda"),
            ]
        );
        let perc = &t.resources[2];
        assert_eq!(perc.type_model, Some(("QD10", "330")));
        assert_eq!(perc.description, "1000:005f PERC H330 Mini");
        assert_eq!(t.resources[1].type_model, Some(("QD03", "001")));
        assert_eq!(t.controller_of("sdc").map(|r| r.name.as_str()), Some("DC02"));
        assert_eq!(t.controller_of("sde").map(|r| r.name.as_str()), Some("DC01"));
        assert_eq!(t.resources[4].description, "sdc 953869 MiB at 0:0:0:0 on DC02");
        let dump = t.dump();
        assert!(dump.contains("DC02      QD10-330"), "{dump}");
        fs::remove_dir_all(root).unwrap();
    }

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
