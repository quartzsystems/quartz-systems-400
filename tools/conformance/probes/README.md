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
| `batch7-sysdsp.tn` | the System Request menu and options 3 to 7 and 80, the Attention program, a break message, `DSPSGNINF`, `CHGPWD`, `GO CMDSIGNOFF`, the `SIGNOFF` prompt (2026-10-23: only the menu, its help and option 3 came back before the keyboard stayed locked) |
| `pdm.tn`, `pdm2.tn` | PDM: `STRPDM` and its help, the Specify panels, Change Defaults, the three list panels over the account's own library and `QSYSINC/QRPGLESRC` with every option and key set, names only, the date view, the subset panels, Work with User-Defined Options, the Find String panel (read-only: every panel left with F3 or F12) |
| `work2-panels.tn` | panels batch 2: `WRKSBMJOB`, `WRKDSKSTS`, `WRKOUTQD`, `DSPSPLF` and its keys and `Control` line, `WRKLIB`, `WRKOBJLCK`, `DSPPFM` (F10 side-by-side hex, F11 vertical hex, F20, F24), `WRKJRN`, `DSPOVR`, `WRKSBSD`, the third `WRKACTJOB` key page (2026-10-23: `WRKSHRPOOL`, `WRKSYSSTS`, `WRKSYSACT`, `WRKPRB` and the `QPGMR` group refused; no `QSQJRN` in the user's library, so no `WRKJRNA`, `DSPJRN`, `DSPJRNRCVA`) |
| `work2-wide.tn` | `WRKACTJOB` at a 27x132 display (`--model m3477`; 2026-10-23: pub400 drew the 80-column panel, snapshots dropped) |
| `batch8-system2.tn` | `DSPPGM` (OPM and ILE, displays 1 to 5), `DSPSRVPGM` (1 to 5), `DSPPGMREF`, `DSPBNDDIR`, `DSPCMD`, `WRKDEVD`, `WRKHDWRSC`, `DSPUSRPRF` `TYPE(*OBJAUT *OBJOWN *OBJPGP *CMDAUT *DEVAUT)`, `EDTLIBL`, `EDTOBJAUT` with F11 and F6 (displayed only); refused there: `DSPHDWRSC`, `DSPPTF`, `DSPAUDJRNE`, `CHGRPYLE`, and `DSPAUTLOBJ`/`EDTAUTL` (no list of that name) |
| `batch8-uim.tn` | the UIM engine's displays on `WRKOBJ`: field help, extended help, keys help, F1 on the title, an option that is not valid; `WRKUSRPRF`, `WRKMSGF` |
| `batch9-uimpos.tn` | the F17 position-to windows of `WRKOBJ` and `WRKUSRPRF` |

A command the pub400 account is not authorized to (`WRKSYSSTS`,
`WRKOUTQ`, `WRKJOBQ`, `WRKJOBSCDE`, `WRKSYSVAL`) leaves a message
instead of a panel; those snapshots were removed and the panels wait
for a system that allows them.
