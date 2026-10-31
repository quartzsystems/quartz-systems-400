// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Independent verification of the final `.config`. The merge is not trusted.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, ensure};
use tracing::info;

/// Symbols that must be `=y`.
const REQUIRED_Y: &[&str] = &[
    "EFI",
    "EFI_STUB",
    "CMDLINE_BOOL",
    "CMDLINE_OVERRIDE",
    "BLK_DEV_INITRD",
    "RD_GZIP",
    "DEVTMPFS",
    "DEVTMPFS_MOUNT",
    "PROC_FS",
    "SYSFS",
    "TMPFS",
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
    "CHR_DEV_ST",
    // Tape: changers through sg, Fibre Channel HBAs.
    "CHR_DEV_SG",
    "SCSI_FC_ATTRS",
    "SCSI_QLA_FC",
    "SCSI_LPFC",
    "E1000",
    "E1000E",
    "IGB",
    "IXGBE",
    "I40E",
    "ICE",
    "MLX5_CORE",
    "MLX5_CORE_EN",
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

/// Symbols that must not be `=y`.
const REQUIRED_N: &[&str] = &[
    "MODULES",
    "MODULE_SIG",
    "DEBUG_INFO",
    "WIRELESS",
    "WLAN",
    "CFG80211",
    "BT",
    "SOUND",
    "SND",
    "MEDIA_SUPPORT",
    "RTC_HCTOSYS",
    // Tape and changer nodes must exist when the SCSI scan returns.
    "SCSI_SCAN_ASYNC",
    // Install media: plain ISO 9660 only.
    "ZISOFS",
];

/// Parse a `.config` into symbol -> value (`y`, `m`, `n`, numbers, quoted strings).
pub fn parse(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("CONFIG_") {
            if let Some((k, v)) = rest.split_once('=') {
                map.insert(k.to_string(), v.to_string());
            }
        } else if let Some(rest) = line.strip_prefix("# CONFIG_")
            && let Some(k) = rest.strip_suffix(" is not set")
        {
            map.insert(k.to_string(), "n".to_string());
        }
    }
    map
}

/// Fail unless every required symbol has the required value and the
/// command line is exactly `cmdline`. With `cpio`, also require the
/// embedded initramfs settings.
pub fn verify(config: &Path, cmdline: &str, cpio: Option<&Path>) -> Result<()> {
    let text =
        fs::read_to_string(config).with_context(|| format!("cannot read {}", config.display()))?;
    let map = parse(&text);
    let get = |k: &str| map.get(k).map_or("n", String::as_str);
    let mut problems = Vec::new();

    for sym in REQUIRED_Y {
        if get(sym) != "y" {
            problems.push(format!("CONFIG_{sym} must be y, is {}", get(sym)));
        }
    }
    for sym in REQUIRED_N {
        if get(sym) == "y" {
            problems.push(format!("CONFIG_{sym} must not be y"));
        }
    }
    let want_cmdline = format!("\"{cmdline}\"");
    if get("CMDLINE") != want_cmdline {
        problems.push(format!(
            "CONFIG_CMDLINE must be {want_cmdline}, is {}",
            get("CMDLINE")
        ));
    }
    if let Some(cpio) = cpio {
        let want = format!("\"{}\"", cpio.display());
        if get("INITRAMFS_SOURCE") != want {
            problems.push(format!(
                "CONFIG_INITRAMFS_SOURCE must be {want}, is {}",
                get("INITRAMFS_SOURCE")
            ));
        }
        if get("INITRAMFS_COMPRESSION_GZIP") != "y" {
            problems.push("CONFIG_INITRAMFS_COMPRESSION_GZIP must be y".to_string());
        }
    }

    ensure!(
        problems.is_empty(),
        "kernel: {} config problem(s) in {}:\n  {}",
        problems.len(),
        config.display(),
        problems.join("\n  ")
    );
    info!(
        "kernel: .config verified ({} symbols)",
        REQUIRED_Y.len() + REQUIRED_N.len() + 1
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn parses_set_unset_and_strings() {
        let m = parse("CONFIG_A=y\n# CONFIG_B is not set\nCONFIG_C=\"x y\"\n# comment\n");
        assert_eq!(m["A"], "y");
        assert_eq!(m["B"], "n");
        assert_eq!(m["C"], "\"x y\"");
        assert!(!m.contains_key("comment"));
    }
}
