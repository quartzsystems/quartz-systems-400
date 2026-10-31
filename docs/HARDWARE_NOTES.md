# Hardware notes

Machines QS/400 has been booted on, what it named their hardware and how
the console came up. The kernel drives storage with these built-in
drivers (`kernel/config/qs400.config`, checked by `build kernel`):
`MEGARAID_SAS` (Dell PERC H330/H730/H740, LSI MegaRAID 3008/3108/3508,
needed in HBA mode too), `SCSI_MPT3SAS` (Dell HBA330, LSI 9300/9400 IT
mode, and the SAS2 parts), `SCSI_SMARTPQI` (Microchip/Adaptec, HPE Smart
Array in HBA mode), `SCSI_HPSA` (older HPE), `SCSI_AACRAID` (Adaptec),
`SCSI_MPI3MR` (Broadcom 9600), beside AHCI, NVMe, virtio and the VMware
controllers. The SCSI scan is synchronous (`SCSI_SCAN_ASYNC=n`), so the
disks come up in the same order on every boot.

## How resources are named

- Every PCI function is a resource in PCI address order. A storage
  controller is `DCnn`, with a type and model from the ID table in
  `lic/hwrsrc/src/scan/storage.rs` (board by subsystem ID first, then
  the chip, then the PCI class): `QD10` MegaRAID/PERC, `QD11` mpt3sas,
  `QD12` smartpqi, `QD13` hpsa, `QD14` aacraid, `QD15` mpi3mr, `QD01`
  virtio block, `QD02` virtio SCSI, `QD03` AHCI, `QD04` NVMe.
- Disks are `DDnnn` (optical drives `OPTnn`), ordered by the controller's
  PCI address, then the SCSI `host:channel:target:lun`, then the kernel
  name: what the kernel calls `sda` does not decide the number.
- `WRKHDWRSC *STG` and `svc.hw` list each disk under its controller
  (`sdc 953869 MiB at 0:0:0:0 on DC02`). The installer's `Select Load
  Source Device` and DST's `Display Non-Configured Units` show the
  resource, the serial (sysfs `serial`, else the unit serial number VPD
  page `device/vpd_pg80`, else `device/wwid`), type and model, size, the
  controller under `Ctl`, and the device.

## Tested hardware

| Machine | Storage | Disks | Display | Controller PCI ID | Resources | Scale | Result |
|---|---|---|---|---|---|---|---|
| Dell PowerEdge (model to be confirmed on hardware by Cody) | PERC H330 in HBA mode | 4× 1 TB SATA SSD | 1080p-class monitor | expected `1000:005f`, subsystem `1028:1f4b` (H330 Mini) or `1028:1f44` (H330 Adapter); to be confirmed with `svc.hw` | expected PERC `DCnn` of type `QD10-330`, disks `DD001` to `DD004` under it in target order (the `nn` depends on the onboard controllers before it); to be confirmed on hardware by Cody | 2 at 1920×1080 (grid 1440×768 at (240, 156)); 1 if the firmware's GOP mode is 1024×768 or 1280×1024; to be confirmed on hardware by Cody | first boot (before these drivers and the scaler): console unscaled in the middle of the panel, no disks in the installer; with them: to be confirmed on hardware by Cody |

The scale comes from the mode the firmware hands over, not the
monitor's: QS/400 does no modesetting yet (CONSOLE.md). A server VGA
often starts its GOP at 1024×768 or 1280×1024, where the integer scale
is 1; the monitor then stretches that mode, and the grid fills about 70%
of its width. If `svc.hw` shows such a mode, the firmware's video
setting (or a native-mode driver, deferred) is what changes it.

## Verification procedure

1. Rebuild the image: `cargo run -p build -- all` (dev) or `-- all
   --release`, then `cargo run -p build -- disk`.
2. Write the raw disk image to a USB stick (until the hybrid ISO lands,
   the `build disk` image is the one a USB stick boots):
   `sudo dd if=target/images/qs400-V1R0M0-<hash>.img of=/dev/sdX bs=4M
   status=progress conv=fsync` (everything on the stick is lost).
3. Boot the Dell from the stick (F11, the one-shot UEFI boot menu).
   The raw disk image is an installed system whose load source is the
   stick itself: it IPLs in normal mode to sign-on. For the IPL menu, set
   `svc.mode manual` on the service console and `svc.pwrdwn restart`.
   Its option 2 installs only from media (a D IPL), so the `Select Load
   Source Device` check below waits for the hybrid ISO, or uses the ISO
   on a virtual CD from the iDRAC.

Expected:

- The console fills the screen at the panel's mode: the IPL menu
  readable at scale 2 at 1920×1080 (3 at 2560×1440). The serial log's
  `slic: display:` line and `svc.hw`'s `Display:` line give the mode,
  the pixel format and the scale chosen; note them here.
- `svc.hw` lists the PERC as `DCnn` with `QD10-330` and
  `1000:005f PERC H330 Mini` (or `Adapter`), and four `DDnnn` disks
  under it. Note the PCI ID, the names and the serials here.
- `3. Use Dedicated Service Tools (DST)`, sign on, `Work with disk
  units`, `Work with disk configuration`, `Display disk configuration`:
  the SSDs belong to no ASP, so `Display disk configuration status`
  lists only the stick's unit (none from the media), and `Display
  non-configured units` shows the four SSDs (resource, serial, type and
  model, about 953 869 MiB each, the PERC under `Ctl`).
- From the media: `2. Install the operating system` lists the four, and
  the stick when the image is on one, in `Select Load Source Device`.
- From the media with nothing installed, `1. Perform an IPL` shows `No
  Operating System Installed` with the four disks, not a sign-on that
  refuses.
