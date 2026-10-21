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

### Shipped objects

| Object | Notes |
|---|---|
| `QGPL/QDFTJOBD *JOBD` | the defaults |
| `QGPL/QBATCH *JOBD` | `USER(QPGMR) RTGDTA(QCMDB)` |
| `QGPL/QINTER *JOBD` | `JOBQ(QGPL/QINTER)` |
| `QSYS/QCTL *JOBD` | `JOBQ(QSYS/QCTL)` |
| `QSYS/QSPLJOBD *JOBD` | `USER(QSPLJOB) JOBQ(QSYS/QSPL) RTGDTA(QCMDB)` |
| `QSYS/QSYSJOBD *JOBD` | `USER(QSYS) JOBQ(QSYS/QSYSNOMAX) RTGDTA(QCMDB)` |
| `QGPL/QBATCH *CLS` | `RUNPTY(50) TIMESLICE(5000) DFTWAIT(120)` |
| `QGPL/QINTER *CLS` | `RUNPTY(20) TIMESLICE(2000) DFTWAIT(30)` |
| `QSYS/QCTL *CLS` | `RUNPTY(10) TIMESLICE(2000) DFTWAIT(30)` |
| `QSYS/QSPL *CLS` | `RUNPTY(15) TIMESLICE(3000) DFTWAIT(120)` |
| `QSYS/QSYSCLS20`, `25`, `50 *CLS` | system jobs at those priorities |
