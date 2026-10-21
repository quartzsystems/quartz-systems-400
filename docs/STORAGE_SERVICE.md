# The storage service

Phase 5 runs every job in its own Linux process. Something has to keep
the object store whole while jobs come, go and crash: the storage
service. This document describes the process, its protocol, and the
rules that keep a dying job from corrupting anything.

## Processes

```
slic (PID 1, SCPF)             IPL, LIC partition, console display, serial line, power
 ├─ /init qsstg                the storage service: ASP 1, ASP 0, database, locks,
 │                             the job table, queues, subsystems, pools, spooling
 ├─ /qsjob  QSYSARB ... QSYSCOMM1   the system jobs (role SYSTEM)
 ├─ /qsjob  QSYSWRK, QBASE     the IPL's subsystem monitors (role MONITOR)
 │   └─ /qsjob  QSYSSCD, QSTRUPJD ...  their autostart, batch and writer jobs
 ├─ /qsjob  QSVCJOB            the serial service console (role QSVCJOB)
 └─ /qsjob  QSCONSOLE          the job signed on at the console display (role INTER)
```

A subsystem a job starts (`STRSBS`, the startup program's) is that
job's child; when the job ends first, the monitor is `slic`'s to reap.
`WORKMGMT.md` describes the roles, routing and the job's life.

`slic` runs the IPL exactly as before: it opens ASP 1, replays the WAL,
synchronizes a resuming mirrored half, creates `QSYS` on a fresh store,
runs database recovery, loads the system values and finishes an
install. At the end of the OS phase it checkpoints and closes the store,
then starts itself again as the service (`/init qsstg`, the same
binary in its storage role) and hands it the unit device nodes and the
system values. The service reopens the units (the checkpoint leaves
nothing to replay), creates the temporary ASP, opens its own handle on
the LIC partition for the mirror resume journal only, binds
`/run/qs400/stg` and answers `slic` with what it opened. `slic` then
shows `C6001015` and carries on.

The service is the only process that touches pages. Job processes link
the runtime (`mi`, `codegen`, `sysapi`) but no storage code that runs:
their programs reach objects, files, data areas, data queues, user
spaces, message descriptions and the APIs' provider through the source
traits the runtime already had (`ObjectSource`, `IoSource`,
`DataSource`, `UserSpaceSource`, `MsgLookup`, `Provider`), implemented
in `stg::remote` by sending requests.

### Why not a shared page cache

The plan had job processes reading pages from a shared-memory cache
and sending writes through the socket. That puts the B+tree, record
and object code in every job, and makes every in-process cache in the
database (shared paths, open data paths, the commit definition)
something to keep coherent across processes. It also lets a job crash
in the middle of a multi-page update that only it understood. Moving
the work instead of the pages gives one address space for every
structure with an invariant, and leaves a job nothing it can leave half
done. The cost is a request per record operation; a page cache in
shared memory can still be added under the service later for bulk
reads, without changing what a job may do.

## Protocol

`ipc` frames every message as a little-endian `u32` length and the
encoded value. `ipc::Wire` encodes integers at fixed width, lengths as
`u32`, enums as a one-byte tag; `ipc::rpc!` turns one line per request
into the request enum, the service trait, the dispatcher and the
client trait. `stg::proto` is that list:

| Tags | Requests |
|---|---|
| 0-3 | `hello` (who the connection speaks for), `exec` (a console command run in the service), `close` (slic only), `ping` |
| 4-8 | the job: a number, its status (the activity-level gate), the active jobs, its attributes, `CHGJOB` |
| 10-12 | programs and system pointers |
| 20-24 | user spaces |
| 30 | message descriptions |
| 40-45 | data areas and data queues |
| 50-52 | files: open, a record operation, close |
| 60-85 | the APIs' provider, one request per method |
| 90-95 | message queues: send, receive, reply, list, break messages, the system reply list |
| 100-110 | subsystems and pools: check, register, state, list, end; objects; `SBMJOB`, taking a queued job, a job's end code |
| 111-112 | the job scheduler's tick; the job log copy |
| 113-118 | spooled files and printer files; the writer's next file and what it printed |
| 119-122 | spool maintenance, a job name's state this IPL, the next IPL's `QIPLSTS`, `QWCRSSTS` |

Every connection starts with `hello`, naming the job and the lock
handle its locks are taken under. Nothing else is answered before it.
Requests are answered in order, one at a time per connection.

`slic`'s own connection is the job process's standard-input pattern in
reverse: the service's standard input is `slic`'s socket. Over it come
the `Start` message (units and system values), then ordinary requests;
at power down `close` ends every job's use of the database, checkpoints
and closes the store, and the service exits after the answer.

## Transactions and crash safety

The rules:

1. Every request runs in store transactions the service begins and
   commits or aborts before it answers. No store transaction outlives
   a request, so no job process ever has one open.
2. A job's higher-level state lives in the service, keyed by its
   connection: its open data paths, its commit definition, its
   overrides, its record locks (one table shared by every job), its
   object locks (the store's table, by lock handle).
3. When a connection closes, for any reason (a clean end, `ENDJOB
   *IMMED`, `SIGKILL`, a crash), the service ends the job's database
   session: an open commit cycle is rolled back from its before
   images, shared paths are released, record and object locks are
   released. The same happens to every job at `close`.
4. The service never waits on a job's behalf. A request that would wait
   (a data queue with nothing in it) answers at once; the job waits
   between tries (`stg::remote::RETRY`, 50 ms). The job's wait status
   is the job's own business.
5. If the service itself ends, `slic` shows `B1001060` and halts: the
   store's WAL recovers it at the next IPL like any crash.

## The service console

`slic` keeps the serial line. After the IPL it starts `QSVCJOB` and
passes it every line, except its own commands typed at the idle
prompt (`help`, `release`, `srcs`, `hist`, `hw`, `lic`, `mode`,
`screen`, `key`, `type`, `pwrdwn`, and `dbgkill`, the debug hook that
kills a job's process as a crash would). `QSVCJOB` is listed under
`QSYSWRK` but no monitor owns it, so ending `QSYSWRK` leaves the
console working. `QSVCJOB` runs calls, the job's
displays and the debugger itself (`wm::ctl::JOB_COMMANDS`), and sends
every other line to the service as `exec`. The service runs it with the
job's console state swapped in (SQL mode, transfer buffers, the SQL
context) and answers with the output, the prompt to show next, the SRC
codes the command raised and, when they changed, the system values;
`QSVCJOB` passes those up with the prompt, and `slic` shows the codes
and keeps the IPL subset of the system values in the LIC record.

A debug stop inside a call reads its commands from the same control
channel; `slic` forwards every line while a line is being worked on.
If `QSVCJOB` ends, `slic` starts it again.
