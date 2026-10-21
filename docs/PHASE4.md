# Phase 4: the ILE runtime

Phase 4 gives QS/400 programs: an intermediate representation the
language compilers of later phases target, a native code generator,
the ILE objects and binder, a runtime that runs programs the way the
machine does, the system APIs, and a service console that compiles,
binds, calls and debugs. Everything is Rust; the only compiled
dependency is Cranelift, pinned at 0.136.0, which is itself Rust.

## What was built

| Crate | Holds |
|---|---|
| `qir` | the IR: types and layouts, modules, the instruction set, the text form, the verifier, the versioned binary form, `qirc` (`QIR.md`) |
| `codegen` | IR to x86_64 through Cranelift: the ABI, the runtime call table, lowering, relocations, statement maps, placing code (`ABI.md`) |
| `ile` | `*MODULE`, `*PGM`, `*SRVPGM`, `*BNDDIR` payloads, the binder, binder language, signatures, displays, store operations (`ILE.md`) |
| `mi` | the runtime: the job, the call stack, activation groups, fat pointers and spaces, heaps, user spaces, message queues, the exception model, file I/O, data areas and queues, the debugger, dumps |
| `msgf` | `*MSGF` objects, `QCPFMSG`, `QCEEMSG` and `QSQLMSG` from `msgs/*.toml`, substitution (`MSGF.md`) |
| `sysapi` | the system APIs as natives of `QSYS` and procedures of `QLEAWI`, with IBM's parameter layouts (`SYSAPI.md`) |
| `slic/src/svc/ile` | the console: `qir asm`, `crtpgm`, `call`, displays, `strdbg`, over the store lent to the job |
| `conformance/qir` | 67 IR programs with their expected outcomes, run in memory and over the console |

The design decisions in force: scalar registers against storage places;
exceptions percolate by return with a pending-flag poll after every
call, the handler search happening at raise time with every frame
intact; one environment record per job, `enter` switching the static
area; entry procedures taking an argument vector; 128-bit fat pointers
validated through a spaces table; storage operands passed as address
and type descriptor pairs to a runtime library; `*SYSTEM` imports bound
to runtime procedures; files, data objects and APIs reached through
traits a store-backed console and an in-memory test world implement.

## Numbers

`cargo test --release -p conformance --test callbench -- --ignored
--nocapture` on the development machine (Windows host, release build):

| Benchmark | Result |
|---|---|
| a million bound calls (`callb` of a one-instruction procedure) | 26.8 ms, 26 ns per call |
| a hundred thousand dynamic calls (`call` of a program in the caller's group) | 24.9 ms, 248 ns per call |

The bound call cost is the prologue and epilogue: the region and
statement words stored, `enter` and `leave` pushing and popping the
frame, the pending poll. The dynamic call adds the program lookup, the
activation check and the argument vector.

## Tests

- `cargo test -p qir -p codegen -p ile -p mi -p msgf -p sysapi`: the
  unit and integration tests of every crate, 150-odd in all: the text
  round trip, the verifier's rejections, lowering of every instruction,
  binding, signatures, activation groups, heaps, user spaces, messages,
  every exception path, the APIs' layouts, file I/O over an in-memory
  store, the debugger, retranslation, the stack budget.
- `cargo test -p conformance --test qir_kit`: the IR kit in memory,
  compared with `conformance/qir/expected/`.
- `cargo run -p build -- test` (WSL): the QEMU suite, now with step 15,
  the IR kit assembled, bound and called through the service console.
- The oracle kit under `conformance/oracle/ile/`: RPG and CL for the
  real machine, one program per open question; the test compares the
  machine's outputs when they are present.

## Unsupported, by design or for now

- `rem` of floats and calls to a bare signature are not lowered.
- Display and printer files (`dspio`, `prtio`) return status `01299`
  until Phase 8.
- The IR's file I/O resolves files in ASP 1; `QTEMP` is a separate
  store until Phase 5 gives each job a `QTEMP` in ASP 1.
- `QCMDEXC` queues the command for the console to run after the call;
  the command analyzer is Phase 7.
- Operational descriptors are not built, so `CEEDOD` reports
  `CEE0820`.
- Condition handler result codes 30 and 31 (promote) act as percolate.
- A stack overflow is caught by a budget check in `enter`, not by the
  guard page; the job's own thread, and the signal handler on its
  alternate stack, come with Phase 5.
- Service programs bind through the binder and the kit's in-memory
  programs, but the console has no `*SRVPGM` signature display beyond
  `dspsrvpgm`.

`ILE_OPEN_QUESTIONS.md` lists the readings the machine must settle.
