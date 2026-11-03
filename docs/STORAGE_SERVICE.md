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
synchronizes the halves start mirrored protection paired in DST
(`C1001075`), creates `QSYS` on a fresh store,
runs database recovery, loads the system values and finishes an
install. At the end of the OS phase it checkpoints and closes the store,
then starts itself again as the service (`/init qsstg`, the same
binary in its storage role) and hands it the unit device nodes and the
system values. The service reopens the units (the checkpoint leaves
nothing to replay), creates the temporary ASP, opens its own handle on
the LIC partition for the mirror resume journal only, binds
`/run/qs400/stg` and answers `slic` with what it opened. `slic` then
shows `C6001015` and carries on.

Disk work that IBM does with the system up runs in the service's serve
loop between requests: a device parity synchronization or rebuild
(`STORE_FORMAT.md` §11) the IPL left unfinished, and a mirror resume:
one started in DST before the IPL, or one a power down or a crash cut
short, which the service picks up from the resume journal in the LIC
partition (the open finds the half still resuming).
The loop is one thread over the one store, so the work takes turns: up
to 40 ms of it when no request waits, 5 ms when one does, then the
requests. The IPL does not wait for device parity work; the service
picks it up from the mark the superblock records.

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
| 9 | `hello_thread`: a secondary thread of an active job, on a link of its own |
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
| 440 | the integrated file system: one request carrying an `IfsOp` (`docs/IFS.md` "The protocol"); descriptors, directory streams and locks are the connection's |
| 690 | TCP/IP: one request carrying a `stg::tcp::TcpOp` (`STRTCP`, `ENDTCP`, `QTCPWRK`'s heartbeat, the status snapshot, the panels' data, the resolver's configuration, the port restrictions, the services table, the servers' attributes, `svc.tcp`), answered as a `TcpReply` (`docs/TCPIP.md`) |

Every connection starts with `hello`, naming the job and the lock
handle its locks are taken under. A secondary thread of a job starts
with `hello_thread` instead, naming the job and its process. It shares
the job (number, `QTEMP`, lock handle) and its security session, so a
profile swap on any of the job's links reaches them all. Its link
closing ends nothing. Nothing else is answered before either.
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
   released, and its IFS descriptors (members written back), directory
   streams and byte-range locks go. The same happens to every job at
   `close`.
4. The service never waits on a job's behalf. A request that would wait
   (a data queue with nothing in it) answers at once; the job waits
   between tries (`stg::remote::RETRY`, 50 ms). The job's wait status
   is the job's own business.
5. If the service itself ends, `slic` shows `B1001060` and halts: the
   store's WAL recovers it at the next IPL like any crash.

## Long statements

The serve loop is one thread over one store, and the SQL engine runs a
statement on a stack: a query over a large table used to hold every
other job's requests (a QAP connection's Run SQL held Run Command,
spooled-file reads, other connections' SQL and its own catalog lists).
No statement holds up another job now: a statement takes turns with
every other request, and with the other statements in progress.

- Which: an embedded statement (`sql_run`) whose descriptor is a query
  or a statement run (`Query`, `Open`, `Exec`, `ExecImm`, `Execute`;
  `sql::embed::may_yield`) at `*NONE`, `*CHG` or `*CS`, the level
  `SET TRANSACTION` chose over the program's. Among statements run,
  `SELECT`, cursor `OPEN`, `INSERT`, `UPDATE` and `DELETE` (a condition
  with a subquery included) and `MERGE` take turns; the rest run to
  their end (DDL, `COMMIT`, `CREATE TABLE ... AS`), as do every statement under
  `*ALL` (a read there is one view), `DESCRIBE`, the console's `sql` and
  `RUNSQLSTM`.
- Where it yields, at most every 5 ms (`SqlContext::yield_every`):
  between batches (256 keys, or 256 records by RRN) of every table scan,
  a correlated subquery's included; between rows of each step that
  works on rows already read (pushed predicates and `EXISTS`, the rest
  of `WHERE`, joins, grouping, `HAVING`, aggregates, the select list, a
  sort's keys); between rows a change writes. A frame that holds what
  it read from the store (a scan's open members while it reads a batch,
  a change's open path) marks itself (`SqlContext::hold_open`): nothing
  under it yields but itself at its own point, so no frame above a
  yield holds pages or positions read before it.
- How: the statement's transaction is `store::Txn::lent`, borrowing
  the store through the job's lender per transaction rather than for
  its life. At a yield it is suspended (`Txn::suspend`: the borrow is
  given back; a clean transaction has nothing to log). A change first
  commits what it wrote so far (`Txn::commit_part`), so its transaction
  is clean; its record locks and commit cycle are the job's and go on
  as they were, and the journal has its entries in the order written.
  The yielder sets the service as between requests (`served::leave`:
  the store back in `System`, the library list and `QTEMP` root reset,
  the statement's audit entries flushed to its job, no guard session
  in force) and suspends the statement's fiber; when the loop resumes
  it, it sets the job up again (`served::enter`), the transaction
  resumes as one begun now, and a scan opens its members again and
  goes on from the last key or RRN it read.
- Fibers (`stgsvc::fiber`): each such statement runs on a stack of its
  own (8 MiB, kept for reuse), with the service parked in a
  thread-local while it runs (`stgsvc::turns`). A pass of the loop
  answers whatever is ready on the connections that are not busy
  (without waiting while statements are in progress), then gives each
  statement in progress one turn, until its next yield or its end, in
  the order they started. So statements interleave in any order: an
  earlier one goes on, and may end, while a later one runs. Sixty-four
  run at once; past that a statement waits for one to end.
- Memory: one statement holds at most `SqlContext::memory_limit` of
  rows (256 MiB by default, `sql::exec::mem`) and past it ends with
  `SQL0904`, so one user's query cannot exhaust the service's process.
- Locks: a statement that may yield holds `*SHRRD` on every file and
  member it scans (the table, the access path's file and member) from
  its first read to its end, released when its transaction ends; a
  change holds its path's `*SHRUPD` as before. So `CLRPFM`, `RGZPFM`,
  `DLTF`, `CHGPF` and reclaim storage are refused with the file in use
  while it runs, as IBM i refuses them under an open cursor; inserts,
  updates and deletes by other jobs go on. No lock is taken for the
  turns themselves: jobs wait only for the record and object locks the
  database rules give (`STRJRNPF` and `ENDJRNPF` take `*EXCL`, a record
  being changed is locked to its job), and the engine never waits for
  one: a conflict is `SQL0913` at once.
- What it sees: each batch reads what is committed (and, as before,
  what another job's open commit cycle has written) when the batch is
  read. Rows changed or added behind the scan's position after it
  passed are not seen; ahead of it, they are (a row whose key another
  job moves from behind to ahead is read twice). That is a read under
  `*CS` or below on IBM i, which gives a statement no single view
  either; `*ALL` keeps one. A change that yielded tests its condition
  again against each row as it is when the row is locked to be changed,
  so a row another job moved out of it meanwhile is left alone.
- A change whose condition has a subquery decides as of its start, as
  the rules require (its subqueries never see what it changed). It
  reads first, taking turns as a query does, and finds every row the
  condition takes, the subqueries' answers kept as they are reached
  (`sql::exec::answers`; one that reads no column of the row being
  tested is kept once, so `v < (SELECT AVG(v) FROM t)` is computed one
  time). Then it writes, taking turns between rows. A row it locks just
  as it read it is changed. A row another job changed meanwhile is
  tested again as it is now, each subquery answered as kept, never run
  again; when one has no kept answer (it reads the row's columns, or
  was not reached the first time) the first decision stands, as a
  change that lists its rows before it writes them decides. A row
  another job deleted is passed by.
- `MERGE` decides the same way: it reads the source, matches each
  source row against the target (`ON`) and chooses its action (the
  `WHEN` conditions), all before it writes and taking turns as a query
  does, so it never matches a row it inserted or changed itself; then
  it writes the changes in order, taking turns between them. A matched
  target row another job changed meanwhile is matched and its action
  chosen again as it is now, the subqueries answered as kept (when one
  has no kept answer the first choice stands); a row it changed itself
  for an earlier source row takes the later action as it is now. Under
  `*NONE` the rows already changed stay changed if the statement ends
  early, as when it fails part way.
- Its connection: busy until the statement answers, so it is not read
  (one request at a time per connection, as always). Its job's other
  links (`hello_thread`) are served in the turns.
- A `close` met while statements run waits until none is in progress
  and makes each end at its next yield (`SQL0952`); the job's own
  cancel (`stg::sql_cancel_path`) is asked between rows, so it ends a
  statement while later ones run.
- Background disk work (parity, a mirror resume) runs only when no
  statement is in progress.
- Crash safety: every transaction, the suspended one's included, ends
  before its request is answered. A crash during a turn leaves a query
  nothing to roll back, and a change the parts it committed, as a crash
  between two of a commit cycle's requests would.


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
