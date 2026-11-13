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
- A function that is none of the kinds with names of their own is
  `OTHnn`, type `QX00-000`; the virtio entropy source (`1af4:1005`,
  `1af4:1044`) has its own type, `QX01-001`.
- Device parity has no hardware behind it: DST's `Work with device
  parity protection` names one virtual controller, `DCnn` after the real
  controllers, type `QDPY-001` (`lic/hwrsrc/src/table/parity.rs`). It
  is not in the resource table, so `WRKHDWRSC` does not list it. A RAID
  card in RAID mode should be put in HBA (pass-through) mode so the store
  sees the plain disks and keeps parity itself.
- Disks are `DDnnn` (optical drives `OPTnn`), ordered by the controller's
  PCI address, then the SCSI `host:channel:target:lun`, then the kernel
  name: what the kernel calls `sda` does not decide the number.
- `WRKHDWRSC *STG` and `svc.hw` list each disk under its controller
  (`sdc 953869 MiB at 0:0:0:0 on DC02`). A disk's type is its medium,
  `SSD`, `HDD`, `NVME` or `VIRTUAL` (`DISK` when sysfs does not say),
  from the name, `device/vendor`, `device/model` and
  `queue/rotational`, with no model; four-character fields (the
  `WRKHDWRSC` all-hardware list, `QGYRHR*`, the `DSPHDWRSC` outfile's
  `DORTYP`, `WRKDSKSTS`) show `VIRT` for virtual. `WRKDSKSTS` and
  Navigator show the model sysfs reports (SATA's `ATA` vendor left out). The installer's `Select Load
  Source Device` and DST's `Display Non-Configured Units` show the
  resource, the serial (sysfs `serial`, else the unit serial number VPD
  page `device/vpd_pg80`, else `device/wwid`), type and model, size, the
  controller under `Ctl`, and the device.

## Names that last

A resource keeps its name from one IPL to the next and across hardware
changes, as on IBM i: adding a card does not renumber the others, so a
line description's `RSRCNAME(CMN04)` still names the same port. The
order above gives the names only once, on a system's first IPL (an
upgrade from V1R0M0 keeps the names it had, which were in that order).
After that:

- Each resource is saved with what identifies its device (the key,
  `lic/hwrsrc/src/names/key.rs`) in the LIC partition (TLV `0x010A`,
  docs/PHASE1.md "Hardware resource names"), which the hardware resource
  service reads before its first walk, long before TCP/IP starts.
- At each IPL a device whose key was saved gets its saved name, wherever
  it now sits. A new device takes the next free name of its kind: one
  past the highest in use, a missing device's included, so a removed
  name is not handed to another card a line description might still
  name.
- A saved device not found stays in the table under its saved name and
  parents, `Not detected` in `WRKHDWRSC`, `DSPHDWRSC` and the APIs (IBM's
  status for it), `Unknown` in the hardware service manager. Its port is
  gone, so a line on it fails to vary on (`CPD26D4`) rather than moving
  to another card.
- The hardware service manager (DST or SST: `Start a service tool`, 7,
  then 4, `Failed and Non-Reporting Logical Hardware Resources`) removes
  one with option 4 and F10, as IBM's does; its name is then free of it
  (though not reused, as above). `WRKHDWRSC` has no remove, as on the
  primary.

The key per kind:

| Kind | Key | Moved to another slot |
|---|---|---|
| Ethernet port (`CMNnn`) | the permanent MAC (netlink `IFLA_PERM_ADDRESS`, else sysfs `address` while `addr_assign_type` is 0); a port with a random address only, its PCI address and port number | keeps its name |
| network function (`LINnn`) | its first port's permanent MAC, else its serial, else its PCI address | keeps its name |
| storage controller (`DCnn`) | its serial, else its PCI address | keeps it when it reports a serial; else a new name |
| disk, optical, tape, media changer | the serial (or WWID), else the device path | keeps it with a serial |
| bridge (`CMBnn`), other PCI function | the PCI address | is another slot's |
| serial port, keyboard, other input | the device path (an input's `inputN` left out) | new name |
| processor, main storage | the CPU number; `MS01` | n/a |
| virtual channel | the virtio-serial port's name, `vsock` | keeps its name |

A MAC set by software (`ADPTADR`, a bond) does not change a port's key:
the permanent one comes from netlink, and within an IPL a port keeps the
key it was named by. A card replaced by another in the same slot is a
new device: it takes a new name and the old one shows as not detected
until removed. Within one IPL hotplug follows the same rules: a card
pulled is not detected, one plugged back (anywhere) is detected again
under its name.

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
