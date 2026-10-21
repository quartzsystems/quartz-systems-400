# Work management

Phase 5 replaces the single service job of Phase 4 with jobs: one Linux
process per job, started, routed, held, ended and accounted for the way
the machine does it. This document grows with the phase; the storage
side of the process model is in `STORAGE_SERVICE.md`.

## 1. Jobs

A job is named `number/user/name`: a six-digit number, the user profile
and the job name. Numbers come from the storage service, which keeps the
last one handed out in the data area `QSYS/QJOBNBR`, wraps after 999999
and skips the numbers of active jobs. `SCPF` is job 000000.

The 16-byte internal job identifier is `QJ`, the job number big-endian,
the start time (Unix seconds, big-endian), a version byte (1) and a
check byte (the byte sum of the first fifteen). Callers treat it as
opaque.

Types and subtypes are `QUSRJOBI`'s one-letter codes; `WRKACTJOB` shows
them as `ASJ BCH BCI EVK INT M36 MRT PJ PDJ RDR SBS SYS WTR`. An active
job's status is one of `RUN DEQW MSGW DSPW LCKW TIMW EVTW SELW CMNW DLYW
HLD END INEL`; the job sets it at every point it blocks and the service
keeps it in the active job table.

### The active job table

Every job process connects to the storage service and says hello with
its name, user, number, type, subsystem and process ID. The service
adds it to the table, gives it a `QTEMP` (the library `QTnnnnnn` in the
root of ASP 0, seen by the job as `QTEMP`) and removes both when the
connection closes, however the job ended.

## 2. Payload layouts

Every work management object is an object of its type whose space holds
a four-byte little-endian length and the payload. Payloads start with a
fixed part laid out below; text is CP037, blank padded; integers are
little-endian. `wm/tests/spec.rs` compares these tables with the code.

### `JobDescription` (352 bytes)

`*JOBD`. After the fixed part: the initial library list (a two-byte
count, then ten-byte names) and the request data (`RqsDtaLen` bytes).

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | Magic | `QSWMJOBD` in ASCII |
| 8 | 2 | Version | 1 |
| 10 | 1 | LiblSysval | 1: `INLLIBL(*SYSVAL)`, the list is empty |
| 11 | 1 | Hold | `HOLD(*YES)` |
| 12 | 10 | User | a profile or `*RQD` |
| 22 | 10 | JobqName | |
| 32 | 10 | JobqLib | |
| 42 | 1 | JobPty | 0-9 |
| 43 | 1 | OutPty | 0-9 |
| 44 | 1 | Syntax | 0-99; 255 is `*NOCHK` |
| 45 | 1 | EndSev | 0-99 |
| 46 | 1 | LogLevel | 0-4 |
| 47 | 1 | LogSev | 0-99 |
| 48 | 10 | LogText | `*MSG`, `*SECLVL`, `*NOLIST` |
| 58 | 10 | LogClPgm | `*YES`, `*NO` |
| 68 | 10 | LogOutput | `*SYSVAL`, `*JOBLOGSVR`, `*JOBEND`, `*PND` |
| 78 | 10 | InqMsgRpy | `*RQD`, `*DFT`, `*SYSRPYL` |
| 88 | 10 | PrtDev | a device, `*USRPRF`, `*SYSVAL`, `*WRKSTN` |
| 98 | 10 | OutqName | a name, or `*USRPRF`, `*DEV`, `*WRKSTN` with no library |
| 108 | 10 | OutqLib | |
| 118 | 10 | InlAspGrp | |
| 128 | 13 | DevRcyAcn | |
| 141 | 1 | AlwMltThd | |
| 142 | 10 | TsePool | |
| 152 | 10 | JobMsgQFl | `*SYSVAL`, `*NOWRAP`, `*WRAP`, `*PRTWRAP` |
| 162 | 2 | JobMsgQMx | megabytes; 0 is `*SYSVAL` |
| 164 | 10 | SplfAcn | |
| 174 | 10 | DdmCnv | |
| 184 | 4 | Reserved | zero |
| 188 | 30 | PrtTxt | |
| 218 | 80 | RtgDta | |
| 298 | 50 | Text | |
| 348 | 4 | RqsDtaLen | |

### `Class` (84 bytes)

`*CLS`. Four-byte limits use `0xFFFFFFFF` for `*NOMAX`.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | Magic | `QSWMCLS ` in ASCII |
| 8 | 2 | Version | 1 |
| 10 | 1 | RunPty | 1-99 |
| 11 | 1 | Purge | |
| 12 | 4 | TimeSlice | milliseconds |
| 16 | 4 | DftWait | seconds |
| 20 | 4 | CpuTime | milliseconds |
| 24 | 4 | MaxTmpStg | megabytes |
| 28 | 4 | MaxThd | |
| 32 | 50 | Text | |
| 82 | 2 | Reserved | zero |

### `JobQueue` (80 bytes)

`*JOBQ`. The fixed part, then `Count` jobs in starting order: each a
`JobQueueJob` and its request data (`RqsLen` bytes, CP037), its
attributes as the job starts with them (`AttrsLen` bytes, the storage
service's wire encoding of `JobAttrs`) and its `*LDA` (`LdaLen` bytes).
How many jobs a queue may start at once is the subsystem's job queue
entry's business.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | Magic | `QSWMJOBQ` in ASCII |
| 8 | 2 | Version | 1 |
| 10 | 1 | Held | `HLDJOBQ` |
| 11 | 1 | OprCtl | `OPRCTL(*YES)` |
| 12 | 4 | Count | jobs that follow |
| 16 | 10 | AutChk | `*OWNER`, `*DTAAUT` |
| 26 | 4 | Reserved | zero |
| 30 | 50 | Text | |

### `JobQueueJob` (176 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 4 | Number | job number, given at submission |
| 4 | 1 | JobPty | 0-9 |
| 5 | 1 | Held | |
| 6 | 2 | Reserved | zero |
| 8 | 8 | Submitted | microseconds since the epoch |
| 16 | 8 | Scheduled | Unix seconds; 0 for none |
| 24 | 10 | Name | |
| 34 | 10 | User | |
| 44 | 10 | JobdName | |
| 54 | 10 | JobdLib | |
| 64 | 10 | MsgqName | completion message queue; blank for none |
| 74 | 10 | MsgqLib | |
| 84 | 10 | SbmName | the submitting job |
| 94 | 10 | SbmUser | |
| 104 | 4 | SbmNumber | |
| 108 | 48 | RtgDta | |
| 156 | 4 | RqsLen | |
| 160 | 4 | AttrsLen | |
| 164 | 4 | LdaLen | |
| 168 | 8 | Reserved2 | zero |

### `MessageQueue` (128 bytes)

`*MSGQ`. The fixed part is followed by `Count` messages, each a
`MessageEntry` and its data, text, second-level text, reply and default
reply. Which job a queue is allocated to for break delivery is not
kept: it ends with the job, so the storage service holds it in memory.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | Magic | `QSWMMSGQ` in ASCII |
| 8 | 2 | Version | 1 |
| 10 | 1 | Severity | `SEV` |
| 11 | 1 | Reserved1 | zero |
| 12 | 10 | Delivery | `*HOLD`, `*BREAK`, `*NOTIFY`, `*DFT` |
| 22 | 10 | Full | `*SNDMSG`, `*WRAP` |
| 32 | 10 | BrkPgmName | `*DSPMSG` for the system's handler |
| 42 | 10 | BrkPgmLib | |
| 52 | 4 | SizeInitial | kilobytes |
| 56 | 4 | SizeIncrement | kilobytes |
| 60 | 4 | SizeMax | kilobytes; 0 is `*NOMAX` |
| 64 | 4 | NextKey | |
| 68 | 4 | Count | messages that follow |
| 72 | 50 | Text | |
| 122 | 6 | Reserved2 | zero |

### `MessageEntry` (112 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 4 | Key | unique on the queue |
| 4 | 4 | ReplyKey | an inquiry's reply, a reply's inquiry |
| 8 | 1 | Type | 1 info, 2 inquiry, 3 reply, 4 completion, 5 diagnostic, 6 notify, 7 escape |
| 9 | 1 | Severity | |
| 10 | 1 | Status | 0 new, 1 old, 2 unanswered, 3 answered |
| 11 | 1 | Reserved | zero |
| 12 | 8 | Time | microseconds since the epoch |
| 20 | 7 | MsgId | blank for an immediate message |
| 27 | 1 | Pad | zero |
| 28 | 10 | MsgfName | |
| 38 | 10 | MsgfLib | |
| 48 | 10 | JobName | the sender |
| 58 | 10 | JobUser | |
| 68 | 4 | JobNumber | |
| 72 | 10 | FromPgm | |
| 82 | 10 | ReplyTo | the queue the reply goes to |
| 92 | 10 | ReplyToLib | |
| 102 | 2 | DataLen | |
| 104 | 2 | TextLen | |
| 106 | 2 | HelpLen | |
| 108 | 2 | ReplyLen | |
| 110 | 2 | DftLen | |

### `ReplyListEntry` (64 bytes)

The system reply list, kept in the user space `QSYS/QRPYLIST`: a
two-byte count, then each entry and its comparison data (`CmpLen`
bytes).

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 2 | SeqNbr | 1-9999 |
| 2 | 1 | Dump | `DUMP(*YES)` |
| 3 | 1 | Reserved | zero |
| 4 | 7 | MsgId | an identifier, generic (`CPA0000`), or `*ANY` |
| 11 | 1 | Pad | zero |
| 12 | 2 | Start | comparison start position, 1-based |
| 14 | 2 | CmpLen | comparison data length; 0 is `*NONE` |
| 16 | 32 | Reply | a reply, `*DFT`, `*RQD` |
| 48 | 16 | Reserved2 | zero |

### `SubsystemDescription` (128 bytes)

`*SBSD`. After the fixed part come the entries, each kind in turn, as
many as its count says: pools, routing, workstation names, workstation
types, job queues, autostart, prestart, communications. Two- and
four-byte limits of 0 mean `*NOMAX`.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | Magic | `QSWMSBSD` in ASCII |
| 8 | 2 | Version | 1 |
| 10 | 1 | Pools | pool entries |
| 11 | 1 | Reserved1 | zero |
| 12 | 4 | MaxJobs | `MAXJOBS` |
| 16 | 10 | SgnDspfName | `*QDSIGNON` or a display file |
| 26 | 10 | SgnDspfLib | |
| 36 | 10 | SysLibLe | `SYSLIBLE`, `*NONE` |
| 46 | 2 | Routing | routing entries |
| 48 | 2 | WsNames | workstation name entries |
| 50 | 2 | WsTypes | workstation type entries |
| 52 | 2 | JobQueues | job queue entries |
| 54 | 2 | Autostart | autostart job entries |
| 56 | 2 | Prestart | prestart job entries |
| 58 | 2 | Comm | communications entries |
| 60 | 18 | Reserved2 | zero |
| 78 | 50 | Text | |

### `SubsystemPool` (16 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | Id | 1-10 |
| 1 | 1 | Kind | 0 private, 1 `*BASE`, 2 `*INTERACT`, 3 `*SPOOL`, 4 `*SHRPOOLn`, 5 `*MACHINE` |
| 2 | 1 | Shared | n of `*SHRPOOLn` |
| 3 | 1 | Reserved | zero |
| 4 | 8 | SizeKb | a private pool's size |
| 12 | 4 | ActLvl | a private pool's activity level |

### `RoutingEntry` (84 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 2 | SeqNbr | |
| 2 | 2 | CmpStart | 1-based |
| 4 | 2 | MaxAct | |
| 6 | 1 | PoolId | |
| 7 | 1 | Reserved | zero |
| 8 | 10 | PgmName | |
| 18 | 10 | PgmLib | |
| 28 | 10 | ClsName | |
| 38 | 10 | ClsLib | |
| 48 | 32 | CmpVal | `*ANY` matches all |
| 80 | 4 | Reserved2 | zero |

### `WorkstationEntry` (36 bytes)

A workstation name entry (a device, generic with `*`) or a type entry
(`*ALL`, `*CONS`, a device type).

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 10 | Name | |
| 10 | 10 | JobdName | or `*USRPRF` |
| 20 | 10 | JobdLib | |
| 30 | 2 | MaxAct | |
| 32 | 1 | At | 1 `*SIGNON`, 0 `*ENTER` |
| 33 | 3 | Reserved | zero |

### `JobQueueEntry` (36 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 10 | Name | |
| 10 | 10 | Lib | |
| 20 | 2 | SeqNbr | |
| 22 | 2 | MaxAct | |
| 24 | 9 | MaxPty | `MAXPTY1`-`MAXPTY9`, one byte each |
| 33 | 3 | Reserved | zero |

### `AutostartEntry` (32 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 10 | Job | |
| 10 | 10 | JobdName | |
| 20 | 10 | JobdLib | |
| 30 | 2 | Reserved | zero |

### `PrestartEntry` (96 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 10 | PgmName | |
| 10 | 10 | PgmLib | |
| 20 | 10 | User | |
| 30 | 10 | JobName | |
| 40 | 10 | JobdName | |
| 50 | 10 | JobdLib | |
| 60 | 10 | ClsName | |
| 70 | 10 | ClsLib | |
| 80 | 1 | StrJobs | |
| 81 | 1 | Wait | |
| 82 | 1 | PoolId | |
| 83 | 1 | Reserved | zero |
| 84 | 2 | InlJobs | |
| 86 | 2 | Threshold | |
| 88 | 2 | AdlJobs | |
| 90 | 2 | MaxJobs | |
| 92 | 2 | MaxUse | |
| 94 | 2 | Reserved2 | zero |

### `CommEntry` (32 bytes)

Kept; inactive until communications exist.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 10 | Dev | |
| 10 | 10 | Mode | |
| 20 | 10 | JobdName | |
| 30 | 2 | MaxAct | |

### `JobSchedule` (32 bytes)

The payload of `QUSRSYS/QDFTJOBSCD *JOBSCD`. `Count` entries follow,
each a `JobScheduleEntry`, then its omit dates (`Omits` little-endian
`u32`s, `YYYYMMDD`), then its command (`CmdLen` bytes, CP037). Dates and
times are UTC.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | Magic | `QSWMJSCD` in ASCII |
| 8 | 2 | Version | 1 |
| 10 | 2 | Reserved | zero |
| 12 | 4 | NextEntry | the next entry number |
| 16 | 4 | Count | entries that follow |
| 20 | 12 | Reserved2 | zero |

### `JobScheduleEntry` (256 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 4 | Number | `ENTRYNBR` |
| 4 | 1 | Frequency | 0 `*ONCE`, 1 `*WEEKLY`, 2 `*MONTHLY` |
| 5 | 1 | RcyAcn | 0 `*SBMRLS`, 1 `*SBMHLD`, 2 `*NOSBM` |
| 6 | 1 | Held | `HLDJOBSCDE` |
| 7 | 1 | Save | `SAVE(*YES)` |
| 8 | 1 | Days | `SCDDAY`: bit 0 Monday to bit 6 Sunday |
| 9 | 1 | RelDays | `RELDAYMON`: bits 0-4 the first to fifth, bit 5 `*LAST` |
| 10 | 1 | DateKind | 0 a date, 1 `*CURRENT`, 2 `*MONTHSTR`, 3 `*MONTHEND`, 4 `*NONE` |
| 11 | 1 | Reserved | zero |
| 12 | 4 | Date | `YYYYMMDD` |
| 16 | 4 | Time | seconds after midnight |
| 20 | 8 | Next | Unix seconds it is next due; 0 for never |
| 28 | 8 | LastSubmit | Unix seconds |
| 36 | 10 | Job | |
| 46 | 10 | JobdName | |
| 56 | 10 | JobdLib | |
| 66 | 10 | JobqName | blank for `*JOBD` |
| 76 | 10 | JobqLib | |
| 86 | 10 | User | |
| 96 | 10 | MsgqName | blank for `*USRPRF` |
| 106 | 10 | MsgqLib | |
| 116 | 28 | LastJob | `number/user/name` |
| 144 | 50 | Text | |
| 194 | 2 | Omits | omit dates that follow |
| 196 | 4 | CmdLen | |
| 200 | 56 | Reserved2 | zero |

The system job `QSYSSCD`, an autostart job of `QSYSWRK` routed to
`QSYS/QEJSCHED`, asks the storage service every few seconds to submit
what is due. An entry due before the current IPL started was missed;
`*SBMRLS` submits it, `*SBMHLD` submits it held, `*NOSBM` moves on to
its next date. Each submission is logged in QHST as `CPI1E63`.

### Shipped objects

| Object | Notes |
|---|---|
| `QGPL/QDFTJOBD *JOBD` | the defaults |
| `QGPL/QBATCH *JOBD` | `USER(QPGMR) RTGDTA(QCMDB)` |
| `QGPL/QINTER *JOBD` | `JOBQ(QGPL/QINTER)` |
| `QSYS/QCTL *JOBD` | `JOBQ(QSYS/QCTL)` |
| `QSYS/QSPLJOBD *JOBD` | `USER(QSPLJOB) JOBQ(QSYS/QSPL) RTGDTA(QCMDB)` |
| `QSYS/QSYSJOBD *JOBD` | `USER(QSYS) JOBQ(QSYS/QSYSNOMAX) RTGDTA(QCMDB)` |
| `QSYS/QSYSSCD *JOBD` | `USER(QSYS) RTGDTA(QSYSSCD)`, the job scheduler |
| `QUSRSYS/QDFTJOBSCD *JOBSCD` | the job schedule, empty |
| `QGPL/QBATCH *CLS` | `RUNPTY(50) TIMESLICE(5000) DFTWAIT(120)` |
| `QGPL/QINTER *CLS` | `RUNPTY(20) TIMESLICE(2000) DFTWAIT(30)` |
| `QSYS/QCTL *CLS` | `RUNPTY(10) TIMESLICE(2000) DFTWAIT(30)` |
| `QSYS/QSPL *CLS` | `RUNPTY(15) TIMESLICE(3000) DFTWAIT(120)` |
| `QSYS/QSYSCLS20`, `25`, `50 *CLS` | system jobs at those priorities |
