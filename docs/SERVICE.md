# Service: problems, dumps, traces, the service tools

Phase 11 completes the service side of QS/400: the problem log and its
displays, the support contact information, the dumps and traces a
service provider asks for, the rest of SST and DST, and the attended IPL
menus. Every screen's text is Quartz's; titles, option numbers, command
names, keywords and special values are IBM's, as the interface.

pub400 refuses every command here (`tools/conformance/golden/phase11/`
holds the refusals, plus the GO PROBLEM and GO PROBLEM2 menus and the
DSPPRB prompt), and the primary oracle prints only DSPPRB's report
(`tools/conformance/oracle/save/primary/35-problems.txt`). Where no
capture exists the layouts follow IBM's conventions and are marked so
below; our renderings are the goldens.

## Where things live

| Piece | Place |
|---|---|
| Problem entries, analysis, reports, contact information | `service/prb` (`problem.rs`, `log.rs`, `analyze.rs`, `report.rs`, `contact.rs`) |
| The problem log in the store | `lic/slic/src/stgsvc/prblog.rs` (`QSYS/QSPRBLOG`, `QSYS/QSCNTINF`) |
| Service commands in the service | `lic/slic/src/cpp/prbcmd.rs`, `lic/slic/src/cpp/svccmd.rs` |
| Dumps | `lic/slic/src/stgsvc/dumps.rs` |
| Traces | `lic/slic/src/stgsvc/trace.rs` |
| The LIC log (event log) | `lic/slic/src/stgsvc/liclog.rs` |
| `service_op` (request 1400) | `lic/slic/src/stgsvc/svcop.rs` |
| Job panels | `work/qsjob/src/panels/work2/service.rs`, panel groups `QGSXPNLS`, `QESCNTINF` |
| DMPJOB, DMPCLPGM | `work/qsjob/src/command/jobsite.rs`, `natives.rs` (`QCLDMP`) |
| DST and SST screens | `lic/svctools/src/dst/tools.rs`, `devtools/sst` |
| The attended IPL menus | `lic/svctools/src/install/define.rs`, `lic/slic/src/ipl/mod.rs` |

## The problem log

A problem has an identifier of ten digits (`yyddd` of the day it was
logged, then a sequence), a status, a type, a severity 1 (high) to 4, a
one-line text, a symptom string, the system, its times, who it is
assigned to, notes and the fixes the provider named.

| Status | Meaning |
|---|---|
| OPENED | logged, not analyzed |
| READY | analyzed (Work with Problem option 1, or `ANZPRB`) |
| PREPARED | the report is built and kept |
| SENT | the service provider has it |
| ANSWERED, VERIFIED | the provider answered; the fix was verified |
| CLOSED | done (Work with Problem option 5) |

Types are IBM's numbers: 1 machine detected, 2 user detected (`ANZPRB`),
3 PTF order, 4 application detected.

What logs a problem, and its symptoms:

| Origin | Symptoms | Severity |
|---|---|---|
| A message of severity 80 or more to `QSYSOPR` | `MSG/msgid` | 1 at 99, else 2 |
| A device failure (the product activity log) | `RSRC/name SRC/code` | 2 when attention, else 3 |
| A dump (system, object, job, CL program) | `DUMP/name` | 2 |
| `ANZPRB` | by area: `PGM/`, `MSG/`, `RSRC/`, `LIND/`, `PERF/` and the object | 1 when it stops work, else the area's |

An open problem with the same origin and symptoms gets a note
("occurred again") instead of a second entry.

The log is the `prb` crate's text in user space `QSYS/QSPRBLOG`: a header
line `QS400PRB 1 next`, `R` lines with the kept reports, one line a
problem with tab-separated fields.

### Reporting to the service provider

Work with Problem option 2 and `WRKPRB` F16 (prepared problems) call
`prblog::report`: `ProblemLog::report` prepares the report (the problem,
its symptoms and notes as text), keeps it with the log, marks the problem
PREPARED and hands it to a `prb::Sender`. Today the sender is
`prb::Unconnected`, which declines; the report stays kept. The hook is
`prblog::sender()` (`MIGRATE(ptf)`): the update service's client under
`ptf/` replaces it, and a sent problem becomes SENT with the provider's
reference as a note. `SNDSRVRQS ACTION(*TEST)` answers `CPF39D3` until
then.

### Support contact information

`QSYS/QSCNTINF` holds `prb::Contact` as `key=value` lines: company,
contact, telephones and faxes (primary, alternative), the mailing
address (street, city/state, country, postal code), the language version,
the media for fixes, the e-mail address, and the providers (control
point, network ID, connection, text). The shipped provider is `QSERVICE`
with connection `*NONE`.

## Commands

| Command | Runs in | What it does |
|---|---|---|
| `WRKPRB` | job panels; service text | the list; options 2 change, 4 delete, 5 details, 6 print, 8 work with problem, 9 alerts (not available), 12 enter text; F16 reports prepared problems |
| `DSPPRB` | job panels; service text and the report | `PRBID` one problem's details; `OUTPUT(*PRINT)` the Problem Log Report (`QPSXPRT`, the oracle's columns) |
| `CHGPRB`, `DLTPRB` | service | as before |
| `ANZPRB` | job panels | the area, what identifies it, the problem created (`CPF39DA` without a display) |
| `SNDSRVRQS` | service | `CPF39D3`: no provider connection |
| `WRKCNTINF` | job panels; service text | 1 change (`?CHGCNTINF`), 2 display, 3 providers, 4 problems |
| `CHGCNTINF` | service | the fields above, `*SAME` keeps one; `CPC39D4` |
| `WRKSRVPVD` | job panels; service text | the providers; 5 display, 9 test request |
| `DSPSRVSTS` | job panels; service text | the job, serviced by, trace active and sessions |
| `DMPOBJ` | service | formatted dump of `OBJ(lib/name) OBJTYPE` to `QPSRVDMP`; logs a problem; `CPC39D8` |
| `DMPSYSOBJ` | service | by `OBJ` in `CONTEXT` with `TYPE` (`*ALL`, a `*TYPE` or a hex code), or `OBJ` an object ID (`aaaa-tttt-serial-gen`) |
| `DMPJOB` | job | the Phase 4 job dump (`mi::dump`) to `QPSRVDMP`; logs a problem |
| `DMPCLPGM` | job (CL, `QCLDMP`) | the job dump to `QPPGMDMP`; logs a problem |
| `STRTRC` | service | a job trace session `SSNID`, `JOB(* | name | *ALL)`, `MAXSTG` KB, `TRCFULL`, `JOBTRCTYPE(*ALL *FLOW *DATA)` |
| `ENDTRC` | service | ends it; `DTAOPT(*LIB)` keeps the data as user space `DTALIB/SSNID`, `*DLT` drops it; `PRTTRC(*YES)` prints |
| `PRTTRC` | service | prints kept data (`DTAMBR`, `DTALIB`) to `QPSRVTRCJ`; `DLTTRC` |
| `TRCJOB` | service | `SET(*ON)` traces this job (session `TRCnnnnnn`), `*OFF` prints and ends, `*END` ends |
| `TRCINT` | service | internal trace `TRCTBL`: `SET(*ON)` with `TRCTYPE` categories and `JOBNBR`, `*OFF` prints (`QPCSMPRT`) and ends, `*HOLD` stops and keeps, `*END` drops |
| `RCLDBXREF` | service | walks every `*FILE` of `LIB`; the cross-reference is the files' own dependents tables, so `*FIX` has nothing more to do; `CPC39DF` |

`GO PROBLEM` and `GO PROBLEM2` match the pub400 goldens; `GO SERVICE`
(refused by pub400) is ours: 1 problems, 2 analyze, 3 service status,
4 contact information, 5 providers, 6 SST, 7 PTFs, 8 dump commands,
9 trace commands, 70 related commands.

## Dumps

A system dump (the main storage dump manager, option 1; `svc.sysdump
take` at the service console) is the storage service's state as text:
the system, time, requesting job, reason and release; the disk units
with their use; the job table; the IPL history; the LIC log; the traces.
It is kept as user space `QSYS/QSDnnnnnnn` (seven digits, from 1) whose
text is the reason, and logs a problem `DUMP/QSDnnnnnnn`. Work with main
storage dumps lists them with 4=Delete, 5=Display, 6=Copy to media (not
available until media is wired: `MIGRATE(media)`).

A formatted object dump shows the header's fields, the header page and up
to eight payload pages in hex with the EBCDIC text.

## Traces

A trace is a named session in the storage service with filters (jobs,
categories, detail), a record buffer of `MAXSTG`/10 records (16 at
least) that wraps (`*WRAP`, counting records lost) or stops
(`*STOPTRC`) when full. Trace points:

| Category | Where |
|---|---|
| `CALL` | `CALL` in a job (a `trace-point` service action) |
| `CMD` | command and exec requests |
| `REQ` | every other request a job makes |
| `MSG` | message and job log requests (with data) |
| `IO` | file, member, spooled file, IFS, space, queue and area requests (with data) |
| `JOB` | job start and end |
| `PRB` | problems logged |
| `DMP` | dumps taken |
| `LOG` | LIC log entries |

A job trace takes `CALL`, `CMD`, `REQ` for `*FLOW`, `MSG` and `IO` for
`*DATA`, all of them for `*ALL`. Requests carrying passwords or tokens
(sign-on, host server, SST) never record data. Kept trace data is text:
`QS400TRC 1` with the name, kind, detail, start and lost count, then one
line a record (milliseconds, job, category, text). The report is a
heading, then time, qualified job, category and event per record.

SST's "Trace Licensed Internal Code" is the internal kind: Work with
Traces (2=Stop, 4=Delete, 5=Display; F6=Start trace with the table name,
categories, job number, size and wrap).

## The Licensed Internal Code log

The LIC log is two things: the IPL history the LIC record keeps (Phase 1)
and the storage service's event log, the newest 2000 entries in memory
for the life of the service. Each event has IBM's form of a major and a
minor code, four hex digits each; the codes are ours:

| Major | Area | Minor codes |
|---|---|---|
| 0100 | IPL and power | 0001 storage service started, 0002 closing |
| 0200 | Storage service | 0001 request refused |
| 0300 | Jobs | 0001 started, 0002 ended, 0003 ended abnormally |
| 0400 | Problem log | 0001 problem logged, 0002 problem reported |
| 0500 | Dumps | 0001 system dump taken, 0002 deleted, 0003 object or job dump printed |
| 0600 | Traces | 0001 started, 0002 ended |
| 0700 | Service tools | 0001 user signed on, 0002 storage altered, 0003 user signed off |
| 0800 | Disk units | 0001 configuration changed, 0002 unit not reporting |

`liclog.rs` checks that every minor belongs to a listed major and that
no pair repeats. The display (SST/DST option 5, 1) selects by major code;
option 2 shows the IPL history; `svc.liclog [MAJOR]` prints the entries.

## SST and DST

`Start a Service Tool` (DST option 7, SST option 1):

1. Product activity log
2. Trace Licensed Internal Code: Work with Traces, F6 start
3. Work with communications trace (not here; `STRCMNTRC`)
4. Display/Alter/Dump: 1 display/alter storage (a unit page or an object
   header), 2 dump to printer (`QPCSMPRT`), 3 alter object storage (an
   object's header or extent page, offset and hex bytes, confirmed with
   F10; the service also requires `*SERVICE` and writes a LIC log entry)
5. Licensed Internal Code log: 1 select entries, 2 IPL history
6. Main storage dump manager: 1 take a dump (F10), 2 work with dumps,
   3 copy to media (not available)
7. Hardware service manager: 1 packaging, 2 logical (1 all, 2 `*CMN`,
   3 `*STG`, 4 `*TAP`), 3 locate by resource name (a trailing `*` is
   generic), 4 failed and non-reporting, 6 to 8 not available, F6 print
   configuration

Resource classes: `Lan`, `Comm`, `Port` are `*CMN`; `Storage`, `Disk`
`*STG`; `Optical`, `Tape` `*TAP`.

SST's `Work with System Capacity`: 1 processor capacity status (the
processors, processor use, main storage, pools and active jobs from the
Phase 5 statistics), 4 storage capacity (the ASP and each unit). `Work
with Service Tools User IDs and Devices` 2 lists the device IDs: the
console only; there are no LAN service tools devices.

Dumps and traces need `DUMPS_AND_TRACES`, display/alter/dump
`DISPLAY_ALTER_DUMP`, the LIC log `LOGS`, the hardware `HARDWARE`. In
DST, before the storage service runs, dumps work on the store directly
and traces are not available.

## The attended IPL

An IPL is attended in manual mode, or in normal mode with `QIPLTYPE` 1
or 2 (2 would leave the console in debug; the serial service console of
development builds is that already).

- After an abnormal end, an attended normal-mode IPL shows "Previous
  System End Was Abnormal": 1 continue, 2 use DST first. A manual IPL
  has its IPL menu for that.
- When the store is marked as needing a reclaim, the IPL runs RCLSTG
  before opening the objects, with a running screen and the report.
- A manual IPL shows `IPL Options`, then `Set Major System Options` when
  asked, then `Define or Change the System at IPL` when its choice is Y:
  1 configuration objects, 2 date and time system values, 3 system values
  (2=Change, checked as `CHGSYSVAL` checks), 4 network attributes,
  5 Edit Rebuild of Access Paths (empty: recovery rebuilds what it must),
  6 Edit Check Pending Constraints (empty: no constraints are pending),
  7 user profiles, 8 the startup program (`QSTRUPPGM`). F3 goes on.
- An escape from the startup program does not stop the IPL: `QWCSTRUP`
  sends `CPF39DE` to `QSYSOPR` with the program and the escape, and the
  system goes on to sign-on.

The option numbers of the define menu and the HSM extensions are from
IBM's documentation as remembered, not captured.

## The protocol

Request 1400 `service_op(op, args) -> lines`: `prb-analyze` (area,
message ID, object, Y/N, text), `prb-ready`, `prb-report`, `prb-note`,
`prb-close`, `prb-dump` (name, reason), `trace-point` (category, text),
and SST's `sst-capacity`, `sst-dump-take|list|show|delete|copy`,
`sst-trace-start|stop|list|show|delete`, `sst-liclog`, `sst-dad-print`,
`sst-dad-alter`, `sst-hsm-print`, `sst-devices`. Display data comes
through `panel2_data`: `WRKPRB`, `DSPPRB`, `CNTINF`, `SRVPVD`, `SRVSTS`.

## Messages

Ours, in QCPFMSG's unused 39Dx block: `CPF39D0` trace already active,
`CPF39D1` trace not found, `CPF39D2` system dump not found, `CPF39D3` no
provider connection, `CPC39D4` contact information changed, `CPC39D5`
trace started, `CPC39D6` trace ended, `CPC39D7` trace data printed,
`CPC39D8` dump printed, `CPC39D9` job dump printed, `CPF39DA` needs a
display, `CPF39DB` trace data not found, `CPC39DC` problem created,
`CPC39DD` job trace set, `CPF39DE` startup program ended with an escape,
`CPC39DF` cross-reference checked. IBM's own identifiers for these were
not captured; when they are, the IDs change with the oracle.
