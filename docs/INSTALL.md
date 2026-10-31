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

On tape the manifest is file 1 (label `QS400MANIFEST`), the payload
files follow in manifest order with 64 KiB blocks, IBM standard labels
throughout (docs/MEDIA.md, "Tape: labels"). Files after them (a
`SAVLIB *NONSYS`, a `SAV`) belong to the recovery.

`instmedia::savsys::recognize_tape` and `recognize_iso` tell install
media from any other volume; `CHKTAP` and `DSPOPT DATA(*SAVRST)` show
what they say (`BLOCKED(phase11)`: the commands come with the device
layer; the recognition and its tests are in `media/instmedia`).

### Install devices

The operating system install reads from an install device:

| Kind | Name | What |
|---|---|---|
| Optical | `OPT01` | the boot media of the D IPL: CD, raw image, or FAT copy |
| Virtual | `TAPVRTnn` | a disk whose first bytes are a virtual tape volume (`QSTAPVOL`) holding install media |
| Virtual | `OPTVRTnn` | a disk holding an ISO 9660 image with install media |
| Tape | `TAPnn` | `BLOCKED(phase11)`: physical drives come with the device layer |

Disks holding install media are never offered as load sources or units
and do not count as non-configured disks (`A6001260`).

## 2. The flow, panel by panel

The panels are Quartz's text on IBM's titles, option numbers and
geometry. Serial shows every screen and every SRC; the harness drives
them with `svc.key` and `svc.type`.

| # | Panel | Golden | Notes |
|---|---|---|---|
| 1 | `IPL or Install the System` | (slic's menu) | D IPL from the media; 1 IPL, 2 install, 3 DST, 4 automatic install |
| 2 | `Install Licensed Internal Code` | `lic-menu.txt` | 1 install, 2 DST, 3 not available |
| 3 | `Install Licensed Internal Code (LIC)` | `lic-options.txt` | 2, 3 and 4 work |
| 4a | `Select Load Source Device` | `load-source.txt` | option 2: every disk |
| 5a | `Install LIC and Initialize System - Confirmation` | `lic-confirm.txt` | F10 |
| 4b | `Select Load Source Device` | `recover-source.txt` | option 3: the disks with QS/400 data |
| 5b | `Install LIC and Recover Configuration - Confirmation` | `recover-confirm.txt` | F10; the release rule is checked here |
| 4c | `Select Load Source Device` | `restore-source.txt` | option 4: the blank disks |
| 5c | `Install LIC and Restore Disk Unit Data - Confirmation` | `restore-confirm.txt` | F10 |
| 6 | `Initialize the Disk - Status` | `disk-status.txt` | options 2 and 4 |
| 7 | `Install Licensed Internal Code - Status` | `lic-status.txt` | C6004001 to C6004090; re-IPL |
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
| 5 | Create the system values, profiles and configuration | C6004150 | `SAVSYS` media: RSTUSRPRF, RSTCFG, RSTAUT (docs/RECOVERY.md) |
| 6 | Complete the operating system install | C6004160 | the licensed programs asked for |

The software agreements of products installed this way are recorded
accepted by QSECOFR: choosing to install them on these panels, or
listing them in a response file, is the acceptance. `GO LICPGM`
option 11 (and option 1) later installs from the same media: type the
install device (`OPT01`) in place of a save file; the agreement panels
show first (F14 accepts, F15 declines), then `RSTLICPGM DEV(OPT01)` per
option.

### LIC options 3 and 4

- **Option 3, Install LIC and Recover Configuration.** Over an installed
  load source: nothing is partitioned, ASP 1 and its unit table stay.
  The release rule (§4) decides between a slip install and an upgrade,
  or refuses (A6004080). The new LIC goes into the slot the system did
  not boot (C6004060), which becomes the default (`TAG_LIC_SLOTS` and
  the firmware's `BootOrder`); where the firmware cannot be told, slot A
  gets the image as well. The operating system install follows at the
  re-IPL. Once it completes, the LIC is copied into the other slot too
  (the record's TLV `0x0109`), so both slots hold the new release.
- **Option 4, Install LIC and Restore Disk Unit Data.** A new load
  source in place of a failed one whose ASP 1 was mirrored: the LIC is
  written as for option 2, ASP 1 is opened on its other units, and the
  new disk's ASP 1 partition replaces the missing half (C6004070); the
  copy from the partner runs in the background as a resume does. The
  IFS partition of the failed disk is not mirrored and comes back empty
  (restore it with `RST`).

## 3. Automatic install and the response file

Option 4 on the IPL menu installs without prompts: the LIC onto the
first disk, the re-IPL, the operating system from the first install
device, the defaults everywhere. A response file answers every prompt
instead. It is read from `QS400/INSTALL.RSP` on the media, or from the
path `qs400.rsp=PATH` names on the command line (a path on the media;
`qs400.rsp=http://...` is `BLOCKED(phase11)` until the network boot).
A D IPL that finds one installs at once, without the menu, when no
disk holds an operating system, or when the file asks for option 3 and
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
| `lic-option` | `2`, `3`, `4` | `2` | the `Install Licensed Internal Code (LIC)` option |
| `load-source` | resource name (`DD001`), device (`vdb`, `/dev/vdb`), `*FIRST` | `*FIRST` | `Select Load Source Device` |
| `mirror` | two disks | none | a mirrored pair; may repeat; a disk already in ASP 1 is paired with the other |
| `unit` | disks | none | further unprotected units; may repeat |
| `device` | `*MEDIA`, `*OPT`, `*TAP`, `*VRT` | `*MEDIA` | `Install Device Type Selection`; `*MEDIA` takes a `SAVSYS` medium first, else the boot media |
| `language` | four digits | `2924` | `Select a Language Group` |
| `system-name` | 1 to 8 letters and digits | the default | `QSYSNAME` |
| `ip` | `a.b.c.d/n` | QEMU's | the first link's address at IPL (`BLOCKED(phase11)`: kept for the install IPL; the TCP/IP configuration takes it after) |
| `gateway` | `a.b.c.d` | none | the default route (with `ip`) |
| `date`, `time` | `YYYY-MM-DD`, `HH:MM:SS`, `*CURRENT` | `*CURRENT` | `IPL Options` |
| `qsecofr-password` | `*EXPIRED` or a password | `*EXPIRED` | QSECOFR's password: the shipped one expired, or this one, not expired |
| `licpgm` | `*NONE`, `*ALL`, a list of `PRODUCT[/OPTION]` | `*NONE` | `GO LICPGM` option 1 (`*ALL`) or 11 |
| `start-print-writers`, `clear-job-queues`, `clear-output-queues`, `clear-job-logs`, `restricted-state` | `Y`, `N` | `Y`, `N`, `N`, `N`, `N` | `IPL Options` |
| `qstrup` | `LIB/PGM`, `*NONE` | `QSYS/QSTRUP` | `QSTRUPPGM` |
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

A test build claims another release with `QS400_RELEASE=V1R1M0` at
build time (`lic/release/build.rs`); the harness's slip step installs
such a build's ISO (`QS400_SLIP_ISO`) over V1R0M0.

## 5. QEMU

`cargo run -p build -- test --media iso --install` runs the install
suite: steps 1 to 7 (docs/PHASE2.md, "Running"), then the install
matrix:

| Step | Proves |
|---|---|
| 1 automatic install | option 4 on a blank disk; QS00SS1 restored from the media; `CHKOBJITG OBJ(QSYS/*ALL) CHKSIG(*SIGNED)` clean |
| 3 interactive install | every panel of §2 over serial, `Specify Install Options` included |
| 8 response file mirrored | a FAT32 copy of the media with `QS400/INSTALL.RSP`: no prompts, two disks mirrored, the answers applied, every licensed program installed, signatures clean |
| 9 SAVSYS and recovery | `SAVSYS DEV(TAPVRT01)`, the system disk wiped, option 4: LIC and operating system from the tape, RSTUSRPRF, RSTCFG, RSTAUT; the profile made before the save is back |
| 10 slip install V1R1M0 | option 3 with V1R1M0 media over V1R0M0, the upgrade's panels, user data kept, the store at V1R1M0; then V1R0M0 media refused |
| 11 power pull mid-install | killed during the LIC install; the reinstall reaches sign-on, `chkstg` clean |

`--media iso-raw` is the D IPL from the USB image: the hybrid image
written raw to a disk boots, installs onto a blank disk beside it, and
the installed disk boots alone.
