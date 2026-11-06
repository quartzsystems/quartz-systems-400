// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The kernel's source offer (GPL-2.0): `QS400/SRC/` on the media holds
//! where the exact source comes from and its SHA-256, the patches (none),
//! the configuration fragment and the configuration the build resolved,
//! how to build it, and the license; `SOURCE-OFFER.txt` says the same
//! beside the release's files.

use std::fmt::Write as _;
use std::fs;

use anyhow::{Context, Result};

use crate::ctx::Ctx;
use crate::kernel::{self, CMDLINE, DEFCONFIG, FRAGMENT, KERNEL_ARCH, MAKE_TARGET, UkiKind, fetch};

/// Files by their path on the media.
pub type Files = Vec<(String, Vec<u8>)>;

/// The directory on the media.
pub const DIR: &str = "QS400/SRC";

/// What the build used, for the texts.
struct Facts {
    version: String,
    sha256: String,
    url: String,
    clang: String,
}

fn facts(ctx: &Ctx) -> Result<Facts> {
    let pin = fetch::read_pin(&ctx.paths)?;
    let sha256 = pin.sha256.context("kernel/SHA256 is missing: the source offer needs the pin")?;
    Ok(Facts {
        url: fetch::tarball_url(&pin.version),
        version: pin.version,
        sha256,
        clang: crate::image::manifest::tool_version("clang", &["--version"]),
    })
}

/// `SOURCE-OFFER.txt`. NEEDS-CODY: the three-year written offer names no
/// address yet; the release page must say where source requests go.
fn offer(f: &Facts) -> String {
    let mut s = String::new();
    let _ = write!(
        s,
        "QS/400 {rel}: source offer for the Linux kernel\n\n\
         The QS/400 install media and the UKI files of this release contain a\n\
         Linux kernel binary, licensed under the GNU General Public License,\n\
         version 2 (GPL-2.0, with the Linux syscall note). Its complete\n\
         corresponding source is:\n\n\
         1. The unmodified kernel.org release {ver}:\n\
         \x20  {url}\n\
         \x20  SHA-256 {sha}\n\
         2. No patches: QS/400 applies none to the kernel source.\n\
         3. The configuration: qs400.config (QS/400's fragment, merged onto\n\
         \x20  {DEFCONFIG}) and config-{ver}-qs400 (the configuration the\n\
         \x20  build resolved), in QS400/SRC/ on the install media.\n\
         4. The build: BUILDING-THE-KERNEL.md in the same directory.\n\n\
         Everything above is on the install media under QS400/SRC/, with the\n\
         GPL-2.0 text (COPYING, GPL-2.0.txt, Linux-syscall-note.txt). For three\n\
         years from the release of {rel}, Quartz Systems also provides this\n\
         source, the kernel tarball included, on request, for no more than the\n\
         cost of distribution.\n\n\
         Every other program on the media is QS/400's own (MIT) or a Rust\n\
         crate under a permissive license; NOTICES.txt at the root of the media\n\
         lists each with its license.\n",
        rel = release::RELEASE,
        ver = f.version,
        url = f.url,
        sha = f.sha256,
    );
    s
}

/// `BUILDING-THE-KERNEL.md`.
fn building(f: &Facts) -> String {
    let mut s = String::new();
    let _ = write!(
        s,
        "# Building the QS/400 {rel} kernel\n\n\
         The kernel is Linux {ver}, built with LLVM (`LLVM=1`), monolithic, with\n\
         the EFI stub and the initramfs (QS/400's userland) built in. The\n\
         bzImage is the UKI that boots.\n\n\
         ## Toolchain\n\n\
         - clang/LLVM: {clang}\n\
         - GNU make, flex, bison, bc, perl, and the usual kernel host tools\n\
         - QS/400's own build (`cargo run -p build -- release`) runs the steps\n\
         \x20 below; they are given here so the kernel can be rebuilt by hand.\n\n\
         ## Steps\n\n\
         ```\n\
         curl -LO {url}\n\
         echo \"{sha}  linux-{ver}.tar.xz\" | sha256sum -c\n\
         tar xf linux-{ver}.tar.xz\n\
         cd linux-{ver}\n\
         mkdir -p ../build\n\
         sh scripts/kconfig/merge_config.sh -m -Q -O ../build {DEFCONFIG} ../{FRAGMENT} ../uki.config\n\
         make O=../build LLVM=1 ARCH={KERNEL_ARCH} olddefconfig\n\
         SOURCE_DATE_EPOCH={epoch} KCONFIG_NOTIMESTAMP=1 make O=../build LLVM=1 ARCH={KERNEL_ARCH} \\\n\
         \x20   KBUILD_BUILD_USER=qs400 KBUILD_BUILD_HOST=qs400 KBUILD_BUILD_VERSION=1 \\\n\
         \x20   KBUILD_BUILD_TIMESTAMP='{stamp}' {MAKE_TARGET}\n\
         ```\n\n\
         `uki.config` embeds the initramfs and the command line:\n\n\
         ```\n\
         CONFIG_INITRAMFS_SOURCE=\"<path to the QS/400 initramfs cpio>\"\n\
         CONFIG_INITRAMFS_COMPRESSION_NONE=y\n\
         # CONFIG_INITRAMFS_COMPRESSION_XZ is not set\n\
         CONFIG_CMDLINE=\"<one of the lines below>\"\n\
         ```\n\n\
         The release has three UKIs, one per command line:\n\n",
        rel = release::RELEASE,
        ver = f.version,
        url = f.url,
        sha = f.sha256,
        clang = f.clang,
        epoch = kernel::SOURCE_DATE_EPOCH,
        stamp = kernel::build::BUILD_TIMESTAMP,
    );
    for kind in UkiKind::ALL {
        let _ = writeln!(s, "- {kind:?}: `{}`", kind.cmdline(false));
    }
    let _ = write!(
        s,
        "\nThe base line is `{CMDLINE}`. The output is `arch/x86/boot/bzImage`.\n\
         No patches are applied and no firmware is built in. `build release`\n\
         then signs each UKI for Secure Boot (an Authenticode signature\n\
         appended to the image; the kernel itself is unchanged).\n"
    );
    s
}

/// The files of `QS400/SRC/`, and the text of `SOURCE-OFFER.txt`.
pub fn files(ctx: &Ctx) -> Result<(Files, String)> {
    let f = facts(ctx)?;
    let offer = offer(&f);
    // The build machine's path to the initramfs is not part of the source.
    let resolved = fs::read_to_string(kernel::config_path(ctx))
        .context("the resolved kernel .config: build the kernel first")?
        .lines()
        .map(|l| if l.starts_with("CONFIG_INITRAMFS_SOURCE=") { "CONFIG_INITRAMFS_SOURCE=\"qs400-initramfs.cpio\"" } else { l })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let resolved = resolved.into_bytes();
    let fragment = fs::read(ctx.paths.fragments().join(FRAGMENT))?;
    let text = |id: &str| crate::notices::text(id).map(|t| t.as_bytes().to_vec()).unwrap_or_default();
    let mut out = vec![
        (format!("{DIR}/SOURCE-OFFER.txt"), offer.clone().into_bytes()),
        (format!("{DIR}/BUILDING-THE-KERNEL.md"), building(&f).into_bytes()),
        (format!("{DIR}/PATCHES.txt"), b"QS/400 applies no patches to the kernel source.\n".to_vec()),
        (format!("{DIR}/{FRAGMENT}"), fragment),
        (format!("{DIR}/config-{}-qs400", f.version), resolved),
        (format!("{DIR}/COPYING"), include_bytes!("../../licenses/linux-COPYING.txt").to_vec()),
        (format!("{DIR}/GPL-2.0.txt"), text("GPL-2.0")),
        (format!("{DIR}/Linux-syscall-note.txt"), text("Linux-syscall-note")),
    ];
    for (_, data) in &mut out {
        // One line ending whatever the checkout's.
        if !data.contains(&0) {
            *data = String::from_utf8_lossy(data).replace("\r\n", "\n").into_bytes();
        }
    }
    Ok((out, offer))
}
