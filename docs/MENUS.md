# Menus

The `menus` crate (`ui/menus`) implements this document: the `*MENU`
object's payload, the state machine behind `GO` and the menu keys,
and the generator of IBM's command-group menus as UIM panel groups
(`ui/menus/pnlgrp/`). The crate renders nothing and never touches the
store; the menu driver, `CRTMNU`, `GO` and the display come with the
rest of Phase 8. `ui/menus/tests/spec.rs` compares the code's layout
with the table in §1; `tests/generated.rs` keeps the committed sources
what the generator writes. IBM's menu names, titles, option numbers
and function keys are the screen interface; every help text and
message here is ours.

## 1. The object

`CRTMNU MENU(lib/name) TYPE(*DSPF|*PGM|*UIM) DSPF() MSGF() PGM()
PNLGRP() CURLIB(*NOCHG|*MNULIB|*CRTDFT|lib) PRDLIB(*NOCHG|*NONE|lib)
CMDLIN(*LONG|*SHORT|*NONE) TEXT()` makes a `*MENU` whose space holds
one `Menu` record. `menus::MenuAttrs` is the record (`encode`,
`decode`, `validate`): a `*DSPF` menu needs `DSPF` and `MSGF`, a
`*PGM` menu `PGM`, a `*UIM` menu `PNLGRP`; the panel group holds a
`:PANEL.` named like the menu with a `:MENU.` area. Names are blank
when absent; a library is a name or `*LIBL`/`*CURLIB` as written.

### `Menu` (168 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | Magic | `QSUIMENU` in ASCII |
| 8 | 2 | Version | 1 |
| 10 | 1 | Kind | 1 `*DSPF`, 2 `*PGM`, 3 `*UIM` |
| 11 | 1 | CmdLin | 0 `*NONE`, 1 `*SHORT`, 2 `*LONG` |
| 12 | 1 | CurLib | 0 `*NOCHG`, 1 `*MNULIB`, 2 `*CRTDFT`, 3 named |
| 13 | 1 | PrdLib | 0 `*NOCHG`, 1 `*NONE`, 2 named |
| 14 | 2 | Reserved | zero |
| 16 | 10 | Dspf | |
| 26 | 10 | DspfLib | |
| 36 | 10 | Msgf | |
| 46 | 10 | MsgfLib | |
| 56 | 10 | Pgm | |
| 66 | 10 | PgmLib | |
| 76 | 10 | Pnlgrp | |
| 86 | 10 | PnlgrpLib | |
| 96 | 10 | CurLibName | when CurLib is 3 |
| 106 | 10 | PrdLibName | when PrdLib is 2 |
| 116 | 50 | Text | |
| 166 | 2 | Reserved2 | zero |

## 2. The menu stack

`menus::MenuStack` is the chain of menus a job is showing, the first
one at the bottom. `MenuStack::new(first, user)` starts it on the menu
a sign-on's initial menu or a `GO` from Command Entry showed; `user`
is the profile's initial menu (`None` for `*SIGNOFF`).

- `GO MENU(lib/name) RTNPNT(*YES)` (`push(menu, true)`) shows the
  menu and keeps the current one as the return point; `RTNPNT(*NO)`
  (`push(menu, false)`) shows it in the current one's place, so from
  the initial menu the new menu becomes the initial one, the classic
  use in an initial program.
- F12=Cancel (`cancel`) returns to the previous menu; F3=Exit (`exit`)
  returns to the menu shown when the stack started. Either on the
  initial menu finishes the stack (`None`): the caller goes back to
  whatever started it, Command Entry or the sign-off when the initial
  menu was the sign-on's.
- F16=System main menu (`main`) and `GO *MAIN` push `QSYS/MAIN`; the
  command-group menus' F16=Major menu is `MENU MAJOR`, a push.
- `GO *USER` pushes the profile's initial menu, `Outcome::NotFound`
  when it is `*SIGNOFF`; `GO *SIGNOFF` is `Outcome::SignOff`.
- `GO CMD*` (a generic name): the caller resolves the names against
  the library list and passes `Target::Choice(list)`; one match is
  pushed, several are `Outcome::Choose(list)` for a selection display
  whose choice the caller then pushes, none is `NotFound`.

`current`, `initial`, `depth` and `frames` read the state.

## 3. The generator

`genmenus` (`cargo run -p menus --bin genmenus`) writes one UIM source
per menu under `ui/menus/pnlgrp/`; `menus::generate` is the library
behind it. The inputs are in the repository:

- `ui/menus/inventory/ibm-menu-list.txt`, IBM's `*MENU` objects with
  their texts: every `CMDxxx` row is a command-group menu whose title
  is the text;
- `tools/conformance/oracle/ibm-cmd-list.txt`, IBM's `*CMD` objects
  (`QSYS` and `QTCP` are taken, one row per name, 7.1 and 7.5
  together): the set that orders and numbers the options;
- `command/sys-cpps/cmds/*.cmd`, our definitions: the `PROMPT` of the
  `CMD` statement is the text an option shows.

### Membership

A menu's code is its name without `CMD`. The verbs (`member::VERBS`,
`ADD` to `WRK`) make verb menus: `CMD<verb>` holds every command whose
name starts with the verb. `CRTF` and `CRTPGM` are verb-and-subject
menus (`CRT`, then the subject). Every other code is a subject: the
name's leading verb is stripped (the longest verb that starts it) and
the rest must carry the code as a whole token: the longest known
token starting at that position is the code itself, and what precedes
it splits into known tokens. The tokens are the menu codes, the verbs,
the one-letter suffixes (`A`, `D`, `E`, `F`, `L`, `M`, `Q`) and a few
abbreviations no menu names (`member::EXTRA_TOKENS`). So `CRTPF`,
`CHGPFM`, `CHGSRCPF` and `CVTPAGSPFM` are physical-file commands and
`DSPFD`, `STRPFRMON` are not; `CHGJOB`, `ADDJOBSCDE`, `BCHJOB` are job
commands and `CHGJOBD`, `WRKJOBQ` are not. The one-letter subject `F`
(`CMDFIL`, `CMDCRTF`) matches every file-type token
(`member::FILE_TOKENS`: `PF`, `LF`, `DSPF`, `PRTF`, `SAVF`, ...), so
`CRTPF` is on the file menu and on the physical-file menu while `CPYF`
is on the file menu only. `member::CURATED` adds what no token finds:
the command-definition statements to `CMDCMDDEF`, the CL control
statements to `CMDCL`, `RETURN` to `CMDRTN`. IBM lists `CMDAUD` and
`CMDCNR` on `VERB` though no command starts with either; they are
filled by the subject rule.

Within a menu the commands are in EBCDIC name order and numbered 1
to n over IBM's whole list. A command QS/400 does not ship keeps its
number and its row is blank, as IBM leaves rows blank for commands
the user may not run (`menu-cmdjob.txt`: option 2 is blank). The
numbers therefore follow IBM's where the two lists agree and shift
where they do not: the 7.1 oracle carries proxies pub400 has not
(`ADDBLDOPTE`, `ADDBUILDER`), pub400's list hides commands its user
may not see (`ADDACC`, `ADDJOBSCDE`, `CHGNETA`), and IBM's menus are
curated by hand (`CMDJOB` lists `ADDAJE`, `CHGGRPA`, `DATA` and
`DLTTRC`; `CMDWRK` lists `NETSTAT` first; `WRKARMJOB` precedes
`WRKAPPNSTS` and `DSPJVMJOB` precedes `DSPJOB` there).
`tests/generated.rs` states each of these per menu.

### The sources

Every menu is one panel group with a `:PANEL.` named like the menu,
`PANELID` the menu name, the inventory text as title, a `:MENU
DEPTH='*' SCROLL=YES` area under the heading `Commands`, a
`:CMDLINE SIZE=LONG` with the prompt `Selection or command`, the keys
`F3=Exit`, `F4=Prompt` (`RETURN 4`: the driver prompts the command
line), `F9=Retrieve`, `F12=Cancel` and `F16=Major menu` (`MENU
MAJOR`), and a help module `HLP<name>` with two Quartz-written
paragraphs. Each option is
`:MENUI OPTION=n ACTION='CMD ?NAME'.Title  NAME`: the `CMD` action
(`docs/UIM.md` §2) runs the command string, the leading `?` prompts
it. Two conventions the menu renderer honours:

- a menu item's text may end with two blanks and a word: the word is
  shown right-aligned in a column starting at column 70, the text
  before it at column 10 (`menu-cmdjob.txt`, `menu-major.txt`);
- a menu item with no text is a reserved option: its row is blank and
  the number is kept; selecting it reports that the command is not
  available. The renderer also blanks the row of a command the user
  may not run.

`MAJOR` (`fixed::MAJOR`) lists the 23 groups of `menu-major.txt` with
`TOPSEP=SYSNAM` (the `System:` line), `F13=Information Assistant`
(`MENU INFO`) and `F16=System Main menu` (`MENU MAIN`); option 1 is
`CMD SLTCMD`, the others `MENU <group>`. `VERB` lists every verb menu
of the inventory in name order (`menu-verb.txt`), `SUBJECT` every
other generated menu (`menu-subject.txt`), both as `MENU <name>` with
the name at column 70.

## 4. Inventory reconciliation

Counts of 2026-10-23 (`genmenus` prints them):

| | |
|---|---|
| `CMDxxx` menus in IBM's list | 520 (506 on 7.1, 14 only on 7.5) |
| generated | 512: 509 command menus and `MAJOR`, `VERB`, `SUBJECT` |
| hand-written later | 11 of `MAJOR`'s groups whose contents IBM curates: `CMDOBJMGT`, `CMDFILE`, `CMDSAVRST`, `CMDWRKMGT`, `CMDDTAMGT`, `CMDSYSCTL`, `CMDCMNMGT`, `CMDDSTSRV`, `CMDPRBMGT`, `CMDMSGHDL`, `CMDS36` (`CMDS38` and `CMDOFC` are on `MAJOR` but in no inventory) |
| generated but empty | 22 whose code starts or tokenizes no command of the list: `CMDADMDMN`, `CMDAMT`, `CMDBGU`, `CMDCPH`, `CMDCRSDMN`, `CMDDEVMGT`, `CMDDPR`, `CMDENC`, `CMDGDF`, `CMDINST`, `CMDIWS`, `CMDJS`, `CMDLBL`, `CMDMAC`, `CMDMGD`, `CMDMGR`, `CMDOBJTYPE`, `CMDRJECTL`, `CMDRQS`, `CMDUBC`, `CMDWG`, `CMDWLCGRP` |
| commands considered | 2218 (`QSYS` and `QTCP`, both releases) |
| options placed | 5171, of which 831 show a title (a command we ship); a command is on 2.3 menus on average |
| commands on no menu | 31: IFS and TCP/IP aliases (`MD`, `RD`, `DEL`, `REN`, `MOUNT`, `DIG`, `NSLOOKUP`, `TRACEROUTE`, `JAVA`, ...) and the 7.1 oracle's proxies (`BLDHELP`, `GETBLDOPTE`, `REPOBJ`, ...) |
| our commands not in IBM's list | 3: `CHGDEVD`, `DLTPRXCMD`, `SNDRPYMSG` |

`VERB` matches `menu-verb.txt` and `menu-verb-2.txt` option for
option; `MAJOR` matches `menu-major.txt` and `menu-major-2.txt`.
`SUBJECT` lists `CMDADMDMN` (6) and `CMDAPPN` (22), which IBM's does
not, so the options after each are one higher here.

## 5. `GO` in the job

`work/qsjob/src/work/menu.rs` runs menus for the interactive job.
`GO MENU(lib/name)` (also typed on any menu or at Command Entry; `*MAIN`,
`*USER`, `*SIGNOFF` and a generic `CMD*` resolved through
`list_objects`) starts a `MenuStack`; the current menu's `*MENU` object
and panel group come through the storage service (`menu_open`,
`pnlgrp_open`, the panel named like the menu) and the UIM engine shows
it. A `MENU` option pushes; `CMD` options and typed commands run as
`QCMD` requests with the first line of their output on the message
line (`CALL QCMD` opens Command Entry, `SIGNOFF` ends the session, a
nested `GO` recurses); `F3` and `F12` pop, and at the first menu
return to where `GO` was typed, or sign off when the menu was the
session's initial one; `F9` retrieves the previous command; `F23`
makes the menu the user's initial menu (`CHGUSRPRF INLMNU`). `*DSPF`
and `*PGM` menus are refused with a message. The session's `MAIN` is
`QSYS/MAIN` through this path, the interim panel standing in when the
object is missing.

The shipped menus (`menus::shipped::FILES`: the generated command menus
under `pnlgrp/` and the hand-written task menus under `sys/`, `MAIN`
first) are compiled and created in `QSYS` at service start when
missing, a `*PNLGRP` and a `*MENU` of the same name each
(`lic/slic/src/svc/ui.rs`). `CRTPNLGRP` and `CRTMNU`
(`lic/slic/src/cpp/uicmd.rs`) make them from a source member and from
parameters; `DLTPNLGRP` and `DLTMNU` delete them.

## 6. The task menus

The hand-written menus under `ui/menus/sys/` are IBM's task menus,
reached from `MAIN` and from each other. Option numbers, titles and
option texts follow the pub400 goldens (`menu-<name>.txt`, the second
page `menu-<name>-2.txt`); `ui/menus/tests/task_menus.rs` renders each
and compares it with its golden row by row, the message line aside.
Help modules are ours. The only renaming is MAIN's: `IBM i Access
tasks` reads `QS/400 Access tasks` (`OFCTSK` 2, `DATA` 4) and leads to
`ACCESS`, which has no golden: the telnet server, display devices and
network attributes a 5250 client meets.

| Menu | Title | Options |
|---|---|---|
| `USER`, `USER2` | User Tasks | 1-9, 60, 90; 1-3 |
| `OFCTSK` | Office Tasks | 2-7 (1 is not shown on pub400) |
| `SYSTEM`, `SYSTEM2` | General System Tasks | 1-10, 60; 1-3, 70, 80 |
| `DATA` | Files, Libraries, and Folders | 1-5 |
| `PROGRAM` | Programming | 1-6, 8, 9, 50, 70 |
| `CMN` | Communications | 1-9, 70 |
| `DEFINE` | Define or Change the System | 1-6, 8, 9 |
| `PROBLEM`, `PROBLEM2` | Problem Handling | 1-6, 60, 70; 1-7, 70 |
| `CFG` | Configuration | 1-4 |
| `DEVICE` | Device Operations | 1-7, 70 |
| `DOCUMENT` | Documents | 1-3, 70 |
| `FILE`, `FILE2` | Files | 1-11, 60; 1-4, 50, 51, 70 |
| `HARDWARE` | Hardware Resources | 1-9, 70 |
| `JOB`, `JOB2` | Jobs | 1-7, 20, 60, 70; 1-8, 70 |
| `LIBRARY` | Libraries | 1-4, 50, 51, 70 |
| `MESSAGE` | Messages | 1-8, 70 |
| `NETWORK` | Network Management | 1-10, 12, 14, 70 (two pages) |
| `PRINTER` | Printer | 1-6, 70 |
| `PTF` | Program Temporary Fix | 1-13, 70 (two pages) |
| `RECOVERY` | Optical Backup/Recovery | 1-8 |
| `RESTORE` | Restore | groups: 1-9, 21-23, 30-32, 40-42, 50, 70 |
| `SAVE` | Save | groups: 1-8, 10, 11, 20-23, 30-33, 40-43, 50, 70 |
| `SECURITY` | Security | 1-5, 8, 70 |
| `SOFTWARE` | Software Resources | 1, 2 |
| `STATUS` | Status | 1-10 |
| `SUPPORT` | User Support and Education | 1-5, 7-9 |
| `TCPADM` | TCP/IP Administration | 1, 2, 7-11, 20 |
| `ACCESS` | QS/400 Access Tasks | 1-6 |

The Operational Assistant menus (`ASSIST`, `BACKUP`, `CLEANUP`,
`DEVICESTS`, `DISKTASKS`, `POWER`, `SETUP`, `TECHHELP`) and `INFO` (the
Information Assistant, golden `information-assistant`) have no command
line: the heading asks for a number typed below, `:BOTINST.` gives the
`Type a menu option below` row over the selection field, and the keys
are `F1=Help`, `F3=Exit`, `F9=Command line` (`CMD CALL QCMD`, Command
Entry) and `F12=Cancel`. IBM's `ASSIST` is a display-file menu whose
title sits one column right of a panel group's centering; its test
compares that title by its words.

Every other menu has the keys of `MAIN` with `F16=System Main menu`
(`MENU MAIN`) in place of `F23`, except `RECOVERY` and `TCPADM` (no
`F13` or `F16`) and `SUPPORT` (`F16=Major menu`, no `F13`), as their
goldens show. An option runs the IBM command or shows the IBM menu it
names (`CMD ?SAVLIB`, `MENU CFGCMN`) whether or not QS/400 ships it;
one we do not ship answers that it is not available. Options pub400
hides keep their number unused, which leaves the blank row a skipped
number gives; `SAVE` 34 is a reserved option (no text), whose blank
row keeps the next group on the third page as IBM's does.

## 7. Open questions

- IBM's subject menus are curated: whether a `CMDJOB` option such as
  `ADDJOBQE` or `CHGJOBD` is listed cannot be read from the goldens
  (the rows are blank for pub400's user). A capture as `QSECOFR`
  would settle the `CMDJOB`, `CMDPF` and `CMDWRK` lists.
- Whether `TOPSEP=SYSNAM` is the attribute IBM's `MAJOR` uses, or
  the `System:` line comes from the menu driver, needs a `DSPPNLGRP`
  equivalent on a real system; the renderer treats both the same.
- The empty menus could be dropped from `SUBJECT`; IBM keeps `CMDAMT`
  there (option 15 of `menu-subject.txt` is blank), so they stay.
- `MAJOR`'s option 1 runs `SLTCMD`, which QS/400 does not ship yet.
