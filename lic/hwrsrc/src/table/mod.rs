// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The resource table: a walk of sysfs with IBM-style names assigned in
//! a stable order, merged with later walks so a resource keeps its name
//! across hotplug, and the parents that make the packaging and logical
//! trees.

pub mod tree;

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use crate::scan::{devpath_in, nic, pci, read_trim, serial_of, sorted_dir, storage};
use crate::uevent::Uevent;

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
    /// A processor: `PNnn`.
    Processor,
    /// Main storage: `MSnn`.
    Memory,
}

impl Kind {
    /// Every kind.
    pub const ALL: [Kind; 13] = [
        Kind::Bridge,
        Kind::Lan,
        Kind::Storage,
        Kind::Disk,
        Kind::Optical,
        Kind::Comm,
        Kind::Port,
        Kind::Keyboard,
        Kind::Other,
        Kind::Tape,
        Kind::TapeLibrary,
        Kind::Processor,
        Kind::Memory,
    ];

    /// The name prefix.
    #[must_use]
    pub fn prefix(self) -> &'static str {
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
            Kind::Processor => "PN",
            Kind::Memory => "MS",
        }
    }

    fn width(self) -> usize {
        match self {
            Kind::Disk => 3,
            _ => 2,
        }
    }

    /// Whether two kinds share a name counter (ports and serial ports).
    fn shares(self, other: Kind) -> bool {
        self.prefix() == other.prefix()
    }

    /// The short text a list shows for a resource of this kind.
    #[must_use]
    pub fn text(self) -> &'static str {
        match self {
            Kind::Bridge => "PCI Bridge",
            Kind::Lan => "Network Adapter",
            Kind::Storage => "Storage Controller",
            Kind::Disk => "Disk Drive",
            Kind::Optical => "Optical Drive",
            Kind::Comm => "Serial Port",
            Kind::Port => "Ethernet Port",
            Kind::Keyboard => "Keyboard",
            Kind::Other => "Other Device",
            Kind::Tape => "Tape Drive",
            Kind::TapeLibrary => "Tape Library",
            Kind::Processor => "Processor",
            Kind::Memory => "Main Storage",
        }
    }

    /// The type and model of a resource that reports none of its own.
    #[must_use]
    pub fn default_type_model(self) -> (&'static str, &'static str) {
        match self {
            Kind::Bridge => ("QB01", "001"),
            Kind::Lan | Kind::Port => ("QN00", "001"),
            Kind::Storage => ("QD00", "000"),
            Kind::Disk => ("QU00", "001"),
            Kind::Optical => ("QO01", "001"),
            Kind::Comm => ("QS01", "001"),
            Kind::Keyboard => ("QK01", "001"),
            Kind::Other => ("QX00", "000"),
            Kind::Tape => ("QT01", "001"),
            Kind::TapeLibrary => ("QT02", "001"),
            Kind::Processor => ("QP01", "001"),
            Kind::Memory => ("QM01", "001"),
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

impl Status {
    /// As the lists show it.
    #[must_use]
    pub fn text(self) -> &'static str {
        match self {
            Status::Operational => "Operational",
            Status::NotDetected => "Not detected",
        }
    }
}

/// One row of the table.
#[derive(Clone, Debug, PartialEq)]
pub struct Resource {
    /// Resource name, e.g. `LIN01`.
    pub name: String,
    /// Class.
    pub kind: Kind,
    /// PCI address or device name.
    pub location: String,
    /// Human description.
    pub description: String,
    /// sysfs device path without the root, used to match hotplug events.
    pub devpath: String,
    /// Presence.
    pub status: Status,
    /// Type and model the device reports, for adapters and ports.
    pub type_model: Option<(String, String)>,
    /// The port, for `Kind::Port`.
    pub port: Option<nic::NetPort>,
    /// Serial number, empty when the device reports none.
    pub serial: String,
    /// The resource it sits in (packaging tree), empty at the top.
    pub parent: String,
    /// The resource that drives it (logical tree), empty at the top.
    pub logical: String,
}

impl Resource {
    /// A new operational resource.
    #[must_use]
    pub fn new(name: String, kind: Kind, location: String, description: String, devpath: String) -> Self {
        Self {
            name,
            kind,
            location,
            description,
            devpath,
            status: Status::Operational,
            type_model: None,
            port: None,
            serial: String::new(),
            parent: String::new(),
            logical: String::new(),
        }
    }

    /// Type and model, the kind's when the device reports none.
    #[must_use]
    pub fn type_model(&self) -> (String, String) {
        self.type_model.clone().unwrap_or_else(|| {
            let (t, m) = self.kind.default_type_model();
            (t.to_string(), m.to_string())
        })
    }

    /// Whether it is present now.
    #[must_use]
    pub fn operational(&self) -> bool {
        self.status == Status::Operational
    }
}

/// Storage controllers that provide device parity protection. Nothing
/// reports any in this release: hardware parity is a later item behind
/// this trait, software RAID is out of scope.
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
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    /// Rows in assignment order.
    pub resources: Vec<Resource>,
}

impl Table {
    /// Walk the sysfs at `root` once. PCI first, in address order, so
    /// names are stable from boot to boot.
    #[must_use]
    pub fn scan(root: &Path) -> Self {
        let mut t = Self::walk(root);
        t.link();
        t
    }

    /// The walk, without the parents.
    fn walk(root: &Path) -> Self {
        let mut t = Self::default();
        t.add_processors(root);
        t.add_pci(root);
        t.add_blocks(root);
        t.add_ports(root);
        for name in sorted_dir(&root.join("class/net")) {
            t.attach_net(root, &name);
        }
        for name in sorted_dir(&root.join("class/tty")) {
            t.add_tty(root, &name);
        }
        for name in sorted_dir(&root.join("class/input")) {
            t.add_input(root, &name);
        }
        t.add_tapes(root);
        t
    }

    /// Walk sysfs again and merge: a resource seen before keeps its name
    /// (matched by device path), a new one takes the next free name of
    /// its kind, one no longer there is not detected. One line per
    /// change.
    pub fn rescan(&mut self, root: &Path) -> Vec<String> {
        let fresh = Self::walk(root);
        let seen: Vec<String> = fresh.resources.iter().map(|r| r.devpath.clone()).collect();
        let mut changes = Vec::new();
        for mut r in fresh.resources {
            if let Some(old) = self
                .resources
                .iter_mut()
                .find(|o| o.devpath == r.devpath && o.kind == r.kind)
            {
                if old.status == Status::NotDetected {
                    changes.push(format!("{} detected again ({})", old.name, old.devpath));
                }
                old.status = Status::Operational;
                old.location = r.location;
                old.description = r.description;
                old.type_model = r.type_model;
                old.port = r.port;
                old.serial = r.serial;
            } else {
                r.name = self.next_name(r.kind);
                changes.push(format!("added {} ({})", r.name, r.devpath));
                self.resources.push(r);
            }
        }
        for r in &mut self.resources {
            if r.status == Status::Operational && !seen.contains(&r.devpath) {
                r.status = Status::NotDetected;
                changes.push(format!("{} not detected ({})", r.name, r.devpath));
            }
        }
        self.link();
        changes
    }

    /// Apply a hotplug event: a rescan for the subsystems that make
    /// resources. The changes, one line each.
    pub fn apply_uevent(&mut self, root: &Path, ev: &Uevent) -> Vec<String> {
        let relevant = matches!(ev.action.as_str(), "add" | "remove" | "change")
            && matches!(
                ev.subsystem.as_str(),
                "pci" | "block" | "net" | "tty" | "input" | "scsi_tape" | "scsi_generic" | "virtio" | "scsi"
            );
        if relevant { self.rescan(root) } else { Vec::new() }
    }

    /// A resource by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Resource> {
        self.resources.iter().find(|r| r.name.eq_ignore_ascii_case(name))
    }

    /// The table as text, for the service console.
    #[must_use]
    pub fn dump(&self) -> String {
        let mut s = String::from(
            "Resource  Type      Status        Location          Parent    Description\n",
        );
        for r in &self.resources {
            let (t, m) = r.type_model();
            let _ = writeln!(
                s,
                "{:<9} {:<9} {:<13} {:<17} {:<9} {}",
                r.name,
                format!("{t}-{m}"),
                format!("{:?}", r.status),
                r.location,
                r.parent,
                r.description
            );
        }
        s
    }

    /// The next free name of a kind: one past the highest in use.
    fn next_name(&self, kind: Kind) -> String {
        let n = self
            .resources
            .iter()
            .filter(|r| r.kind.shares(kind) && r.name.starts_with(kind.prefix()))
            .filter_map(|r| r.name[kind.prefix().len()..].parse::<u32>().ok())
            .max()
            .unwrap_or(0);
        format!("{}{:0width$}", kind.prefix(), n + 1, width = kind.width())
    }

    fn add(&mut self, kind: Kind, location: String, description: String, devpath: String) -> &mut Resource {
        let name = self.next_name(kind);
        self.resources
            .push(Resource::new(name, kind, location, description, devpath));
        self.resources.last_mut().expect("just pushed")
    }

    /// Set the packaging and logical parents, and the controller in the
    /// description of the units behind one.
    pub fn link(&mut self) {
        let paths: Vec<(String, String)> = self
            .resources
            .iter()
            .map(|r| (r.name.clone(), r.devpath.clone()))
            .collect();
        let parent_of = |devpath: &str| {
            paths
                .iter()
                .filter(|(_, p)| tree::under(devpath, p))
                .max_by_key(|(_, p)| p.len())
                .map(|(n, _)| n.clone())
                .unwrap_or_default()
        };
        let ctl_at = |t: &Table, addr: &str| {
            t.resources
                .iter()
                .find(|r| r.kind == Kind::Storage && r.location == addr)
                .map(|r| r.name.clone())
        };
        let lan_at = |t: &Table, addr: &str| {
            t.resources
                .iter()
                .find(|r| r.kind == Kind::Lan && r.location == addr)
                .map(|r| r.name.clone())
        };
        let mut links = Vec::new();
        for r in &self.resources {
            let parent = parent_of(&r.devpath);
            let logical = match r.kind {
                Kind::Port => r
                    .port
                    .as_ref()
                    .and_then(|p| lan_at(self, &p.pci))
                    .unwrap_or_else(|| parent.clone()),
                Kind::Disk | Kind::Optical | Kind::Tape | Kind::TapeLibrary => {
                    pci::address_in(&r.devpath)
                        .and_then(|a| ctl_at(self, &a))
                        .unwrap_or_else(|| parent.clone())
                }
                _ => parent.clone(),
            };
            links.push((parent, logical));
        }
        for (r, (parent, logical)) in self.resources.iter_mut().zip(links) {
            if matches!(r.kind, Kind::Disk | Kind::Optical)
                && !logical.is_empty()
                && !r.description.contains(" on ")
            {
                let _ = write!(r.description, " on {logical}");
            }
            r.parent = parent;
            r.logical = logical;
        }
    }

    /// The processors and main storage under `root`: one `PNnn` per
    /// online CPU, one `MS01` for memory.
    pub fn add_processors(&mut self, root: &Path) {
        let cpus = root.join("devices/system/cpu");
        let mut ids: Vec<u32> = sorted_dir(&cpus)
            .iter()
            .filter_map(|n| n.strip_prefix("cpu").and_then(|d| d.parse().ok()))
            .collect();
        ids.sort_unstable();
        for id in ids {
            let dir = cpus.join(format!("cpu{id}"));
            let Some(devpath) = devpath_in(root, &dir) else { continue };
            self.add(Kind::Processor, format!("cpu{id}"), format!("processor {id}"), devpath);
        }
        let mem = root.join("devices/system/memory");
        if mem.is_dir()
            && let Some(devpath) = devpath_in(root, &mem)
        {
            let blocks = sorted_dir(&mem)
                .iter()
                .filter(|n| n.starts_with("memory") && read_trim(&mem.join(n).join("online")) == "1")
                .count();
            let size = u64::from_str_radix(read_trim(&mem.join("block_size_bytes")).trim_start_matches("0x"), 16)
                .unwrap_or(0);
            let mib = size * blocks as u64 / (1024 * 1024);
            self.add(Kind::Memory, "memory".into(), format!("main storage {mib} MiB"), devpath);
        }
    }

    /// Tape drives (`class/scsi_tape`) and changers (SCSI type 8 under
    /// `class/scsi_generic`), named `TAPnn` and `TAPMLBnn` by
    /// `tape::device::naming` in SCSI address order.
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
        let entries = sorted_dir(&class("scsi_tape"));
        for node in tape_nodes(&entries) {
            let dev = class("scsi_tape").join(&node).join("device");
            if let Some(hctl) = hctl_of(&dev) {
                scanned.push(Scanned { kind: ResourceKind::Tape, node, hctl, serial: serial(&dev) });
            }
        }
        for node in sorted_dir(&class("scsi_generic")) {
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
            let devpath = devpath_in(root, &class(sub).join(&r.device.node)).unwrap_or_default();
            let [h, c, t, l] = r.device.hctl;
            let what = if kind == Kind::Tape { "tape drive" } else { "tape media changer" };
            let mut res = Resource::new(
                r.name,
                kind,
                r.device.node.clone(),
                format!("{what} {} at {h}:{c}:{t}:{l}", r.device.node),
                devpath,
            );
            res.serial = r.device.serial.clone().unwrap_or_default();
            self.resources.push(res);
        }
    }

    /// Every PCI function under `root`, in address order.
    pub fn add_pci(&mut self, root: &Path) {
        for dev in pci::scan_in(root) {
            self.add_function(root, dev);
        }
    }

    /// One PCI function; a storage controller keeps its type and model.
    fn add_function(&mut self, root: &Path, dev: pci::PciDevice) -> String {
        let tm = dev.type_model;
        let sys = root.join("bus/pci/devices").join(&dev.address);
        let r = self.add(dev.kind, dev.address, dev.description, dev.devpath);
        r.type_model = tm.map(|(t, m)| (t.to_string(), m.to_string()));
        r.serial = serial_of(&sys);
        r.name.clone()
    }

    /// Whole disks and optical drives under `root`, named in the order
    /// of `storage::disk_key`: by controller, then SCSI address, so a
    /// disk keeps its `DDnnn` from boot to boot whatever the kernel
    /// calls it.
    pub fn add_blocks(&mut self, root: &Path) {
        let dir = root.join("class/block");
        let mut names: Vec<(_, String)> = sorted_dir(&dir)
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
            || ["loop", "ram", "zram", "dm-", "md"].iter().any(|p| name.starts_with(p))
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
        let desc = format!("{name} {mib} MiB{at}");
        let serial = serial_of(&sys.join("device"));
        let serial = if serial.is_empty() { serial_of(&sys) } else { serial };
        let r = self.add(kind, name.to_string(), desc, devpath);
        r.serial = serial;
        Some(r.name.clone())
    }

    /// The storage controller at a PCI address.
    fn controller_at(&self, address: &str) -> Option<&Resource> {
        self.resources
            .iter()
            .find(|r| r.kind == Kind::Storage && r.location == address)
    }

    /// The storage controller a disk (`sda`) is attached through.
    #[must_use]
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

    fn push_port(&mut self, p: nic::NetPort) -> String {
        let tm = (p.family.type_code.to_string(), p.family.model.to_string());
        if let Some(lan) = self
            .resources
            .iter_mut()
            .find(|r| r.kind == Kind::Lan && r.location == p.pci)
        {
            lan.type_model = Some(tm.clone());
        }
        let location = if p.dev_port > 0 {
            format!("{}/{}", p.pci, p.dev_port)
        } else {
            p.pci.clone()
        };
        let r = self.add(Kind::Port, location, p.description(), p.devpath.clone());
        r.type_model = Some(tm);
        r.serial = p.mac.replace(':', "").to_ascii_uppercase();
        r.port = Some(p);
        r.name.clone()
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
    fn attach_net(&mut self, root: &Path, name: &str) -> Option<String> {
        let devpath = devpath_in(root, &root.join("class/net").join(name))?;
        let addr = pci::address_in(&devpath)?;
        let lan = self
            .resources
            .iter_mut()
            .find(|r| r.kind == Kind::Lan && r.location == addr)?;
        let _ = write!(lan.description, ", port {name}");
        Some(lan.name.clone())
    }

    /// Serial ports with real hardware behind them.
    fn add_tty(&mut self, root: &Path, name: &str) -> Option<String> {
        let sys = root.join("class/tty").join(name);
        if !name.starts_with("ttyS")
            || !sys.join("device").exists()
            || read_trim(&sys.join("type")) == "0"
        {
            return None;
        }
        let devpath = devpath_in(root, &sys)?;
        let desc = format!("serial port {name}");
        Some(self.add(Kind::Comm, name.to_string(), desc, devpath).name.clone())
    }

    /// Input devices that report keys.
    fn add_input(&mut self, root: &Path, name: &str) -> Option<String> {
        if !name.starts_with("input") {
            return None;
        }
        let sys = root.join("class/input").join(name);
        let ev = read_trim(&sys.join("capabilities/ev"));
        let bits = u64::from_str_radix(&ev, 16).unwrap_or(0);
        if bits & 0b10 == 0 {
            return None; // no EV_KEY
        }
        let devpath = devpath_in(root, &sys)?;
        let desc = read_trim(&sys.join("name"));
        let kind = if desc.to_ascii_lowercase().contains("keyboard") {
            Kind::Keyboard
        } else {
            Kind::Other
        };
        Some(self.add(kind, name.to_string(), desc, devpath).name.clone())
    }
}

#[cfg(test)]
mod tests;
