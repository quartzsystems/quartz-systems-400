// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Option 2 of the attended IPL menu on installation media: pick a disk,
//! confirm, write the QS/400 layout (ESP with both slots, LIC partition,
//! ASP 1), initialize the LIC record and ASP 1, and power down so the
//! system can IPL from the disk. The minimum working installer; no
//! dedicated service tools yet.

use std::fs::{File, OpenOptions};
use std::sync::mpsc::Receiver;
use std::time::Duration;

use blockdev::{BlockDev, LinuxBlockDev};
use diskimg::locate::DiskInfo;
use diskimg::{Guids, Plan, fat, iso9660};
use licpart::LicPart;
use store::{Store, StoreConfig};

use crate::hwrsrc::Kind;
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

/// Run the installer dialog. Returns when the user backs out; a
/// finished installation powers the system down.
pub fn run(system: &mut System, rx: &Receiver<Event>) {
    system.out.both("\nInstall the Operating System\n\n");
    let disks = match diskimg::locate::whole_disks() {
        Ok(d) => d,
        Err(e) => {
            system.out.both(&format!("Cannot list disk units: {e}\n"));
            return;
        }
    };
    if disks.is_empty() {
        system.out.both("No disk units found.\n");
        return;
    }
    system.out.both("Select the disk unit to install on:\n\n");
    for (i, d) in disks.iter().enumerate() {
        let resource = system
            .hw
            .resources
            .iter()
            .find(|r| r.kind == Kind::Disk && r.location == d.name)
            .map_or("-----", |r| r.name.as_str());
        system.out.both(&format!(
            "  {}. {:<6} {:<5} {:>8} MiB  {}{}\n",
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
        ));
    }
    system.out.both("\nSelection (0 to return): ");
    let Some(choice) = read_line(system, rx) else {
        return;
    };
    let Ok(n) = choice.trim().parse::<usize>() else {
        system.out.both("\nSelection is not valid\n");
        return;
    };
    if n == 0 || n > disks.len() {
        if n != 0 {
            system.out.both("\nSelection is not valid\n");
        }
        return;
    }
    let disk = disks[n - 1].clone();
    system.out.both(&format!(
        "\nInstall on {} ({} MiB)? All data on it will be lost.\nType YES to continue: ",
        disk.dev.display(),
        disk.bytes >> 20
    ));
    let Some(answer) = read_line(system, rx) else {
        return;
    };
    if answer.trim() != "YES" {
        system.out.both("\nInstallation cancelled\n");
        return;
    }
    match install(system, &disk) {
        Ok(()) => {
            system.out.both(
                "\nInstallation complete. Remove the installation media and press Enter to power down.\n",
            );
            let _ = read_line(system, rx);
            power::down(system, false);
        }
        Err(e) => system.out.both(&format!("\nInstallation failed: {e}\n")),
    }
}

/// Write the disk and initialize it.
fn install(system: &mut System, disk: &DiskInfo) -> Result<(), String> {
    let now = now_unix();
    let (uki_a, uki_b) = read_media_ukis(system)?;
    system.out.both(&format!(
        "Licensed Internal Code read from the media ({} and {} bytes)\n",
        uki_a.len(),
        uki_b.len()
    ));

    let plan = Plan::for_bytes(disk.bytes).map_err(|e| e.to_string())?;
    let esp = fat::build(&fat::DISK_ESP, &[(SLOT_A, &uki_a), (SLOT_B, &uki_b)])
        .map_err(|e| e.to_string())?;
    let guids = Guids::from_random([random(), random(), random(), random()]);
    system.out.both(&format!(
        "Writing partition table and licensed internal code to {} (ESP {} MiB, LIC {} MiB, ASP 1 {} MiB)\n",
        disk.dev.display(),
        plan.esp.bytes() >> 20,
        plan.lic.bytes() >> 20,
        plan.asp1.bytes() >> 20
    ));
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
    system.out.both("Initializing the LIC partition\n");
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
    system.out.both("Initializing ASP 1\n");
    let asp_dev = LinuxBlockDev::open(std::path::Path::new(&format!("/dev/{}", part(3))))
        .map_err(|e| e.to_string())?;
    let pages = asp_dev.blocks();
    let devs: Vec<Box<dyn BlockDev>> = vec![Box::new(asp_dev)];
    let mut store = Store::format(devs, &storage::format_params(now), StoreConfig::default())
        .map_err(|e| e.to_string())?;
    let qsys = storage::bootstrap(&mut store, now, "QS400").map_err(|e| e.to_string())?;
    store.close().map_err(|e| e.to_string())?;
    system.out.both(&format!(
        "ASP 1 initialized: {pages} pages, QSYS is {qsys}\n"
    ));
    Ok(())
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
