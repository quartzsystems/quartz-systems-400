# Phase 11: save and restore, media, licensed programs, PTFs, service

Phase 11 makes QS/400 saveable, restorable, installable and serviceable
the way IBM i is: the save/restore family over the QS/400 save format,
save files, tape and optical media (virtual now; physical as code ready
for hardware), image catalogs, the licensed-program model behind
`GO LICPGM`, PTFs, problem management and the remaining service tools.

Each area has its own document; this file is the inventory, the
deviations, what is blocked or deferred, the bench and the acceptance
state. It grows with the phase.

| Area | Document | Crates |
|---|---|---|
| Save format and engine | `SAVE_FORMAT.md` | `save/savfmt`, `save/saverst` |
| Tape, optical, image catalogs | `MEDIA.md` | `media/tape`, `media/optical`, `media/imgclg` |
| Licensed programs | `LICPGM.md` | `licpgm/licpgm`, `licpgm/lpstore`, `licpgm/lpbuild`, `lic/osobjects` |
| PTFs, the update service | `PTF.md`, `UPDATE_PROTOCOL.md` | `ptf/ptf`, `ptf/update` |
| Problem log, service tools | `SERVICE.md` | `service/prb`, `devtools/sst` |
| Media in the system | `MEDIA.md` | `media/medio` |
| Console scaling, storage controllers | `CONSOLE.md`, `HARDWARE_NOTES.md` | `lic/console`, `lic/slic` |
| Cleanup, backup, `GO SAVE`/`RESTORE` 20 to 23 | `BACKUP.md` | `work/wm` (`opa`) |
| Open questions | `SVC_OPEN_QUESTIONS.md` | |

## Oracles

| System | Release | Reached through | Used for |
|---|---|---|---|
| Cody's box (primary) | V7R1M0 | JDBC on 8471 (`QSYS.QCMDEXC`); the command server on 8475 dropped every connection | the restore rules: 20 cases in `tools/conformance/oracle/save/primary/` against `QSTEST` and `/home/QSTEST` only, the `QASAVOBJ`/`QASRRSTO` outfile formats, the print captures of `DSPSFWRSC`, `DSPPTF`, `DSPIPLA`, devices and problems |
| pub400.com | V7R5M0 | telnet 23, host servers | 369 screen goldens under `tools/conformance/golden/phase11/` (menus, prompts, `DSPSFWRSC`, `DSPSAVF`), one `SAVLIB` of the account's library to a save file |

Both systems were left as found (`tools/conformance/oracle/save/README.md`
records the before and after); two inquiries on the primary's `QSYSOPR`
were answered by hand during the capture, before every job got
`INQMSGRPY(*DFT)`.

## Inventory

| Commit | What |
|---|---|
| `cac190fa` | the save stream format and its codec |
| `e9390580` | the saver and restorer, save locks, save history |
| `771e28f1`..`8bc811fe` | tape: transports, virtual volumes, labels, `st`/`sg` transcripts, media library |
| `b9445c0b` | save files, `SAVLIB`/`SAVOBJ`/`RSTLIB`/`RSTOBJ` |
| `4a44e647`, `afdae805` | the licensed-program model, payloads, APIs, keys |
| `1896ee0b` | authority restore, `SAVSECDTA`, `RSTUSRPRF`, `RSTAUT` |
| `c147d2e0` | object signing, `QVFYOBJRST`, `QALWOBJRST` |
| `9266247e` | spooled files with `SPLFDTA` |
| `ee33d632` | IFS `SAV` and `RST` |
| `3d631c82` | `SAVCHGOBJ`, `SAVCFG`, `RSTCFG` |
| `b40ad656`, `e19527ca`, `5d8d654d` | PTFs: format, states, load/apply/remove, commands, delayed PTFs at IPL |
| `690737e2` | `DSPSAVF` from the save's index |
| `c008a8b0`, `b8492597` | the problem log, `WRKPRB`, `DSPPRB`, `CHGPRB`, `DLTPRB` |
| `9c729af6` | QEMU step 71: save and restore through a save file |
| `a48189d7`, `de97a7d8` | oracle captures: restore rules and Phase 11 goldens |
| `a3225423` | the restore rules and messages reconciled with the oracle |
| `d69c3015`..`c861fa85` | licensed programs in the system, `lic/osobjects`, `build lp`, `GO LICPGM` |
| `1a9442ce`..`88b7a10c` | optical, the hybrid install ISO, image catalogs, `/QOPT` |
| `7ccb4096` | `OUTPUT(*OUTFILE)` in `QSRSAV` and `QSRRST` |
| `62e9bb06`, `56924c4b` | the outfile layouts checked by the spec; the save and restore bench |
| `b1473ab2`..`a44fa665` | the update service (protocol, client, reference server), `SNDPTFORD`, `QSCHKUPD`, `INSPTF`, `CPYPTF`, `WRKPTFGRP`, `QSCRTPTF`, trusted signers, LIC PTFs through the A/B slots, `CHGIPLA`/`DSPIPLA` |
| `5095fbe9`..`012660c0` | no sign-on from the install media, the console scaler, SAS/RAID controllers and `DCnn`, scaled screendumps |
| `48e99af1`..`30ce91bb` | `WRKPRB` options, `ANZPRB`, contact information, `GO SERVICE`, dumps, traces, the LIC log, STRSST completion, Define or Change the System at IPL, `RCLSTG` at IPL |
| `94c25bbf` | the restore matrix: every `OPTION` x `MBROPT` x `ALWOBJDIF` |
| `576e1bee`, `95cf2820`, `d5bb97d8` | the product directory states against the spec, the Phase 11 tripwire, the PTF bench |
| `987700f7`..`bd9b2ea9` | media devices, image catalogs, saves and restores on virtual tape and optical, multi-volume tape |
| `d0431e78` | cleanup (`CHGCLNUP`, `STRCLNUP`, `ENDCLNUP`), backup options (`CHGBCKUP`, `DSPBCKUP`, `DSPBCKSTS`), `PRTSYSINF` |
| `c14cf97f` | `GO SAVE` 20 to 23, `GO RESTORE` 21 to 23, `RUNBCKUP`, the `DSPBCKUP` panels, prompt override values |
| `a21359ce`, `460a59a3` | `CHKOBJITG` over the trusted keys |
| `9886d385`..`b0a7d038` | every Phase 11 golden inventoried (`golden/phase11/COVERAGE.md`); prompts, menus and panels compared with pub400's; the prompter's field widths and `PMTCTL` timing |
| `4918d9fb`, `8e98e32e` | `DLTLICPGM` of a product whose records live in its library; the commands pub400 refuses shipped `*PUBLIC *EXCLUDE` |

## QEMU

| Step | Proves |
|---|---|
| 71 | `CRTSAVF`, `SAVLIB` to a save file, a non-empty save file refused without `CLEAR`, `RSTLIB` to a new name, `SAVOBJ`/`RSTOBJ` of one object, `OUTPUT(*OUTFILE)` |
| 72, 73 | the console scaled at 1920x1080 and 1024x768, screendump against the scaler's rendering |
| 74 | problems from QSYSOPR messages, `WRKPRB`/`DSPPRB`, `DMPOBJ`, a system dump, `STRTRC`/`ENDTRC`/`PRTTRC`, the LIC log |
| 75 | `INZTAP`, `SAVLIB`/`RSTLIB` on virtual tape across two volumes, `DSPTAP`; `INZOPT`, `DSPOPT`, `/QOPT` on virtual optical |
| 76 | a virtual media library (cartridges inserted, added, mounted by a save, demounted by `ENDOPT(*UNLOAD)`, ejected; `WRKMLBSTS`), `SAVLIB` of two libraries as a tape file each, a save across two virtual optical volumes and back, `CRTTAPF`/`CPYTOTAP`/`CPYFRMTAP`, `ENDOPT(*UNLOAD)` on virtual tape, the next-volume inquiry answered `G` from the reply list |
| 80 | `SAVLICPGM`/`DLTLICPGM QS00WDS`: `CRTBNDRPG` ends `CPF0C1F`; `RSTLICPGM` *BASE and option 31 bring it back; a vendor product packaged by `QSPKGPRD`, installed with `RSTLICPGM`, keyed with `ADDLICKEY` (a key for other limits refused), shown by `WRKLICINF`, deleted |
| 81 | `QSCRTPTF` with the build's key, `LODPTF`, `APYPTF *TEMP`, `RMVPTF`, a prerequisite refused, `DELAYED(*YES)` applied by the IPL (`C6004501`), `APYPTF *PERM`, `DSPPTFCVR`, `CHKOBJITG` clean then after a change |
| `--media iso`, `iso-raw`, `iso-fat` | the hybrid install image as a CD, written raw to a disk, and copied onto FAT32 |
| `--media iso` 1, 4 | the media IPL without an installed system shows No Operating System Installed; with one, option 1 IPLs the disk |

## Deviations

- **Save files are their own type.** IBM i keeps a save file as a
  `*FILE` with attribute `SAVF`; QS/400 has had `*SAVF` (0x001A) since
  Phase 2. Commands that take a save file accept it as before; `DLTF`
  does not delete one (`DLTOBJ`-style deletion through `svc.obj dlt`).
- **zstd levels.** `DTACPR(*MEDIUM)` and `*HIGH` both use the pure-Rust
  encoder's one level; the format records the codec, not the level.
- **Message IDs not settled by the oracle** are listed in
  `save/saverst/src/msg.rs` and `SVC_OPEN_QUESTIONS.md`: `QALWOBJRST`
  (`CPF3846`) and `QVFYOBJRST` (`CPF3847`) refusals, the default owner
  (`CPI3722`), a damaged object (`CPF3782`), the save of several
  libraries with failures (`CPF3777`).
- **`*OPSYS` is `QS00SS1`.** `QSZRTVPR` answers QS/400's product ID; the
  IBM ID `5770SS1` finds the same product as an alias.

## Deferred

| Item | Why |
|---|---|
| BRMS | not in scope |
| Save while active (`SAVACT` other than `*NO`) | `CPF3700` |
| Parallel saves (`PARALLEL`) | accepted, run serially |
| `SAVRSTOBJ`/`SAVRST*`, ObjectConnect | not in scope |
| `SAVDLO`/`RSTDLO` | not available |
| Optical media libraries, encrypted backups, compaction beyond the drive's setting | not in scope |
| `GO POWER` scheduling, remote journaling, IASP save/restore | not in scope; `ASPDEV` is `*SYSBAS` |
| Physical tape validation | the procedure is in `MEDIA.md`, Cody runs it on hardware |
| UDF read support | read-only ISO 9660 only |
| The backup list (`DSPBCKUPL`, `EDTBCKUPL`), `RTVCLNUP`, `RTVBCKUP` | `*BCKUPL` is every user library (`BACKUP.md`) |
| A panel for "IPL after the backup" | kept in the options, acted on by `RUNBCKUP` |

## Blocked

| Item | Where | Waits for |
|---|---|---|
| `SAVSYS` in `GO SAVE` 21 and 22 | `work/wm/src/opa/gosave.rs` | the install agent's `SAVSYS`; the options run without it until the command exists |
| The job that runs `ENDSBS *ALL` surviving it | `work/qsjob/src/panels/backup.rs` (`CPF1E36`) | an exemption in the subsystem monitor; the options run from `QSVCJOB` |

## Bench

Release builds on the development machine (WSL2, Ryzen, 64 GiB),
stores in memory unless noted.

**Save and restore through a save stream**
(`cargo test --release -p saverst --test bench -- --ignored --nocapture`,
64 MiB of user spaces, half text-like, half noise):

| `DTACPR` | Save | Restore |
|---|---|---|
| `*NO` | 498 MiB/s | 308 MiB/s |
| `*LOW` (LZ4) | 799 MiB/s | 319 MiB/s |
| `*MEDIUM` (zstd) | 119 MiB/s | 308 MiB/s |

**Save and restore on media**
(`cargo test --release -p saverst --test media_bench -- --ignored --nocapture`,
256 MiB, no compression):

| Medium | Save | Restore |
|---|---|---|
| Virtual tape | 252 MiB/s | 200 MiB/s |
| Virtual optical | 312 MiB/s | 349 MiB/s |

**PTFs** (`cargo test --release -p ptf --test bench -- --ignored --nocapture`):
see the acceptance run below for the measured times.

**Install** (`--media iso --install`): see `INSTALL.md`.

## Acceptance checklist

(Filled in at the end of the phase.)
