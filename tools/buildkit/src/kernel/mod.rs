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
/// Make target that produces the EFI-bootable image.
pub const MAKE_TARGET: &str = "bzImage";
/// Where the image lands inside the build directory.
pub const KERNEL_IMAGE: &str = "arch/x86/boot/bzImage";

/// Which UKI is being built. A product builds the kinds it lists and
/// gives each its command line; the two slots usually differ only in
/// the word that names the slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UkiKind {
    /// `EFI/BOOT/BOOTX64.EFI` on the disk.
    SlotA,
    /// `EFI/<VENDOR>/B.EFI` on the disk.
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
}
