# CPP audit: display commands and their data

The Phase 7 inventory wired some commands to the service console's
words or to a text listing, so a 5250 session saw the first line of a
listing (or an internal error string) on the menu's message line
instead of a panel. This audit lists every command of that class: the
display commands (`WRK*`, `DSP*`, `EDT*`, `GO`, the `STR*` tools,
`QSWRK*`), the commands that reach a console-only word, and the
operator/system batch named in the addendum.

How a display command runs in a job with a 5250 display, after this
audit's fixes:

1. `work/qsjob/src/command/jobsite.rs` hands it to its panel driver
   (`work/qsjob/src/panels/**`, the developer tools, `sysdsp/`).
2. The driver reads from the storage service over the job's link, or
   from the IFS through the service.
3. A failure of that read with a real message (an object not found) is
   that message as the `*ESCAPE`. Any other failure is `CPF9898`
   ("Function check. Unexpected condition in CMD.") with the detail in
   a `CPD9898` `*DIAG` in the job log only (`command::check`).
4. It never falls back to text. The service's text form stays for jobs
   with no display (batch, `QSVCJOB`) and for `OUTPUT(*PRINT)`.

`sys_cpps::inventory::PANELS` lists the display commands;
`command/sys-cpps/tests/panels.rs` holds every display command of the
shipped definitions to that list, and `work/qsjob` checks each listed
command has a driver. Step 82 (`tools/conformance/src/dspcmds.rs`)
types every one of them from `MAIN` over TN5250 and fails on any that
leaves text on the menu.

## Legend

- **Wired today** (at 38df48fb): `panel/fallback` a job-side panel
  whose driver returned `None` on a failed read, so the service's text
  answered (the shim); `text` the service's text listing only, no
  panel; `console word` the inventory's bridge word is answered by the
  service console process, not the storage service; `text parse` a
  panel filled by parsing the service's text form; `report view` the
  printed report in the spooled-file viewer, which is what IBM shows
  for `OUTPUT(*)`; `panel` a panel over the service's data with no
  fallback.
- **Reads from**: the storage service request (`stg::proto`) or the
  service module behind it.
- **Panel**: the panel group; `new` where this audit added it.
- **Fix**: the commit on main, or `open`.

## Operator/system batch (named in the addendum)

| Command | Wired today | Should read from | Panel | Fix |
|---|---|---|---|---|
| DSPSFWRSC | panel/fallback; service text when the read failed | `sfwrsc_panel` (licpgm directory); `OUTPUT(*OUTFILE)` as `QSYS/QARZLCOF` format `QARZLCGD` (`cpp/dspout.rs`, fields from `verified/oracle71/audit/outfiles/dspffd-qarzlcof-*`) | QUISFWRSC | b1da8ba2 (class fix), 07b5ed85 |
| WRKCFGSTS | panel/fallback for `CFGTYPE` other than `*LIN`/`*CTL`/`*DEV` | `cmn_panel CFGSTS` (cmncfg) | QUICFG | b1da8ba2 (class fix) |
| DSPDEVD | panel/fallback (`.ok()?` on `devd_panel`) | `devd_panel` (`*DEVD` objects) | QUICFG | b1da8ba2 (class fix) |
| CHGDEVD | service CPP over the store (`cpp/sysinfo.rs`) | the `*DEVD` object | none (action) | 9750760c (step 82 setup runs it) |
| VRYCFG | service CPP (`cpp/cmncmd.rs`) over cmncfg | cmncfg status table | none (action) | 9750760c (step 82 setup runs it) |
| DSPPTF | panel showing a fixed empty list (`sfwrsc_panel`), "none in this release" | the PTF database `QSYS/QSPTFDB` (`cpp/ptfcmd.rs`) | QUIDSPPTF | 46aec2dd |
| RTVCFGSTS | job CPP over `cfg_status` | cmncfg status table | none (retrieve) | 9750760c (step 82 setup runs it) |
| CRTDEVDSP | service CPP over the store (`cpp/sysinfo.rs`) | the `*DEVD` object | none (action) | 9750760c (step 82 setup runs it) |
| DSPNETA | panel/fallback (`.ok()?` on `neta_panel`) | `neta_panel` (network attributes) | QUIDSPNETA | b1da8ba2 (class fix) |
| CHGNETA | service CPP over the store (`cpp/sysinfo.rs`) | the network attributes | none (action) | 9750760c (step 82 setup runs it) |
| WRKHDWRSC | console word `hw` ("answered by the console") behind a panel/fallback | the hwrsrc service | QUIHDWRSC | 1b38949a, 8b4b2d26 |
| DSPHDWRSC | console word `hw` behind a panel/fallback | the hwrsrc service | QUIHDWRSC | 1b38949a, 8b4b2d26 |

## Messages batch

| Command | Wired today | Should read from | Panel | Fix |
|---|---|---|---|---|
| DSPMSG | panel/fallback to bridge word `dspmsg` | message queues over the link | QUIDSPMSG | 87be5c4c |
| WRKMSG | panel/fallback to bridge word `dspmsg` | message queues over the link | QUIDSPMSG | 87be5c4c |
| WRKMSGD | text: bridge word `wrkmsgd`, no panel | `msgf_list` / message file payload | QUIDSPMSGD (WRKMSGD panel) new | 46aec2dd |
| DSPMSGD | panel/fallback | message file over the link | QUIDSPMSGD | b1da8ba2 (class fix) |
| DSPLOG | panel/fallback | `QHST` over the link | QUIDSPLOG | b1da8ba2 (class fix) |
| WRKRPYLE | panel/fallback | the reply list | QUIWRKRPYL | b1da8ba2 (class fix) |

## Display commands with only the service's text

"Report viewer" was `QUIRPTVW`: the command's own processing program
ran in the storage service and its lines showed on a panel. The eight
that had it now have panels of their own; their drivers are in
`work/qsjob/src/panels/info/` and their data comes from the service as
structured `sys2_panel` requests (`stgsvc/cppdata.rs`,
`stgsvc/cppmedia.rs`) or through the IFS client (`DSPUDFS`).
`OUTPUT(*PRINT)` still prints the service's text form. Captures are
indexed in `tools/conformance/golden/audit/displays/INDEX.toml` and
`verified/oracle71/audit/INDEX.toml`; panels no oracle can show are
compared with goldens of our own (`ui/uim/tests/golden/audit/`).

| Command | Wired today | Reads from (after) | Panel | Golden | Fix |
|---|---|---|---|---|---|
| DSPPTF | panel over a fixed empty list | `sys2_panel PTF`, `PTFD` (`stgsvc/cppdata.rs` over `QSYS/QSPTFDB`) | QUIDSPPTF `DSPPTF`, `DSPPTFD`; `OUTPUT(*OUTFILE)` as `QSYS/QADSPPTF` format `QSCPTF` (`cpp/dspout.rs`, fields from `verified/oracle71/audit/outfiles/dspffd-qadspptf-*`) | `verified/oracle71/audit/dspptf` (7.1) row for row and input fields (`ui/uim/tests/ptf_panels.rs`) | 46aec2dd, b1aaa415, 07b5ed85 |
| WRKPTFGRP | text (`cpp/ptfsvc.rs`) | `sys2_panel PTFGRP` (`ptf::grpdb`) | QUIDSPPTF `WRKPTFGRP`, F11 the text view | `verified/oracle71/audit/wrkptfgrp` (7.1) row for row and input fields (`ui/uim/tests/ptf_panels.rs`) | 46aec2dd, b1aaa415 |
| DSPIPLA | text (`cpp/iplcmd.rs`) | `sys2_panel IPLA` (the LIC record) | QUIIPLA new | `phase11/displays/dspipla`, row for row | 46aec2dd |
| WRKMSGD | text: bridge word `wrkmsgd` | `msgf_list` | QUIDSPMSGD `WORK` | `audit/displays/wrkmsgd`, geometry | 46aec2dd, this batch |
| WRKRDBDIRE | text: screen rows printed as lines (`command/ddm.rs`) | the RDB directory data area | QUIRDBDIR new | `phase10/misc/wrkrdbdire`, geometry | 46aec2dd |
| DSPPTFCVR | text (`cpp/ptfcmd.rs`) | `sys2_panel PTFCVR` (the cover letters in `QSYS/QSPTFDB`) | QUIDSPPTF `SELCVR`, `DSPCVR`; no IBM outfile | refused on pub400, `CPF6613` on 7.1 (`verified/oracle71/audit/ids-dspptfcvr`, `dspptfcvr`); `ui/uim/tests/audit_panels.rs` against ours (`selcvr`, `dspcvr`) | 58fefdaf |
| DSPBCKSTS | text (`cpp/opacmd.rs`) | `sys2_panel BCKSTS` (`QUSRSYS/QEZBCKHST`, `wm::opa::hist`) | QEZBCKUP `DSPBCKSTS`, F22 `DSPBCKHST`, option 5 `DSPBCKDTL`; `OUTPUT(* *PRINT)` only, as on pub400 | `audit/displays/dspbcksts`, `-2`, `dspbckhst` row for row; ours with backups | c448bb77 |
| DSPLICKEY | text (`cpp/lpcmd.rs`) | `sys2_panel LICKEY` (the license repository) | QUILICKEY `DSPLICKEY`, one key to a display; `OUTPUT(*LICKEYFILE)` in the service | `verified/oracle71/audit/dsplickey` (7.1, key and serial masked) row for row | 58fefdaf |
| DSPUDFS | text (`cpp/ifscmd.rs`) | the IFS client `dspudfs` (`ifscmd::display::udfs`) | QUIDSPUDFS `DSPUDFS` new; `OUTPUT(*PRINT)` now printed; no IBM outfile (`audit/displays/dspudfs-prompt`) | no oracle has a UDFS: pub400 answers `CPFA0A9` (`audit/displays/dspudfs-none`), the 7.1 box has none in `/dev/QASP01` (`audit/displays/wrklnk-qasp01`) and a UDFS can only be made there, a system directory, so none was made; ours (`dspudfs`, `dspudfs-unmounted`) | e0698746 |
| DSPTAP | text (`cpp/medcmd.rs`) | `sys2_panel TAP` (`cpp/meddsp.rs` over the drive) | QSMEDIA `DSPTAP`, `DSPTAPVOL`, `DSPTAPSAV`; `OUTPUT(*OUTFILE)` as `QSYS/QATADOF` format `QTADOUTF` (`cpp/medout.rs`, fields from `audit/outfiles/MODELS.txt`) | no tape on pub400 (`CPF6718`, `audit/displays/ids-dsptap`); the 7.1 box's TAP01 holds no cartridge (`CPF6760`, `audit/displays/ids-dsptap-notready71`, the identifier `stgsvc/media/drive.rs` sends); layout from IBM's documentation, ours (`dsptap`, `dsptapvol`, `dsptapsav`) | a88effcc |
| DSPTAPCTG | text (`cpp/medcmd.rs`) | `sys2_panel TAPCTG` (the library's cartridges) | QSMEDIA `DSPTAPCTG`, one cartridge to a display; `OUTPUT(*OUTFILE)` as `QSYS/QATAVOF` format `QTAVOUTF` | no library on pub400 (`CPF9814`, `audit/displays/ids-dsptapctg`) or on 7.1 (`audit/displays/wrkdevd-tapmlb71`, `wrkmlbsts71`); ours (`dsptapctg`) | a88effcc |
| DSPOPT | text (`cpp/medcmd.rs`) | `sys2_panel OPT` (the volume set) | QSMEDIA `DSPOPTVOL`, `DSPOPTDIR`, `DSPOPTFIL`, `DSPOPTSAV`; `OUTPUT(*OUTFILE)` as `QSYS/QAMODVAF`, `QAMODPAF`, `QAMODFAF` (`QAMODVA`, `QAMODPA`, `QAMODFA`) | no volume on pub400 (`OPT1860`, `audit/displays/ids-dspopt`); 7.1's OPT01 and OPTVRT01 are empty (`verified/oracle71/audit/wrkdevd-opt`, `wrkoptvol71`) and its one image catalog is not ready, so nothing was loaded to read; ours (`dspoptvol`, `-dir`, `-fil`, `-sav`) | a88effcc |
| WRKOPTVOL | text (`cpp/medcmd.rs`) | `sys2_panel OPTVOLS` (the optical devices' catalogs) | QSMEDIA `WRKOPTVOL` with pub400's parameters (`DEV VOL CSI RTVEXTINFO`); no IBM outfile | `audit/displays/wrkoptvol` and 7.1's `wrkoptvol71` row for row with every input field, the add row (row 11) an extended action entry (`docs/UIM.md` R27); option 1 on it prompts `ADDOPTCTG` for the volume typed, as 7.1 does (`wrkoptvol-ext71`, `-add-prompt71`, `-add-back71`; QS/400 has no `ADDOPTCTG` command yet, so the message line says it is not found) | a88effcc, this batch |
| GO | the service answers `CPF0901`; menus and Command Entry intercept `GO` | `*MENU` objects | the menu | phase 8 | correct as wired |

## Panels filled by parsing the service's text

| Command | Wired today | Reads from (after) | Panel | Fix |
|---|---|---|---|---|
| DSPOBJAUT | text parse of `dspobjaut` | `sys2_panel OBJAUT` (`stgsvc/authdata.rs` over `sec::cmd::object::authorities`); `OUTPUT(*OUTFILE)` as `QSYS/QAOBJAUT` format `QSYDSAUT` (`cpp/dspout.rs`, fields from `verified/oracle71/audit/outfiles/dspffd-qaobjaut-*`) | QUIDSPOAUT; golden `verified/pub400/dspobjaut` (its test stays ignored: the golden centers a two-row heading, the renderer's R9 left-aligns it) | 8b89ef72, 07b5ed85 |
| EDTOBJAUT | text parse of `DSPOBJAUT` | `sys2_panel OBJAUT`, the single authorities from the bits | QUIEDTAUT; goldens `verified/pub400/edtobjaut`, `-f11`, `-f6` row for row | 8b89ef72 |
| DSPAUTLOBJ | text parse of `DSPAUTLOBJ` | `sys2_panel AUTLOBJ` (`sec::cmd::object::secured_objects`) | QUIAUTLOBJ; golden `verified/oracle71/audit/dspautlobj` (7.1; pub400 has no list the account may display) row for row, except the column headings the system hides on an empty list | 8b89ef72, 6b046a9f |
| WRKOBJOWN | text parse of `wrkobjown` | `sys2_panel OBJOWN` (the owner index, `sec::cmd::object::user_objects`) | QUIWRKOOWN, F11 the text view; goldens `verified/pub400/wrkobjown`, `-f11` row for row | 8b89ef72, 780a18ae |
| DSPFD, DSPFFD, DSPDBR | report view of the service's report | the file description | QUIDSPFD (`SPLF`) | correct as wired |
| DSPPGMREF, DSPAUDJRNE | report view of the service's report | program references, `QAUDJRN` | report viewer | correct as wired |

## Job-side panels (class fix only)

These had a panel driver whose failed read (`.ok()?`, `return None`)
let the service's text answer. The class fix makes the failure an
escape; nothing else changed.

| Commands | Driver | Reads from |
|---|---|---|
| WRKACTJOB, DSPJOB, WRKJOB, WRKUSRJOB, WRKSBSJOB, WRKSBS, DSPSBSD, WRKJOBD, DSPJOBD, WRKCLS, DSPCLS, WRKSPLF, WRKOUTQ, WRKJOBQ, WRKWTR, WRKJOBSCDE, DSPJOBLOG | panels/work | work management requests |
| DSPDBR, DSPDTAARA, DSPFD, DSPFFD, DSPLIB, DSPLIBL, DSPOBJD, WRKBNDDIR, WRKOBJ, WRKOBJOWN | panels/objects | object and file requests |
| DSPSYSVAL, WRKSYSVAL, DSPUSRPRF, WRKUSRPRF, DSPAUTUSR, DSPAUTL, WRKAUTL, DSPSECAUD, WRKREGINF | panels/system | system values, `sec_api`, registration facility |
| WRKSBMJOB, WRKSHRPOOL, WRKSYSSTS, WRKDSKSTS, WRKSYSACT, WRKOUTQD, DSPSPLF, WRKLIB, WRKOBJLCK, DSPPFM, DSPJRN, WRKJRN, WRKJRNA, DSPJRNRCVA, DSPOVR, WRKOBJPGP, WRKSBSD, DSPSAVF | panels/work2 | `panel_data` |
| WRKPRB, DSPPRB, ANZPRB, WRKCNTINF, WRKSRVPVD, DSPSRVSTS | panels/work2/service | problem log |
| WRKIMGCLG, WRKIMGCLGE, WRKTAPCTG, WRKMLBSTS | panels/work2/media | media requests; WRKIMGCLGE and WRKMLBSTS follow the 7.1 captures (`wrkimgclge71`, `wrkmlbsts71`), WRKIMGCLG's add row both oracles' |
| DSPPGM, DSPSRVPGM, DSPMOD, DSPBNDDIR, DSPCMD, WRKDEVD, WRKLIND, DSPLIND, DSPAUTLOBJ, EDTLIBL, EDTOBJAUT, EDTAUTL | panels/system2 | `sys2_panel`, `cmn_panel` |
| QSLICPGM, WRKSFWAGR, WRKLICINF | panels/licpgm | `lp_panel` |
| DSPBCKUP | panels/backup | `QSBCKUP` fields |
| WRKLNK, DSPLNK, WRKAUT, DSPAUT | panels/ifs | the IFS client |
| QSWRKCRT, QSWRKAPPID | panels/certs | certificate store |
| WRKSRVTBLE, WRKPCLTBLE, WRKNETTBLE, NETSTAT, CFGTCP menus | panels/tcp | TCP/IP configuration |
| WRKENVVAR | qsh/env | the job's and the system's variables |
| STRPDM, WRKLIBPDM, WRKOBJPDM, WRKMBRPDM, STRSEU, STRSDA, STRDFU, DSPDTA, STRRLU, STRSQL, STRSST, STRDBG, DSPMODSRC, DSPBKP, EDTF, DSPF, STRQSH | the developer tools | their own requests |
| DSPWSUSR | sysdsp | the TN5250 session |

## Framebuffer console

The local console had no 5250 display, so every command above took
the text path there. Fixed in ffdd9b6a: `QSCONSOLE` is a real display
served by the telnet session code over a local socket, with no TCP/IP
needed; step 84 checks it with TCP/IP ended.

## Step 82 (QEMU, `--only 82`)

The step runs every display command of the shipped definitions from
`MAIN` (134 with `GO`, `DSPMODSRC` and `DSPBKP` inside a `STRDBG`
session; `DSPDTA` is not run: it needs a DFU program). Its setup makes
the objects the parameter table names and runs `CHGDEVD`, `CHGNETA`,
`CRTDEVDSP`, `VRYCFG` and, in a CL program, `RTVCFGSTS`, so every
command of this audit has a harness invocation.

Last run at 33daa722: one failure.

| Command | Result | Owner |
|---|---|---|
| GO | `GO USER` from MAIN's command line shows no menu and no message (the job log has the request only); fixed: `GO` parsed the menu name as the command word | c432a9ff |

`WRKHDWRSC`, `DSPHDWRSC`, `DSPMSG` and `WRKMSG` passed in this
worktree through the class fix (a panel or the function check); their
owners' changes replace the `hw` word and the message panels.

## Message lines by identifier (round 3)

`command/sys-cpps/tests/messages.rs` holds the line: every message
line text is a message ID (`message_lines_carry_message_ids`), no
driver shows a library's own wording or an error value's text
(`drivers_show_no_library_text`, which also catches a sentence a
driver words and hands back), the LIC's screens show no internal
reason (`lic_screens_show_no_error_text`), and every identifier any
crate sends has a description in a shipped message file
(`every_sent_identifier_is_described`; the identifiers owed before
this check are listed in `tests/undescribed.txt`, which may only
shrink).

The LIC's screens (the IPL menu, DST, the installer and SST's panels)
are exempt from identifiers: they run before the object store and its
message files exist, and IBM's LIC panels word their message lines with
no ID. A failure there names what did not complete
(`svctools::dst::failed`); the reason goes to the console log.

IBM's identifiers now used, read with F1 on the message line and with
DSPMSGD on pub400 (V7R5M0); the texts are ours:

| Where | Situation | ID |
|---|---|---|
| STRSQL | SELECT ran | SQL6072 |
| STRSQL | INSERT, UPDATE, DELETE ran | SQL7956, SQL7957, SQL7955 |
| STRSQL | no row changed | SQL0100 |
| STRSQL | session start | SQL7971 |
| STRSQL | COMMIT, ROLLBACK, other statements | SQL7960, SQL7961, SQL7977 |
| SEU | command not known, quote not closed | EDT0100, EDT0120 |
| SEU | conflicting line commands, block open | EDT1020, EDT1005 |
| SEU | shift stopped, line command not here | EDT1021, EDT1002 |
| SEU | string not found, found, changed (n) | EDT1618, EDT1619, EDT1626, EDT1646 |
| SEU | F4 off a record, no such prompt | EDT1103, EDT1023 |
| SEU | SAVE or F4 while browsing | EDT0102, EDT0001 |
| Debugger | breakpoint added, removed, not found, none | CPF9C01, CPF9C17, CPF9C29, CPF9C04 |
| Debugger | all breakpoints removed, line not valid | CPF9C37, CPF9C19 |
| Debugger | watch added, cleared, all cleared, none | CPF9C08, CPF9C10, CPF9C12, CPF9C65 |
| Debugger | unknown command, no operand, syntax | CPF9C43, CPF9C42, CPF9C59 |
| Debugger | qualified, string not found, no equates | CPF9C33, CPF9C14, CPF9C38 |
| Debugger | stops (breakpoint, step, watch) | CPF9C31, CPF9C34, CPF9C64 |
| Debugger | program added, removed; module not found | CPF9C20, CPF9C26, CPF9C39 |
| Debugger | cursor not on source, no source view | CPF9C28, CPF9C02 |
| Debugger | not a variable (EVAL, ATTR) | CPF7E12 |
| Security | special authority missing | CPF222E |
| Security | special authority given beyond the giver's | CPF2291 (IBM's CRTUSRPRF documentation) |
| DSPTAPCTG | no cartridge matched | CPF67A1 (an ID asked for), CPF67EE (a category) |

IBM's `CPF2222` is a storage-limit message; nothing sends it now. SEU's
message file could not be named on pub400 (not on the library list);
ours is `QEDTMSG`. SEU's save and exit messages, the debugger's stops and
the DSPTAPCTG answer were not reproduced on an oracle (calling a
program and a tape library are not available read-only); their IDs come
from the message texts read with DSPMSGD.
