# Probe scripts

Keystroke scripts for `tn5250-driver` (the step language is documented
in `tools/tn5250-driver/src/script.rs`). The same script runs against
an oracle and against QS/400 under QEMU; `snap NAME` writes a golden
with `--update` and compares with it otherwise.

Capturing from pub400.com (the only oracle with a reachable telnet;
read-only, one session at a time, the driver throttles to 500 ms):

```
cargo run -p tn5250-driver -- run --target pub400 --auto-signon \
    --golden tools/conformance/golden/verified/pub400 --update --release V7R5M0 \
    --timeout 20000 tools/conformance/probes/batch1-main.tn
```

The goldens land in `tools/conformance/golden/verified/pub400/` as
`NAME.txt` (the masked character grid) and `NAME.attr` (the attribute
grid), with their provenance in `INDEX.toml`. Masks keep every row's
width: dates, times, the system name and the signed-on user by
default; `+jobnum`, `+device`, `+cpu`, `+number` on a `snap` where the
panel shows values that change between runs.

| Script | What it captures |
|---|---|
| `batch1-main.tn` | MAIN and its help, `GO CMDWRK`, Command Entry, `DSPLIBL`, `WRKACTJOB` and its views |
| `batch2-wrk.tn` | `DSPJOB`, `WRKSPLF`, `DSPMSG`, `WRKMSG`, `WRKUSRJOB`, `DSPUSRPRF`, `WRKSBS`, `DSPJOBLOG`, `WRKOBJ`, `DSPLIB` |
| `batch3-wrk.tn` | `WRKJOB` and its options, `WRKSBSJOB`, `DSPSBSD`, `WRKJOBD`/`DSPJOBD`, `WRKCLS`/`DSPCLS`, `DSPSYSVAL`, `DSPOBJD`, `DSPLOG`, `WRKRPYLE` |
| `batch4-misc.tn` | `DSPMSGD`, `WRKMSGD`, `DSPSFWRSC`, `DSPNETA`, `DSPOBJAUT`, `WRKBNDDIR`, `DSPDTAARA`, `DSPAUTUSR`, the Information Assistant, Additional Message Information, more `WRKACTJOB` keys, Command Entry's keys, two prompts |
| `batch5-menus.tn` | every menu `GO` reaches from MAIN: `MAJOR`, `VERB`, `SUBJECT`, the `CMDxxx` command menus with their second pages, the task menus |
| `batch6-prompts.tn` | 68 command prompts, with F10 (additional parameters), F11 (keywords) and F24 (more keys) where they differ |
| `seu-browse.tn`, `seu-shift.tn` | SEU browse mode (`OPTION(5)` on `QSYSINC` members): the display, its key sets, roll, shift and toggle, find, the find, browse and defaults displays |
| `seu-edit.tn`, `seu-prompts.tn`, `seu-lines.tn` | SEU edit mode on a member of the user's library (never saved): key sets, session defaults, find/change and browse/copy options, the system command window, member in use, Select Prompt, the prompts by source type, insert, pending blocks, exclude, the line commands, a CL syntax error, the Exit display, the recovery display |
| `batch7-sysdsp.tn` | the System Request menu and options 3 to 7 and 80, the Attention program, a break message, `DSPSGNINF`, `CHGPWD`, `GO CMDSIGNOFF`, the `SIGNOFF` prompt (2026-10-23: only the menu, its help and option 3 came back before the keyboard stayed locked) |
| `pdm.tn`, `pdm2.tn` | PDM: `STRPDM` and its help, the Specify panels, Change Defaults, the three list panels over the account's own library and `QSYSINC/QRPGLESRC` with every option and key set, names only, the date view, the subset panels, Work with User-Defined Options, the Find String panel (read-only: every panel left with F3 or F12) |
| `work2-panels.tn` | panels batch 2: `WRKSBMJOB`, `WRKDSKSTS`, `WRKOUTQD`, `DSPSPLF` and its keys and `Control` line, `WRKLIB`, `WRKOBJLCK`, `DSPPFM` (F10 side-by-side hex, F11 vertical hex, F20, F24), `WRKJRN`, `DSPOVR`, `WRKSBSD`, the third `WRKACTJOB` key page (2026-10-23: `WRKSHRPOOL`, `WRKSYSSTS`, `WRKSYSACT`, `WRKPRB` and the `QPGMR` group refused; no `QSQJRN` in the user's library, so no `WRKJRNA`, `DSPJRN`, `DSPJRNRCVA`) |
| `work2-wide.tn` | `WRKACTJOB` at a 27x132 display (`--model m3477`; 2026-10-23: pub400 drew the 80-column panel, snapshots dropped) |
| `batch8-system2.tn` | `DSPPGM` (OPM and ILE, displays 1 to 5), `DSPSRVPGM` (1 to 5), `DSPPGMREF`, `DSPBNDDIR`, `DSPCMD`, `WRKDEVD`, `WRKHDWRSC`, `DSPUSRPRF` `TYPE(*OBJAUT *OBJOWN *OBJPGP *CMDAUT *DEVAUT)`, `EDTLIBL`, `EDTOBJAUT` with F11 and F6 (displayed only); refused there: `DSPHDWRSC`, `DSPPTF`, `DSPAUDJRNE`, `CHGRPYLE`, and `DSPAUTLOBJ`/`EDTAUTL` (no list of that name) |
| `batch8-uim.tn` | the UIM engine's displays on `WRKOBJ`: field help, extended help, keys help, F1 on the title, an option that is not valid; `WRKUSRPRF`, `WRKMSGF` |
| `batch9-uimpos.tn` | the F17 position-to windows of `WRKOBJ` and `WRKUSRPRF` |
| `batch8-sysreq.tn` | the rest of batch 7 in one session (2026-10-23: option 4, the `DSPMSG` prompt, came back; the session stalled after it) |
| `batch8/*.tn` | one system display per session: System Request options 5 and 7 (6 was dropped: other users' messages; 80 was cut off by a waiting break message), the Attention program (`ASSIST`, the same grid as `menu-assist`), a break message on Display Messages; `SNDPGMMSG` to `*EXT` is refused from a command line, so Display Program Messages has no golden |
| `strsst.tn` | `STRSST` (2026-10-23: refused on pub400, the account has no `*SERVICE`: "not authorized to command"; no snapshot kept, SST's screens follow the DST goldens in `golden/dst/` and `golden/sst/`) |
| `dbg.tn`, `dbg2.tn`, `dbg3.tn` | `STRDBG UPDPROD(*NO)` over the account's own programs (compiled without a source view): Display Module Source and its three key pages, Page Down/Up at the ends, every function key (F14 Work with Module List, F18 Work with Watch, F21 the command window, F1 help, the rest as answers), every debug command's answer (`DISPLAY EQUATE` shows Evaluate Expression, `THREAD` Work With Debugged Threads, `HELP` its help), the `STRSRVJOB` prompt (2026-10-23; F13 and F15 need a source view, F10/F22/F3 and `STEP` leave the display, `STRSRVJOB`/`ENDSRVJOB` are refused in debug mode, the first pass lost most snapshots to F12 and was rerun as `dbg2.tn`) |
| `dfu.tn`, `dfu2.tn` | DFU and RLU: the `STRDFU` menu, Run a DFU Program and its two entry panels, Create/Change/Delete a DFU Program, the temporary program's entry panel, Define General Information (left with F12, nothing saved), `UPDDTA` over the account's own files in change, entry and insert modes with a record shown, a record not found, the start of the file, Select Record Format and End Data Entry (no record changed), `STRRLU`'s Design Report with its three key sets and Exit RLU (left with option 2). `DSPDTA` needs a DFU program, which pub400 has none of |
| `strsql.tn` to `strsql4.tn` | STRSQL: Enter SQL Statements and its key pages, Select SQL Statement, every statement prompt, Services and Change Session Attributes, Display Data (one column; a wide catalog query shifted, paged, positioned), the history, a statement in error, the help, the select lists behind F16 to F18, Exit Interactive SQL. Only SELECTs run. The first version of `strsql.tn` (2026-10-23, since corrected) pressed F12 on Enter SQL Statements, which leads to the Exit display, and its next Enter took the default option 1: the session was saved once on pub400 (a later run's option 2 discarded it). After a prompt the cursor sits on a protected row; the later scripts type after Erase Input and Backtab and use no `continue` |
| `sda.tn` | SDA over the account's `QDDSSRC` member `SDAPROBE`: the `STRSDA` menu, Design Screens, Design Menus, Test Display File, Work with Display Records with both key sets and the text view, Add New Record, Select Record/Field/File Keywords, record and file comments (the SEU display and its exit), Subset Records, Change Defaults, the design image with the ruler, F9's message, Select Database Files, `H` and `R` typed before a constant and a field, Work with Fields, Exit SDA Work Screen, Save DDS - Create Display File and Recover SDA Session (read-only: the image left with option 2, the Save display with both prompts cleared; 2026-10-23) |

A command the pub400 account is not authorized to (`WRKSYSSTS`,
`WRKOUTQ`, `WRKJOBQ`, `WRKJOBSCDE`, `WRKSYSVAL`) leaves a message
instead of a panel; those snapshots were removed and the panels wait
for a system that allows them.

Phase 10's scripts (TCP/IP configuration and status, communications
resources, the IFS displays, the registration facility, QSH, the
command prompts) are under `phase10/` with their own README.
