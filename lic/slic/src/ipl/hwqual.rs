// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSHWQUAL` on the machine (`lic/hwqual`): the report over the
//! hardware resource service's table (ports read again for their link)
//! or a fresh walk when the service is not up, for the installer's menu,
//! the service console's `qshwqual` and the `QSHWQUAL` command.

use std::path::Path;

use hwqual::{Report, Roots};

use crate::ipl::system::{System, now_unix};

/// The report now.
pub fn report(system: &mut System) -> Report {
    let roots = Roots::default();
    // The Secure Boot variable is read from efivarfs.
    if std::path::Path::new("/sys/firmware/efi").is_dir() {
        let _ = crate::platform::efivar::mount_vars();
    }
    let t = hwrsrc::client::table(true).unwrap_or_else(|e| {
        system.out.serial(&format!("slic: hardware resource service: {e}; walking sysfs\n"));
        hwrsrc::Table::scan(&roots.sys)
    });
    hwqual::run(&t, &roots, now_unix())
}

/// Write `text` to the first FAT USB stick; the line saying where.
///
/// # Errors
/// No stick, or the write failed.
pub fn to_usb(text: &str) -> Result<String, String> {
    hwqual::usb::write_report(Path::new("/sys"), Path::new("/dev"), text)
}

/// The service console's `qshwqual [usb]`: the report, and with `usb`
/// written to a stick as well.
pub fn command(system: &mut System, args: &[&str]) -> String {
    let r = report(system);
    let mut out = r.text();
    match args {
        [] => {}
        ["usb"] => match to_usb(&out) {
            Ok(done) => {
                out.push_str(&done);
                out.push('\n');
            }
            Err(e) => {
                out.push_str("qshwqual: ");
                out.push_str(&e);
                out.push('\n');
            }
        },
        _ => return "usage: qshwqual [usb]\n".into(),
    }
    out
}
