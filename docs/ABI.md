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
pointers are 16 opaque bytes and travel as `i128` values.

## 2. The environment record

Every procedure receives a pointer to an `Env` as its first parameter:

| Offset | Field | Meaning |
|---|---|---|
| 0 | `statics` | base of the program's static area in the current activation group |
| 8 | `job` | the job record |
| 16 | `entry` | the current call-stack entry, runtime-owned |

The job record's first field, `pending`, is a `u64` the runtime sets
when an exception is raised and not handled where it occurred.

One `Env` exists per (program, activation group). A module's static
frame lies inside the program's static area at an offset fixed at bind
time; that offset reaches the code through a relocation (§5), so the
same code serves every activation group.

## 3. Procedure signatures

```
proc(env: i64, argc: i64, opdesc: i64, [retbuf: i64,] params...) -> [value]
```

- `env`: the environment record.
- `argc`: the number of arguments the caller passed, `*OMIT` ones
  included; `%PARMS` reads it. Trailing `*NOPASS` parameters that were
  not passed are present as null pointers or zero values.
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
fills a table of addresses in the same order. All take `env` first and
use the System V convention:

| Index | Function | Signature |
|---|---|---|
| 0 | `raise` | `(env, cond: i32)` |
| 1 | `enter` | `(env, code: i64)` |
| 2 | `leave` | `(env)` |
| 3 | `deref` | `(env, lo: i64, hi: i64) -> i64` |
| 4 | `copy` | `(dst, src, len)` |
| 5 | `mov_char` | `(dst, dst_len, src, src_len)` |

`enter` is called in every prologue with the procedure's own code
address, so the runtime can push a call-stack entry and identify the
procedure from the program's procedure table; `leave` is called before
every return. `cond` is the number of the `MCHnnnn` message generated
code raises: `0601`, `0603`, `1210`, `1211`, `3601`.

*Phase 4, pending*: decimal, string, date, pointer, I/O, message, data
area and data queue entries, which widen the table in later
checkpoints without renumbering.

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

*Phase 4, pending*: `monitor` regions dispatch from the poll to their
handler tables; condition handlers and cancel handlers run inside
`raise` and `leave`. This design needs no unwind tables; the statement
map (§8) serves the call stack display and dumps instead.

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
types, control flow, `callb` to local and imported procedures, `mov`
between same-size storage and between fixed `char` fields, loads and
stores of every register type, `parms`, `passed`, `omitted`,
`debug.stmt`. Everything else returns `CodegenError::Unsupported` and
arrives with the runtime library.
