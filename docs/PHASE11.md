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
| Install media and flow, recovery | `INSTALL.md`, `RECOVERY.md` | `media/instmedia`, `lic/svctools` |
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
| `8204a2a4`..`759f52fe` | install media and flow: LIC options 3 and 4, the operating system from media, the response file, upgrades and slip installs, `SAVSYS` and the recovery, the install matrix (docs/INSTALL.md, docs/RECOVERY.md) |

## QEMU

| Step | Proves |
|---|---|
| 71 | `CRTSAVF`, `SAVLIB` to a save file, a non-empty save file refused without `CLEAR`, `RSTLIB` to a new name, `SAVOBJ`/`RSTOBJ` of one object, `OUTPUT(*OUTFILE)` |
| 72, 73 | the console scaled at 1920x1080 and 1024x768, screendump against the scaler's rendering |
| 74 | problems from QSYSOPR messages, `WRKPRB`/`DSPPRB`, `DMPOBJ`, a system dump, `STRTRC`/`ENDTRC`/`PRTTRC`, the LIC log |
| 75 | `INZTAP`, `SAVLIB`/`RSTLIB` on virtual tape across two volumes, `DSPTAP`; `INZOPT`, `DSPOPT`, `/QOPT` on virtual optical |
| 76 | a virtual media library (cartridges inserted, added, mounted by a save, demounted by `ENDOPT(*UNLOAD)`, ejected; `WRKMLBSTS`), `SAVLIB` of two libraries as a tape file each, a save across two virtual optical volumes and back, `CRTTAPF`/`CPYTOTAP`/`CPYFRMTAP`, `ENDOPT(*UNLOAD)` on virtual tape, the next-volume inquiry answered `G` from the reply list |
| 80 | `SAVLICPGM`/`DLTLICPGM QS00WDS`: `CRTBNDRPG` ends `CPF0C1F`; `RSTLICPGM` *BASE and option 31 bring it back; a vendor product packaged by `QSPKGPRD`, installed with `RSTLICPGM`, keyed with `ADDLICKEY` (a key for other limits refused), shown by `WRKLICINF`, deleted |
| 81 | `QSCRTPTF` with the build's key, `LODPTF`, `APYPTF *TEMP`, `RMVPTF`, a prerequisite refused, `DELAYED(*YES)` applied by the IPL (`C6004501`), `APYPTF *PERM`, `DSPPTFCVR`, `CHKOBJITG` of the commands |
| `--media iso`, `iso-raw`, `iso-fat` | the hybrid install image as a CD, written raw to a disk, and copied onto FAT32 |
| `--media iso` 1, 4 | the media IPL without an installed system shows No Operating System Installed; with one, option 1 IPLs the disk |
| `--media iso --install` 1, 3, 8 to 11 | the install matrix: automatic install with the signatures checked, every panel over serial, a response file with two mirrored disks, `SAVSYS`, `SAVLIB` and `SAV` to a virtual tape over an image catalog and the recovery with the user data, the slip install of V1R1M0 with the downgrade refused, a power pull mid-install (docs/INSTALL.md section 5) |

Install suite, 2026-10-31, QEMU with KVM on the development host,
`--media iso --install` whole: PASS 11 steps. 1 automatic install 34 s,
2 boot installed disk 10 s, 3 interactive install 34 s, 4 to 7 the DST
mirror steps, 8 response file with two mirrored disks 43 s, 9 `SAVSYS`
to virtual tape, wipe and recovery 61 s (8 payloads on the tape, 55
profiles restored, the profile made before the save back), 10 slip
install of V1R1M0 over V1R0M0 70 s (1828 objects replaced, 18 removed,
1845 signatures verified, the user library kept, the V1R0M0 media
refused afterwards), 11 power pull mid-install 53 s. `--media iso-raw`
(the D IPL from the USB image), `iso-fat` and `iso` pass. `CHKOBJITG`
is not in this worktree; the steps report it as not run until the
merge brings it.

After the media device layer, 2026-10-31 (`--media iso --install`
whole: PASS 11 steps): step 9 makes `TAPVRT01` and the image catalog
`SAVSYS`, runs `SAVSYS DEV(TAPVRT01)`, `CHKTAP` (a system save),
`SAVLIB LIB(*NONSYS)` and `SAV OBJ(('/*') ...)` onto the same volume (9
system save files, then the user files), copies the volume onto a disk
(`svc.vrtout`), wipes the system disk and recovers by option 4: 55
profiles back, then the storage service's `RSTLIB *NONSYS` (`CPF3770`, some objects
of the other libraries not restored; which ones the run did not
record; `RCVUSR` whole), `RST` (12 links, 7 not restored: `CPF3838`)
and `RSTAUT`; the profile, the library and the IFS file made before the
save are back, 190 s. Steps 75 and 76 of the disk suite pass.

Two earlier-phase bugs the matrix found, fixed in their own commits: a
store formatted over units another store had used replayed that store's
WAL (`Asp::format` now zeroes the WAL; `storage/store/tests/wal_reuse.rs`),
and a mirror resume started in DST was lost when the store passed to the
storage service (the IPL now finishes it first).

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

- **Install.** LIC options 3 and 4 work (docs/INSTALL.md); the
  operating system install restores `QS00SS1` from the media instead of
  making the objects at the first IPL (which stays as the fallback
  without media); `SAVSYS` saves while active, and a recovery restores
  the user data after the system save by itself once the storage
  service starts (docs/RECOVERY.md; `SVC_OPEN_QUESTIONS.md`, "Install
  and recovery").

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
| The job that runs `ENDSBS *ALL` surviving it | `work/qsjob/src/panels/backup.rs` (`CPF1E36`) | an exemption in the subsystem monitor; the options run from `QSVCJOB` |
| `qs400.rsp=` naming a URL | `lic/slic/src/ipl/media.rs` | the network boot (PXE) |

## Bench

Release builds on the development machine (WSL2 on an i9-12900K, 32 GiB),
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

**PTFs** (`cargo test --release -p ptf --test bench -- --ignored --nocapture`,
200 replacement objects of 64 KiB, 12.5 MiB, a 6.7 MiB stream): build
and sign 0.13 s, open (every signature checked) and `LODPTF` 0.02 s,
`APYPTF *TEMP` 0.06 s, `RMVPTF` 0.01 s, `APYPTF *PERM` under 0.01 s. Under
QEMU, step 81's whole cycle with an IPL takes 13 s.

**Install** (`--media iso --install`): see `INSTALL.md`.

## Acceptance checklist

The state at `35d314dd` (2026-10-31). The QEMU runs are KVM guests on
the bench machine, from a worktree pinned to that commit.

**1. Tests, goldens and matrices; `build test`.**

| Item | State |
|---|---|
| `cargo run -p build -- test` (the disk suite) | PASS, 78 steps |
| `--media iso --install` (the install matrix) | PASS, 11 steps: automatic install with `CHKOBJITG` clean, the installed disk, every panel over serial, the DST mirror steps, a response file with two mirrored disks, `SAVSYS`, `SAVLIB` and `SAV` to a virtual tape over an image catalog and the recovery with the user data, the slip install of V1R1M0 with the downgrade refused, a power pull mid-install |
| `--media iso`, `iso-raw` (the USB image), `iso-fat` | PASS, 4, 3 and 3 steps |
| Workspace tests, goldens | pass; `golden/phase11/COVERAGE.md` lists every pub400 capture and how it is compared (112 compared, 175 covered as later pages, 45 refused on pub400 and checked, 21 deferred with reasons) |
| Save/restore matrix | `save/saverst/tests/matrix.rs`: every `OPTION` x `MBROPT` x `ALWOBJDIF`, the oracle's 27 rules asserted where they apply; QEMU 71, 75, 76 |
| Install matrix | the install suite above |
| PTF matrix | QEMU 81 (immediate and delayed with an IPL, `RMVPTF`, a prerequisite, `*PERM`); `ptf/ptf/tests` (supersedes, groups, LIC slots, a power loss during a delayed apply, `INSPTF`); `ptf/update/tests` (`SNDPTFORD` against the reference server) |
| LP matrix | QEMU 80 (`DLTLICPGM QS00WDS` then `CPF0C1F`, `RSTLICPGM`, a `QSPKGPRD` product keyed and deleted); `QLZAREQ` limits in `licpgm/licpgm/tests/license.rs` |
| Physical tape | `StTransport` transcript tests in `media/tape`; the procedure is `MEDIA.md` "Tape: physical tape acceptance", for Cody's hardware |
| The ACS checklist of Phase 10 on a freshly installed system | Cody's sign-off (it needs ACS on a workstation) |

Not every combination was run on the primary oracle: it settled 20
cases (`tools/conformance/oracle/save/primary/README.md`), and the
matrix applies their rules to every combination in memory.

**2. Documents and spec tests.** `SAVE_FORMAT.md`, `MEDIA.md` (with the
physical tape procedure), `LICPGM.md`, `INSTALL.md`, `RECOVERY.md`,
`PTF.md`, `UPDATE_PROTOCOL.md`, `SERVICE.md`, `BACKUP.md` and this file.
The spec-vs-code tests cover the save stream and the `QSRSAV`/`QSRRST`
outfiles (`save/savfmt/tests/spec.rs`), the PTF header
(`ptf/ptf/tests/spec.rs`), the product payloads and the product
directory's states (`licpgm/licpgm/tests/spec.rs`).

**3. Lints, dependencies, signatures.** `missing_docs = deny` in all 15
new crates; `cargo clippy --workspace --all-targets` and the musl lint
of slic, qsjob, svctools and conformance clean; `build deny` clean (310
crates, no C build helpers, no `links`); the binaries static (`build
userland` checks the ELF); the media manifest signed and checked at a D
IPL; every shipped object signed by `build lp`, the signatures checked
on restore (1845 in the slip install) and by `CHKOBJITG` after a fresh
install (install steps 1 and 8).

**4. Text and the oracles.** No IBM text in shipped panels, messages,
help, agreements or cover letters: `phase8_hygiene`, `phase10_hygiene`
and `phase11_hygiene` pass. Oracle output lives under `tools/conformance/`
only; `tools/conformance/oracle/.env` is ignored by git. Both oracles
were left as found (`tools/conformance/oracle/save/README.md`, "State
after the session"): the primary's `QSTEST` empty and `/home/QSTEST`
holding only its 2026-10-22 folders; on pub400 the save file deleted.
Nothing outside them was saved, restored, loaded, applied, initialized
or varied.
