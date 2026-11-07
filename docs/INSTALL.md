# Installing QS/400

How QS/400 is installed from its media: the media layout, every panel
of the install in the order the operator meets it (each with its golden
file under `tools/conformance/golden/install/`), the automatic install
and its response file, slip installs and upgrades with the release rule,
and the QEMU steps that prove it. The recovery from a system save is
docs/RECOVERY.md. The code: `media/instmedia` (layout, signature,
response file, the operating system install over a store),
`lic/svctools/src/install/` (the panels), `lic/slic/src/ipl/install.rs`
and `instdev.rs` (the work and the install devices).

## 1. Media layout

Install media is any volume whose root holds `QS400/MEDIA.MANIFEST`
(docs/MEDIA.md, "Install media manifest"). The manifest names the
release, the kind (`install` for distribution media, `savsys` for a
system save), every payload file with its size and SHA-256, and ends
with an ed25519 signature by the build key over the lines before it.

| Payload | Optical path | Tape label | Holds |
|---|---|---|---|
| LIC, slot A | `QS400A.EFI` | `QS400LICA` | the UKI the installer writes to the ESP's slot A |
| LIC, slot B | `QS400B.EFI` | `QS400LICB` | slot B's UKI |
| product load | `QS400/LP/<ID>.SAVF` | `QS400LP<ID>` | one product's SAVLICPGM stream: `QS00SS1` (the operating system), `QS00WDS`, `QS00TC1`, `QS00ST1` |
| security data | `QSAVSYS/SECDTA` | `QSAVSECDTA` | `SAVSYS` only: the SAVSECDTA stream |
| configuration | `QSAVSYS/CFG` | `QSAVCFG` | `SAVSYS` only: the SAVCFG stream |

Distribution media (`build iso`) also carries the media UKI at
`EFI/BOOT/BOOTX64.EFI` and in the El Torito image, `QS400/LIC/qs400.efi`,
and, when `QS400_INSTALL_RSP` names a file at build time,
`QS400/INSTALL.RSP`. The same image boots as a CD, written raw to a disk
or USB stick, and with its tree copied onto FAT32 (`build test --media
iso`, `iso-raw`, `iso-fat`).

A release (`build release`) adds, at the root, `NOTICES.txt` (every
third-party component with its license, `build notices`) and
`RELEASE_NOTES_V1R1M0.md`; `QS400/SRC/`, the Linux kernel's source
offer (`SOURCE-OFFER.txt`, `BUILDING-THE-KERNEL.md`, the configuration,
the GPL-2.0 text); and `QS400/EFI/keys/`, the Secure Boot db key's
certificate (§6). The installed system keeps the notices at
`/QQS/ProdData/Notices/NOTICES.txt`; F19 on `DSPSFWRSC` and on `GO
LICPGM` option 10 names that file (CPI9B83).

On tape the manifest is file 1 (label `QS400MANIFEST`), the payload
files follow in manifest order with 64 KiB blocks, IBM standard labels
throughout (docs/MEDIA.md, "Tape: labels"). Files after them (a
`SAVLIB *NONSYS`, a `SAV`) belong to the recovery
(`instmedia::savsys::user_files`; docs/RECOVERY.md, §4).

`instmedia::savsys::recognize_tape`, `recognize_iso` and
`recognize_volume` (a mounted optical volume) tell install media from
any other volume. `CHKTAP` and `DSPOPT DATA(*SAVRST)` show what they
say: distribution media or a system save, the release, the volume the
manifest names, the products, and each payload file with its size;
`DSPOPT` then lists the save streams among them (the security data,
the configuration, the product loads) as for any save.

### Install devices

The operating system install reads from an install device:

| Kind | Name | What |
|---|---|---|
| Optical | `OPT01` | the boot media of the D IPL: CD, raw image, or FAT copy |
| Virtual | `TAPVRTnn` | a disk whose first bytes are a virtual tape volume (`QSTAPVOL`) holding install media |
| Virtual | `OPTVRTnn` | a disk holding an ISO 9660 image with install media |
| Tape | `TAPnn` | a tape drive (`/sys/class/scsi_tape`, named by `tape::device::naming` as the hardware table names it) with a cartridge of install media or a system save loaded, read through its no-rewind node `/dev/nstN` |

Disks holding install media are never offered as load sources or units
and do not count as non-configured disks (`A6001260`). A `TAPVRTnn`
disk is how a system save made to an image catalog reaches a recovery:
the catalog's image file is copied off the system (FTP, SFTP) and
attached as a disk; the harness copies it with the dev word `svc.vrtout
DEV PATH`. No real drive was available to test `TAPnn`: the probe and
the reads are tested over the tape crate's in-memory volumes
(`lic/slic/src/ipl/instdev.rs`), the drive's transport by the `st`
transcripts (`media/tape/tests/st_replay.rs`); the procedure for real
hardware is docs/MEDIA.md, "Tape: physical tape acceptance".

## 2. The flow, panel by panel

The panels are Quartz's text on IBM's titles, option numbers and
geometry. Serial shows every screen and every SRC; the harness drives
them with `svc.key` and `svc.type`.

| # | Panel | Golden | Notes |
|---|---|---|---|
| 1 | `IPL or Install the System` | (slic's menu) | D IPL from the media; 1 IPL, 2 install, 3 DST, 4 automatic install |
| 2 | `Install Licensed Internal Code` | `lic-menu.txt` | 1 install, 2 DST, 3 not available |
| 3 | `Install Licensed Internal Code (LIC)` | `lic-options.txt`, `lic-options-no-source.txt` | the current load source above the options; 1 to 4 work |
| 4d | (option 1) | | no load source choice and no confirmation, as on IBM i: straight to panel 7 |
| 4a | `Select Load Source Device` | `load-source.txt` | option 2: every disk |
| 5a | `Install LIC and Initialize System - Confirmation` | `lic-confirm.txt` | F10 |
| 4b | `Select Load Source Device` | `recover-source.txt` | option 3: the disks with QS/400 data |
| 5b | `Install LIC and Recover Configuration - Confirmation` | `recover-confirm.txt` | F10; the release rule is checked here |
| 4c | `Select Load Source Device` | `restore-source.txt` | option 4: the blank disks |
| 5c | `Install LIC and Restore Disk Unit Data - Confirmation` | `restore-confirm.txt` | F10 |
| 6 | `Initialize the Disk - Status` | `disk-status.txt` | options 2 and 4 |
| 7 | `Install Licensed Internal Code - Status` | `lic-status.txt`, `restore-status.txt` | C6004001 to C6004090; re-IPL; options 1 and 3 show their own steps |
| 8 | DST sign-on, `Use Dedicated Service Tools (DST)` | `dst/` | the re-IPL finds the install pending (C6004100); option 2 |
| 9 | `Install Device Type Selection` | `device-type.txt` | 1 tape, 2 optical, 3 virtual |
| 10 | `Select Install Device` | `install-device.txt` | when more than one device of the kind holds media |
| 11 | `Confirm Install of the Operating System` | `os-confirm.txt`, `os-confirm-upgrade.txt`, `os-confirm-refused.txt` | the device, the media, the install action (fresh, slip, upgrade, refused); F10 |
| 12 | `Select a Language Group` | `language.txt` | 2924 |
| 13 | `Confirm Language Feature Selection` | `language-confirm.txt` | F10 |
| 14 | `Install the Operating System` | `install-os.txt` | 1 take the defaults, 2 change the install options; date and time |
| 15 | `Specify Install Options` | `install-options.txt` | licensed programs (1 none, 2 all on the media), job and output queues of a replaced system |
| 16 | `Install the Operating System - Status` | `os-status.txt` | the six stages below |
| 17 | `IPL Options` | `ipl-options.txt` | date, time, the seven choices |
| 18 | `Set Major System Options` | `major-options.txt` | `QAUTOCFG`, `QDEVNAMING`, `QSPCENV` |
| 19 | `Define or Change the System at IPL` | (SST's) | when the IPL options ask for it: the hook `DstEnv::define_system` |
| 20 | Sign On | | C6004190, then the IPL completes |

`No Operating System Installed` (`no-os.txt`, `no-os-no-disks.txt`) is
option 1 on media with no ASP 1 anywhere.

### The stages

| Stage | Text on the panel | SRC | Work |
|---|---|---|---|
| 1 | Prepare ASP 1 and create the system libraries | C6004110, C6004120 | ASP 1 opened (formatted when blank), QSYS and the system libraries when new |
| 2 | Restore the operating system's code into QSYS | C6004130 | `QS00SS1` read from the device and installed through the RSTLICPGM planning: every option whose code is on the media, `*BASE` first |
| 3 | Restore the language objects | | the 2924 loads, in the same restore |
| 4 | Check signatures and translate programs again | C6004140 | every object's signature checked against `/qs400/keys/system.pub` as it is restored; after an upgrade every program in a user library translated from its IR |
| 5 | Create the system values, profiles and configuration | C6004150 | `SAVSYS` media: RSTUSRPRF, RSTCFG, RSTAUT; files after the system save on a tape are left for the storage service's RSTLIB and RST (docs/RECOVERY.md) |
| 6 | Complete the operating system install | C6004160 | the licensed programs asked for |

The software agreements of products installed this way are recorded
accepted by QSECOFR: choosing to install them on these panels, or
listing them in a response file, is the acceptance. `GO LICPGM`
option 11 (and option 1) later installs from the same media: type the
install device (`OPT01`) in place of a save file; the agreement panels
show first (F14 accepts, F15 declines), then `RSTLICPGM DEV(OPT01)` per
option.

### LIC options 1, 3 and 4

- **Option 1, Restore Licensed Internal Code.** The way to put new media
  onto an installed system (a slip install or an upgrade). As on IBM i,
  the menu shows the current load source above the options (the disk
  with the first LIC partition that is not install media) and option 1
  asks nothing more: no load source choice, no confirmation (IBM shows
  a confirmation only for options 2 to 5). With no installed load
  source the menu says so and option 1 refuses. Nothing is partitioned:
  ASP 1 and its unit table, the configuration, the service tools users
  (in the LIC record) and the firmware's boot entries stay. The release
  rule (§4) decides between a slip install and an upgrade, or refuses
  (A6004080, on the menu's message line). The new LIC goes into the
  slot the system did not boot (C6004060), which becomes the default
  (`TAG_LIC_SLOTS` and the firmware's `BootOrder`); where the firmware
  cannot be told, slot A gets the image as well. The status display
  shows the five steps of `RESTORE_STEPS`, then the system re-IPLs from
  the disk, where the operating system install is pending (C6004100;
  DST option 2, as after any LIC install). Once it completes, the LIC is
  copied into the other slot too (the record's TLV `0x0109`), so both
  slots hold the new release. A response file asks for it with
  `lic-option = 1`.
- **Option 3, Install LIC and Recover Configuration.** On IBM i this is
  for a load source that was replaced: the LIC goes onto the new, blank
  load source, and at the re-IPL DST recovers the disk configuration
  (which units belong to which ASP) from what the other configured
  units record about themselves; the data that was on the old load
  source is gone and is restored from a save. QS/400 cannot do that yet,
  and option 3 still does what option 1 does over a load source chosen
  from the disks with QS/400 data (`recover-source.txt`,
  `recover-confirm.txt`). What exists: every unit carries its header
  (unit UUID, ASP UUID and number) and both superblock copies with the
  whole unit table (STORE_FORMAT.md §4 and §5), so the configuration
  survives on the other units. What is missing: (1) a DST `Recover
  configuration` display that reads the newest superblock off the
  surviving units, shows the configuration it found, and on F10 puts
  the new load source's ASP 1 partition in as unit 1; (2) a store that
  opens ASP 1 with unit 1's pages gone. QS/400 has no user ASPs, so
  every object of ASP 1 may have pages on the lost unit: as on IBM i
  for the system ASP, the objects are lost and ASP 1 has to be cleared
  and loaded again from a system save (docs/RECOVERY.md), which makes
  the recovered configuration worth only the units' membership until
  user ASPs exist. A replaced load source of a mirrored ASP 1 is
  option 4, which works.
- **Option 4, Install LIC and Restore Disk Unit Data.** A new load
  source in place of a failed one whose ASP 1 was mirrored: the LIC is
  written as for option 2, ASP 1 is opened on its other units, and the
  new disk's ASP 1 partition replaces the missing half (C6004070); the
  copy from the partner runs in the background as a resume does. The
  IFS is in ASP 1 and comes back with it. On a system installed before
  the IFS moved into ASP 1, the IFS partition of the failed disk was not
  mirrored and comes back empty (restore it with `RST`).

## 3. Automatic install and the response file

Option 4 on the IPL menu installs without prompts: the LIC onto the
first disk, the re-IPL, the operating system from the first install
device, the defaults everywhere. A response file answers every prompt
instead. It is read from `QS400/INSTALL.RSP` on the media, or from the
path `qs400.rsp=PATH` names on the command line (a path on the media),
or over the network: `qs400.rsp=http://...`, `https://...`, `tftp://...`,
or `qs400.rsp=dhcp` for the file beside the network boot's boot file
(below, "Network install").
A D IPL that finds one installs at once, without the menu, when no
disk holds an operating system, or when the file asks for option 1 or 3 and
the media's release is not the installed one; otherwise the menu shows,
so media left in the drive does not install again. The file's text is
kept in the LIC record (TLV `0x0108`) from the LIC install to the end
of the operating system's.

Text, one `key = value` per line, `#` starts a comment, keys in any case
and order, an optional first line `QS400-INSTALL-RSP 1`. Every key has
a default; an empty file installs as option 4 does. A line that does not
parse stops the automatic install with its line number; the defaults
apply.

| Key | Values | Default | Answers |
|---|---|---|---|
| `lic-option` | `1`, `2`, `3`, `4` | `2` | the `Install Licensed Internal Code (LIC)` option |
| `load-source` | resource name (`DD001`), device (`vdb`, `/dev/vdb`), `*FIRST` | `*FIRST` | `Select Load Source Device`; ignored with option 1, which takes the current load source |
| `mirror` | two disks | none | a mirrored pair; may repeat; a disk already in ASP 1 is paired with the other |
| `unit` | disks | none | further unprotected units; may repeat |
| `device` | `*MEDIA`, `*OPT`, `*TAP`, `*VRT` | `*MEDIA` | `Install Device Type Selection`; `*MEDIA` takes a `SAVSYS` medium first, else the boot media |
| `language` | four digits | `2924` | `Select a Language Group` |
| `system-name` | 1 to 8 letters and digits | the default | `QSYSNAME` |
| `ip` | `a.b.c.d/n`, `*IP4DHCP` | none | the installed system's first interface: its first IPL adds it as `ADDTCPIFC INTNETADR('a.b.c.d') LIND(line) SUBNETMASK(...)` (or `INTNETADR(*IP4DHCP) LIND(line)`, an address from DHCP) on the first Ethernet line (a line description, else the first port's link, `ETH0`), autostarted with TCP/IP, `AUTOSTART(*YES)`: QSTRUP's `STRTCP` starts it at the first IPL and every IPL after, and a `*IP4DHCP` interface gets its lease from the DHCP client then |
| `gateway` | `a.b.c.d` | none | with `ip`, `ADDTCPRTE RTEDEST(*DFTROUTE) NEXTHOP('a.b.c.d')` at the same IPL |
| `date`, `time` | `YYYY-MM-DD`, `HH:MM:SS`, `*CURRENT` | `*CURRENT` | `IPL Options` |
| `qsecofr-password` | `*EXPIRED` or a password | `*EXPIRED` | QSECOFR's password: the shipped one expired, or this one, not expired |
| `licpgm` | `*NONE`, `*ALL`, a list of `PRODUCT[/OPTION]` | `*NONE` | `GO LICPGM` option 1 (`*ALL`) or 11 |
| `start-print-writers`, `clear-job-queues`, `clear-output-queues`, `clear-job-logs`, `restricted-state` | `Y`, `N` | `Y`, `N`, `N`, `N`, `N` | `IPL Options` |
| `qstrup` | `LIB/PGM`, `*NONE` | `QSYS/QSTRUP` | `QSTRUPPGM`, written as program and library (`QSYS/QSTRUP` is stored as `QSTRUP    QSYS`) |
| `auto-configure`, `device-naming`, `special-environment` | `Y`/`N`, `*NORMAL`/`*S36`/`*DEVADR`, `*NONE`/`*S36` | `Y`, `*NORMAL`, `*NONE` | `Set Major System Options` |

The harness's two-disk file (`tools/conformance/src/install.rs`):

```
QS400-INSTALL-RSP 1
lic-option = 2
load-source = vdb
mirror = vdb vdc
language = 2924
system-name = QSINST
qsecofr-password = Quartz1
licpgm = *ALL
```

### Network install

IBM's installer has no network display (TCP/IP is configured once the
system runs); QS/400's network answers are the response file's `ip`
(an address, or `*IP4DHCP`) and `gateway`. What the network adds is
where the answers come from:

- `qs400.rsp=URL`: `http://` and `https://` (HTTP/1.1, up to three
  redirects, 1 MiB at most) or `tftp://` (RFC 1350, octet mode).
  `https` checks the server against the shipped roots (`certs::roots`)
  and the PEM certificates in `QS400/TRUST.PEM` on the media, by name.
- `qs400.rsp=dhcp`: `INSTALL.RSP` beside the boot file the DHCP server
  named: the same directory of a URL boot file (UEFI HTTP boot), or of a
  TFTP one on the next server (`siaddr`) for PXE.
- A D IPL the firmware booted over the network (`BootCurrent`'s device
  path has a MAC, IPv4, IPv6 or URI node) with no response file on the
  media looks beside the boot file the same way; none there is no error.

Either way the IPL first gives the first Ethernet link an address from
DHCP (`net/dhcp`, `TCPIP.md` "DHCP client"; `qs400.ip=` overrides it),
with the lease's router as the default route. The fetched text is used
as a file on the media would be and kept in the LIC record the same way.

The installed system does not depend on the IPL's own DHCP address: once
it is installed, an IPL from its disk takes no address itself, and TCP/IP
brings up the interface the answers configured (`ADDTCPIFC ...
AUTOSTART(*YES)`), asking DHCP for a `*IP4DHCP` one.

The kernel command line is baked into each UKI
(`CONFIG_CMDLINE_OVERRIDE`), so a boot loader cannot add `qs400.rsp=`:
a PXE install serves the media UKI (`EFI/BOOT/BOOTX64.EFI` of the ISO)
as the boot file and `INSTALL.RSP` beside it. With UEFI PXE the DHCP
server's `filename` (option 67) is the UKI's TFTP path; with UEFI HTTP
boot it is the UKI's URL; an iPXE chain loads the UKI with `chain
http://server/qs400/BOOTX64.EFI` and must hand it the DHCP server's
answer unchanged (iPXE's own DHCP lease is not seen by the kernel, so the
IPL asks again). The boot file's directory then holds `INSTALL.RSP`.

The payload (the LIC, the operating system, the licensed programs)
still comes from install media the system can read: a CD, a stick, the
image a BMC presents as virtual media. `BLOCKED(rel)`: a diskless
install with the payload over the network needs the ISO fetched into
memory and attached as a block device (a loop device: an ioctl outside
the `unsafe` sites) or a payload-carrying netboot UKI.

QEMU's user network is a PXE setup on its own: `-netdev
user,tftp=DIR,bootfile=qs400/BOOTX64.EFI` with the NIC first in the
boot order (`bootindex=0`). Install step 15 boots that way with the ISO
as a CD for the payload and `ip = *IP4DHCP` in the served answers; the
installed system's first IPL then holds a lease from QEMU's DHCP server.

## 4. Upgrades and slip installs

Installing over an existing ASP 1 keeps every object. The release rule
is `release::check_upgrade(installed, media)`:

| Media against the system | Result |
|---|---|
| the same `VxRyMz` | slip install: objects replaced, release unchanged |
| a newer modification, same version and release | upgrade |
| a newer version or release, the system at its `*PRV` or later | upgrade |
| a newer version or release, the system older than its `*PRV` | refused: install `*PRV` first |
| older | refused: downgrade |

`*PRV` of a release is the newest modification 0 release below its
version and release among the releases Quartz has defined
(`release::KNOWN`: V1R0M0, V1R1M0). The store opens an ASP 1 of an older
release only when the rule allows the upgrade (`release::store_may_open`,
`STORE_FORMAT.md` §3); the install records the new release in the
superblock once the operating system is in.

The operating system install over an installed one is a product
replacement with RSTLICPGM's semantics:

- every object of the new `QS00SS1` load restored over the old one in
  QSYS (and the other libraries the load names); objects only the old
  load listed deleted; nothing else in the system libraries and nothing
  in user libraries touched;
- the restored objects carry the new build's signatures, checked
  against the system key as they come in;
- after an upgrade, every `*PGM`, `*SRVPGM` and `*MODULE` outside QSYS,
  QSYS2, QTEMP, QSPL and QHLPSYS is translated again from its IR
  (`ile::store::retranslate`), so the new release's code generator
  makes every native copy; a slip install translates only code an older
  code generator made.

The current release is V1R1M0. A build claims another release with
`QS400_RELEASE=VxRyMz` at build time (`lic/release/build.rs`); the
harness's step 10 installs a `QS400_RELEASE=V1R0M0` build's ISO
(`QS400_OLD_ISO`; built from `becee24b`, the last tree with object header
and save stream version 1) on a blank disk, makes user data (a data area,
a journaled SQL table with rows, spooled files, a `SAVLIB` holding an SQL
table in a save file), upgrades it with the suite's V1R1M0 media, checks
every piece, uses the V1R0M0 table from SQL (`SELECT`, `INSERT`, `CREATE
INDEX`, `CREATE VIEW`; it has no `*SQLINF`), restores the V1R0M0 save
with `RSTLIB` and reads its table, and has the V1R0M0 media refused.

## 5. QEMU

`cargo run -p build -- test --media iso --install` runs the install
suite: steps 1 to 7 (docs/PHASE2.md, "Running"), then the install
matrix:

| Step | Proves |
|---|---|
| 1 automatic install | option 4 on a blank disk; QS00SS1 restored from the media; `CHKOBJITG OBJ(QSYS/*ALL) CHKSIG(*SIGNED)` clean |
| 3 interactive install | every panel of §2 over serial, `Specify Install Options` included |
| 8 response file mirrored | a FAT32 copy of the media with `QS400/INSTALL.RSP`: no prompts, two disks mirrored, the answers applied, every licensed program installed, signatures clean |
| 9 SAVSYS and recovery | `CRTDEVTAP`, `CRTIMGCLG`, `ADDIMGCLGE`, `LODIMGCLG`, then `SAVSYS DEV(TAPVRT01)`, `CHKTAP` (a system save), `SAVLIB LIB(*NONSYS)` and `SAV` after it; the volume copied onto a disk (`svc.vrtout`), the system disk wiped, option 4: LIC and operating system from the tape, RSTUSRPRF, RSTCFG, RSTAUT, then the storage service's RSTLIB, RST and RSTAUT; the profile, the library and the IFS file made before the save are back |
| 10 slip install V1R1M0 | option 1 with V1R1M0 media over V1R0M0 (header and stream version 1), the upgrade's panels, user data kept, the store at V1R1M0; the V1R0M0 SQL table selected, inserted into, indexed and viewed; the V1R0M0 `SAVLIB` restored with its SQL table; then V1R0M0 media refused |
| 11 power pull mid-install | killed during the LIC install; the reinstall reaches sign-on, `chkstg` clean |
| 12 four-unit mirror | step 1's disk with three blank disks: added to ASP 1 in DST, `svc.obj fill` puts data on every unit, Start mirrored protection pairs 1 with 4 and 2 with 3 (the data of 3 and 4 moved first), the copies at the re-IPL, the data intact; disk 4 pulled: A6001255, its unit Missing, the IPL completes |
| 13 device parity RAID 5 | the same four units, RAID 5 started over all of them in DST and synchronized at the IPL; disk 4 pulled: A6001257, `DPY/Failed`, every page read and written through parity; disk 4 back: rebuilt at the IPL, `DPY/Active`, data intact, `chkstg` clean |
| 14 restore LIC in place | step 1's disk with a user library: the media's option 1 restores the LIC onto the load source with no prompts (a slip install of the same release), the operating system goes in again from DST, then the disk alone IPLs from the new slot with the library intact and `chkstg` clean |
| 15 network install | PXE boot of the media UKI over QEMU's TFTP, `INSTALL.RSP` beside it, payload from the CD; QSTRUP starts the `*IP4DHCP` interface (`AUTOSTART(*YES)`) and it gets a lease; again after an IPL from the disk alone |
| 16 hot spare | four units: RAID 5 with a hot spare started in DST (option 7); disk 4 pulled before an IPL: A6001257, the spare takes its place and is rebuilt, `DPY/Active`, data intact, `chkstg` clean |

`--media iso-raw` is the D IPL from the USB image: the hybrid image
written raw to a disk boots, installs onto a blank disk beside it, and
the installed disk boots alone.

## 6. Secure Boot

QS/400 V1R1M0 boots with Secure Boot **off**. Its UKIs carry no
signature from the Microsoft UEFI CA (there is no shim), so a firmware
that enforces Secure Boot with its factory keys refuses them ("Access
Denied", or the firmware's own security violation screen). Either turn
Secure Boot off, or enrol the QS/400 key and keep it on.

`QSHWQUAL` reports `SECBOOT` as `PASS` with Secure Boot off and, with it
on, `WARN: QS/400 V1R1M0 is not Secure Boot signed; disable Secure Boot
or enrol the QS/400 key`. The installer checks the same variable when it
writes the LIC: with Secure Boot on and signed slot UKIs it says the
system boots while the QS/400 key stays enrolled; with unsigned UKIs it
prints the warning above.

**The QS/400 key.** `build release` signs the three UKIs (slot A, slot B,
the media) with an Authenticode signature (PKCS#7, SHA-256, RSA-2048) by
the QS/400 Secure Boot db key (`tools/pesign`). The release key comes
from `QS400_SB_KEY` (PKCS#8, PEM or DER) and `QS400_SB_CERT` (X.509,
PEM or DER); `cargo run -p pesign -- keygen DIR` makes one. Without them
the development pair checked in as `tools/pesign/keys/qs400-db-DEV-ONLY.*`
signs, and the build says so: a release that leaves the building is
never signed with it. `cargo run -p pesign -- verify FILE CERT` checks a
signed UKI as the firmware does.

The media carry the certificate: `QS400/EFI/keys/qs400-db.cer` (DER)
and `qs400-db.esl` (an `EFI_SIGNATURE_LIST`) in the ISO tree, and the
same two files under `EFI/keys/` in the boot image, the FAT partition a
firmware's file browser reads when the ISO is written to a USB stick.

**Enrolling it.** The certificate goes into the firmware's `db`
(authorized signatures), beside the keys already there:

- *OVMF (QEMU, Proxmox, libvirt).* Boot to the firmware setup (Esc at
  the logo), Device Manager, Secure Boot Configuration, Secure Boot Mode
  `Custom Mode`, Custom Secure Boot Options, DB Options, Enroll
  Signature, Enroll Signature Using File: the boot image's
  `EFI/keys/qs400-db.cer`; Commit Changes and Exit, then reset. On
  Proxmox the EFI disk must have been made with pre-enrolled keys (a PK
  and KEK exist) for Secure Boot to be enforced at all.
- *VMware ESXi and Workstation.* The EFI setup offers no enrolment of a
  file; set `uefi.allowAuthBypass = "TRUE"` in the VM's `.vmx`, then
  enrol from the setup as for OVMF, and remove the setting afterwards.
- *Hyper-V Generation 2.* The Secure Boot templates (Microsoft Windows,
  Microsoft UEFI Certificate Authority, Open Source Shielded VM) cannot
  take another key, and none of them starts the QS/400 UKI: turn Secure
  Boot off in the VM's Security settings.
- *Physical machines.* In the firmware setup's Secure Boot key
  management (on a Dell PowerEdge: System Security, Secure Boot, Secure
  Boot Policy `Custom`, Expert Key Management, `db`, Append from File),
  choose `qs400-db.cer` on the USB stick. The menu names differ from one
  firmware to the next.

`qs400-db.esl` is for tools that take a signature list (`efi-updatevar
-a -f qs400-db.esl db`, `virt-fw-vars --add-db`). No `.auth` file is
shipped: an authenticated `db` update must be signed by the machine's
own KEK, which only its owner holds. No enrolment EFI application is
shipped either; the firmware's own setup does the work.

The signing was checked under QEMU with OVMF's Secure Boot build
(`OVMF_CODE_4M.secboot.fd`): an unsigned UKI and a UKI signed with a key
not in `db` are refused (`Access Denied -- rejected probably by Secure
Boot`); the same UKI boots to the IPL (`SRC C1001000`) once the QS/400
certificate is in `db`.
