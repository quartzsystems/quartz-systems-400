// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Option 2 of the attended IPL menu on installation media: pick a disk,
//! confirm, write the QS/400 layout (ESP with both slots, LIC partition,
//! ASP 1), initialize the LIC record and ASP 1, and power down so the
//! system can IPL from the disk. The minimum working installer; no
//! dedicated service tools yet.

use std::fs::{File, OpenOptions};
use std::io::Cursor;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use blockdev::{BlockDev, LinuxBlockDev};
use diskimg::locate::DiskInfo;
use diskimg::{Guids, Plan, fat, iso9660};
use licpart::{Device, LicPart, SLOT_BYTES};
use store::{Store, StoreConfig};

use crate::hwrsrc::Kind;
use crate::ipl::display::Command;
use crate::ipl::menu::read_line;
use crate::ipl::system::{System, now_unix};
use crate::ipl::{Event, power, storage};

/// Slot A's path on the ESP.
const SLOT_A: &str = "EFI/BOOT/BOOTX64.EFI";
/// Slot B's path on the ESP.
const SLOT_B: &str = "EFI/QS400/B.EFI";
/// The slot UKIs on the installation media.
const MEDIA_UKI_A: &str = "QS400A.EFI";
/// Slot B's UKI on the media.
const MEDIA_UKI_B: &str = "QS400B.EFI";

/// Title of every installer screen.
const TITLE: &str = "Install the Operating System";

/// The installer's text, kept for the console: every line goes to serial
/// as it is produced and the console shows the whole page again.
struct Page {
    lines: Vec<String>,
}

impl Page {
    /// Add a line, write it to serial and the kernel console.
    fn line(&mut self, system: &mut System, text: &str) {
        system.out.both(&format!("{text}\n"));
        self.lines.push(text.to_string());
    }

    /// Show the page on the display, with a prompt field when asked.
    fn show(&self, system: &mut System, prompt: Option<(&str, usize)>) {
        system.show(Command::ShowDialog {
            system: system.system_name(),
            title: TITLE.to_string(),
            lines: self.lines.clone(),
            prompt: prompt.map(|(label, len)| (label.to_string(), len)),
        });
    }

    /// Prompt on both outputs and wait for the answer.
    fn ask(
        &self,
        system: &mut System,
        rx: &Receiver<Event>,
        label: &str,
        len: usize,
    ) -> Option<String> {
        self.show(system, Some((label, len)));
        system.out.both(&format!("\n{label}: "));
        read_line(system, rx)
    }

    /// Show a final line and wait for Enter before going back.
    fn notice(&mut self, system: &mut System, rx: &Receiver<Event>, text: &str) {
        self.line(system, "");
        self.line(system, text);
        self.ask(system, rx, "Press Enter to return to the menu", 1);
    }
}

/// Run the installer dialog. Returns when the user backs out; a
/// finished installation powers the system down.
pub fn run(system: &mut System, rx: &Receiver<Event>) {
    let mut page = Page { lines: Vec::new() };
    system.out.both(&format!("\n{TITLE}\n\n"));
    let disks = match diskimg::locate::whole_disks() {
        Ok(d) => d,
        Err(e) => {
            page.notice(system, rx, &format!("Cannot list disk units: {e}"));
            return;
        }
    };
    if disks.is_empty() {
        page.notice(system, rx, "No disk units found.");
        return;
    }
    page.line(system, "Select the disk unit to install on:");
    page.line(system, "");
    for (i, d) in disks.iter().enumerate() {
        let resource = system
            .hw
            .resources
            .iter()
            .find(|r| r.kind == Kind::Disk && r.location == d.name)
            .map_or("-----", |r| r.name.as_str());
        let text = format!(
            "  {}. {:<6} {:<5} {:>8} MiB  {}{}",
            i + 1,
            resource,
            d.name,
            d.bytes >> 20,
            d.model,
            if d.has_qs400 {
                "  (QS/400 partitions present)"
            } else {
                ""
            }
        );
        page.line(system, &text);
    }
    let Some(choice) = page.ask(system, rx, "Selection (0 to return)", 2) else {
        return;
    };
    let Ok(n) = choice.trim().parse::<usize>() else {
        page.notice(system, rx, "Selection is not valid");
        return;
    };
    if n == 0 || n > disks.len() {
        if n != 0 {
            page.notice(system, rx, "Selection is not valid");
        }
        return;
    }
    let disk = disks[n - 1].clone();
    page.line(system, "");
    page.line(
        system,
        &format!(
            "Install on {} ({} MiB)? All data on it will be lost.",
            disk.dev.display(),
            disk.bytes >> 20
        ),
    );
    let Some(answer) = page.ask(system, rx, "Type YES to continue", 3) else {
        return;
    };
    if answer.trim() != "YES" {
        page.notice(system, rx, "Installation cancelled");
        return;
    }
    page.lines.clear();
    page.show(system, None);
    match install(system, &mut page, &disk) {
        Ok(()) => {
            page.line(system, "");
            page.line(
                system,
                "Installation complete. Remove the installation media and press Enter to power down.",
            );
            page.ask(system, rx, "Press Enter to power down", 1);
            power::down(system, false);
        }
        Err(e) => page.notice(system, rx, &format!("Installation failed: {e}")),
    }
}

/// Write the disk and initialize it, reporting progress on `page`.
fn install(system: &mut System, page: &mut Page, disk: &DiskInfo) -> Result<(), String> {
    let now = now_unix();
    let (uki_a, uki_b) = read_media_ukis(system)?;
    page.line(
        system,
        &format!(
            "Licensed Internal Code read from the media ({} and {} bytes)",
            uki_a.len(),
            uki_b.len()
        ),
    );
    page.show(system, None);

    let plan = Plan::for_bytes(disk.bytes).map_err(|e| e.to_string())?;
    let esp = fat::build(&fat::DISK_ESP, &[(SLOT_A, &uki_a), (SLOT_B, &uki_b)])
        .map_err(|e| e.to_string())?;
    let guids = Guids::from_random([random(), random(), random(), random()]);
    page.line(
        system,
        &format!(
            "Writing partition table and licensed internal code to {}",
            disk.dev.display()
        ),
    );
    page.line(
        system,
        &format!(
            "  (ESP {} MiB, LIC {} MiB, ASP 1 {} MiB)",
            plan.esp.bytes() >> 20,
            plan.lic.bytes() >> 20,
            plan.asp1.bytes() >> 20
        ),
    );
    page.show(system, None);
    release_lic_on(system, disk);
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&disk.dev)
        .map_err(|e| format!("open {}: {e}", disk.dev.display()))?;
    diskimg::write_image(&mut file, &plan, &guids, &esp).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    crate::platform::blk::reread_partitions(&file)
        .map_err(|e| format!("re-read partitions: {e}"))?;
    drop(file);
    let part = |n: u32| {
        let sep = if disk.name.ends_with(|c: char| c.is_ascii_digit()) {
            "p"
        } else {
            ""
        };
        format!("{}{sep}{n}", disk.name)
    };
    for n in [2, 3] {
        if !crate::platform::blk::wait_for_block(&part(n), Duration::from_secs(10)) {
            return Err(format!("partition {} did not appear", part(n)));
        }
    }

    // LIC record with the current system values.
    page.line(system, "Initializing the LIC partition");
    page.show(system, None);
    let lic_dev: File = OpenOptions::new()
        .read(true)
        .write(true)
        .open(format!("/dev/{}", part(2)))
        .map_err(|e| e.to_string())?;
    let (mut part_lic, mut record) = LicPart::init(lic_dev, now).map_err(|e| e.to_string())?;
    if let Some(vals) = system.sysvals.as_ref() {
        vals.store(&mut record).map_err(|e| e.to_string())?;
    }
    part_lic.write(&mut record).map_err(|e| e.to_string())?;
    drop(part_lic);

    // ASP 1 with QSYS and the system libraries.
    page.line(system, "Initializing ASP 1");
    page.show(system, None);
    let asp_path = std::path::PathBuf::from(format!("/dev/{}", part(3)));
    let asp_dev = LinuxBlockDev::open(&asp_path).map_err(|e| e.to_string())?;
    let pages = asp_dev.blocks();
    let (_, serial) = diskimg::locate::identity(&asp_path);
    let devs: Vec<Box<dyn BlockDev>> = vec![Box::new(asp_dev)];
    let mut store = Store::format(
        devs,
        &storage::format_params(now, &serial),
        StoreConfig::default(),
    )
    .map_err(|e| e.to_string())?;
    let qsys = storage::bootstrap(&mut store, now, "QS400").map_err(|e| e.to_string())?;
    store.close().map_err(|e| e.to_string())?;
    page.line(
        system,
        &format!("ASP 1 initialized: {pages} pages, QSYS is {qsys}"),
    );
    Ok(())
}

/// A D IPL next to an installed disk has that disk's LIC partition open,
/// and the kernel will not re-read a partition table while a partition
/// on the disk is open. Keep the record in memory instead.
fn release_lic_on(system: &mut System, disk: &DiskInfo) {
    let on_disk = licpart::locate()
        .ok()
        .and_then(|p| diskimg::locate::disk_of(&p))
        .is_some_and(|d| d == disk.name);
    if !on_disk {
        return;
    }
    if let Some((part, record)) = system.lic.take() {
        drop(part);
        let memory: Box<dyn Device> = Box::new(Cursor::new(vec![0u8; 2 * SLOT_BYTES]));
        system.lic = Some((LicPart::open(memory), record));
        system
            .out
            .serial("slic: LIC partition on the target disk released\n");
    }
}

/// The two slot UKIs from the installation media: the first optical
/// device whose volume is a QS/400 one.
fn read_media_ukis(system: &mut System) -> Result<(Vec<u8>, Vec<u8>), String> {
    let optical: Vec<String> = system
        .hw
        .resources
        .iter()
        .filter(|r| r.kind == Kind::Optical)
        .map(|r| r.location.clone())
        .collect();
    let mut tried = Vec::new();
    for name in &optical {
        let path = format!("/dev/{name}");
        let Ok(mut f) = File::open(&path) else {
            tried.push(format!("{path}: cannot open"));
            continue;
        };
        match iso9660::read_root(&mut f) {
            Ok((id, _)) if id.starts_with("QS400") => {
                let a = iso9660::read_file(&mut f, MEDIA_UKI_A).map_err(|e| e.to_string())?;
                let b = iso9660::read_file(&mut f, MEDIA_UKI_B).map_err(|e| e.to_string())?;
                return Ok((a, b));
            }
            Ok((id, _)) => tried.push(format!("{path}: volume {id}")),
            Err(e) => tried.push(format!("{path}: {e}")),
        }
    }
    Err(format!(
        "no QS/400 installation media found ({})",
        if tried.is_empty() {
            "no optical device".to_string()
        } else {
            tried.join("; ")
        }
    ))
}

fn random() -> [u8; 16] {
    use std::io::Read as _;
    let mut b = [0u8; 16];
    if let Ok(mut f) = File::open("/dev/urandom") {
        let _ = f.read_exact(&mut b);
    }
    b
}
