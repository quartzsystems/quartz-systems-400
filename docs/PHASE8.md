# Phase 8: 5250, display files, UIM, menus, developer tools

Phase 8 makes QS/400 reachable at a 5250 terminal: a TN5250E server
with virtual displays, the 5250 data stream and a screen model, then
display files, UIM panel groups, `*MENU` objects and the developer
tools. This document records what landed, where each screen's golden
came from, the oracle-kit results and what is blocked. The design
documents are `TN5250.md`, `DSPF.md`, `UIM.md`, `MENUS.md` and
`TOOLS.md`; open questions are in `UI_OPEN_QUESTIONS.md`.

## Oracles

| System | Release | Reached through | Used for |
|---|---|---|---|
| Cody's box (`QS_ORACLE_HOST`) | V7R1M0 | JTOpen host servers only (no telnet, FTP or TLS reachable from this machine) | the inventories, `QSTEST` objects, kits that need no screen |
| pub400.com (`QS_PUB400_USER`) | V7R5M0 | telnet 23 (TLS 992, FTP 21, JDBC also open) | every screen golden; read-only, one session, 500 ms between screens |

Credentials live in `tools/conformance/oracle/.env` (ignored by git);
the driver and the Java helpers read them from the environment.

## Section 0

- `tools/tn5250-driver`: the scripted display with `--target
  qs400|oracle|pub400`, trace mode, goldens with masks, an FTP client.
  Signed on to pub400 on 2026-10-23, by the sign-on display and by
  RFC 4777 auto-signon (SHA-1). The primary oracle has no telnet, so
  the driver cannot sign on there; its release (V7R1M0) was read over
  the remote command server.
- Inventories (2026-10-23): `ui/menus/inventory/ibm-menu-list.txt`
  (625 menus on 7.1, 619 on 7.5), `ui/menus/inventory/
  ibm-pnlgrp-list.txt` (424 on 7.1; 393 in QSYS and 467 in QHLPSYS on
  7.5; 7.1 has no QHLPSYS), `tools/conformance/oracle/ibm-cmd-list.txt`
  (1821 on 7.1, 1787 on 7.5 in QSYS and QTCP). Listed with `DSPOBJD` to
  an outfile on 7.1 and `QSYS2.OBJECT_STATISTICS` on 7.5; both carry
  the object attribute (`UIM`, `DSPF`, `PGM` for menus) and text.
- Earlier phases' oracle kits: all comparers (`ile_oracle`,
  `sec_oracle`, `cmd_oracle`, `db_kit`) have their expected files from
  the 2026-10-22/23 captures and pass; nothing reports itself skipped.
  What still waits: the ILE kit's two inquiry-reply cases (need an
  interactive reply on a system where `QTEMP/ORACLE` may be created),
  the 19 analyzer cases of `CMD_OPEN_QUESTIONS.md` item 15 (need the
  box's command definitions), and job logs of the CL corpus (never
  captured; the corpus compares its own outputs).

## Section 1: TN5250E

See `TN5250.md`. The server, the devices, the data stream codec, the
terminal and the screen model, the Phase 6 sign-on and the interim
MAIN menu and Command Entry rendered through the model, and harness
step 51 over a real session.

## Section 2: display files

See `DSPF.md`: the compiler (§1-6) and the runtime (§7). `CRTDSPF`
compiles a DDS source member into a `*FILE` object with attribute
`DSPF` (`lic/slic/src/svc/db/dspf.rs`; `DSPFD`, `DLTF`, `OVRDSPF` and
`DLTOVR` know it); the job opens it through the storage service
(`dsp_open`, the overrides applied) and its programs' `WRITE`, `READ`
and `EXFMT` go through `work/qsjob/src/display/dds.rs` to the job's
5250 device, or to `slic`'s console as a panel for a job without one.
CL's `DCLF`/`SNDF`/`RCVF`/`SNDRCVF` declare `&IN01`-`&IN99` and copy
through both buffers. The shipped `QDSIGNON` and `QDSIGNON2`
(`ui/dspf/sys/`) are installed into `QSYS` at service start when
missing, and the TN5250 session's sign-on is written through them.
Harness step 54 creates a display file from DDS, compiles a CL program
with `SNDRCVF` and drives it over a session.

## Section 3: UIM

See `UIM.md`: the compiler (§1-6), the renderer rules measured on the
goldens (§7) and the dialog engine in the job (§8). `CRTPNLGRP` makes
`*PNLGRP` objects; `pnlgrp_open` hands a job the payload. The panels
behind the work-management, message, object and system displays and
the `QUI*` APIs are in progress (the engine's `Response` is what each
panel driver acts on).

## Section 4: menus

See `MENUS.md`: `*MENU` objects, the stack, the generator (512
command menus), `GO` in the job (§5) and the shipped `MAIN`. The task
menus of the inventory (`USER`, `OFCTSK`, `SYSTEM`, ...) are in
progress.

## Section 7: the console

See `TN5250.md`, "The console as a client": the framebuffer console is
a TN5250E client of the system's own telnet server (device
`QSCONSOLE`), so one path renders every display; the console's own
screens remain for the IPL, DST and the time the server is not there.

## Blocked

- **The primary oracle shows no screens.** Ports 23, 992 and 21 do
  not reach it from this machine (only 449, 8470, 8475, 8476 do); every
  screen golden comes from pub400, whose sign-on display is customized
  and whose user is not `QSECOFR`. The DDS sign-on (`QDSIGNON`) could
  therefore not be diffed against an oracle's; its geometry is IBM's
  `QDSIGNON` source as known (constants at column 48, values at 70,
  fields at 53). `QSECOFR`-only panels wait for a system that allows
  them.
- **ACS negotiation strings.** No ACS trace can be taken (ACS cannot
  reach the primary oracle either); the start-up response record is
  built from RFC 2877 as read and marked `BLOCKED(phase8)`.
- **Password level 0/1 auto-signon** (the DES substitute) has no
  reachable system to verify against; implemented after JTOpen.

## Goldens

Every screen golden is a `NAME.txt` (the masked character grid) and
`NAME.attr` (the attribute grid) under `tools/conformance/golden/
verified/<source>/`, with its provenance in that directory's
`INDEX.toml`, written by the driver with `--update`. The capture
scripts are under `tools/conformance/probes/` (its README lists what
each batch covers). Batches 1 to 4 (2026-10-23, pub400, V7R5M0) hold
69 panels: MAIN and its help, Command Entry in every key state, the
work-management panels (`WRKACTJOB` views, `WRKUSRJOB`, `WRKSBSJOB`,
`WRKJOB` and its options, `WRKSBS`, `DSPSBSD` and its options,
`WRKJOBD`/`DSPJOBD`, `WRKCLS`/`DSPCLS`, `WRKSPLF`, `DSPJOB`,
`DSPJOBLOG`), messages (`DSPMSG`, `WRKMSG`, `DSPMSGD`, Additional
Message Information, `WRKRPYLE`, `DSPLOG`), objects (`WRKOBJ`,
`DSPLIB`, `DSPOBJD` basic and full, `DSPOBJAUT`, `WRKBNDDIR`,
`DSPLIBL`), the system (`DSPSYSVAL`, `DSPSFWRSC`, `DSPNETA`,
`DSPAUTUSR`, `DSPUSRPRF`), the Information Assistant and `MAJOR`,
`CMDWRK` menus, and two prompts. Panels the pub400 account may not
run (`WRKSYSSTS`, `WRKOUTQ`, `WRKJOBQ`, `WRKJOBSCDE`, `WRKSYSVAL`,
`WRKMSGD`, `DSPDTAARA`, the `CRTLIB` prompt) have no verified golden
yet.

Batch 5 (`batch5-menus.tn`) adds every menu reached by `GO` from
MAIN: the `MAJOR`, `VERB`, `SUBJECT` and `CMDxxx` command menus with
their second pages and the task menus (163 `menu-*` goldens). Batch 6
(`batch6-prompts.tn`) adds 68 command prompts (`prompt-*`), each with
F10, F11 and F24 states where they differ; the prompts of commands the
account may not run (`CHGSYSVAL`, `CRTCLS`, `CRTJOBD`, `CRTSBSD`,
`RSTLIB`, `SNDBRKMSG`, `WRKSYSVAL`, `RTVJOBA`) were dropped. Batch 7
(`batch7-sysdsp.tn`) got the System Request menu, its help and option 3
(`DSPJOB`); after that pub400 kept the keyboard locked and the rest of
the script (the other options, the Attention program, a break message,
`DSPSGNINF`, `CHGPWD`, the sign-off prompt) waits for a rerun with
longer settles.

Harness steps over TN5250: 51 (sign-on, MAIN, Command Entry,
`WRKACTJOB`, sign-off, the device freed), 52 (Command Entry compared
with the oracle's capture, characters only), 53 (device recovery:
`*DSCMSG` keeps and resumes the job, `*ENDJOB` ends it), 54 (a
display file: `CRTDSPF`, a CL `SNDRCVF` shown, read and echoed, `F3`
through `&IN03`).

## Acceptance checklist

| Item | State |
|---|---|
| 1 tests and goldens green, `build test`, ACS checklist | in progress |
| 2 documents | in progress |
| 3 `missing_docs = deny`, clippy, deny, static binaries | `tn5250`, `dspf`, `uim`, `menus` deny missing docs; clippy clean |
| 4 no IBM text outside goldens; tripwire extended | pending |
| 5 `.env` untracked, no credential in the tree, `QSTEST` empty | `.env` untracked (2026-10-23) |
