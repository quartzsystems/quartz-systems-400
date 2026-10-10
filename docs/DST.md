# Dedicated Service Tools

DST is the set of screens that run before the object store is in use:
from option 3 of the attended IPL menu, from a manual IPL that stops on
a missing disk unit (B1001030), and from the installer. The `svctools`
crate holds every screen and the service tools user model; `slic`
provides the system behind them through the `DstEnv` trait and drives
the screens from the console thread. Phase 8's SST shows the same
panels inside a job through another `DstEnv`.

Titles and option numbers are the functional interface and follow IBM's
menus; every sentence on the screens is Quartz's.

## Screen kit

All screens share the geometry of the attended IPL menu, built by
`svctools::ui`:

| Row | Content |
|---|---|
| 1 | title, centered, white |
| 2 | `System:  <name>`, right-aligned |
| 4.. | body: menu options from row 6, list headings on row 6 and rows from row 7, form lines from row 4 |
| 20 | `Selection` on menus |
| 21 | the selection field |
| 23 | function keys |
| 24 | message line (red for errors) |

Panels are a stack (`svctools::panel::Runner`): Enter reports every
input field in order, F12 goes back one panel, F3 back to the first, F10
continues a confirmation, F7 and F8 page a long display. A panel can
push another, replace itself, unwind several with a message, or end the
application with an outcome (`PerformIpl`, `InstallOs`, `AutoInstall`,
`Exit`).

Over serial the same screens are echoed as text and driven with the
service console: `type TEXT` types into the current field, `key NAME`
presses a key (`enter`, `tab`, `backtab`, `fieldexit`, `home`, `f1`..
`f24`, ...), and a line that is not a command is typed into the current
field and entered. `screen` dumps the display.

## Golden files

Every screen has a golden file in `conformance/golden/dst/`, rendered
against a fixed in-memory environment by `svctools/tests/golden.rs`,
`disk.rs` and `environment.rs` and compared whitespace-exact. Run those
tests with `QS400_UPDATE_GOLDEN=1` to rewrite the files after a
deliberate change. The harness step `12 DST sign-on` compares the
guest's own screens with the same files.

| Screen | Golden file |
|---|---|
| Dedicated Service Tools (DST) Sign On | `signon.txt` |
| Change Service Tools User Password | `change-password.txt` |
| Use Dedicated Service Tools (DST) | `main.txt`, `main-not-authorized.txt` |
| Work with Disk Units | `disk.txt` |
| Work with Disk Configuration | `disk-config.txt` |
| Display Disk Configuration (menu, status, capacity, protection, non-configured) | `disk-display.txt`, `disk-status.txt`, `disk-capacity.txt`, `disk-protection.txt`, `disk-nonconfigured.txt` |
| Work with ASP Threshold | `asp-threshold.txt` |
| Work with ASP Configuration, Add Units, Remove Units | `asp-config.txt`, `add-units.txt`, `add-units-confirm.txt`, `remove-units.txt` |
| Work with Mirrored Protection, Start, Stop | `mirror.txt`, `mirror-start.txt`, `mirror-start-confirm.txt`, `mirror-stop.txt` |
| Work with Device Parity Protection | `disk-parity.txt` |
| Work with Disk Unit Recovery, Replace, Procedures, Suspend, Resume | `disk-recovery.txt`, `replace-unit.txt`, `disk-procedures.txt`, `suspend.txt`, `resume.txt` |
| Work with DST Environment, Active Service Tools, System Devices | `env.txt`, `env-active.txt`, `env-devices.txt` |
| Work with Service Tools User IDs, Display, Privileges, Create | `users.txt`, `user-display.txt`, `user-privileges.txt`, `user-create.txt` |
| Work with System Values | `sysvals.txt` |
| Start a Service Tool, Product Activity Log, Display/Alter/Dump, LIC Log, Hardware Service Manager | `tools.txt`, `pal.txt`, `dad.txt`, `lic-log.txt`, `hsm.txt`, `hsm-packaging.txt`, `hsm-failed.txt` |
| Work with System Security | `security.txt` |

The installer's screens have their own golden files under
`conformance/golden/install/`, from `svctools/tests/install.rs`:
`lic-menu`, `lic-options`, `load-source`, `lic-confirm`, `disk-status`,
`lic-status`, `device-type`, `os-confirm`, `language`,
`language-confirm`, `os-status`, `ipl-options` and `major-options`. The
sequence they form is described in PHASE2.md.

## Menus

```
Use Dedicated Service Tools (DST)
  1. Perform an IPL                      -> re-IPL (restart)
  2. Install the operating system        -> the installer, on media
  3. Work with Licensed Internal Code    not available
  4. Work with disk units
  5. Work with DST environment
  6. Select DST console mode             not available
  7. Start a service tool
  8. Perform automatic installation      -> the installer without prompts
  9..12, 14                              not available
 13. Work with system security
```

`Work with disk units`: 1 disk configuration, 2 disk unit recovery;
3 and 4 are not available.

`Work with disk configuration`: 1 display (status, capacity,
protection, non-configured units), 2 ASP threshold, 3 ASP configuration
(2 add units, 4 remove units; 3 and 6 are not available), 4 mirrored
protection (1 display, 2 start, 3 stop; 4 not available), 5 device
parity protection (reports no capable controllers), 6 and 7 not
available.

`Work with disk unit recovery`: 3 replace configured unit, 6 problem
recovery procedures, 7 suspend, 8 resume; the rest are not available.

`Work with DST environment`: 1 active service tools, 2 system devices,
3 service tools user IDs, 4 system values (the LIC-side subset), 5 not
available.

`Start a service tool`: 1 product activity log, 4 display/alter/dump
(display only), 5 LIC log, 7 hardware service manager (packaging,
logical, failed views); 2, 3 and 6 not available.

## Disk configuration

Rows come from the store's unit table and the hardware table:

```
ASP Unit  Serial Number     Type Model    Resource  Status
  1    1  QM00000           QEMU HARDDISK DD001     Active
  1    2  QM00001           QEMU HARDDISK DD002     Suspended
  1    3  QM00002           QEMU HARDDISK DD003     Unprotected
```

Unit numbers on the displays start at 1; the store numbers units from
0. Status is `Unprotected`, `Active`, `Suspended`, `Resuming nn%`,
`Not operational` or `Missing`.

- Add units: pick non-configured disks (whole disks with no QS/400
  partitions and no unit header); each becomes an unprotected unit.
- Remove units: data on the unit moves to the other units, then it
  leaves the configuration (only the last unit can leave).
- Start mirrored protection: each unprotected unit is paired with the
  first non-configured disk at least as large (another disk preferred).
  The pair is formed at once and the copy runs at the next IPL
  (`C1001075 Mirrored protection synchronization`).
- Stop mirrored protection: the chosen half stays, its partner leaves
  the configuration and its header is wiped.
- Replace configured unit: a missing, failed or suspended half is
  replaced by a non-configured disk and the copy starts in the
  background.
- Suspend and resume: resume runs in the background; the status column
  shows its percent.

A D IPL opens the store on demand when a disk option needs it.

## Service tools user IDs

Users live in the LIC record (TLV `0x0102`), separate from the
operating system's user profiles (`MIGRATE(phase6)`). Passwords are
Argon2id hashes in PHC form. Three wrong passwords disable the ID; a
user with the user ID privilege enables it again. A new or reset
password is expired: the first sign-on goes through `Change Service
Tools User Password`. The last ID with the user ID privilege cannot
lose it or be deleted.

Shipped IDs and privileges:

| User | Default password | Privileges |
|---|---|---|
| QSECOFR | QSECOFR, expired | all |
| QSRV | QSRV | all but security and user IDs |
| 11111111 | 11111111 | disk operations, logs, hardware, IPL |
| 22222222 | 22222222 | disk operations, IPL |

Privileges: disk units administration, disk units operations, service
tool security, service tool user IDs, system values, display/alter/dump,
logs, hardware service manager, install, IPL, dumps and traces.

## Other records in the LIC partition

- Product activity log: attention and fatal reference codes with their
  times, the newest 64, as reserved TLVs (`0x0003`); `slic` records one
  whenever it emits an `A6`/`B1` code.
- Security flags: one byte under TLV `0x0103` (allow default-password
  change, new certificates, system value security changes, add
  certificates); only the first and third have an effect.
- Resume journal: at offset 1 MiB, the chunk bitmap of a mirrored half
  being copied, so a crash continues where it stopped.
