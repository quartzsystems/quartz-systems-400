# Collection Services and the performance tools

Collection Services samples the machine at an interval and keeps the
intervals in a management collection (`*MGTCOL`); `CRTPFRDTA` turns a
collection into the `QAPM*` database files. The statistics displays
(`WRKSYSSTS`, `WRKACTJOB`, `WRKDSKSTS`, `WRKSHRPOOL`, `WRKSYSACT`) and
the collector read the same samples, so figures shown at once agree.
`QPFRADJ` adjusts the shared pools from them. `WRKSYSACT` belongs to
Performance Tools, `QS00PT1` (IBM's 5770PT1); the rest is the operating
system's.

| Piece | Where |
|---|---|
| Categories, profiles, configuration, `*MGTCOL` payload, `QAPM*` models and records, `QPFRADJ` | `work/perf` |
| The model files' DDS (IBM's V7R1M0 layout) | `work/perf/dds/` |
| The captured layout | `tools/conformance/oracle/perf/qapm-ffd.txt` |
| The collector in the storage service | `lic/slic/src/stgsvc/perfcol.rs` |
| Samples and intervals (shared with the displays) | `lic/slic/src/stgsvc/stats.rs` |
| The commands | `lic/slic/src/cpp/pfrcmd.rs`, `command/sys-cpps/cmds/*PFR*.cmd` |
| Prompt and screen captures | `tools/conformance/golden/phase12/perf/` |

## Sampling

`stats::sample` is the one look at the machine: `/proc/<pid>/stat` per
job, `/proc/stat`, `/proc/meminfo`, each pool's cgroup `memory.stat`,
`/sys/class/block/<unit>/stat`, `/sys/class/net/<if>/statistics` of
every varied-on line, and the service's own counters. A look less than
half a second old is shared, so a display and the collector asking at
once see one sample. No daemon is added: the storage service counts
what jobs ask of it over the existing socket protocol, and the jobs'
status reports (`SetStatus`) carry what the service cannot see itself:

| Counter | Source |
|---|---|
| a job's database reads and writes, full opens | its `Io`, `SqlRun` and `Open` requests (`Stats::request`) |
| other storage requests | every other request it sends (`aux_io`) |
| interactive transactions and response time | `SetStatus`: input from the display starts a transaction (`DSPW` to running), the next `DSPW` ends it |
| a job's active-to-wait and ineligible transitions | `SetStatus`: a wait; a runnable status not granted a level |

## Categories and profiles

| Category | Records | Files |
|---|---|---|
| `*SYSLVL` | the machine: CPU used and available, storage, jobs, opens, transactions | `QAPMSYSTEM` |
| `*JOBMI` | each job's CPU, I/O, transactions, transitions, threads | `QAPMJOBMI`, `QAPMJSUM` |
| `*JOBOS` | each job's subsystem, transactions, logical I/O | `QAPMJOBOS` |
| `*DISK` | each unit's requests, blocks, busy time, capacity | `QAPMDISK` |
| `*CMNBASE` | each Ethernet line's bytes, frames and errors | `QAPMETH` |
| `*POOL` | each pool's size, activity level, faults, transitions | `QAPMPOOL` |
| `*SYSBUS` | nothing: QS/400 has no system bus; the category is accepted so IBM's profiles name it | none |

`QAPMCONF` (the collection's release, system, interval, start, profile,
processors and main storage) is written for every collection.

| Profile | Categories |
|---|---|
| `*MINIMUM` | `*POOL`, `*SYSBUS`, `*SYSLVL` |
| `*STANDARD`, `*STANDARDP`, `*ENHCPCPLN` | all seven (the protocol and capacity planning categories of the larger profiles have nothing more to measure here) |
| `*CUSTOM` | the configuration's list (`*STANDARD`'s by default) |

## Commands

| Command | What it does |
|---|---|
| `STRPFRCOL COLPRF() CYCCOL()` | starts a collection in the configured library (created when missing), first deleting collections older than the retention period. `*JOBCTL`. `CPIB920`; `CPFB993` when one is active |
| `ENDPFRCOL FRCCOLEND()` | records a last interval and ends the collection. `CPIB921`; `CPFB994` when none is active |
| `CHKPFRCOL` | `CPIB922` with the collection, or the escape `CPFB994` |
| `CFGPFRCOL` | changes the configuration (`*SAME` keeps a value); its prompt shows the current values (`QSPMTOVR`). `CPIB923`; `CPFB94C` for an interval not allowed |
| `CRTPFRDTA FROMMGTCOL() TOMBR() TOLIB() CGY()` | writes the collection's records, member `TOMBR` (the collection's name by default) of each file, created from the model when missing. `CPIB924`; `CPFB995` for a collection not found |

The configuration (`QSYS/QYPSCOLCFG`, a `*CHAR` data area of
`KEY=VALUE;` entries) holds the interval (15, 30, 60, 300, 900, 1800 or
3600 seconds), the library (`QPFRDATA`), the default profile
(`*STANDARDP`), the categories of `*CUSTOM`, the cycle time and interval,
the retention (120 hours) and `CRTDBF`/`CRTPFRSUM`. With `CRTDBF(*YES)`
each interval's records go to the files as it is recorded, member named
after the collection. With `CYCCOL(*YES)` a collection ends at the cycle
time (then every cycle interval) and the next starts.

The messages `CPFB94C` and `CPF8225` are IBM's; the identifiers of the
start, end, active and configuration messages could not be read on the
primary without starting Collection Services there, so `CPIB920` to
`CPIB924` and `CPFB993` to `CPFB995` are this release's, chosen from
ranges IBM's V7R5M0 message file leaves unused.

## Management collections

A collection is a `*MGTCOL` (type code `0x002C`, attribute `*PFR`)
named `Q`, the day of the year and the time it started, local time:
`Q303055930` started on day 303 at 05:59:30, IBM's naming. Its payload
(`work/perf/src/mgtcol.rs`) is a fixed head, the header, then one frame
per interval appended as the collector records it:

| Offset | Size | Field |
|---|---|---|
| 0 | 8 | `QSMGTCOL` |
| 8 | 2 | version, big-endian (1) |
| 10 | 2 | reserved |
| 12 | 8 | end time, Unix seconds local, 0 while active |
| 20 | 4 | intervals recorded |
| 24 | 4 | header length |
| 28 | n | the header: start, interval, profile, categories (bits), system, release, the library of `CRTDBF(*YES)`, cycling |
| 28+n | | frames: a 4-byte big-endian length and one `Interval` |

The header and the intervals (`work/perf/src/sample.rs`) are in the wire
encoding of `work/ipc`. An interval holds its number, end time and
length, and per category the figures of the table above, counts for the
interval and sizes at its end. Reading keeps frames past the recorded
count (a frame whose head update was lost).

## The QAPM files

The eight model files ship in `QSYS`, made from `work/perf/dds/` by
`lic/osobjects` at the first IPL and by `build lp`. Their layout is
IBM's V7R1M0 one, field for field: names, types (hex fields as `H`),
lengths, digits, decimals and buffer positions, as `DSPFFD
OUTPUT(*OUTFILE)` on the primary gave them on 2026-11-03.
`work/perf/tests/ffd.rs` creates each from its DDS and diffs our
`DSPFFD` with the capture. Field texts and column headings are not
copied (IBM text).

Fields this release fills (others are zero or blank):

| File | Fields |
|---|---|
| every interval file | `INTNUM`, `DTETIM` (`YYMMDDHHMMSS`), `INTSEC`, `DTECEN` |
| `QAPMSYSTEM` | `SYSASP`, `SYNUAL` (bytes used), `SYFOPN`, `SYSPTU` (CPU used, ms), `SYSCTA`/`SYSUTA`/`SYSPLA` (CPU available, ms), `SYSPLU`, `SYVCPU`, `SYDPGF`, `SYAPGF` |
| `QAPMJOBMI` | `JBNAME`, `JBUSER`, `JBNBR`, `JBTYPE`, `JBSTYP`, `JBPOOL`, `JBPRTY`, `JBCPU`/`JBTCPU` (seconds), `JBRSP`, `JBNTR`, `JBDBR`, `JBNDB`, `JBDBW`, `JBWRT`, `JBAW`, `JBWI`, `JBAI`, `JBTHAC`, `JBTHCT`, `JBCUSR` |
| `QAPMJOBOS` | the job's names, `JBSSYS`, `JBSLIB`, `JBRSP`, `JBNTR`, `JBLRD`, `JBLWT`, `JBDBU`, `JBFSOPN` |
| `QAPMJSUM` | per group (`BCH`, `INT`, `SYS`): `JSCBKT`, `JSCPU` (ms), `JSTRNT`, `JSTRNS`, `JSJBCT`, `JSPDBR`, `JSPNDB`, `JSPWRT`, `JSLDBR`, `JSLDBW` |
| `QAPMPOOL` | `PONBR`, `POACTL`, `POSIZ`, `PORES` (KB), `PODBF`, `PONDBF`, `PODBPG`, `PONDPG`, `POAW`, `POWI`, `POAI`, `PTTYPE`, `PTPAGE` |
| `QAPMDISK` | `IOPRN`, `DSARM` (unit), `DSTYPE`, `DSDRN`, `DSMDLN`, `DSSRLN`, `DSBLKR`, `DSBLKW`, `DSRDS`, `DSWRTS`, `DSSMPL`, `DSNBSY` (10 ms samples), `DSCAP`, `DSAVL`, `DSASP`, `DSASPN` |
| `QAPMETH` | `IOPRN`, `ETTYPE`, `ETLLND`, `ETLLSP`, `ETLTFT`, `ETLTFR` (bytes), `ETMFTG`, `ETMFRG` (frames), `ETMDCN` (errors) |
| `QAPMCONF` | `GKEY`/`GDES`: `VR` release, `SN` system, `I ` interval seconds, `ST` start date, `SM` start time, `CP` profile, `PN` processors, `MS` main storage (MB) |

## QPFRADJ

`QPFRADJ` 1 and 2 give `*BASE` what the other pools leave of main
storage at the first tick after IPL. 2 and 3 adjust the shared pools
every minute (`work/perf/src/adjust.rs`): each pool has IBM's tuning
defaults (`WRKSHRPOOL` F11: priority, minimum and maximum size, the
fault guideline as a minimum, a part per active thread and a maximum);
a pool faulting above its guideline takes storage from the pool
furthest under its own, `*BASE` first, a tenth of what that pool can
spare above its minimum (at least 256 KB), never past the taker's
maximum. A pool whose jobs went ineligible gets a tenth more activity
levels (at least one); a pool with more than twice its active jobs (at
least ten) gives a tenth back. `*MACHINE` is never changed. Changes go
to the pool table, the pool's cgroup `memory.high` and `QWCSHRPOOL`, as
`CHGSHRPOOL` does.

## The displays

`WRKSYSACT` is display file `QPYSYSAC` (`ui/dspf/sys/`), laid out by
`work/perf/src/sysact.rs`: its four views (CPU and I/O totals,
synchronous I/O, asynchronous I/O, storage and wait), its three key
sets and the first display's copyright row are identical to the
primary's (`work/perf/tests/wrksysact.rs` against
`tools/conformance/golden/phase12/perf/wrksysact*.txt`). Option 5 works
with the job; F10 and Enter look again, F11 steps the views, F24 the
keys, F16 orders by I/O, F19 looks again every five seconds, F20 to F23
show `WRKACTJOB`, `WRKSYSSTS`, `WRKSBS`, `WRKDSKSTS`. `WRKACTJOB`'s
`Int` and `Rsp` columns show interactive jobs' transactions and average
response time from the same counters.

`WRKSYSSTS`, `WRKSHRPOOL` and `WRKDSKSTS` stay UIM panels: the
primary's are display files on hand grids the UIM renderer does not
make (as `WRKACTJOB`'s, `ui/uim/tests/wm_panels.rs`). Their captures
are under `tools/conformance/golden/phase12/perf/`; each panel group
carries a `BLOCKED(phase12)` marker. On the primary `WRKSYSSTS` at the
basic assistance level is "Display System Status"; QS/400 shows the
intermediate panel.

## Deviations

- The collector's interval is the configured one; IBM keeps separate
  intervals per category. All categories share the collection's.
- `*SYSBUS` measures nothing, and the protocol categories of
  `*STANDARDP` and `*ENHCPCPLN` (`*CMNSAP`, `*CMNSTN`, `*IPCS`...) are not
  collected.
- `CRTPFRDTA` takes `INTERVAL`, `STRTIME` and `ENDTIME` and writes every
  interval; `FROMMGTCOL(*SELECT)` (a list display) is refused.
- `CRTPFRSUM(*YES)` is kept but makes no summary files.
- `CFGPFRCOL`'s prompt shows the current values through `QSPMTOVR`; the
  F10 display puts `CRTPFRSUM` under the prompter's "Additional
  Parameters" heading, which IBM's display does not show.

## Frozen at V1R1M0

Frozen: the `*MGTCOL` payload (`work/perf/src/mgtcol.rs`, `VERSION` 1)
and the interval it carries (`work/perf/src/sample.rs`). A change needs
a version bump and a reader of the old version; `tools/fmtcheck` holds
both files (`mgtcol` in `formats.toml`).
