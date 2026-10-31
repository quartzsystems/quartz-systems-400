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
- **Fix**: the commit, or the agent that owns it.

## Operator/system batch (named in the addendum)

| Command | Wired today | Should read from | Panel | Fix |
|---|---|---|---|---|
| DSPSFWRSC | panel/fallback; service text when the read failed | `sfwrsc_panel` (licpgm directory) | QUISFWRSC | 0f564e8f (class fix) |
| WRKCFGSTS | panel/fallback for `CFGTYPE` other than `*LIN`/`*CTL`/`*DEV` | `cmn_panel CFGSTS` (cmncfg) | QUICFG | 0f564e8f (class fix) |
| DSPDEVD | panel/fallback (`.ok()?` on `devd_panel`) | `devd_panel` (`*DEVD` objects) | QUICFG | 0f564e8f (class fix) |
| CHGDEVD | service CPP over the store (`cpp/sysinfo.rs`) | the `*DEVD` object | none (action) | b370c63d (step 82 setup runs it) |
| VRYCFG | service CPP (`cpp/cmncmd.rs`) over cmncfg | cmncfg status table | none (action) | b370c63d (step 82 setup runs it) |
| DSPPTF | panel showing a fixed empty list (`sfwrsc_panel`), "none in this release" | the PTF database `QSYS/QSPTFDB` (`cpp/ptfcmd.rs`) | QUIDSPPTF | 554b33a4 |
| RTVCFGSTS | job CPP over `cfg_status` | cmncfg status table | none (retrieve) | b370c63d (step 82 setup runs it) |
| CRTDEVDSP | service CPP over the store (`cpp/sysinfo.rs`) | the `*DEVD` object | none (action) | b370c63d (step 82 setup runs it) |
| DSPNETA | panel/fallback (`.ok()?` on `neta_panel`) | `neta_panel` (network attributes) | QUIDSPNETA | 0f564e8f (class fix) |
| CHGNETA | service CPP over the store (`cpp/sysinfo.rs`) | the network attributes | none (action) | b370c63d (step 82 setup runs it) |
| WRKHDWRSC | console word `hw` ("answered by the console") behind a panel/fallback | the hwrsrc service | QUIHDWRSC | hwrsrc agent |
| DSPHDWRSC | console word `hw` behind a panel/fallback | the hwrsrc service | QUIHDWRSC | hwrsrc agent |

## Messages batch

| Command | Wired today | Should read from | Panel | Fix |
|---|---|---|---|---|
| DSPMSG | panel/fallback to bridge word `dspmsg` | message queues over the link | QUIDSPMSG | DSPMSG agent |
| WRKMSG | panel/fallback to bridge word `dspmsg` | message queues over the link | QUIDSPMSG | DSPMSG agent |
| WRKMSGD | text: bridge word `wrkmsgd`, no panel | `msgf_list` / message file payload | QUIDSPMSGD (WRKMSGD panel) new | 554b33a4 |
| DSPMSGD | panel/fallback | message file over the link | QUIDSPMSGD | 0f564e8f (class fix) |
| DSPLOG | panel/fallback | `QHST` over the link | QUIDSPLOG | 0f564e8f (class fix) |
| WRKRPYLE | panel/fallback | the reply list | QUIWRKRPYL | 0f564e8f (class fix) |

## Display commands with only the service's text

"Report viewer" is `QUIRPTVW`: the command's own processing program
runs in the storage service and its lines show on a panel, never on a
menu's message line. A list panel of its own is still open for those
(`TODO(phase12)` in `work/qsjob/src/panels/info/reports.rs`).

| Command | Wired today | Reads from (after) | Panel | Golden | Fix |
|---|---|---|---|---|---|
| DSPPTF | panel over a fixed empty list | `sys2_panel PTF`, `PTFD` (`stgsvc/cppdata.rs` over `QSYS/QSPTFDB`) | QUIDSPPTF `DSPPTF`, `DSPPTFD` | `verified/oracle71/audit/dspptf` (7.1) captured; no geometry test yet | 554b33a4 |
| WRKPTFGRP | text (`cpp/ptfsvc.rs`) | `sys2_panel PTFGRP` (`ptf::grpdb`) | QUIDSPPTF `WRKPTFGRP` | `verified/oracle71/audit/wrkptfgrp` (7.1) captured; no geometry test yet | 554b33a4 |
| DSPIPLA | text (`cpp/iplcmd.rs`) | `sys2_panel IPLA` (the LIC record) | QUIIPLA new | `phase11/displays/dspipla`, row for row | 554b33a4 |
| WRKMSGD | text: bridge word `wrkmsgd` | `msgf_list` | QUIDSPMSGD `WORK` | `audit/displays/wrkmsgd`, geometry | 554b33a4, this batch |
| WRKRDBDIRE | text: screen rows printed as lines (`command/ddm.rs`) | the RDB directory data area | QUIRDBDIR new | `phase10/misc/wrkrdbdire`, geometry | 554b33a4 |
| DSPPTFCVR | text (`cpp/ptfcmd.rs`) | the service's CPP | report viewer | refused on pub400 | 554b33a4 |
| DSPBCKSTS | text (`cpp/opacmd.rs`) | the service's CPP | report viewer | `audit/displays/dspbcksts` captured; panel open | 554b33a4 |
| DSPLICKEY | text (`cpp/lpcmd.rs`) | the service's CPP | report viewer | `verified/oracle71/audit/dsplickey` (7.1, key and serial masked); panel open | 554b33a4 |
| DSPUDFS | text (`cpp/ifscmd.rs`) | the service's CPP | report viewer | none (needs a UDFS) | 554b33a4 |
| DSPTAP | text (`cpp/medcmd.rs`) | the service's CPP | report viewer | none (no tape on pub400) | 554b33a4 |
| DSPTAPCTG | text (`cpp/medcmd.rs`) | the service's CPP | report viewer | none (no library) | 554b33a4 |
| DSPOPT | text (`cpp/medcmd.rs`) | the service's CPP | report viewer | none (no volume) | 554b33a4 |
| WRKOPTVOL | text (`cpp/medcmd.rs`) | the service's CPP | report viewer | `audit/displays/wrkoptvol` captured; panel open | 554b33a4 |
| GO | the service answers `CPF0901`; menus and Command Entry intercept `GO` | `*MENU` objects | the menu | phase 8 | correct as wired |

## Panels filled by parsing the service's text

| Command | Wired today | Should read from | Panel | Fix |
|---|---|---|---|---|
| DSPOBJAUT | text parse of `dspobjaut` | the object's authorities | QUIDSPOAUT | open |
| EDTOBJAUT | text parse of `DSPOBJAUT` | the object's authorities | QUIEDTAUT | open |
| DSPAUTLOBJ | text parse of `DSPAUTLOBJ` | the list's objects | QUIAUTLOBJ | open |
| WRKOBJOWN | text parse of `wrkobjown` | the owner index | QUIWRKOOWN | open |
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
| WRKIMGCLG, WRKIMGCLGE, WRKTAPCTG, WRKMLBSTS | panels/work2/media | media requests |
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
the text path there. Owner: the console agent (console as a 5250
device).

## Step 82 (QEMU, `--only 82`)

The step runs every display command of the shipped definitions from
`MAIN` (134 with `GO`, `DSPMODSRC` and `DSPBKP` inside a `STRDBG`
session; `DSPDTA` is not run: it needs a DFU program). Its setup makes
the objects the parameter table names and runs `CHGDEVD`, `CHGNETA`,
`CRTDEVDSP`, `VRYCFG` and, in a CL program, `RTVCFGSTS`, so every
command of this audit has a harness invocation.

Last run at b5c2fe68: one failure.

| Command | Result | Owner |
|---|---|---|
| GO | `GO USER` from MAIN's command line shows no menu and no message (the job log has the request only); not diagnosed | open |

`WRKHDWRSC`, `DSPHDWRSC`, `DSPMSG` and `WRKMSG` passed in this
worktree through the class fix (a panel or the function check); their
owners' changes replace the `hw` word and the message panels.
