// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QS/400 as `buildkit` sees it (docs/BUILDKIT.md): the names, the
//! kernel's needs, the userland, the initramfs of each UKI, the command
//! lines, and the steps that are QS/400's own.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use buildkit::host::doctor::{Row, Status};
use buildkit::image::cpio::Entry;
use buildkit::image::initramfs::binary;
use buildkit::{Ctx, Notices, Package, Product, Standalone, UkiKind};
use tracing::info;

/// Baked kernel command line, before the slot and service-console flags.
pub const CMDLINE: &str = "console=ttyS0,115200 console=tty0 panic=-1 quiet";
/// The kernel configuration fragment in `kernel/config/`.
pub const FRAGMENT: &str = "qs400.config";
/// Where the generated notices file lives in the tree.
pub const NOTICES_FILE: &str = "lic/osobjects/shipped/NOTICES.txt";

/// QS/400, from this crate's place in the repository.
pub fn qs400() -> Result<Product> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .context("build crate is not under tools/")?
        .to_path_buf();
    Ok(Product {
        id: "qs400",
        name: "QS/400",
        vendor: "QS400",
        env_prefix: "QS400_",
        key_prefix: "qs400.",
        repo,
        release: release::RELEASE.to_string(),
        build_id: release::build_id().to_string(),
        fragments: &[FRAGMENT],
        required_y: REQUIRED_Y,
        required_n: REQUIRED_N,
        cmdline: CMDLINE,
        packages: &[
            Package {
                name: "slic",
                manifest: true,
            },
            Package {
                name: "qsjob",
                manifest: true,
            },
            // The media UKI's loader.
            Package {
                name: "instload",
                manifest: false,
            },
        ],
        init: "slic",
        kinds: &UkiKind::ALL,
        uki_cmdline,
        initramfs,
        disk_bytes: diskimg::DEFAULT_DISK_BYTES,
        // The ESP, the LIC partition and ASP 1 (`diskimg::Plan`); the IFS
        // is made in ASP 1 at the first IPL.
        disk_parts: diskimg::Plan::parts,
        disk_guids: || diskimg::Guids::FIXED.layout(),
        media: Some(crate::image::media::media),
        doctor,
        deny: &[Standalone {
            manifest: "tools/smbtest/Cargo.toml",
            // `rsa` comes only through the `smb` client's sspi/picky, which pin a
            // release candidate; the host test client signs on with NTLM only.
            ignore: &[(
                "RUSTSEC-2023-0071",
                "rsa only via sspi/picky in the host test client tools/smbtest (NTLM, no RSA use)",
            )],
        }],
        notices: Some(Notices {
            file: NOTICES_FILE,
            intro: notices_intro,
        }),
        secure_boot_db: || pesign::DbKey::from_env().ok().map(|k| k.cert),
    })
}

/// Slots A and B differ only in `qs400.slot=`; the media UKI boots from
/// the ISO as an attended IPL type D, so the "IPL or Install the System"
/// menu is shown. `dev` enables the serial service console; release
/// builds leave it out.
fn uki_cmdline(p: &Product, kind: UkiKind, dev: bool) -> String {
    let base = p.cmdline;
    let mut s = match kind {
        UkiKind::SlotA => format!("{base} {}=A", p.key("slot")),
        UkiKind::SlotB => format!("{base} {}=B", p.key("slot")),
        UkiKind::Media => format!(
            "{base} {}=D {}=iso {}=manual",
            p.key("ipl"),
            p.key("media"),
            p.key("mode")
        ),
    };
    if dev {
        let _ = write!(s, " {}=1", p.key("svc"));
    }
    s
}

/// The initramfs: `/init` (slic, which is also the storage service),
/// `/qsjob` (the job process), mount points, a console node and
/// `/qs400/keys/system.pub`: the public keys of the signers the system
/// trusts, the build key first, 32 bytes each; they check signed objects,
/// PTFs and license keys (`docs/LICPGM.md` §7, `docs/PTF.md` §6). The
/// media's `/init` is the loader, which takes slic and the job program
/// from the media (docs/INSTALL.md, "The media UKI").
fn initramfs(ctx: &Ctx, kind: UkiKind) -> Result<Vec<Entry>> {
    let keys = trusted_keys(ctx)?;
    let (init, job) = match kind {
        UkiKind::SlotA | UkiKind::SlotB => (binary(ctx, "slic")?, Some(binary(ctx, "qsjob")?)),
        UkiKind::Media => (binary(ctx, "instload")?, None),
    };
    // The kernel does not auto-mount devtmpfs for an initramfs, so a static
    // console node lets it open the initial console before slic runs.
    let mut v = vec![
        Entry::dir("dev"),
        Entry::chardev("dev/console", 5, 1),
        Entry::file("init", init),
        Entry::dir("proc"),
        Entry::dir("qs400"),
        Entry::dir("qs400/keys"),
        Entry::file("qs400/keys/system.pub", keys),
    ];
    v.extend(job.map(|j| Entry::file("qsjob", j)));
    v.extend([Entry::dir("run"), Entry::dir("sys")]);
    Ok(v)
}

/// Third-party signers the image trusts: a file of ed25519 public keys,
/// 64 hex digits each, one per line (`#` comments).
const TRUSTED_ENV: &str = "QS400_TRUSTED_KEYS";

/// `/qs400/keys/system.pub` (docs/PTF.md section 6): the build key's
/// public key, then each key `QS400_TRUSTED_KEYS` lists; 32 bytes each.
fn trusted_keys(ctx: &Ctx) -> Result<Vec<u8>> {
    let mut out = buildkit::image::manifest::signing_key(ctx)?
        .verifying_key()
        .to_bytes()
        .to_vec();
    if let Some(path) = std::env::var_os(TRUSTED_ENV) {
        let text = fs::read_to_string(&path).with_context(|| {
            format!(
                "cannot read {TRUSTED_ENV}={}",
                PathBuf::from(&path).display()
            )
        })?;
        for line in text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
        {
            let k =
                hex::decode(line).with_context(|| format!("{TRUSTED_ENV}: {line:?} is not hex"))?;
            ensure!(
                k.len() == 32,
                "{TRUSTED_ENV}: {line:?} is not a 32-byte key"
            );
            if !out.chunks(32).any(|c| c == k.as_slice()) {
                out.extend_from_slice(&k);
            }
        }
    }
    info!("initramfs: {} trusted signer(s)", out.len() / 32);
    Ok(out)
}

/// The firmware the QEMU suites boot with.
fn doctor(_: &Ctx) -> Vec<Row> {
    let name = "OVMF (QS400_OVMF_CODE)".to_string();
    vec![match conformance::firmware::locate() {
        Ok(p) => Row {
            name,
            required: true,
            status: Status::Ok,
            detail: p.display().to_string(),
        },
        Err(e) => Row {
            name,
            required: true,
            status: Status::Missing,
            detail: e.to_string(),
        },
    }]
}

/// The opening lines of `NOTICES.txt`.
fn notices_intro(release: &str, kernel_version: &str, crates: usize) -> String {
    format!(
        "QS/400 {release} third-party notices\n\
         \nQS/400 is licensed under the MIT license (the LICENSE file of its source).\n\
         It contains the work of others, listed here with their licenses:\n\n\
         1. The Linux kernel {kernel_version}, under GPL-2.0 with the Linux syscall note.\n\
         \x20  The kernel's source offer is QS400/SRC/ on the install media and\n\
         \x20  SOURCE-OFFER.txt beside each release. No firmware is built into the\n\
         \x20  kernel (CONFIG_EXTRA_FIRMWARE is empty) and the media carry none.\n\
         2. musl libc, under MIT: Copyright (c) 2005-2020 Rich Felker, et al.\n\
         \x20  musl's COPYRIGHT file names its authors and the few files it holds\n\
         \x20  under other permissive terms.\n\
         3. The Rust standard library (core, alloc, std and their dependencies),\n\
         \x20  under MIT OR Apache-2.0, taken under MIT; LLVM's libunwind and\n\
         \x20  compiler-rt builtins, under Apache-2.0 WITH LLVM-exception.\n\
         4. The {crates} Rust crates below, each under the license named.\n"
    )
}

/// Symbols QS/400 needs `=y`, past the pipeline's baseline.
pub const REQUIRED_Y: &[&str] = &[
    "CGROUPS",
    "MEMCG",
    "CGROUP_SCHED",
    "CGROUP_PIDS",
    "NAMESPACES",
    // PASE (qsh/pase): user, mount and PID namespaces.
    "USER_NS",
    "PID_NS",
    "SECCOMP",
    "SECCOMP_FILTER",
    "IO_URING",
    "NET",
    "INET",
    "IPV6",
    "UNIX",
    "NETLINK_DIAG",
    "INET_DIAG",
    "INET_TCP_DIAG",
    "INET_UDP_DIAG",
    "INET_DIAG_DESTROY",
    "PACKET",
    "BONDING",
    "VLAN_8021Q",
    "NETFILTER",
    "NF_TABLES",
    "NF_TABLES_INET",
    "NF_TABLES_IPV4",
    "NF_TABLES_IPV6",
    "NFT_SOCKET",
    "TLS",
    "CRYPTO_AES",
    "CRYPTO_GCM",
    "CRYPTO_CHACHA20POLY1305",
    "NET_SCHED",
    "DUMMY",
    "VIRTIO_PCI",
    "VIRTIO_BLK",
    "VIRTIO_NET",
    "VIRTIO_CONSOLE",
    "BLK_DEV_NVME",
    "SATA_AHCI",
    "FUSION_SPI",
    "FUSION_SAS",
    "VMWARE_PVSCSI",
    "ATA_PIIX",
    "SCSI",
    "BLK_DEV_SD",
    // The IFS volume (lic/ublk).
    "BLK_DEV_UBLK",
    "CHR_DEV_ST",
    // Tape: changers through sg, Fibre Channel HBAs.
    "CHR_DEV_SG",
    "SCSI_FC_ATTRS",
    "SCSI_QLA_FC",
    "SCSI_LPFC",
    // SAS and RAID HBAs: PERC and LSI MegaRAID, LSI IT mode, Microchip
    // and HPE, Adaptec, Broadcom 9600.
    "MEGARAID_SAS",
    "SCSI_MPT3SAS",
    "SCSI_SMARTPQI",
    "SCSI_HPSA",
    "SCSI_AACRAID",
    "SCSI_MPI3MR",
    "E1000",
    "E1000E",
    "IGB",
    "IXGBE",
    "I40E",
    "ICE",
    "MLX5_CORE",
    "MLX5_CORE_EN",
    "IGC",
    "TIGON3",
    "BNXT",
    "R8169",
    "VMXNET3",
    "SCSI_VIRTIO",
    "HYPERV",
    "HYPERV_NET",
    "HYPERV_STORAGE",
    "HYPERV_KEYBOARD",
    // The guest agent's channels.
    "VSOCKETS",
    "VIRTIO_VSOCKETS",
    "HYPERV_VSOCKETS",
    "VMWARE_VMCI_VSOCKETS",
    "TCG_TPM",
    "TCG_TIS",
    "TCG_CRB",
    "USB_UAS",
    "XFS_FS",
    "EXT4_FS",
    "VFAT_FS",
    "NLS_CODEPAGE_437",
    "NLS_ISO8859_1",
    // Install media.
    "ISO9660_FS",
    "JOLIET",
    "BLK_DEV_SR",
    "USB_STORAGE",
    "FB",
    "FRAMEBUFFER_CONSOLE",
    "DRM_SIMPLEDRM",
    "DRM_FBDEV_EMULATION",
    "VT",
    "VT_CONSOLE",
    "SERIAL_8250",
    "SERIAL_8250_CONSOLE",
    "KEYBOARD_ATKBD",
    "INPUT_EVDEV",
    "HID",
    "USB_HID",
    "HW_RANDOM",
    "RTC_CLASS",
    "RTC_DRV_CMOS",
    "DMIID",
];

/// Symbols QS/400 must not have `=y`, past the baseline.
pub const REQUIRED_N: &[&str] = &[
    "WIRELESS",
    "WLAN",
    "CFG80211",
    "BT",
    "SOUND",
    "SND",
    "MEDIA_SUPPORT",
    "RTC_HCTOSYS",
    // Tape and changer nodes must exist when the SCSI scan returns, and
    // disks behind an HBA keep their order from boot to boot.
    "SCSI_SCAN_ASYNC",
    // Install media: plain ISO 9660 only.
    "ZISOFS",
];
