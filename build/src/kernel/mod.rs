// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Kernel source, configuration and build. `x86_64` only.

pub mod build;
pub mod config;
pub mod fetch;

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

/// Which UKI is being built. Slots A and B differ only in `qs400.slot=`;
/// the media UKI boots from the ISO as an IPL type D.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UkiKind {
    /// `EFI/BOOT/BOOTX64.EFI` on the disk.
    SlotA,
    /// `EFI/QS400/B.EFI` on the disk.
    SlotB,
    /// `EFI/BOOT/BOOTX64.EFI` on the ISO's boot image.
    Media,
}

impl UkiKind {
    /// All three, in build order.
    pub const ALL: [UkiKind; 3] = [UkiKind::SlotA, UkiKind::SlotB, UkiKind::Media];

    /// Letter used in file names.
    pub fn tag(self) -> &'static str {
        match self {
            UkiKind::SlotA => "a",
            UkiKind::SlotB => "b",
            UkiKind::Media => "d",
        }
    }

    /// Pointer file name under `target/images/`.
    pub fn pointer(self) -> &'static str {
        match self {
            UkiKind::SlotA => "uki-a.path",
            UkiKind::SlotB => "uki-b.path",
            UkiKind::Media => "uki-d.path",
        }
    }

    /// Manifest kind.
    pub fn manifest_kind(self) -> &'static str {
        match self {
            UkiKind::SlotA => "uki-a",
            UkiKind::SlotB => "uki-b",
            UkiKind::Media => "uki-d",
        }
    }

    /// The full command line. `svc` enables the serial service console
    /// (dev builds); release builds leave it out.
    pub fn cmdline(self, svc: bool) -> String {
        let mut s = match self {
            UkiKind::SlotA => format!("{CMDLINE} qs400.slot=A"),
            UkiKind::SlotB => format!("{CMDLINE} qs400.slot=B"),
            UkiKind::Media => format!("{CMDLINE} qs400.ipl=D qs400.media=iso"),
        };
        if svc {
            s.push_str(" qs400.svc=1");
        }
        s
    }
}
