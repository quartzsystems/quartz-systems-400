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
`*PNLGRP` objects; `pnlgrp_open` hands a job the payload.

The system's displays are UIM panels for a job that owns a 5250
device: 34 shipped panel groups under `ui/uim/pnlgrp/`, installed into
`QSYS` at service start, shown by the drivers in
`work/qsjob/src/panels/` (`work.rs`: WRKACTJOB, WRKUSRJOB, WRKSBSJOB,
WRKJOB and its options, DSPJOB, DSPJOBLOG, WRKSBS, DSPSBSD, WRKJOBD,
DSPJOBD, WRKCLS, DSPCLS, WRKSPLF, WRKOUTQ, WRKJOBQ, WRKWTR, WRKJOBSCDE;
`objects.rs`: DSPMSG, WRKMSG, Additional Message Information, DSPLIBL,
WRKOBJ, DSPLIB, DSPOBJD, DSPOBJAUT, WRKBNDDIR, DSPMSGD, WRKRPYLE,
DSPLOG, DSPFD, DSPFFD, DSPDBR, DSPDTAARA, WRKOBJOWN; `system.rs`:
DSPSYSVAL, WRKSYSVAL, DSPUSRPRF, WRKUSRPRF, DSPNETA, DSPSFWRSC,
DSPAUTUSR, WRKCFGSTS, DSPDEVD, DSPAUTL, WRKAUTL, DSPSECAUD, the
Information Assistant). Their data comes through storage requests
140-191; list options run their commands through the engine. The
serial service console and batch jobs keep the text answers.

Golden tests (`ui/uim/tests/{wm,object,system}_panels.rs`) render each
panel with the golden's visible values and compare the character grid:

| Area | Matching the golden | Ignored, with the reason in the test |
|---|---|---|
| work management | 17 | 13 against goldens (IBM display-file grids R7/R9 do not reproduce), 7 with no golden (title and keys checked) |
| messages and objects | 13 | 2 against goldens (`dspmsgd-list` wraps text into column 1; `dspobjaut` centers a heading over another), 3 with no golden |
| system and security | 8 | 1 against a golden (`dspautusr`: one key row on row 22), 7 with no golden |

Panels batch 2, work (`panels/work2.rs`, storage request 240):
WRKSBMJOB, WRKSBSD, WRKSYSSTS, WRKDSKSTS, WRKSYSACT, WRKSHRPOOL (sizes
and activity levels typed over run `CHGSHRPOOL`), WRKOUTQD, DSPSPLF
(F19/F20, F11 folded, F16 find, the Control line), WRKLIB, WRKOBJLCK,
WRKOBJPGP, DSPPFM (F10 side-by-side hex, F11 vertical hex), WRKJRN,
WRKJRNA, DSPJRN, DSPJRNRCVA, DSPOVR, DSPSAVF (stub), WRKPRB (empty);
WRKACTJOB gains its 27x132 view, three key sets, F14 Include, F20
Subset, F16 Resequence and option 9. Goldens: `work2-panels.tn` (19);
pub400 refused WRKSHRPOOL, WRKSYSSTS, WRKSYSACT, WRKPRB and WRKOBJPGP
and the account has no journal, so those are tested for title and keys
only.

Panels batch 2, programs, configuration, security and editors
(`panels/system2.rs`, storage request 270): DSPPGM, DSPSRVPGM and
DSPMOD (one display per detail), DSPBNDDIR, DSPCMD, DSPPGMREF and
DSPAUDJRNE (report viewer), WRKDEVD, WRKHDWRSC/DSPHDWRSC, DSPPTF,
DSPUSRPRF's `*OBJAUT`, `*OBJOWN`, `*OBJPGP`, `*CMDAUT`, `*DEVAUT` and
`*GRPMBR`, DSPAUTLOBJ, and the editors EDTLIBL, EDTOBJAUT, EDTAUTL and
WRKSYSVAL option 2. Goldens: batch 8; 19 match, 4 are ignored, 10 are
blocked on pub400 (title and keys only).

UIM APIs and engine: the dialog engine is `uim::engine` behind a
`Terminal` trait (the job's `Surface` implements it): field, extended,
keys and search-index help, links, printing to `QSYSPRT`, confirmation
panels, pop-up windows (R24), position-to and subsets, option errors
(R26) and 27x132 panels. The `QUI*`/`QUH*` APIs are natives in
`sysapi::uiapi`, with `*SCHIDX`, `CRTSCHIDX` and `ADDSCHIDXE`; the CL
kit's `UIAPI` case drives QUIOPNDA, QUIDSPP and QUICLOA over a panel
group. Goldens: 11 from pub400 (`batch8-uim`, `batch9-uimpos`);
blocked: help windows and confirm panels (no oracle capture).

Not done: DSPSGNINF (no command definition), another job's job log and
call stack from WRKJOB.

## Section 4: menus

See `MENUS.md`: `*MENU` objects, the stack, the generator (512
command menus), `GO` in the job (§5), and 43 hand-written task menus
in `ui/menus/sys/` (MAIN, USER, OFCTSK, SYSTEM, DATA, PROGRAM, CMN,
DEFINE, PROBLEM, the Operational Assistant menus, SAVE, RESTORE,
NETWORK, ... and ACCESS; §6). Every task-menu golden (46, with second
pages) matches row for row except the copyright line and titles that
name IBM i (`ui/menus/tests/task_menus.rs`, 49 tests). A session's
`MAIN` is the `QSYS/MAIN` menu object through the UIM engine.

## Section 5: system displays

`work/qsjob/src/sysdsp/`: System Request, Attention and Print are
taken in one place (`keys.rs`) wherever a job waits at its display or
a program runs; the screen is put back after. The System Request menu
is display file `QWSSYSRQ` (help in panel group `QWSSYSRQ`); option 2
ends the innermost request (`CPF1907`, the programs cancelled to their
control boundary), 80 disconnects, 90 signs off, 10 and 11 answer not
available. A TN5250 session holds several jobs per display: secondary
jobs (`TFRSECJOB`, option 1) and group jobs (`CHGGRPA`, `TFRGRPJOB`,
`ENDGRPJOB`, `*GDA`; storage requests 200-207). The Attention key runs
`SETATNPGM`'s program or the profile's `ATNPGM` (`*ASSIST` the
`ASSIST` menu, `QSYS/QUSCMDLN` a command-line window). Break messages
come up on Display Messages and `*EXT` inquiries on Display Program
Messages. The Print key spools the screen to `QSYSPRT`. About QS/400
(`QUIABOUT`) is F14 on the Information Assistant. Goldens:
`sysreq-menu`, `sysreq-help`, `sysreq-3-dspjob`, `sysreq-4-dspmsg`,
`sysreq-5-sndmsg`, `sysreq-7-wrkstnusr`, `attn-program`,
`break-message` (pub400, batches 7 and 8). Blocked: options 1, 80 and
90 on pub400, and Display Program Messages (`SNDPGMMSG` to `*EXT` is
refused from a command line).

## Section 6: developer tools

See `TOOLS.md` for each tool's screens, keys and gaps. Each tool is a
crate under `devtools/` (pure logic, host-tested) with a driver under
`work/qsjob/src/devtools/`; IBM's tool screens are display files, so
they are built as `uim::Screen` values or shipped DDS and compared with
pub400 goldens.

| Tool | Goldens (pub400, V7R5M0) | Screen tests | Storage requests | Blocked |
|---|---|---|---|---|
| PDM (`STRPDM`, `WRK*PDM`, `FNDSTRPDM`) | 32 `pdm-*` (`pdm.tn`, `pdm2.tn`) | all match by character | 320-325 | Confirm Delete, Rename, Change Text, the user-defined option displays (reaching them changes things) |
| SEU (`STRSEU`) | 80 `seu-*` (five `seu-*.tn`) | 26 tests, about 60 goldens; message rows skipped | 300-302 | CMD and PF/LF prompts, browse of a CL member, recovery |
| DFU, RLU (`STRDFU`, `UPDDTA`, `DSPDTA`, `CHGDTA`, `STRRLU`) | 24 `dfu-*`, 4 `rlu-*` (`dfu.tn`, `dfu2.tn`) | 16 tests, ten without the message line | 360-363 | DSPDTA's display (no DFU program), keyed-file key entry, the creation displays after General Information, RLU over a real printer file |
| STRDBG, `STRSRVJOB` | 52 `dbg-*` (`dbg*.tn`) | 20 whole, 32 without the message or debug line | 380-387 | source rows, F13 and F15 (no program with a source view), `STRSRVJOB` refused in debug mode |
| STRSQL | 37 `strsql-*` (`strsql.tn` to `strsql4.tn`) | 21 tests, message rows skipped | 400-404 | the second displays of the INSERT, UPDATE and CREATE INDEX prompts, Save Session in Source File, Print session |
| STRSST | none: pub400 refused (no `*SERVICE`) | 16 screens equal `golden/dst/`; 10 of SST's own in `golden/sst/` | 420-432 | all of SST against an oracle |

Two captures changed something on pub400, against its read-only rule.
During one SEU capture a key that leaked past its screen reached
MAIN's "Set initial menu" (F23) and pub400 answered that the user
profile was changed (most likely to the value it held). The first
STRSQL capture left Enter SQL Statements with F12 and the Exit
display's default (option 1) saved the Interactive SQL session; a
dropped connection saves it too. A later run left with option 2, which
discards the saved session, and a fresh session was confirmed after.
The scripts were rewritten: no `continue`, and every exit sets its
option explicitly.

## Section 7: the console

See `TN5250.md`, "The console as a client": the framebuffer console is
a TN5250E client of the system's own telnet server (device
`QSCONSOLE`), so one path renders every display; the console's own
screens remain for the IPL, DST and the time the server is not there.

## Load

`cargo run -p conformance -- --load 200` boots the image under QEMU
(4 CPUs, 6 GiB), creates 200 user profiles and opens 200 TN5250
sessions 50 ms apart; each signs on, runs `WRKACTJOB`, presses F5 five
times and signs off. On 2026-10-23 (host: WSL Ubuntu, 24 threads):

| Figure | Median | p95 |
|---|---|---|
| Sign-on (connect to MAIN) | 36.84 s | 52.95 s |
| `WRKACTJOB` | 1.29 s | 3.14 s |
| F5 refresh | 0.68 s | 5.39 s |

All 200 completed in 103.5 s. QEMU's peak resident memory was
1024 MiB and it used 66.6 s of CPU over the run; `WRKSYSSTS` at the
peak showed 425 jobs. Sign-on dominates: every session arrives within
ten seconds and each sign-on verifies an Argon2id hash. Two fixes came
out of the run: `slic` raises its open-file limit to 65,536 (the
telnet server ran out at about 500 sockets), and a job waiting at a
display now gives its activity level back (`stg::remote::display_wait`),
so 20 jobs sitting in `WRKACTJOB` no longer hold the interactive
pool's 20 levels.

## Blocked

- **The primary oracle shows no screens.** Ports 23, 992 and 21 do
  not reach it from this machine (only 449, 8470, 8475, 8476 do); every
  screen golden comes from pub400, whose sign-on display is customized
  and whose user is not `QSECOFR`. The DDS sign-on (`QDSIGNON`) could
  therefore not be diffed against an oracle's; its geometry is IBM's
  `QDSIGNON` source as known (constants at column 48, values at 68 as the Phase 1 specification lays them out,
  fields at 53). `QSECOFR`-only panels wait for a system that allows
  them.
- **ACS negotiation strings.** No ACS trace can be taken (ACS cannot
  reach the primary oracle either); the start-up response record is
  built from RFC 2877 as read and marked `BLOCKED(phase8)`.
- **Password level 0/1 auto-signon** (the DES substitute) has no
  reachable system to verify against; implemented after JTOpen.
- **HLINV is not on the primary oracle.** The specification says it
  lives there; on 2026-10-23 the box held no library, object, file or
  source member named `HL*` or containing `INV` in a user library
  (`QSYS2.SYSPARTITIONSTAT`, `QSYS2.SYSTABLES`, `OBJECT_STATISTICS` of
  every non-IBM library: only `ORACLE`, `SECORA` and `QSTEST` exist,
  the first two from earlier phases' kits). Nothing to compile or
  capture; the display-file runtime is exercised instead by step 54 and
  the DDS corpus. Needs Cody: the HLINV source (DDS and RPG) or the
  library it was restored to.

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
through `&IN03`), 55 (the developer tools at a display: PDM's menu,
SEU browsing step 54's member, a `SELECT` in STRSQL left without
saving, DFU's menu).

## ACS smoke checklist (Cody)

Run IBM i Access Client Solutions against a QS/400 image (QEMU with
`hostfwd` to port 23, or the installed system) and mark each line with
the date; this is the one acceptance item only Cody can sign off.

| # | Check | Expected | Signed off |
|---|---|---|---|
| 1 | 5250 session, 24x80, sign on as a user | sign-on display (`QDSIGNON`), MAIN | |
| 2 | Same with auto-signon (user and password in the session profile) | MAIN without the sign-on display | |
| 3 | 27x132 session (3477-FC) | sign-on, `WRKACTJOB` in its wide view | |
| 4 | Printer session to a named device | the device appears in `WRKCFGSTS`, a spooled file printed by `STRPRTWTR` arrives in ACS | |
| 5 | Screen print (Print key) | a `QSYSPRT` spooled file of the screen | |
| 6 | F-keys F1-F24, Page Up/Down, Help, Field Exit | as on a real system | |
| 7 | Sys Req, option 3 | Display Job of the session's job | |
| 8 | Attn | the user's attention program (Operational Assistant menu) | |
| 9 | Drop the connection, reconnect, sign on | the job comes back under `QDEVRCYACN *DSCMSG` | |
| 10 | Record a packet trace of 1 and 2 | traces to `tools/conformance/probes/` for `docs/UI_OPEN_QUESTIONS.md` items 4 and 8 | |

## Acceptance checklist

| Item | State |
|---|---|
| 1 tests and goldens green, `build test`, ACS checklist | in progress |
| 2 documents | in progress |
| 3 `missing_docs = deny`, clippy, deny, static binaries | `tn5250`, `dspf`, `uim`, `menus` deny missing docs; clippy clean |
| 4 no IBM text outside goldens; tripwire extended | pending |
| 5 `.env` untracked, no credential in the tree, `QSTEST` empty | `.env` untracked (2026-10-23) |
