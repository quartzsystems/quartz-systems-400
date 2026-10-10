// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Hardware resources: a coldplug walk of sysfs plus hotplug events, with
//! IBM-style resource names assigned in a stable order. Phase 10 turns this
//! table into WRKHDWRSC.

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
            Kind::Comm => "CMN",
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

    const ALL: [Kind; 8] = [
        Kind::Bridge,
        Kind::Lan,
        Kind::Storage,
        Kind::Disk,
        Kind::Optical,
        Kind::Comm,
        Kind::Keyboard,
        Kind::Other,
    ];
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
                    "net" => self.attach_net(&name),
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
                format!("{:?}", r.kind),
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
        let i = Kind::ALL.iter().position(|k| *k == kind).unwrap_or(7);
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
