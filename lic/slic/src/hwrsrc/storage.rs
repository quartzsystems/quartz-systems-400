// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Storage controllers by PCI ID, with a Quartz type and model for the
//! `DCnn` resource, and the order disks are named in: controller by PCI
//! address, then SCSI host:channel:target:lun.

/// What a storage controller is called and shown as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Controller {
    /// `WRKHDWRSC` type.
    pub type_code: &'static str,
    /// Model.
    pub model: &'static str,
    /// Readable name.
    pub name: &'static str,
}

/// `(vendor, device, subsystem)`: a `None` subsystem matches any. The
/// subsystem rows come first so the board wins over the chip. IDs as
/// the public pci.ids lists them.
type Row = (&'static str, &'static str, Option<(&'static str, &'static str)>, Controller);

const fn c(type_code: &'static str, model: &'static str, name: &'static str) -> Controller {
    Controller {
        type_code,
        model,
        name,
    }
}

/// Broadcom/LSI `MegaRAID`, the Dell PERC boards (`megaraid_sas`).
const MEGARAID: &str = "QD10";
/// Broadcom/LSI Fusion-MPT SAS IT mode, the Dell HBA330 (`mpt3sas`).
const MPT3SAS: &str = "QD11";
/// Microchip/Adaptec `SmartRAID` and HPE Smart Array in PQI mode (`smartpqi`).
const SMARTPQI: &str = "QD12";
/// Older HPE Smart Array (`hpsa`).
const HPSA: &str = "QD13";
/// Adaptec (`aacraid`).
const AACRAID: &str = "QD14";
/// Broadcom 9600 (`mpi3mr`).
const MPI3MR: &str = "QD15";

const TABLE: &[Row] = &[
    // Dell PERC on MegaRAID SAS-3 3008 [Fury].
    ("1000", "005f", Some(("1028", "1f44")), c(MEGARAID, "330", "PERC H330 Adapter")),
    ("1000", "005f", Some(("1028", "1f4b")), c(MEGARAID, "330", "PERC H330 Mini")),
    ("1000", "005f", Some(("1028", "1f4c")), c(MEGARAID, "330", "PERC H330 Embedded")),
    ("1000", "005f", Some(("1028", "1f4d")), c(MEGARAID, "330", "PERC H330 Mini (blades)")),
    // On MegaRAID SAS-3 3108 [Invader].
    ("1000", "005d", Some(("1028", "1f41")), c(MEGARAID, "830", "PERC H830 Adapter")),
    ("1000", "005d", Some(("1028", "1f42")), c(MEGARAID, "73P", "PERC H730P Adapter")),
    ("1000", "005d", Some(("1028", "1f43")), c(MEGARAID, "730", "PERC H730 Adapter")),
    ("1000", "005d", Some(("1028", "1f47")), c(MEGARAID, "73P", "PERC H730P Mini")),
    ("1000", "005d", Some(("1028", "1f49")), c(MEGARAID, "730", "PERC H730 Mini")),
    // On MegaRAID Tri-Mode SAS3508.
    ("1000", "0016", Some(("1028", "1fc9")), c(MEGARAID, "840", "PERC H840 Adapter")),
    ("1000", "0016", Some(("1028", "1fcb")), c(MEGARAID, "74P", "PERC H740P Adapter")),
    ("1000", "0016", Some(("1028", "1fcd")), c(MEGARAID, "74P", "PERC H740P Mini")),
    // Dell HBA330 on SAS3008 Fusion-MPT SAS-3.
    ("1000", "0097", Some(("1028", "1f45")), c(MPT3SAS, "330", "HBA330 Adapter")),
    ("1000", "0097", Some(("1028", "1f53")), c(MPT3SAS, "330", "HBA330 Mini")),
    // The chips, under any board.
    ("1000", "005f", None, c(MEGARAID, "008", "MegaRAID SAS-3 3008")),
    ("1000", "005d", None, c(MEGARAID, "108", "MegaRAID SAS-3 3108")),
    ("1000", "0016", None, c(MEGARAID, "508", "MegaRAID Tri-Mode SAS3508")),
    ("1000", "0017", None, c(MEGARAID, "408", "MegaRAID Tri-Mode SAS3408")),
    ("1000", "005b", None, c(MEGARAID, "208", "MegaRAID SAS 2208")),
    ("1000", "0073", None, c(MEGARAID, "200", "MegaRAID SAS 2008")),
    ("1000", "0097", None, c(MPT3SAS, "008", "SAS3008 Fusion-MPT SAS-3")),
    ("1000", "00ac", None, c(MPT3SAS, "416", "SAS3416 Fusion-MPT Tri-Mode")),
    ("1000", "00af", None, c(MPT3SAS, "408", "SAS3408 Fusion-MPT Tri-Mode")),
    ("1000", "0087", None, c(MPT3SAS, "238", "SAS2308 Fusion-MPT SAS-2")),
    ("1000", "0072", None, c(MPT3SAS, "208", "SAS2008 Fusion-MPT SAS-2")),
    ("1000", "00a5", None, c(MPI3MR, "400", "Fusion-MPT 24GSAS/PCIe SAS40xx")),
    ("9005", "028f", None, c(SMARTPQI, "001", "Smart Storage PQI 12G SAS/PCIe 3")),
    ("103c", "323a", None, c(HPSA, "006", "Smart Array G6")),
    ("103c", "323b", None, c(HPSA, "008", "Smart Array Gen8")),
    ("103c", "3239", None, c(HPSA, "009", "Smart Array Gen9")),
    ("9005", "0285", None, c(AACRAID, "001", "AAC-RAID")),
    ("9005", "028b", None, c(AACRAID, "006", "Series 6 6G SAS/PCIe 2")),
    ("9005", "028c", None, c(AACRAID, "007", "Series 7 6G SAS/PCIe 3")),
    ("9005", "028d", None, c(AACRAID, "008", "Series 8 12G SAS/PCIe 3")),
    // Emulated and generic controllers.
    ("1af4", "1001", None, c("QD01", "001", "virtio block")),
    ("1af4", "1042", None, c("QD01", "001", "virtio block")),
    ("1af4", "1004", None, c("QD02", "001", "virtio SCSI")),
    ("1af4", "1048", None, c("QD02", "001", "virtio SCSI")),
    ("1000", "0030", None, c("QD06", "030", "53c1030 Fusion-MPT SPI")),
    ("1000", "0054", None, c("QD06", "068", "SAS1068 Fusion-MPT SAS")),
    ("15ad", "07c0", None, c("QD07", "001", "VMware PVSCSI")),
    ("8086", "2922", None, c("QD03", "001", "ICH9 AHCI")),
    ("1b36", "0010", None, c("QD04", "001", "NVMe")),
];

/// The controller a PCI function is, by IDs, else by class.
#[must_use]
pub fn controller(vendor: &str, device: &str, subsys: (&str, &str), class: u32) -> Controller {
    TABLE
        .iter()
        .find(|(v, d, s, _)| *v == vendor && *d == device && s.is_none_or(|s| s == subsys))
        .map_or_else(
            || match class >> 8 {
                0x0101 => c("QD05", "001", "IDE controller"),
                0x0104 => c("QD00", "000", "RAID controller"),
                0x0106 => c("QD03", "001", "SATA AHCI controller"),
                0x0107 => c("QD00", "000", "SAS controller"),
                0x0108 => c("QD04", "001", "NVMe controller"),
                _ => c("QD00", "000", "storage controller"),
            },
            |(.., ctl)| *ctl,
        )
}

/// The SCSI address `host:channel:target:lun` in a sysfs device path:
/// the segment of four numbers nearest the device.
#[must_use]
pub fn hctl(devpath: &str) -> Option<[u32; 4]> {
    devpath.split('/').rev().find_map(|seg| {
        let n: Vec<u32> = seg
            .split(':')
            .map(str::parse)
            .collect::<Result<_, _>>()
            .ok()?;
        <[u32; 4]>::try_from(n).ok()
    })
}

/// The key disks are named in: the controller's PCI address (none
/// last), the SCSI address (none first), the kernel name.
#[must_use]
pub fn disk_key(devpath: &str, name: &str) -> (bool, String, [u32; 4], String) {
    let pci = crate::hwrsrc::pci::address_in(devpath);
    (
        pci.is_none(),
        pci.unwrap_or_default(),
        hctl(devpath).unwrap_or_default(),
        name.to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_board_wins_over_the_chip_and_the_class_backs_both() {
        let perc = controller("1000", "005f", ("1028", "1f4b"), 0x01_0400);
        assert_eq!((perc.type_code, perc.model, perc.name), ("QD10", "330", "PERC H330 Mini"));
        let chip = controller("1000", "005f", ("1000", "9361"), 0x01_0400);
        assert_eq!(chip.name, "MegaRAID SAS-3 3008");
        let hba = controller("1000", "0097", ("1028", "1f45"), 0x01_0700);
        assert_eq!((hba.type_code, hba.name), ("QD11", "HBA330 Adapter"));
        assert_eq!(controller("8086", "2922", ("1af4", "1100"), 0x01_0601).type_code, "QD03");
        assert_eq!(controller("1b36", "0010", ("1af4", "1100"), 0x01_0802).type_code, "QD04");
        assert_eq!(controller("1234", "5678", ("0", "0"), 0x01_8000).name, "storage controller");
    }

    #[test]
    fn the_scsi_address_comes_from_the_path() {
        let p = "/devices/pci0000:00/0000:00:01.0/0000:02:00.0/host0/target0:2:3/0:2:3:0/block/sdc";
        assert_eq!(hctl(p), Some([0, 2, 3, 0]));
        let sas = "/devices/pci0000:00/0000:03:00.0/host1/port-1:0/end_device-1:0/target1:0:0/1:0:0:0/block/sdb";
        assert_eq!(hctl(sas), Some([1, 0, 0, 0]));
        assert_eq!(hctl("/devices/pci0000:00/0000:00:03.0/virtio1/block/vda"), None);
        // 0:0:10:0 after 0:0:9:0, numerically.
        let a = disk_key("/devices/pci0000:00/0000:02:00.0/host0/target0:0:9/0:0:9:0/block/sdz", "sdz");
        let b = disk_key("/devices/pci0000:00/0000:02:00.0/host0/target0:0:10/0:0:10:0/block/sda", "sda");
        assert!(a < b);
    }
}
