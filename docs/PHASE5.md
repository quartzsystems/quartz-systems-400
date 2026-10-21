# Phase 5: work management

Phase 5 turns the one service job of Phase 4 into a system that runs
work the way the machine does: jobs that are processes, started by
subsystems from routing, job queues and the console, held, ended and
accounted for; their messages, logs and history; their printed output
spooled, printed by writers and kept; the system jobs, the startup
program and power down; and the displays that show it all.

## What was built

| Crate or place | Holds |
|---|---|
| `ipc` | process messages: the `Wire` codec, frames, the `rpc!` macro, Unix socket channels |
| `stg` | the storage service protocol (`proto.rs`), a job's link, the runtime's sources answered remotely, the job's spool sink |
| `lic/slic/src/stgsvc` | the storage service: open and serve, per-job contexts, job numbers and `QTEMP`, message queues, `QHST`, subsystems, pools, job queues, the job schedule, spooled files, printer files, writers, the job log copy, statistics |
| `lic/slic/src/procs` | the processes `slic` starts: the storage service, the system jobs and the IPL's subsystems, `QSVCJOB`, the console's signed-on job |
| `wm` | work management's model: job identifiers, attributes, statuses, the job log, the active job table, pools, the job control channel, every work management object's payload, the shipped objects, the statistics parsers |
| `qsjob` | the job process: the console runtime and its roles (monitor, batch, interactive, system, scheduler, writer) |
| `spool` | SCS, the line printer, DDS printer formats, the text, FCFC, `*PRTCTL` and hex views, the PDF writer |
| `database/dds/src/prtf` | the printer file compiler and the encoding of its formats |
| `conformance` | QEMU steps 16 to 36, the IR kit's printer cases |

`WORKMGMT.md` describes the model, the layouts, spooling, the system
jobs and the statistics; `STORAGE_SERVICE.md` the service and its
protocol; `WM_OPEN_QUESTIONS.md` the readings the machine must settle.

The decisions in force:

- One Linux process per job; a job never touches a page. Every object,
  file, queue and lock is the storage service's, reached by request;
  a job that dies leaves nothing half done, and its end is the closing
  of its connection.
- Pools are memory cgroups; activity levels are a gate the service
  keeps; a job's process joins its pool's cgroup.
- Waits happen in the job, never in the service: a data queue, a
  message, a locked record (`WAITRCD`), an activity level.
- The job keeps the service's copy of its job log current; the service
  prints it at the end, however the job ended.
- Spooled files are `*SPLF` objects in `QSPL` holding SCS; the job runs
  the line printer and sends the data in chunks.
- `slic` is `SCPF`: it starts the system jobs, `QSYSWRK` and the
  controlling subsystem, waits for the startup program, and is the
  console's workstation controller.

## Numbers

Step 36 of the QEMU suite (`cargo run -p build -- test`, KVM on the
development machine, 2026-10-21):

| Benchmark | Result |
|---|---|
| a console command's round trip (`dspjob`), for scale | 21.9 ms |
| `SBMJOB` to a held queue, over the console | 14.9 ms each |
| fifty jobs started, run (`DLYJOB DLY(0)`) and ended | 0.91 s |
| job start latency: taken off the queue to the job's hello | 7.7 ms average, 25.1 ms longest |
| spool write rate: a program printing 10,000 lines on `QSYSPRT` | 0.07 s, about 135,000 lines a second |

The console numbers include the serial line; the start latency is
measured by the service (`wmstats`). A subsystem monitor looks at its
job queues every 100 ms, which bounds how long a released job waits
before it is taken.

## Tests

- `cargo test -p wm -p spool -p dds -p db -p mi -p stg`: the payloads'
  round trips and the spec tables (`work/wm/tests/spec.rs` against
  `WORKMGMT.md`), the shipped objects in a store, the schedule's date
  arithmetic, SCS and the printer, DDS printer formats, the PDF
  writer's structure, the statistics parsers.
- `cargo test -p conformance --test qir_kit`: the IR kit, now with the
  printer cases `PRT01` (program-described, overflow) and `PRT02` (DDS
  formats, editing, indicators, underline) and the spooled dumps.
- `cargo run -p build -- test` (WSL): the QEMU suite, 36 steps. Phase
  5's are 16 to 36: active jobs and the IPL's work, job descriptions,
  the library list, the job log, message queues, `QHST`, the reply
  list, subsystems, console sign-on, job queues, the job scheduler,
  spooled files, printer files, writers, power down from Command Entry
  with a restart, statistics under a memory-heavy job, a report job end
  to end, a batch inquiry answered at `QSYSOPR`, `ENDSBS *IMMED` with
  five busy jobs holding a lock, a job killed in a commit cycle, and
  the numbers above. Phases 1 to 4's steps still pass.

## Unsupported, by design or for now

- User profiles: a job's user is whatever the job description, the
  sign-on or `SBMJOB` says; nothing is validated or authorized
  (`MIGRATE(phase6)`).
- Commands a program runs through `QCMDEXC` run after the call returns,
  so overrides are command-level ones (`MIGRATE(phase7)`).
- Prestart and communications entries are kept and never start a job;
  `STRRMTWTR`, real printer devices and `RMTSYS` output queues wait for
  communications and devices (`MIGRATE(phase10)`).
- Display files and `DSPLY` on a workstation (`MIGRATE(phase8)`); the
  console's Command Entry is `QCMD`'s own panel.
- `DEVTYPE` is `*SCS` only; `CHLVAL` channels are not supported (FCFC
  channels map to fixed lines).
- A program's file I/O does not reach its job's `QTEMP`, which lives in
  the temporary ASP (`MIGRATE(phase6)`).
- Break messages arrive at a job's wait points, not in the middle of a
  running program.
- `PWRDWNSYS IPLSRC` is `*PANEL` only; DB and non-DB faults are a
  pool's major and minor page faults.

`WM_OPEN_QUESTIONS.md` lists the readings the machine must settle.
