# Recovering a QS/400 system

A runbook for bringing a system back from its saves after the loss of
its disks, laid out the way IBM's recovery guide is: what to save so a
recovery is possible, how to choose the procedure, then the procedure
step by step with what each step restores and what to check. The words
are Quartz's; the command names, menu options and their order are IBM
i's, since they are the interface. The install panels are docs/INSTALL.md.

## 1. Saving for a recovery

A system can be recovered completely from two kinds of save:

| Save | Command | Holds | How often |
|---|---|---|---|
| System save | `SAVSYS DEV(TAPVRT01)` (or `OPTVRT01`) | both slots of the Licensed Internal Code, every installed product's load (QSYS, the operating system first), the security data (`SAVSECDTA`), the configuration (`SAVCFG`), in the install media layout | after every PTF, upgrade or configuration change, and at least once a release |
| User data | `SAVLIB LIB(*NONSYS)`, `SAV OBJ(('/*') ('/QSYS.LIB' *OMIT))` | every user library and the IFS | daily, or with `SAVCHGOBJ` between |

`SAVSYS` needs `*SAVSYS` special authority. Run it with the system as
quiet as it can be (IBM i requires the restricted state; QS/400 saves
consistent product loads and security data while active, but objects
changed during the save are saved as they were when reached). Its
output is install media: `CHKTAP` and `DSPOPT DATA(*SAVRST)` show the
release and what it holds, and the installer can boot a recovery from
it.

Keep with the saves: the release and PTF level (`DSPPTF`), the disk
configuration (DST option 4, `Work with disk units`), the service tools
user IDs, and QSECOFR's password at the time of the save.

> `BLOCKED(phase11)`: until the device layer resolves virtual devices
> through image catalogs, a virtual device is bound to a volume file or
> disk with the dev word `svc.vrtdev TAPVRT01 PATH`. `SAVLIB` and `SAV`
> to tape come with the same layer; until then user data is saved to
> save files and copied off the system.

## 2. Choosing the procedure

| What was lost | Procedure |
|---|---|
| A disk of a mirrored pair, the system still running | DST, `Work with disk units`: replace the unit; nothing is restored (docs/DST.md) |
| The load source of a mirrored ASP 1 | §3 with LIC option 4, Install LIC and Restore Disk Unit Data |
| The LIC damaged, ASP 1 intact | §3 with LIC option 3, Install LIC and Recover Configuration |
| Every disk, or ASP 1 unusable | §3 and §4 in full: the complete system recovery |
| One library or object | `RSTLIB`, `RSTOBJ`, `RST` from the user-data saves; no reinstall |

## 3. Recovering the Licensed Internal Code

1. **Prepare.** Have the system save media and the distribution media of
   the same release. Attach the system save (a tape volume, a virtual
   optical image or the disk it was copied onto) and the distribution
   media (CD, USB image or FAT copy).
2. **IPL from the media.** The D IPL shows `IPL or Install the System`.
   Take option 2 (Install the operating system), then on `Install
   Licensed Internal Code` option 1.
3. **Choose the LIC option** on `Install Licensed Internal Code (LIC)`:
   - 2, Install Licensed Internal Code and Initialize System, for new
     or wiped disks: everything on the chosen load source is lost.
   - 3, Install Licensed Internal Code and Recover Configuration, when
     ASP 1 is intact: the disk configuration and every object stay.
   - 4, Install Licensed Internal Code and Restore Disk Unit Data, for
     a new load source in a mirrored ASP 1.
4. **Select the load source** and confirm with F10. When a system save is
   attached the installer takes the LIC from it (the saved system's own
   LIC and LIC PTFs; the serial log says `the LIC from the system save
   on TAPVRT01`), else from the distribution media. Status:
   C6004001 to C6004090, then the re-IPL.
5. **Check.** The re-IPL reports C6004100 (operating system install
   pending). For option 4, C6004070 and the copy from the partner
   running in the background.

## 4. Recovering the operating system and the data

6. **Sign on to DST** (the re-IPL lands there unless the install is
   automatic) and take option 2, Install the operating system.
7. **Choose the system save as the install device**: `Install Device
   Type Selection` option 1 (tape) or 3 (virtual), the device, F10 on
   `Confirm Install of the Operating System`, language 2924, F10, and on
   `Install the Operating System` option 1. The stages
   (docs/INSTALL.md, "The stages") restore, in IBM's order:
   - the operating system and licensed programs, as saved (C6004130);
   - `RSTUSRPRF USRPRF(*ALL)`: user profiles, their passwords and
     authorization lists from the security data;
   - `RSTCFG OBJ(*ALL)`: device descriptions and the configuration;
   - `RSTAUT`: the private authorities held for the profiles, applied
     to the objects that are back (C6004150).
8. **Answer `IPL Options`** (date and time; start the system to
   restricted state for the rest of the recovery) and `Set Major System
   Options`; the IPL reaches sign-on.
9. **Sign on as QSECOFR** with the password it had when the system was
   saved.
10. **Restore the user libraries**: `RSTLIB SAVLIB(*NONSYS) DEV(...)`
    (or library by library from save files: `RSTLIB SAVLIB(lib)
    DEV(*SAVF) SAVF(...)`).
11. **Restore the IFS**: `RST DEV(...) OBJ(('/*') ('/QSYS.LIB' *OMIT))`.
12. **Restore the authorities again**: `RSTAUT USRPRF(*ALL)`, since the
    objects of steps 10 and 11 came back after the first one.
13. **Apply the journals** where files were journaled after the saves:
    `APYJRNCHG` from the receivers that were saved or kept.
14. **Check**: `DSPSFWRSC` lists the products; `CHKPRDOPT` for each;
    `CHKOBJITG OBJ(QSYS/*ALL) CHKSIG(*SIGNED)` clean; `DSPLOG` and the
    job log of the restores for objects not restored; `WRKDSKSTS` for
    the units; then an IPL to normal mode.

> `BLOCKED(phase11)`: steps 10 and 11 from tape wait for `SAVLIB` and
> `SAV` to tape in the device layer; the installer restores the files of
> a system save only. Save files copied back onto the system work today.

## 5. An automatic recovery

The same procedure without prompts: the system save attached, the
distribution media with a response file (docs/INSTALL.md, "Automatic
install and the response file") or option 4 on the IPL menu. The
installer takes the LIC from the system save, re-IPLs, takes the
operating system from it (`device = *MEDIA` prefers a system save) and
runs the restores of step 7. Steps 9 to 14 follow by hand. The harness
does exactly this in the install suite's step 9.

## 6. After the recovery

- Save the system again (`SAVSYS`) and the user data: the recovered
  system's LIC record and slots are new.
- The service tools user IDs are the shipped ones after an option 2
  install (they live in the LIC record, not in the saves): change their
  passwords in DST.
- System values that live in the LIC record (`QIPLTYPE` and the rest of
  the IPL subset) are the defaults after option 2 or 4; set them again.
