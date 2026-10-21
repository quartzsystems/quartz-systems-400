# The calling convention of generated code

How `codegen` turns IR procedures into x86_64 functions and how those
functions call each other, the runtime and system APIs. The `codegen`
crate implements this document; `mi` builds the records it describes.

## 1. Target and backend

Cranelift `0.136.0`, pinned exactly, with the `x86` backend only:
System V calling convention, no position-independent code, no stack
probes, no libcalls. The ISA is `x86_64-unknown-linux-musl`. Generated
code also runs on another x86_64 host that can call `sysv64` functions,
which is how the tests run it.

Storage keeps IBM's byte layouts (`QIR.md` §2), so every multi-byte
integer is big-endian in memory. The x86 backend has no big-endian
memory operations; generated code byte-swaps around every integer load
and store, and moves floats through their integer bit patterns. Fat
pointers are 16 bytes (`ILE.md` has the layout) and travel as `i128`
values; Cranelift's `enable_llvm_abi_extensions` is on so an `i128`
parameter or return value uses a register pair, as Rust's `u128` does.

## 2. The environment record

Every procedure receives a pointer to an `Env` as its first parameter:

| Offset | Field | Meaning |
|---|---|---|
| 0 | `statics` | base of the program's static area in the current activation group |
| 8 | `job` | the job record |
| 16 | `entry` | the current call-stack entry, runtime-owned |

The job record begins with three words generated code touches:

| offset | field | meaning |
|---|---|---|
| 0 | `pending` | `u64`, nonzero while an exception percolates |
| 8 | `cur_region` | `u32`, the `monitor` region the running code is in, 0 outside every region |
| 12 | `cur_stmt` | `u32`, the statement the running code is at |

Generated code stores `cur_region` before every call and every
runtime call that can raise, and `cur_stmt` at every `debug.stmt`.
The runtime's `enter` copies both into the caller's call-stack entry
and `leave` restores them, so each suspended entry's region and
statement are known at raise time (§7) and for the call stack display.

One `Env` exists per job. The runtime's `enter` sets `statics` to the
static area of the running program's activation in the current
activation group and `leave` restores the caller's, so a call across
programs needs no stub (`ILE.md` §4). A module's static frame lies
inside the program's static area at an offset fixed at bind time; that
offset reaches the code through a relocation (§5), so the same code
serves every activation group.

## 3. Procedure signatures

```
proc(env: i64, argc: i64, opdesc: i64, [retbuf: i64,] params...) -> [value]
```

- `env`: the environment record.
- `argc`: the number of arguments the caller passed, `*OMIT` ones
  included; `%PARMS` reads it. Trailing `*NOPASS` parameters that were
  not passed are present as null pointers or zero values.
- A program entry procedure takes `argv` instead of its parameters:
  the address of an array of `argc` parameter addresses, which is what
  a dynamic call (`call_pgm`) and a bound call to the entry both build.
  The prologue reads each parameter's address from it, null when not
  passed.
- `opdesc`: the operational descriptor list, or 0 when the caller
  passed none. *Phase 4, pending*: `CEEDOD` and `*VARSIZE` lengths.
- `retbuf`: present when the return type is not a register type; the
  caller provides storage of the type's size and the callee writes it.
- Parameters in declaration order: a by-reference parameter is the
  native address of the caller's storage (an `*OMIT` argument is 0); a
  by-value parameter of a register type is the value itself; a by-value
  parameter of a storage type is the address of the caller's storage,
  and the callee copies it into its own frame on entry.
- The return value, for a register return type, in the convention's
  return register.

Register types map to Cranelift types: `i8 u8 ind` are `i8` (`ind` is
0 or 1 in a register, `'0'`/`'1'` in storage), `i16 u16` `i16`, `i32
u32` `i32`, `i64 u64` `i64`, `f32`, `f64`, `ptr procptr` `i128`.

## 4. Frames and storage

Automatic variables and by-value parameters are stack slots in the
procedure's native frame, initialized on entry from the variable's
`inz` bytes or the type's defaults. Static variables, including a
procedure's own `static` and `const` ones, lie in the module's static
frame, laid out in declaration order with pointer-holding types aligned
to 16; `StaticLayout` gives every slot and the frame's initial bytes.
A literal passed by reference is placed in a temporary slot of the
parameter's type.

A `based` variable or a `[%p]` deref goes through the runtime's `deref`
(§6), which validates the fat pointer and returns a native address.
Array indexes are bounds-checked against the declared element count;
an index out of range raises `MCH0603`.

## 5. Relocations

Each procedure is compiled on its own and the module's procedures are
concatenated, 16-byte aligned. Calls between procedures of one module
are PC-relative and resolved at code generation. What remains for the
loader, in `ModuleCode::relocs`:

| Target | Kind | Value written |
|---|---|---|
| `Runtime(fn)` | `Abs8` | the address of runtime table entry `fn` |
| `ImportProc(name)` | `Abs8` | the address of the exported procedure |
| `StaticFrame` | `Abs8` | this module's frame offset in the program's static area |
| `ImportData(name)` | `Abs8` | the frame offset of the exporting module plus the variable's offset |

`Abs8` sites are the 8-byte immediate of a `movabs`; the loader writes
the value plus the addend. `Loaded::new` does this for a `ModuleCode`
against a resolver and makes the mapping executable.

## 6. The runtime call table

Generated code never looks a symbol up. Each runtime function has a
fixed index (`RtFn`), the relocation names the index, and the runtime
(`mi::rtlib::table`) fills a table of addresses in the same order. All
take `env` first (except `copy`) and use the System V convention. A
storage operand is a pair of `i64` words: the native address and a
type descriptor (`TypeDesc`: kind, length or digits, scale or
precision, date or time format). An absent optional operand is 0, an
absent optional length is -1.

| Index | Function | Operands after `env` | Returns |
|---|---|---|---|
| 0 | `raise` | `cond: i32` | |
| 1 | `enter` | `code, argc` | |
| 2 | `leave` | | |
| 3 | `deref` | `lo, hi` | native address |
| 4 | `copy` | `dst, src, len` (no `env`) | |
| 5 | `mov_str` | `dst, dty, src, sty` | |
| 6 | `dec_bin` | `op, dst, dty, a, aty, b, bty, prec, flags` | |
| 7 | `dec_un` | `op, dst, dty, src, sty, halfadj` | |
| 8 | `dec_cmp` | `a, aty, b, bty` | `i32` -1, 0, 1 |
| 9 | `dec_to_int` | `src, sty, bits, signed, chk` | `i64` |
| 10 | `dec_to_float` | `src, sty` | `f64` |
| 11 | `convert` | `dst, dty, src, sty, opts` | |
| 12 | `str_cat` | `dst, dty, a, aty, b, bty` | |
| 13 | `str_subst` | `dst, dty, src, sty, start, len` | |
| 14 | `str_scan` | `needle, nty, hay, hty, start` | `i32` |
| 15 | `str_check` | `set, sty, src, srcty, start, reverse` | `i32` |
| 16 | `str_xlate` | `dst, dty, from, fty, to, tty, src, sty, start` | |
| 17 | `str_trim` | `dst, dty, src, sty, chars, cty, side` | |
| 18 | `str_cmp` | `a, aty, b, bty` | `i32` -1, 0, 1 |
| 19 | `str_len` | `src, sty` | `i32` |
| 20 | `str_setlen` | `dst, dty, n` | |
| 21 | `str_replace` | `dst, dty, src, sty, repl, rty, start, len` | |
| 22 | `dt_add` | `dst, dty, src, sty, n, unit` | |
| 23 | `dt_diff` | `a, aty, b, bty, unit` | `i64` |
| 24 | `dt_extract` | `src, sty, unit` | `i32` |
| 25 | `dt_test` | `src, sty, datfmt, timfmt` | `i32` 0 or 1 |
| 26 | `dt_now` | `dst, dty` | |
| 27 | `ptr_addr` | `out, native, len` | 16 bytes at `out` |
| 28 | `ptr_add` | `out, ptr, n` | 16 bytes at `out` |
| 29 | `ptr_diff` | `p, q` | `i64` |
| 30 | `proc_addr` | `out, code` | 16 bytes at `out` |
| 31 | `dsply` | `src, sty, reply, rty` | |
| 32 | `monitor_match` | `kind, a, b` | `i32` 0 or 1 |
| 33 | `call_pgm` | `name, lib, argc, argv` | |
| 34 | `call_pgm_var` | `addr, len, argc, argv` | |
| 35 | `call_pgm_ptr` | `ptr, argc, argv` | |
| 36 | `proc_target` | `ptr` | code address, 0 when invalid |
| 37 | `alloc` | `out, n` | 16 bytes at `out` |
| 38 | `realloc` | `out, ptr, n` | 16 bytes at `out` |
| 39 | `dealloc` | `ptr` | |
| 40 | `sndpgmmsg` | `msgid, msgf, lib, data, dty, type, to_kind, to_n, key` | |
| 41 | `rcvmsg` | `pgmq_kind, pgmq_n, type, key, rmv, msg, mty, msgid, data, dty, keyout, sev, sevty` | `i32` 0 or 1 |
| 42 | `rmvmsg` | `pgmq_kind, pgmq_n, key, clear` | |
| 43 | `db_open` | `file, infds, infds_desc` | `i32` status |
| 44 | `db_close` | `file` | `i32` status |
| 45 | `db_io` | `file, op, rec, rec_desc, key, key_desc, rrn, fmt, nolock` | `i32` status |
| 46 | `dev_io` | `file, op, fmt, rec, rec_desc` | `i32` status |
| 47 | `dtaara` | `op, name, lib, data, data_desc` | |
| 48 | `dtaq_snd` | `name, lib, data, len` | |
| 49 | `dtaq_rcv` | `name, lib, data, cap, wait` | `i32` length |
| 50 | `rslvsp` | `out, type, name, lib` | 16 bytes at `out` |

`file` is the index of the module's file declaration; the runtime
finds the declaration through the running entry's program and keeps
the open file per activation (static area), module and index, so each
activation of a program has its own files, closed when its group is
reclaimed. Operation codes: `db_io` 1 `READ`, 2 `READP`, 3 `READE`, 4
`READPE`, 5 `CHAIN`, 6 `SETLL`, 7 `SETGT`, 8 `WRITE`, 9 `UPDATE`, 10
`DELETE`, 11 `UNLOCK`, 12 `FEOD`; `dev_io` 1 `WRITE`, 2 `READ`, 3
`EXFMT`; `dtaara` 1 lock, 2 read, 3 write, 4 unlock. A key operand is
the key fields' bytes in record layout, a prefix allowed; a relative
record number is passed when there is no key.

`enter` is called in every prologue with the procedure's own code
address and its argument count, so the runtime can push a call-stack
entry (with `%PARMS` for `CEETSTA`) and identify the procedure from
the program's procedure table; `leave` is called before
every return, and the spaces of automatic storage the entry handed out
die with it. `cond` is the number of the `MCHnnnn` message: `0601`,
`0602`, `0603`, `1202`, `1210`, `1211`, `1602`, `3401`, `3402`, `3601`,
`3602`. `prec` packs digits in its low byte and decimal positions in
the next; `flags` bit 0 is half-adjust; `opts` of `convert` packs
half-adjust, the date and time format index plus one, and a CCSID
(`codegen::rt::cvt_opts`). Literal operands are placed in temporaries
typed by the literal (`packed(d,s)` for numbers, `char(n)` for text);
a register operand is spilled to a temporary of its type.

*Phase 4, pending*: I/O, message, data area and data queue entries,
which widen the table in later checkpoints without renumbering.

## 7. Exceptions

An exception travels by return, not by unwinding. When generated code
detects a condition it calls `raise`; after `raise`, and after every
call to a procedure or a runtime function, it polls `job.pending` and,
when the flag is set, branches to its unwind block: `leave`, then
return zeros. The caller polls after the call and does the same, so the
pending exception percolates up the native stack one frame at a time
until a frame handles it. `raise` returning with the flag clear means a
handler resumed execution at the instruction after the failing one,
which is what IBM's reply `G` does; the failing operation's result is
then zero.

Inside a `monitor` region the poll branches to the region's dispatch
block instead of the unwind block. The dispatch block calls
`monitor_match` once per handler, in order, with the handler's
selector (`*ALL`, `*PROGRAM`, `*FILE`, a status range, or a message
identifier or prefix packed into a word); a match clears the pending
flag on the job and jumps to the handler's block; no match falls
through to the enclosing region's dispatch block, and the outermost
falls through to the unwind block, so an unhandled exception leaves
the procedure and percolates. Regions nest lexically, so the dispatch
chain follows the static nesting the verifier checked.

The handler search happens before any of that, inside `raise` (or
`sndpgmmsg`), while every frame is intact. `ModuleCode::regions`
lists each `monitor` region's handler selectors and its enclosing
region, numbered from 1 in lowering order; the region a suspended
entry is in comes from `cur_region` (§2). The runtime walks the
entries from the one the exception was sent to down to its control
boundary, asking each entry's condition handlers (`CEEHDLR`) newest
first, then its regions innermost first. A handler that resumes
clears the flag and `raise` returns; a region that matches leaves the
flag set and the dispatch chain above takes the exception once
percolation reaches that entry. Nothing up to the boundary brings the
function check and the inquiry (`ILE.md` §5). Cancel handlers
(`CEERTX`) run inside `leave` when an entry ends with the flag set.
This design needs no unwind tables; the statement map (§8) and
`cur_stmt` serve the call stack display and dumps.

Integer division by zero, overflow under `.chk`, `MIN / -1`, a narrowing
`cvt.chk` that loses a value, an out-of-range index and a null
by-reference parameter are checked explicitly and raised; nothing in
generated code relies on a hardware trap. `unreachable` is a trap.

## 8. Maps

`ProcCode::stmts` lists the native byte ranges of every `debug.stmt`
in code order; `ModuleCode::stmt_at` answers "which statement is this
offset in" for the call stack display and the debugger.

## 9. Limits of this checkpoint

Lowered: scalar arithmetic, comparisons, conversions between register
types and from numeric storage, control flow, `callb` to local and
imported procedures, `mov` between same-size storage and between text
types, loads and stores of every register type, `parms`, `passed`,
`omitted`, `debug.stmt`, every `dec.*`, `cvt`, `str.*` and `dt.*`
instruction, `addr`, `ptradd`, `ptrdiff`, `procaddr`, based and
deref places, `monitor`/`endmon`, `dsply`, `sndpgmmsg`, `rcvmsg`,
`rmvmsg`, `alloc`/`realloc`/`dealloc`, `dbopen`/`dbio`/`dbclose`,
`dspio`/`prtio`, `dtaara.*`, `dtaq.*`, `rslvsp`. Dynamic program calls,
`sysapi` (a program call into `QSYS`), calls through procedure and
system pointers and every I/O go through the runtime. Still
`CodegenError::Unsupported`: `rem` of floats and calls to a bare
signature.
