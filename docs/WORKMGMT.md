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

### `OutputQueue` (256 bytes)

The payload of an `*OUTQ`. `Count` spool identifiers follow, in queue
order; `SEQ(*JOBNBR)` keeps them sorted by job number.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | Magic | `QSWMOUTQ` in ASCII |
| 8 | 2 | Version | 1 |
| 10 | 1 | Held | `HLDOUTQ` |
| 11 | 1 | Seq | 0 `*FIFO`, 1 `*JOBNBR` |
| 12 | 4 | Count | spool identifiers that follow, `u32` each |
| 16 | 4 | MaxPages | 0 `*NONE` |
| 20 | 2 | AutoStrWtr | 0 `*NONE` |
| 22 | 1 | DspDta | 0 `*NO`, 1 `*YES`, 2 `*OWNER` |
| 23 | 1 | OprCtl |  |
| 24 | 10 | AutChk |  |
| 34 | 10 | Writer | the writer started to it |
| 44 | 1 | CnnType | 0 `*IP`; `MIGRATE(phase10)` |
| 45 | 3 | Reserved | zero |
| 48 | 50 | Text |  |
| 98 | 30 | Reserved2 | zero |
| 128 | 64 | RmtSys | `MIGRATE(phase10)` |
| 192 | 64 | RmtPrtQ | `MIGRATE(phase10)` |

### `SpooledFile` (512 bytes)

The first 512 bytes of a `*SPLF` object's space in `QSPL`; the SCS
data follows at offset 512 and grows in place, the space doubling as
needed.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | Magic | `QSWMSPLF` in ASCII |
| 8 | 2 | Version | 1 |
| 10 | 1 | Status | 0 `RDY`, 1 `HLD`, 2 `SAV`, 3 `WTR`, 4 `OPN`, 5 `CLO`, 6 `PND`, 7 `DFR`, 8 `MSGW` |
| 11 | 1 | Schedule | 0 `*IMMED`, 1 `*FILEEND`, 2 `*JOBEND` |
| 12 | 4 | Id | spool identifier; the object is `QSPL/SPnnnnnnnn` |
| 16 | 4 | Number | file number in the job |
| 20 | 4 | JobNumber |  |
| 24 | 4 | Pages |  |
| 28 | 4 | Records |  |
| 32 | 8 | DataLen | bytes of SCS data from offset 512 |
| 40 | 8 | Created | microseconds since the epoch |
| 48 | 4 | Expires | `YYYYMMDD`; 0 `*NONE` |
| 52 | 4 | LastPage |  |
| 56 | 2 | Copies |  |
| 58 | 1 | OutPty |  |
| 59 | 1 | Save |  |
| 60 | 2 | PageLen |  |
| 62 | 2 | PageWidth |  |
| 64 | 1 | Lpi |  |
| 65 | 1 | Cpi |  |
| 66 | 2 | Ovrflw |  |
| 68 | 1 | Drawer |  |
| 69 | 1 | Duplex | 0 `*NO`, 1 `*YES`, 2 `*TUMBLE` |
| 70 | 1 | Align |  |
| 71 | 1 | PrtQlty | 0 `*STD`, 1 `*DRAFT`, 2 `*NLQ`, 3 `*DEVD` |
| 72 | 2 | CopiesLeft |  |
| 74 | 1 | HeldFrom | the status `RLSSPLF` restores |
| 75 | 1 | Reserved | zero |
| 76 | 10 | Name |  |
| 86 | 10 | JobName |  |
| 96 | 10 | JobUser |  |
| 106 | 10 | UsrDta |  |
| 116 | 10 | FormType |  |
| 126 | 10 | OutqName |  |
| 136 | 10 | OutqLib |  |
| 146 | 10 | DevType | `*SCS` |
| 156 | 10 | Font |  |
| 166 | 10 | PrtfName |  |
| 176 | 10 | PrtfLib |  |
| 186 | 10 | Writer |  |
| 196 | 255 | UsrDfnDta |  |
| 451 | 61 | Reserved2 | zero |

### `PrinterFile` (256 bytes)

The payload of a `*FILE` whose attribute is `PRTF`. The compiled DDS
formats follow (`FmtLen` bytes, the `spool` crate's encoding); none for
a program-described file. The attribute area keeps no database magic,
so database scans pass printer files by.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | Magic | `QSWMPRTF` in ASCII |
| 8 | 2 | Version | 1 |
| 10 | 2 | PageLen |  |
| 12 | 2 | PageWidth |  |
| 14 | 1 | Lpi |  |
| 15 | 1 | Cpi |  |
| 16 | 2 | Ovrflw |  |
| 18 | 1 | Fold |  |
| 19 | 1 | RplUnprt |  |
| 20 | 1 | RplChar |  |
| 21 | 1 | Align |  |
| 22 | 1 | PrtQlty | 0 `*STD`, 1 `*DRAFT`, 2 `*NLQ`, 3 `*DEVD` |
| 23 | 1 | Hold |  |
| 24 | 1 | Save |  |
| 25 | 1 | OutPty | 0 `*JOB` |
| 26 | 2 | Copies |  |
| 28 | 1 | Schedule | 0 `*IMMED`, 1 `*FILEEND`, 2 `*JOBEND` |
| 29 | 1 | CtlChar | 0 `*NONE`, 1 `*FCFC` |
| 30 | 2 | FileSep |  |
| 32 | 4 | MaxRcds | 0 `*NOMAX` |
| 36 | 1 | Drawer |  |
| 37 | 1 | Duplex |  |
| 38 | 2 | PagRtt | degrees; 0xFFFF `*DEVD` |
| 40 | 1 | Share |  |
| 41 | 1 | LvlChk |  |
| 42 | 2 | Reserved | zero |
| 44 | 10 | FormType |  |
| 54 | 10 | OutqName | `*JOB` with a blank library |
| 64 | 10 | OutqLib |  |
| 74 | 10 | Dev |  |
| 84 | 10 | UsrDta |  |
| 94 | 10 | SplfName |  |
| 104 | 10 | Font |  |
| 114 | 10 | DevType |  |
| 124 | 10 | ChrId |  |
| 134 | 50 | Text |  |
| 184 | 30 | PrtTxt |  |
| 214 | 38 | Reserved2 | zero |
| 252 | 4 | FmtLen | bytes of compiled DDS formats that follow |

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

## 3. Spooling

A spooled file is a `*SPLF` object in `QSPL` named `SPnnnnnnnn` after
its spool identifier: the `SpooledFile` attributes, then SCS data. Its
output queue lists it by identifier. The storage service indexes every
spooled file at start; files a lost job left `OPN` (or a writer left
`WTR`) become `RDY`.

A program writes through a printer file (`*FILE PRTF`). The job runs
the line printer itself (the `spool` crate) and sends the SCS to the
service in chunks, so a report costs a few requests, not one a line.
The file opens `OPN`; at close it is `RDY`, `HLD` for `HOLD(*YES)`, or
`CLO` until the job ends for `SCHEDULE(*JOBEND)`.

The output queue of a new file: the printer file's `OUTQ`, else the
job's, else `QGPL/QPRINT` (`MIGRATE(phase10)`: the printer device's).

The job log: the job keeps the service's copy of its log current after
every request, so a job that vanishes still has one. At the end the
service prints the copy as `LOG` keeps it into `QPJOBLOG`
(`OUTQ(QUSRSYS/QEZJOBLOG)`): `*NOLIST` prints only after an abnormal
end, `LOGOUTPUT(*PND)` (or `QLOGOUTPUT`) prints nothing.

### The SCS subset

| Bytes | Name | What it does |
|---|---|---|
| `0C` | FF | line 1 of the next page |
| `0D` | CR | column 1, same line |
| `15` | NL | column 1 of the next line |
| `16` | BS | back one column |
| `1E` | IRS | as NL |
| `25` | LF | the next line, same column |
| `2F` | BEL | alert; nothing printed |
| `34 C0 n` | PP AHPP | to column `n` |
| `34 C8 n` | PP RHPP | right `n` columns |
| `34 4C n` | PP AVPP | to line `n`; the next page when `n` is above the line |
| `34 C4 n` | PP RVPP | down `n` lines |
| `35 n data` | TRN | `n` bytes printed as they are |
| `2B C1 02 w` | SHF | maximum presentation position `w` |
| `2B C2 02 l` | SVF | maximum presentation line `l` |
| `2B C6 02 d` | SLD | line density, `d`/72 inch |
| `2B D1 06 01 g g c c` | SCG | graphic character set `g`, code page `c` (697, 37) |
| `2B D2 04 29 00 n` | SCD | `n` characters per inch |

Other `2B` commands are skipped by their length byte, which counts
itself and the parameters. Bytes from `40` up are CP037 graphics; the
printer sends control bytes in a record's text as blanks. When several
characters land in one position the first non-blank one stays.

`CTLCHAR(*FCFC)`: blank, `0`, `-` space one, two, three lines before;
`+` overprints; `1` skips to line 1; `2`-`9`, `A`, `B` skip to lines 2
to 11; `C` skips to the overflow line. `CPYSPLF CTLCHAR(*PRTCTL)` puts
the line skipped to (three digits, blank for none) and the lines spaced
(one digit) before each line.
