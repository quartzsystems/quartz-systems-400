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
| PTFs | `PTF.md` | `ptf/ptf` |
| Problem log, service tools | `SERVICE.md` | `service/prb` |
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

## QEMU

| Step | Proves |
|---|---|
| 71 | `CRTSAVF`, `SAVLIB` to a save file, a non-empty save file refused without `CLEAR`, `RSTLIB` to a new name, `SAVOBJ`/`RSTOBJ` of one object, `OUTPUT(*OUTFILE)` |
| `--media iso`, `iso-raw`, `iso-fat` | the hybrid install image as a CD, written raw to a disk, and copied onto FAT32 |

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

## Blocked

| Item | Where | Waits for |
|---|---|---|

## Bench

(Measured at the end of the phase.)

## Acceptance checklist

(Filled in at the end of the phase.)
