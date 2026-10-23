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

A command the pub400 account is not authorized to (`WRKSYSSTS`,
`WRKOUTQ`, `WRKJOBQ`, `WRKJOBSCDE`, `WRKSYSVAL`) leaves a message
instead of a panel; those snapshots were removed and the panels wait
for a system that allows them.
