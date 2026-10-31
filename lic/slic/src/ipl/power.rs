// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Power down: D1001000, LIC bookkeeping, the IFS, sync, unmount,
//! D1001090, reboot.

use crate::ipl::ifs;
use crate::ipl::lic;
use crate::ipl::park;
use crate::ipl::storage;
use crate::ipl::system::System;
use crate::platform::power;
use crate::src;

/// Where the storage service leaves a job's `PWRDWNSYS` options
/// (`immediate delay restart`) before signalling `SIGUSR1`.
pub const REQUEST_FILE: &str = "/run/qs400/pwrdwn";

/// Power off, or restart. Never returns.
pub fn down(system: &mut System, restart: bool) -> ! {
    down_ending(system, restart, true, 0, 0)
}

/// `SIGUSR1`: a power-down a job asked the service for, with its
/// options when the request file holds them (`immediate delay restart
/// iplsrc`); else a plain power off.
pub fn requested(system: &mut System) -> ! {
    let text = std::fs::read_to_string(REQUEST_FILE).unwrap_or_default();
    let _ = std::fs::remove_file(REQUEST_FILE);
    let mut it = text.split_whitespace();
    let immediate = it.next() == Some("1");
    let delay: u32 = it.next().and_then(|d| d.parse().ok()).unwrap_or(0);
    let restart = it.next() == Some("1");
    let iplsrc: u8 = it.next().and_then(|d| d.parse().ok()).unwrap_or(0);
    if text.trim().is_empty() {
        down(system, false)
    } else {
        down_ending(system, restart, immediate, delay, iplsrc)
    }
}

/// The slot the next IPL boots once: `IPLSRC(A|B)`, else a LIC PTF's
/// waiting one (`docs/PTF.md` §5). The record was just refreshed.
fn arm_slot(system: &mut System, iplsrc: u8) {
    let named = match iplsrc {
        1 => Some(licpart::Slot::A),
        2 => Some(licpart::Slot::B),
        _ => None,
    };
    let Some((_, record)) = system.lic.as_ref() else {
        return;
    };
    match crate::cpp::ptflic::arm(record, named) {
        Ok(Some(s)) => system.out.serial(&format!("slic: next IPL from slot {s:?}\n")),
        Ok(None) => {}
        Err(e) => system.out.serial(&format!("slic: IPL source not set: {e}\n")),
    }
}

/// `PWRDWNSYS`: power down with the work ending controlled within
/// `delay` seconds, or at once. A restart is `QIPLSTS` 2 next time;
/// `iplsrc` 1 or 2 boots slot A or B once.
pub fn down_ending(system: &mut System, restart: bool, immediate: bool, delay: u32, iplsrc: u8) -> ! {
    system.emit(&src::PWRDWN_STARTED);
    // The console's session ends first; its jobs end with their
    // subsystems.
    crate::ipl::link::disconnect();
    if restart && let Some(s) = system.procs.stg.as_mut() {
        let _ = stg::Client::next_iplsts(s, 2);
    }
    crate::procs::end_all(system, immediate, delay);
    storage::close(system);
    // The service has ended, so nothing holds the IFS any more.
    ifs::unmount(system);
    lic::shutdown(system, &src::PWRDWN_STARTED);
    arm_slot(system, iplsrc);
    if let Some((part, _)) = system.lic.take() {
        drop(part); // closes the partition before unmounting /dev
    }
    power::sync();
    for (target, e) in power::unmount_all() {
        system.out.serial(&format!("slic: unmount {target}: {e}\n"));
    }
    system.emit(&src::PWRDWN_COMPLETE);
    let e = power::reboot_now(restart);
    system.out.serial(&format!("slic: reboot: {e}\n"));
    park()
}
