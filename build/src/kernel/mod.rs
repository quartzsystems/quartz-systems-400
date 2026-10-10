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
/// Baked kernel command line.
pub const CMDLINE: &str = "console=ttyS0,115200 console=tty0 panic=-1 quiet";
