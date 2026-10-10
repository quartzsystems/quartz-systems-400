// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Kernel source, configuration and build. `x86_64` only.

pub mod build;
pub mod config;
pub mod fetch;

use licpart::Slot;

pub use build::{SOURCE_DATE_EPOCH, config_path, kernel, uki};

/// Value of the kernel's `ARCH=` variable.
pub const KERNEL_ARCH: &str = "x86_64";
/// Base defconfig inside the kernel tree.
pub const DEFCONFIG: &str = "arch/x86/configs/x86_64_defconfig";
/// Our fragment in `kernel/config/`.
pub const FRAGMENT: &str = "qs400.config";
/// Make target that produces the EFI-bootable image.
pub const MAKE_TARGET: &str = "bzImage";
/// Where the image lands inside the build directory.
pub const KERNEL_IMAGE: &str = "arch/x86/boot/bzImage";
/// Baked kernel command line, before the slot and service-console flags.
pub const CMDLINE: &str = "console=ttyS0,115200 console=tty0 panic=-1 quiet";

/// The full command line for one UKI slot. `svc` enables the serial
/// service console (dev builds); release builds leave it out.
pub fn cmdline(slot: Slot, svc: bool) -> String {
    let slot = match slot {
        Slot::A => "A",
        Slot::B => "B",
    };
    let mut s = format!("{CMDLINE} qs400.slot={slot}");
    if svc {
        s.push_str(" qs400.svc=1");
    }
    s
}
