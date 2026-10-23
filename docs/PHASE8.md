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

## Blocked

- **The primary oracle shows no screens.** Ports 23, 992 and 21 do
  not reach it from this machine (only 449, 8470, 8475, 8476 do); every
  screen golden comes from pub400, whose sign-on display is customized
  and whose user is not `QSECOFR`. `QSECOFR`-only panels and the
  standard sign-on are compared with `unverified/` goldens.
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

Harness steps over TN5250: 51 (sign-on, MAIN, Command Entry,
`WRKACTJOB`, sign-off, the device freed), 52 (Command Entry compared
with the oracle's capture, characters only), 53 (device recovery:
`*DSCMSG` keeps and resumes the job, `*ENDJOB` ends it).

## Acceptance checklist

| Item | State |
|---|---|
| 1 tests and goldens green, `build test`, ACS checklist | in progress |
| 2 documents | in progress |
| 3 `missing_docs = deny`, clippy, deny, static binaries | `tn5250` denies missing docs; the rest as crates land |
| 4 no IBM text outside goldens; tripwire extended | pending |
| 5 `.env` untracked, no credential in the tree, `QSTEST` empty | `.env` untracked (2026-10-23) |
