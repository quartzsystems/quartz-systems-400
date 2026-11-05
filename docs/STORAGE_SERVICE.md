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
  `sql::embed::may_yield`) at any commitment level. Among statements
  run, `SELECT`, cursor `OPEN`, `INSERT` (from a query too), `UPDATE`
  and `DELETE` (a condition with a subquery included), `MERGE`, `CREATE
  INDEX` (a `UNIQUE` constraint's index too), `ALTER TABLE` and
  `ROLLBACK` take turns. The rest run to their end, each short: one
  that only names objects or describes them (`CREATE TABLE`, `CREATE
  VIEW`, `CREATE SCHEMA`, `CREATE ALIAS`, `LABEL ON`, `COMMENT ON`,
  `SET`, `DECLARE`, `FETCH`, `CLOSE`), `COMMIT` (its cycle's entries
  and its locks released), and `DROP` (see "Definitions" below). `CREATE
  TABLE ... AS` is refused by the engine. `DESCRIBE` and the console's
  `sql` and `sqlrun` words run to their end too.
- Commands (`run_cmd`) that can run long take turns the same way
  (`cpp::longcmd`): `RUNSQL`, `RUNSQLSTM`, `CPYF`, `CPYFRMIMPF`,
  `CPYTOIMPF`, `CRTLF` and `RGZPFM`. What a command reads from its job
  and the system (its parameters, the statements of a source member or
  stream file, the text `CPYFRMIMPF` reads, the library list) is read
  first, as the command would; its work then runs on a fiber on the
  store lent to it (`longcmd::Work`), with a context of its own under
  its job's handle, so its locks are its job's; what it gives back (an
  `xfer` buffer, `CPYTOIMPF`'s stream file, the reply and its
  completion message) is done at the end with the job set up as for
  any command. At the database's pauses (every 256 records, at most
  every 5 ms) the work commits what it did so far and yields as a
  statement does. A command whose work fails after it committed parts
  commits the rest too (what the failure undid included); one that
  failed with nothing committed is dropped, as before.
  - `RUNSQL`, `RUNSQLSTM`: each statement runs as an embedded one does,
    taking turns inside it, with a turn between statements; a statement
    that failed after it committed parts keeps them. Its context is the
    job's own, not the console's (its overrides copied).
  - `CPYF`, `CPYFRMIMPF`: records are written in parts, the from-file
    held `*SHRRD` and the to-member `*SHRUPD` (`*EXCL` under
    `MBROPT(*REPLACE)`) throughout; others read the from-file and may
    change it, as the rules allow. A copy ended part way (an error past
    `ERRLVL`, an end, a crash) keeps the records copied so far, as an
    error part way does.
  - `CPYTOIMPF`: reads in parts under `*SHRRD`; the text is written at
    the end.
  - `CRTLF`: the file is made, then its member's path built as `CREATE
    INDEX` builds one (above), the based-on files and their members held
    `*SHRNUP` and the new file `*EXCL`; a build that fails (`CPF5090` for
    a `UNIQUE` file over repeated keys) or is ended deletes the file.
  - `RGZPFM`: the member is held `*EXCL`; its records are read with
    pauses, then cleared and written again in one step. That step is not
    split: the records get new numbers in the same member, and rebuilding
    it beside itself would change the member object its logical files
    and LOB values name (`ALTER TABLE` refuses a file with dependents;
    `RGZPFM` must not).
  Still running to their end: `CLRPFM` (one step whose cost goes by
  extents and path pages, as `DROP`'s; a member cleared in parts would
  be half there), `INZPFM`, `CHGPF SRCFILE` (one transaction, crash safe
  by the same rebuild beside the member as `ALTER TABLE`), the save and
  restore commands (`SAVLIB`, `SAVOBJ`, `RSTLIB`, `RSTOBJ`: the save
  engine takes its save locks and reads or writes the save file or media
  with the store and its history in the system for the whole command; a
  save that let changes in between parts would need save-while-active
  checkpoints the engine does not have, and a restore half done would
  be seen), `CPYTOSTMF` and `CPYFRMSTMF` (they go through the service's
  `Ifs`, which borrows the store per operation from the system, not
  from a lender a fiber can give back), and `RCLSTG` (it quiesces the
  store: no other request may run under it).
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
- Definitions and maintenance take turns through `db::Pause`, called
  every 256 steps of their long loops (`db::PAUSE_EVERY`): it commits
  what was done so far and yields as a change does
  (`SqlContext::pause`); the locks the rules give are held throughout.
  - `CREATE INDEX`: the table is held `*SHRNUP` from the start (others
    read it; a change, `CLRPFM`, `DROP` or another `*EXCL` use of it is
    refused with the file in use), the new index `*EXCL` from its
    creation (nobody opens, uses or drops it; a query's plan passes it
    by). Its member's path is marked invalid before the first pause,
    so a build cut short by a crash leaves an index rebuilt when next
    opened. Every step takes turns: the records are read with pauses,
    their keys sorted in runs of 16,384 as they come
    (`db::path::load`), and the runs merged into the tree by a loader
    in steps (`store::btree::Loader`) with a pause every 32 leaves;
    before each such pause the tree so far is made whole (the open
    leaf and the right edge written) and its root stored in the member,
    so what a crash leaves is a tree the rebuild frees, never pages
    nobody can find. A `UNIQUE` index refuses repeated keys as they
    meet in the merge (`SQL0603`). A build that fails or is ended
    deletes the index, the parts it committed included.
  - `ALTER TABLE` that changes the record format (and `CHGPF
    SRCFILE`): the table is held `*EXCL`; each member is rebuilt beside
    itself (`db::member::rebuild`, `docs/DB_FORMAT.md` §4): a new
    member the old one names takes the records read, converted and
    written with pauses, the old member and format untouched; then one
    short step deletes the old members, names the new ones in the
    member table and replaces the format. A crash part way leaves the
    table as it was (the next format change or the member's deletion
    removes what the rebuild made); a rebuild that fails (a `PRIMARY
    KEY` over repeated keys: `SQL0603`) or is ended deletes it at once.
  - `ROLLBACK`: the cycle's changes are undone newest first with
    pauses, its record locks held until the end. A rollback cut short
    by a crash is finished by the IPL's recovery: the undoing entries
    belong to the cycle, so recovery undoes them and then the rest. The
    journal entries of the cycle are read in one step first.
  - `DROP`: one step. An object is freed in one transaction (its header
    names its pages; a part freed alone would leave it naming freed
    pages); the cost goes by extents and path pages, not records.
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
  being changed is locked to its job, a record read under `*ALL` is
  read-locked). A change (`UPDATE`, `DELETE`, `MERGE`) that meets a
  record another job holds waits for it up to the file's `WAITRCD`, as
  a program's record I/O does, then fails with `SQL0913`: it tries
  again after each turn it gives the others (`SqlContext::wait_turn`,
  what it wrote so far committed first), so the holder runs and can
  commit or roll back meanwhile. `*IMMED` waits not at all, `*NOMAX`
  until the record is free or the statement is cancelled; with no
  turns to give (the console's `sql` and `sqlrun`) there is nothing to
  wait for and the conflict is `SQL0913` at once. While every
  statement in progress is waiting for a record, the loop waits up to
  10 ms for requests between its turns rather than spinning
  (`sql::exec::record_waits`). Other conflicts (an object lock, an
  `*ALL` read of a record held for update) are `SQL0913` at once.
- Under `*ALL` (`RepeatableRead` and `Serializable` over QAP, `SET
  TRANSACTION` RS or RR, `COMMIT(*ALL)`): each record a scan reads is
  read-locked as it is read, in the same turn and before any yield
  (`SqlContext::lock_read`), by the job's commit definition until
  `COMMIT` or `ROLLBACK` (`CommitDef::lock_read`), or by the statement
  until its end when the job has none. Read locks share: another job
  reads those records, at `*ALL` too, but its change of one waits (an
  SQL change, as above, and a program's record I/O: its `WAITRCD`, then
  `SQL0913` or `CPF5027`); an `*ALL` read of a record another job
  holds for update is refused. The job's own change of a record it read takes it when
  no other job shares it. The locks are in the service's record lock
  table (`db::odp::locks`, memory only), released at commit, rollback
  and the end of the job. Nothing else is locked: rows other jobs add
  meanwhile may be read (phantoms, as the level allows).
- What it sees: each batch reads what is committed (and, as before,
  what another job's open commit cycle has written) when the batch is
  read. Rows changed or added behind the scan's position after it
  passed are not seen; ahead of it, they are (a row whose key another
  job moves from behind to ahead is read twice). That is a read under
  `*CS` or below on IBM i, which gives a statement no single view
  either. Under `*ALL` a record read cannot change until the unit of
  work ends, so the rows read are the same at every yield and read
  again the same; only rows added or moved ahead of the scan are new.
  A change that yielded tests its condition
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


## Trigger programs

A trigger program (`ADDPFTRG`, `docs/DB_FORMAT.md` §8.1 and §8.3) runs in
the job that changes or reads the record, as on IBM i: its own requests,
its activation group and its job log are that job's. The change itself
is made in the service. The open data path never calls a program; it
makes its calls through its trigger mode (`db::trg::Mode`):

- `Refuse` (a new path's): a change or read any trigger of the file fires
  for is refused before anything changes (`CPF502B`; SQL `SQL0084`).
  Functions that run wholly in the service use it: the console, `CPYF`,
  `DFU`, `FTP`, the DDM server, a statement not on a fiber (all
  BLOCKED(sql) until each can reach its job).
- `Defer`, record I/O (`mi::io::trigger`): the job's open says it calls
  triggers (`OpenSpec.triggers`). An operation with before triggers
  answers status `TRIGGERS_DUE` with their buffers and changes nothing;
  the record read for update stays held. The job calls the programs in
  order, a before trigger with `ALWREPCHG(*YES)` passing its new record
  on, and gives the same operation again with the answer
  (`TriggerResume`): the last changed buffer, or the call that failed.
  The path takes the answer only for the operation it handed the calls
  out for. The constraints are checked on the record the triggers left,
  the change is made and its transaction ends with the request; the
  reply carries the after calls, which the job makes then. A before
  failure is status 01023, an after one 01024 (the change stays, as on
  the primary without commitment control), both `CPF502B`. Nothing of
  the service waits on the job meanwhile, so the program's own requests
  (other files, the same file through another open) are ordinary ones.
- `Call`, SQL (`SqlContext::midcall`, `sql::exec::routine::midcall`): a
  statement fires triggers row by row, so the calls are made in the
  middle of it through a `db::trg::Caller`. The engine's caller commits
  what the statement wrote so far (`Txn::commit_part`, as a yield does)
  and gives the store back; the service's mid caller (`turns.rs`) answers
  the job's request with a call plan of style trigger (the buffer) and
  suspends the statement's fiber, which takes no turns until its job
  answers. The job's connection is read again meanwhile, so the
  program's own requests are served, its statements on fibers of their
  own. The job (`mi::io::trigger::sql_run`, which every embedded
  statement of the runtime goes through) calls the program and sends
  `Op::Called` named `*MIDCALL` with the buffer back, or the failure;
  that resumes the latest statement of the job waiting for an answer,
  the member is read again, and the statement goes on. A job that goes
  while a statement of its waits ends after the statement does. The
  engine's side runs in memory in `tests/trigger.rs` of `database/sql`
  (`SQL0443`/38501 for a refusal, `SQL0204` for a program not found, as
  the primary). The same plan and answer carry any call a statement
  needs made in its middle (`MidCaller`); a statement run where there is
  no fiber or no job to call (the service console, `STRSQL`'s and the
  servers' statements run in the service) still refuses a change of a
  table with triggers with `SQL0084` (BLOCKED(sql)).

Read triggers fire for record I/O of a physical file; a query of a table
with read triggers and a read through a logical file over one are
refused (BLOCKED(sql)). `ALWREPCHG(*NO)` on an after trigger does not
yet refuse the program's own change of the same record (BLOCKED(sql)).

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
