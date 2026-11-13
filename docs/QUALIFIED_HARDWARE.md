# Qualified hardware

Which machines and hypervisors QS/400 V1R1M0 is qualified on, how a
machine is qualified, and the guest agent hypervisors talk to. A
platform is **qualified** once `QSHWQUAL` has run on it with no `FAIL`,
the install from the media has completed, and the system has IPLed from
its own disk and powered down cleanly. Everything listed as NEEDS-CODY
needs that run on hardware or a hypervisor this build machine does not
have; the checklist under each is what to do and what to write back
into the table.

**Secure Boot.** V1R1M0 boots with Secure Boot off. Its UKIs are signed
only with the QS/400 db key (no Microsoft UEFI CA signature, no shim),
so with Secure Boot on the firmware refuses them unless the QS/400 key
is enrolled in its `db` (`INSTALL.md` §6 gives the steps per platform).
Qualify each platform with Secure Boot off; a run with Secure Boot on
and the key enrolled is optional and is recorded as such.

| Platform | Firmware | Storage | Network | Console | Guest agent | State |
| --- | --- | --- | --- | --- | --- | --- |
| QEMU/KVM, `q35` | OVMF (UEFI) | virtio-blk, SATA on the q35 AHCI controller, virtio-scsi, NVMe | virtio-net, e1000e | VGA through the GOP (simpledrm), serial `ttyS0` | virtio-serial `org.qemu.guest_agent.0` | **Qualified** here: QEMU steps 114 to 117 and media step 5 |
| Proxmox VE 8 | OVMF | virtio-scsi (default), virtio-blk, SATA | virtio-net | `Default` VGA, serial | QEMU guest agent option | NEEDS-CODY |
| VMware ESXi 8 | EFI | PVSCSI, LSI Logic SAS, SATA, NVMe | vmxnet3, e1000e | SVGA through the GOP | none (VMware Tools is not offered); vsock | NEEDS-CODY |
| VMware Workstation 17 | EFI | as ESXi | vmxnet3, e1000e | as ESXi | none; vsock | NEEDS-CODY |
| Oracle VirtualBox 7 | EFI | AHCI (SATA), virtio-scsi, NVMe | Intel PRO/1000 (e1000), virtio-net | VBoxVGA/VMSVGA through the GOP | none | NEEDS-CODY |
| Microsoft Hyper-V, Generation 2 | UEFI | SCSI (storvsc) | synthetic (netvsc) | Gen2 EFI framebuffer | none; vsock (hv_sock) | NEEDS-CODY |
| Dell PowerEdge with PERC H330 in HBA mode, 4 x 1 TB SSD (reference physical target, `HARDWARE_NOTES.md`) | UEFI | MegaRAID (`megaraid_sas`) | the board's Broadcom or Intel ports | iDRAC/Matrox VGA through the GOP, serial | n/a | NEEDS-CODY |

## QSHWQUAL

`QSHWQUAL` grades each part of the machine `PASS`, `WARN` or `FAIL`,
names it as `WRKHDWRSC` does (`PN01`, `MS01`, `DC01`, `DD001`, `CMN01`)
and ends with one result: the worst line's grade. A `WARN` is a limit
the machine runs with; a `FAIL` is something QS/400 cannot run on as it
is. `INFO` lines (optical and tape units, keyboards) count for nothing.
The checks are `lic/hwqual`; the same report comes from three places:

- **The install media**, before anything is installed: at `IPL or
  Install the System` take option 2, then option 4 `Run hardware
  qualification (QSHWQUAL)` on `Install Licensed Internal Code`. Option
  1 shows the report (F7 and F8 page it); option 2 writes it to the
  first USB stick with a FAT file system, as `QSHWQUAL.TXT` in its root
  (replaced if there, with DOS line ends; nothing else on the stick is
  touched and it is never mounted). IBM's menu has three options; the
  fourth is QS/400's.
- **The command** `QSHWQUAL OUTPUT(*|*PRINT) TODEV(*NONE|*USB)` on an
  installed system: shown, or printed as `QSYSPRT` file `QSHWQUAL`, and
  with `TODEV(*USB)` also written to a stick. It runs in the storage
  service, with the hardware resource service's table.
- **The service console**: `QSHWQUAL` as a command, or the dev word
  `svc.qshwqual [usb]`.

What it checks, and the limits (QS/400's own):

| Area | `PASS` | `WARN` | `FAIL` |
| --- | --- | --- | --- |
| Firmware | UEFI (`/sys/firmware/efi`) | | legacy BIOS boot |
| Secure Boot | the `SecureBoot` variable is 0 (V1R1M0 boots with Secure Boot off) | it is 1 (`WARN: QS/400 V1R1M0 is not Secure Boot signed; disable Secure Boot or enrol the QS/400 key`), or cannot be read | |
| TPM | a TPM 2.0 (`/sys/class/tpm/tpm0`) | none, or a TPM 1.2 | |
| Processors | each `PNnn`, with the CPU's model | only one processor | none found |
| Main storage | 2048 MiB or more | 512 to 2047 MiB | under 512 MiB |
| Storage controllers | each `DCnn` with its driver bound and a type QS/400 knows (`HARDWARE_NOTES.md`) | a controller of type `QD00` (a class with no table entry) | a PCI controller no driver took |
| Disk units | each `DDnnn` | a RAID volume (a PERC or MegaRAID virtual disk; the line names the controller, `DCnn` and its description: put it in HBA mode); a disk too small for a load source; only one disk (no mirroring) | no disk at all |
| Network | each Ethernet port with link | a port without link; no port | |
| Guest agent | the `org.qemu.guest_agent.0` channel, under a hypervisor | under a hypervisor, no channel | |
| Console | the serial port; the firmware's framebuffer (name, size, depth) | no framebuffer (serial only) | neither a serial port nor a framebuffer |

The report's head names the machine (DMI vendor and product), the
hypervisor (by the CPU's `hypervisor` flag and the DMI identity: KVM/QEMU,
VMware, Hyper-V, VirtualBox), the release, the time in UTC and the
result with its counts. QS/VS has no DMI and no firmware: a partition of
it is known by its ACPI tables (`QUARTZVS` as the FADT's hypervisor
vendor identity, or OEM ID `QUARTZ` and OEM table ID `QSVSPART` in
every table; SMBIOS tables naming it would answer too), and there the
UEFI check passes, since QS/VS loads the UKI of the IPL's side itself
and reads that side from the LIC record (docs/PHASE1.md, "The side of
the next IPL in a partition"). Any other system without UEFI fails the
check as before. Lines are at most 78 columns, so the installer's
display, the console and a printout show the same text.

## Guest agent

Under a hypervisor `slic` starts the guest agent (`lic/guestagent`,
`/init qsga`) once the IPL completes, when the guest has a virtio-serial
port named `org.qemu.guest_agent.0` or a vsock device. It speaks the QEMU
guest agent's protocol, JSON objects over the port (and over vsock port
9400), so Proxmox, libvirt and QEMU see a standard agent:

| Command | Answer |
| --- | --- |
| `guest-sync`, `guest-sync-delimited`, `guest-ping`, `guest-info` | as the QEMU agent; `0xFF` resynchronises |
| `guest-get-host-name` | the system name (`QSYSNAME`) |
| `guest-get-osinfo` | `id` `qs400`, `name` `QS/400`, the release, the kernel underneath |
| `guest-network-get-interfaces` | every interface with its MAC, addresses and counters |
| `guest-get-fsinfo` | the IFS volume's used and total bytes |
| `guest-get-time`, `guest-get-timezone`, `guest-get-vcpus`, `guest-get-users` | the clock (UTC), the processors, no users |
| `guest-fsfreeze-status` | `thawed`: QS/400 does not freeze its file systems; a backup takes a crash-consistent copy, which the store's journal recovers |
| `guest-shutdown` | `powerdown` and `halt`: `PWRDWNSYS OPTION(*CNTRLD) DELAY(600) RESTART(*NO)`; `reboot`: `RESTART(*YES)`. As the QEMU agent, no reply once it starts. Refused while the IPL is still running |
| `qs400-get-status` | the release, the IPL's state, mode and type, the slot, the last SRC and its text |

The agent's facts come from `/run/qs400/status`, which `slic` rewrites
at each SRC it shows (`guestagent::status`), from netlink, and from the
IFS mount. The channel is a communications resource: `WRKHDWRSC
TYPE(*CMN)` lists it as `CMNnn`, type `QV01`, text `Virtual Channel`,
after the serial ports; a vsock device is one too (location `vsock`).

QEMU and libvirt: `-chardev socket,path=/run/qga.sock,server=on,wait=off,id=qga0
-device virtio-serial-pci -device virtserialport,chardev=qga0,name=org.qemu.guest_agent.0`
(libvirt: a `<channel type='unix'>` with `<target type='virtio'
name='org.qemu.guest_agent.0'/>`). Proxmox: VM Options, QEMU Guest Agent,
Enabled.

## Tested here: QEMU/KVM

QEMU 8 on WSL Ubuntu with KVM, `q35`, OVMF, 1 GiB, two vCPUs, the system
disk on virtio-blk, a second disk on the q35 AHCI controller, a USB stick
on xHCI, virtio-net, the guest agent channel; no `swtpm` on this host, so
the TPM line is `WARN` (with `swtpm` on `PATH` the suite gives the guest a
`tpm-crb` and the line passes). Steps:

- 114 `QSHWQUAL`: the report at the console (AHCI `QD03-001` and virtio
  `QD01-001` controllers, the agent's channel), `OUTPUT(*PRINT)`,
  `TODEV(*USB)`, `svc.qshwqual`.
- 115 the guest agent: `guest-sync-delimited`, `guest-ping`,
  `guest-info`, the host name against `QSYSNAME`, OS information,
  interfaces, file systems, `qs400-get-status`.
- 116 `WRKHDWRSC TYPE(*CMN)` lists the channel; `svc.hw` names its port.
- 117 `guest-shutdown`: SRC `D1001000` to `D1001090`, QEMU exits.
- Media step 5: the ISO alone with a SATA disk and a stick, `QSHWQUAL`
  from the menu, the report written to the stick and read back.

Result here (2026-11-04): `WARN`, 11 passed, 4 warnings, 0 failed: the plain OVMF
has no `SecureBoot` variable (state unreadable), no TPM,
1 GiB of main storage).

## NEEDS-CODY: hypervisor checklists

The same run on each; write the results into the table above (result
line, any `WARN`/`FAIL` lines, time to sign-on).

1. Build the media: `cargo run -p build -- all --release` and
   `cargo run -p build -- iso`; the ISO is `target/images/` (`iso.path`).
2. Create the VM as below, attach the ISO and a blank 16 GiB disk, and a
   small FAT32 USB stick (or a second virtual disk formatted FAT32) for
   the report.
3. Boot; at `IPL or Install the System` take 2, then 4, then 1. Note the
   result line and every `WARN`/`FAIL`. F12, then 2 to write the report;
   keep `QSHWQUAL.TXT`.
4. F12 back to `Install Licensed Internal Code`, 1, then 2, install onto
   the blank disk, then the operating system (`INSTALL.md`).
5. After sign-on: `QSHWQUAL` at Command Entry; `WRKHDWRSC TYPE(*CMN)`.
6. Where there is a guest agent: check the hypervisor shows the VM's IP
   address and host name, then shut the VM down from the hypervisor: the
   console must show `D1001000` and the VM power off within the delay.
7. `PWRDWNSYS RESTART(*YES)`: the VM comes back to sign-on.

Per platform:

- **Proxmox VE 8**: Machine `q35`, BIOS `OVMF (UEFI)` with an EFI disk
  (pre-enrolled keys off: Secure Boot is not enforced; a run with the
  keys on needs the QS/400 key enrolled, `INSTALL.md` §6), SCSI
  controller `VirtIO SCSI single`, disk on `scsi0`, network `VirtIO`,
  Display `Default`, Options: QEMU Guest Agent `Enabled`, add a TPM
  state (v2.0). Expect `DC01` virtio SCSI (`QD02-001`), the TPM `PASS`,
  the agent channel `PASS`; in step 6 the Summary page shows the IPs and
  `Shutdown` powers down through the agent.
- **VMware ESXi 8**: guest OS `Other 5.x or later Linux (64-bit)`, EFI
  firmware, Secure Boot off (on only with the QS/400 key enrolled,
  `INSTALL.md` §6); controller `VMware Paravirtual`,
  then repeat with `LSI Logic SAS`; NIC `VMXNET 3`; add a virtual TPM
  (needs a key provider). Expect `QD07-001` (PVSCSI) or `QD06` (LSI
  SAS), `QN` vmxnet3 ports, `WARN` for the guest agent (ESXi has no
  QEMU agent channel; VMware Tools is not part of V1R1M0); step 6 is
  `Power Off` from the console only.
- **VMware Workstation 17**: as ESXi, with an `NVMe` disk as a second
  run; `vTPM` needs the VM encrypted.
- **Oracle VirtualBox 7**: `Other Linux (64-bit)`, Enable EFI, storage
  `AHCI` (SATA), network `Intel PRO/1000 MT Desktop` then `virtio-net`,
  graphics `VMSVGA`, TPM `v2.0`, Secure Boot off. Expect AHCI
  `QD03`, e1000 ports, the guest agent `WARN` (VirtualBox has no QEMU
  agent channel). Check the console's scale at the default GOP mode.
- **Hyper-V Generation 2**: Security, Secure Boot **off**: no template
  boots V1R1M0 (the `Microsoft UEFI Certificate Authority` template
  trusts only loaders that CA signed, and Hyper-V takes no other key), a SCSI
  disk and DVD, a synthetic network adapter, a TPM (Security). Expect
  the disks under no PCI controller (VMBus), `WARN` for the guest agent
  (Hyper-V offers vsock; the agent listens on port 9400 but Hyper-V's
  integration services do not speak the QEMU protocol), the framebuffer
  `PASS` through Gen2's EFI framebuffer.

## NEEDS-CODY: the Dell with the PERC H330

The reference physical machine (`HARDWARE_NOTES.md`, "Tested hardware"):

1. In the PERC's configuration (Ctrl-R at boot, or the System Setup's
   Device Settings), set the controller to **HBA mode** (H330: "Convert
   to HBA"/"HBA Personality"); clear any virtual disk on the four SSDs.
   In System Setup, Boot Settings: **UEFI**; Secure Boot **off** (or
   on with the QS/400 key appended to `db`, `INSTALL.md` §6); TPM
   Security on.
2. Build the ISO (`cargo run -p build -- all --release`,
   `cargo run -p build -- iso`) and write it raw to a USB stick (`dd`,
   `HARDWARE_NOTES.md`); keep a second, FAT32 stick for the report.
3. Boot the stick (F11, one-shot UEFI boot menu). At `IPL or Install
   the System`: 2, then 4, then 1. Expected: `UEFI` `PASS`; `SECBOOT`
   `PASS` with Secure Boot off (`WARN` with it on); `TPM 2.0 present`
   `PASS`; `PN01` and on with the Xeon's model;
   `MS01` `PASS`; the PERC as `DCnn` type `QD10-330`, driver
   `megaraid_sas`, 4 disk units, `PASS`; `DD001` to `DD004`, `SSD`, about
   953 869 MiB each, `PASS` (a `WARN` "a RAID volume" means the
   controller is still in RAID mode: back to step 1); the board's ports
   as `CMNnn` (`PASS` with a cable, `WARN` without); `FB0` the iDRAC's
   VGA with its mode; the serial port if the BIOS exposes `COM1`. The
   result should be `PASS`, or `WARN` only for ports without link.
4. F12, then 2 with the FAT32 stick in a second port: `QSHWQUAL.TXT`.
   Send it back with the photos of the screens; the result, the PCI ID
   of the PERC and the resource names go into `HARDWARE_NOTES.md` and
   the table above.
5. Install onto `DD001` (`Install Licensed Internal Code`, 1, 2), the
   operating system, sign on; start mirrored protection over the four
   units in DST; run `QSHWQUAL` again: the disks still `PASS`.
6. `PWRDWNSYS RESTART(*YES)` and a cold power-on: both reach sign-on.
