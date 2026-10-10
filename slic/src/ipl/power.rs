// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Power down: D1001000, LIC bookkeeping, sync, unmount, D1001090, reboot.

use crate::ipl::lic;
use crate::ipl::park;
use crate::ipl::system::System;
use crate::platform::power;
use crate::src;

/// Power off, or restart. Never returns.
pub fn down(system: &mut System, restart: bool) -> ! {
    system.emit(&src::PWRDWN_STARTED);
    lic::shutdown(system, &src::PWRDWN_STARTED);
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
