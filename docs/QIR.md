# QIR: the QS/400 intermediate representation

The IR every compiled program is translated to. The `qir` crate
implements this document: the types with their byte layouts, the
module structure, the text form and the verifier. Code generation is
`runtime/codegen/`, the runtime it calls is `runtime/mi/`. Sections marked *Phase 4,
pending* describe parts that are specified but not yet in the crate.

## 1. Design

Scalars live in typed **registers** (`%name`); everything with an IBM
byte layout lives in **storage** and is named by a **place** (`$var`,
`$rec.amt`, `$arr[%i]`, `[%p +16]:i32`). Integer, float, indicator and
pointer registers lower to machine registers. Decimal, string, date and
pointer instructions take places and lower to calls into the runtime
library; by-reference parameters are places in the caller's storage.
Control flow is a graph of labelled blocks ending in a terminator;
registers are mutable virtual registers that the verifier proves
assigned on every path before use. The IR is designed for RPG IV, CL
and later COBOL, not as a thin layer over the code generator.

## 2. Types and layouts

Multi-byte integers and floats are big-endian in storage, as on the
machine. Nothing is aligned except pointers, which sit on 16-byte
boundaries; a structure holding a pointer anywhere has alignment 16.

| Type | Bytes | Notes |
|---|---|---|
| `i8 i16 i32 i64` | 1 2 4 8 | two's complement |
| `u8 u16 u32 u64` | 1 2 4 8 | unsigned |
| `f32 f64` | 4 8 | IEEE 754 |
| `packed(d,s)` | `d/2 + 1` | `DECIMAL.md` §2; 1 ≤ d ≤ 63, s ≤ d |
| `zoned(d,s)` | `d` | `DECIMAL.md` §2 |
| `bin(d,s)` | 2, 4 or 8 | big-endian two's complement with `s` implied decimals; up to 4, 9 or 18 digits (RPG and DDS `B`) |
| `char(n)` | `n` | CP037, blank padded; 1 ≤ n ≤ 16773104 |
| `varchar(n)` | `2 + n` | big-endian length then data; n ≤ 65535 |
| `ucs2(n)` | `2n` | UCS-2 big-endian, blank (`0020`) padded |
| `varucs2(n)` | `2 + 2n` | length in code units; n ≤ 65535 |
| `graphic(n)` | `2n` | DBCS |
| `date(*FMT)` | 10, 8 or 6 | characters in `DATFMT` form; `*ISO *USA *EUR *JIS` 10, `*MDY *DMY *YMD` 8, `*JUL` 6 |
| `time(*FMT)` | 8 | `*HMS *ISO *USA *EUR *JIS` |
| `timestamp(p)` | 19 or `20 + p` | `yyyy-mm-dd-hh.mm.ss[.f…]`, 0 ≤ p ≤ 12 |
| `ind` | 1 | `'0'` (`F0`) or `'1'` (`F1`) |
| `ptr` | 16 | fat pointer, `ABI.md`; aligned 16 |
| `procptr` | 16 | procedure pointer; aligned 16 |
| `struct(size) { f: T @off, … }` | `size` | explicit offsets, overlays allowed; default size is the end of the last field |
| `array[n] of T` | `n × size(T)` | zero-based index in the IR |
| `bytes(n)` | `n` | opaque |

Register types: the integers, the floats, `ind`, `ptr`, `procptr`.
Every other type is storage-only.

Field offsets may be omitted in the text form: a field then follows the
previous one, rounded up to 16 when it holds a pointer. A field holding
a pointer at an offset that is not a multiple of 16 is rejected.

Default contents when no initializer is given: blanks for text, dates,
times and timestamps; packed `+0`, zoned `F0…C0`; zero for integers,
floats and pointers; `'0'` for indicators; aggregates take their
fields' defaults.

## 3. Modules

A module is one `*MODULE` IR unit:

```
module NAME lang(QIR|RPGLE|CLLE) ccsid(37) actgrp(*NEW|*CALLER|name)
       [dftactgrp] [nomain] [srcstmt] [nodebugio] dbgview(*NONE|*STMT|*SOURCE|*LIST|*ALL)
```

followed by declarations in any order: named types, module-scope
variables, files, prototypes and procedures. Procedure bodies may call
procedures declared later in the text.

**Variables** `$name` have a storage class:

| Class | Where | Text |
|---|---|---|
| `static` | the module's static frame, one instance per activation group | `static $x: T [inz(LIT)] [export]` |
| `auto` | the procedure's call frame | `auto $x: T [inz(LIT)]` |
| `based` | storage a `ptr` variable addresses | `based $x: T on $p` |
| `import` | another module's exported static, bound at bind time | `import $x: T` |
| `const` | read-only storage with a value | `const $x: T = LIT` |
| `param` | a procedure parameter | declared in the signature |

Initializers are literals encoded as the type's bytes (§7); the module
stores the bytes, so a disassembled module prints them as `x"…"`.

**Files** declare what the module opens:

```
file NAME file(OBJECT) lib(*LIBL) [dev(*DB|*DSP|*PRT)] usage(*INPUT|*OUTPUT|*UPDATE|*ALL)
     [keyed] [nolvlchk] fmt(FORMAT:"LEVELID")…
```

The level identifier is the 13-digit value from `DB_FORMAT.md`; the
check happens at open (`CPF4131`), not at activation.

**Prototypes** describe what a call reaches:

```
extern proc NAME(params) [-> T] [opdesc]          ; a bound procedure, resolved at bind time
extern pgm NAME(params) pgm(OBJECT) lib(*LIBL)     ; a program called dynamically
extern sysapi NAME(params) [-> T]                 ; a system API in QSYS
proto NAME(params) [-> T] [opdesc]                ; a signature: for calls through a procedure pointer, or by name as an import
```

**Parameters** are `[byval] [const] $name: T [*NOPASS] [*OMIT] [*VARSIZE]`.
By reference is the default. `*NOPASS` parameters come last; a
`*VARSIZE` parameter is text or an aggregate passed by reference, and a
`*VARSIZE` `bytes` parameter takes storage of any type (the runtime
library's operands, described by a separate word).

**Procedures**:

```
proc NAME(params) [-> T] [export] [entry] [opdesc] {
  auto $x: T                     ; declarations first
  static $y: T inz(0)
  based $z: T on $p
LABEL:                           ; then blocks, the first is the entry block
  instructions
}
```

At most one procedure is `entry`, the program entry procedure; its
parameters are by reference (the `*ENTRY PLIST`). A `nomain` module has
none.

## 4. Places, operands, literals

A place is a variable followed by field and index steps, or storage a
pointer register addresses:

```
$var  $rec.amt  $rows[%i].name  $flags[2]  [%p]:i32  [%p +16]:Rec.amt  [%p -4]:char(4)
```

Indexes are zero-based; a constant index past the array is rejected by
the verifier and a register index past it is `MCH0603` at run time.

An operand is a register, a place or a literal. Literals: integers
(`123`, `-5`), decimals (`123.45`), floats with an exponent (`1.5e3`),
strings (`"ABC"`, `""` doubles a quote; CP037 in storage), hex bytes
(`x"C1C2"`) and the figurative constants `*BLANKS *ZEROS *HIVAL *LOVAL
*NULL *ON *OFF`. A literal takes its type from the context: the other
operand, the destination place, the parameter it is passed to, or the
type named by `const.T`.

## 5. Instructions

Blocks end in exactly one terminator (`br`, `brif`, `switch`, `ret`,
`unreachable`). `%d = …` defines a register; a register may be assigned
in several places but always with the same type.

**Scalars**

| Instruction | Meaning |
|---|---|
| `%d = const.T lit` | a literal of a register type |
| `%d = load place` | a scalar from storage (`ind` loads as `ind`) |
| `store place, v` | a scalar to storage; `v` is a register or literal of the place's type |
| `mov place, src` | copy storage: same type; text to text of another length, blank padded or truncated; aggregates of equal size; a literal into storage |
| `%d = add\|sub\|mul\|div\|rem a, b` | integer or float arithmetic; `.chk` on `add sub mul` raises `MCH1210` on overflow instead of wrapping; integer `div`/`rem` by zero is `MCH1211` |
| `%d = and\|or\|xor\|shl\|shr a, b` | integers; `shr` is arithmetic for signed types; the shift count is any integer |
| `%d = neg\|abs\|not a` | `not` integers only |
| `%d = cmp.eq\|ne\|lt\|le\|gt\|ge a, b` | an `ind`; pointers and indicators compare for equality only |
| `%d = select c, a, b` | `a` when `c` is nonzero |
| `%d = cvt.T[.chk] a` | between register types; integer narrowing wraps unless `.chk` (`MCH1210`); float to integer truncates toward zero |

**Control flow**

| Instruction | Meaning |
|---|---|
| `br L` | |
| `brif c, Lt, Lf` | `c` is an `ind` or an integer, nonzero is true |
| `switch v, [k: L, …], default L` | integer `v`, distinct `k` |
| `ret [v]` | the value is a register for register return types, a place or literal otherwise |
| `unreachable` | a trap |

**Calls**

| Instruction | Reaches |
|---|---|
| `[%d =] callb NAME(args) [-> place]` | a procedure of this module or an `extern proc` |
| `[%d =] call NAME(args)` | an `extern pgm` prototype, or an untyped program by name |
| `call LIB/NAME(args)`, `call $place(args)` | an untyped program; the place is a `char(10)` name or a `char(20)` name and library |
| `[%d =] callpp %p as PROTO(args) [-> place]` | a `procptr` with a prototype's signature |
| `callp %p(args)` | a `ptr` (system pointer) to a program |
| `[%d =] sysapi NAME(args) [-> place]` | an `extern sysapi` |

Arguments: a place for a by-reference parameter (a literal is placed in
a temporary), a register, place or literal for a by-value one, `*OMIT`
where the parameter allows it, and trailing `*NOPASS` parameters may be
left out. `*VARSIZE` accepts any length of the parameter's class. A
return value goes to `%d` when its type is a register type and to
`-> place` otherwise. Untyped program calls pass every argument by
reference and return nothing.

`%d = parms` is the number of parameters passed (`%PARMS`, an `i32`);
`%d = passed $p` and `%d = omitted $p` test a parameter.

**Debug** pseudo-instructions carry the maps the debugger uses:
`debug.stmt n`, `debug.var "NAME", place`, `debug.scope begin|end`.

**Decimal arithmetic.** Operands are numeric places (packed, zoned,
integer or float), integer or float registers, or numeric literals;
the result is assigned to a numeric place. Every operation states the
precision of its intermediate result, which the front end derives from
`DECIMAL.md` §4; the exact result is truncated to it, and an integer
part that does not fit is `MCH1210`.

| Instruction | Meaning |
|---|---|
| `dec.add\|sub\|mul\|div\|rem dst, a, b prec(d,s) [halfadj] [resdecpos]` | the operation at `prec`, then assignment to `dst`, half-adjusted when flagged; `resdecpos` keeps at least `dst`'s decimal positions in the intermediate; `div` and `rem` by zero are `MCH1211` |
| `dec.mov\|neg\|abs dst, src [halfadj] [trunc]` | numeric assignment across any numeric types; `MCH1210` when the integer part does not fit, unless `trunc` drops the high-order digits that do not fit a packed or zoned target (fixed-form RPG arithmetic); `MCH1202` for invalid packed or zoned data |
| `%d = dec.cmp.cond a, b` | numeric comparison by value, an `ind` |
| `%d = cvt.T place` | a numeric place into an integer or float register, truncating decimals toward zero; `.chk` raises `MCH1210` when out of range |

**Conversions** between storage classes: `cvt dst, src [halfadj]
[datfmt(*X)] [timfmt(*X)] [ccsid(n)]`. Pairs: character to numeric with
the machine's blank and sign rules (`%DEC`, `%INT`) and numeric to
character (`%CHAR`); character to and from date, time and timestamp in
the format named by `datfmt`/`timfmt` (the character side's format; the
date side knows its own); numeric to and from date, time and timestamp
(`%DEC(d:*ISO)` gives `yyyymmdd`); date to timestamp, timestamp to date
or time; `char` to and from `ucs2`/`graphic` through `ccsid`; `ind` to
and from `char`. Invalid data is `MCH1202` for numbers and a date error
for dates. Same-class copies use `mov` and `dec.mov`.

**Strings.** Operands are string places or string literals; both sides
of an operation are the same class (`char`, `ucs2` or `graphic`).
Positions are 1-based; a position or length outside the source is
`RNX0100`, the RPG runtime's identifier (status 100); `MCH0603` is kept
for an array index out of range.

| Instruction | Meaning |
|---|---|
| `str.cat dst, a, b` | `a` then `b`, padded or truncated to `dst` |
| `str.subst dst, src, start, len` | `%SUBST` |
| `%d = str.scan needle, hay [, start]` | 1-based position, 0 when absent (`i32`) |
| `%d = str.check\|checkr set, src [, start]` | first (last) position of a character not in `set` |
| `str.xlate dst, from, to, src [, start]` | `%XLATE` |
| `str.trim\|triml\|trimr dst, src [, chars]` | `%TRIM` family |
| `%d = str.cmp.cond a, b` | blank-padded, EBCDIC collation for `char`, an `ind` |
| `%d = str.len src` | current length of a varying string, declared length otherwise |
| `str.setlen dst, n` | `%LEN(v) = n` on a varying string |
| `str.replace dst, src, repl, start [, len]` | `%REPLACE` |

**Dates, times and timestamps.** Units are `*YEARS *MONTHS *DAYS`
for dates, `*HOURS *MINUTES *SECONDS` for times, all seven with
`*MSECONDS` for timestamps.

| Instruction | Meaning |
|---|---|
| `dt.add dst, src, n, unit` | `src + n` units, `n` an integer and may be negative; `dst` has `src`'s type |
| `%d = dt.diff a, b, unit` | `a - b` in whole units (`i64`); both dates/timestamps or both times |
| `%d = dt.extract src, unit` | `%SUBDT` (`i32`) |
| `%d = dt.test src [datfmt(*X)] [timfmt(*X)]` | whether character or numeric data is a valid value (`ind`) |
| `dt.now dst` | the current value in `dst`'s type |

**Pointers.** `[%p +n]:T` derefs; the runtime rules for a bad pointer
are in `ILE.md`. A region is a static area, a heap block, a mapped user
space, or an entry's automatic storage as a whole (`ABI.md` §4).

| Instruction | Meaning |
|---|---|
| `%d = addr place` | a space pointer to the place |
| `%d = ptradd p, n` | `n` bytes on; leaving the region is `MCH0601` |
| `%d = ptrdiff p, q` | `p - q` in bytes (`i64`); different regions are `MCH0601` |
| `%d = cmp.cond p, q` | pointers compare for equality, and for order within one region |
| `%d = procaddr NAME` | a procedure pointer to a procedure of this module or an `extern proc` |
| `%d = rslvsp *TYPE, name, lib` | a system pointer to an object; `MCH3401` when absent |
| `%d = const.ptr *NULL` | a null pointer |
| `%d = alloc n` | `n` bytes from the activation group's default heap, zeroed (`%ALLOC`) |
| `%d = realloc p, n` | the storage resized to `n` bytes with its contents kept; the old pointer is dead (`%REALLOC`) |
| `dealloc p` | the storage freed (`DEALLOC`); a pointer not from `alloc` is `MCH3601` |

**Monitors.** `monitor NAME, sel -> L, sel -> L, …` opens a region
that `endmon NAME` closes; regions nest and are lexical over the
block list. Selectors, tried in order: `*ALL`; `*PROGRAM` (anything
but file errors); `*FILE`; a status code or range `100..999` in the
RPG numbering the runtime maps message identifiers to; a message
identifier `MCH1211`; a prefix `"MCH*"`. Control may leave a region
by any branch; it may enter only through the `monitor` instruction
and the handlers lie outside their region. An exception raised inside
goes to the first matching handler of the innermost region, with the
condition's message on the entry's queue (`ILE.md` has the model).

**I/O.** Every operation yields the status of its `IoFeedback` as an
`i32`: 0, or the five-digit RPG status (`00011` end of file, `00012` not
found, `01021` duplicate key, `01218` record locked, ...; `PHASE3.md`).
Level checks happen at `dbopen` (`CPF4131` as an escape message).

| Instruction | Meaning |
|---|---|
| `%d = dbopen FILE [infds(place)]` | open the declared file; the runtime keeps `infds` current |
| `%d = dbclose FILE` | |
| `%d = dbio FILE OP [rec(place)] [key(x)] [rrn(x)] [fmt(NAME)] [nolock]` | `READ READP READE READPE CHAIN SETLL SETGT WRITE UPDATE DELETE UNLOCK FEOD`; reads and writes need `rec`, positioning needs `key` or `rrn` |
| `%d = dbfb FILE` | the feedback of the database file's last operation as bits: 1 end of file, 2 found, 4 equal, 8 open (`%EOF`, `%FOUND`, `%EQUAL`, `%OPEN`); 0 for a file that is not open. For a display file bits 8-15 are the last read's attention identifier and bits 16-30 its subfile record number |
| `%d = dspio FILE OP fmt(NAME) [rec(place)] [rrn(x)] [inds(place)]` | `WRITE READ EXFMT CHAIN UPDATE READC` on a `dev(*DSP)` file: `WRITE` with `rrn` puts a subfile record at that number, `CHAIN` reads one by number, `UPDATE` replaces the one last read, `READC` reads the next changed one; a read whose `fmt` is not a record of the file reads the one last written that takes input; `inds` is the separate indicator area (`INDARA`), its response indicators set by a read |
| `%d = prtio FILE WRITE fmt(NAME) [rec(place)] [ctl(sb, sa, kb, ka)]` | print a record of a `dev(*PRT)` file: 0, or 1 when the carriage reached the overflow line; `ctl` gives the line's carriage control (space before, space after, skip before, skip after, integers, 0 for none), as RPG output does for a program-described file |

`dbopen` and `dbclose` also open and close a printer file: the open
creates its spooled file (`OPN`), the close makes it ready. On a
display file they open and close it through the job's display source,
and the information data structure gets the display part after a read:
the attention identifier at 368, the cursor row and column at 369, the
data length at 371, the subfile record number at 375, the lowest one
shown at 377, the record count at 379, the return code `0000` at 400.
A display operation of an RPG module keeps a failure as its status; a
CL module's is sent as an escape (and a CL file opens with its first
operation). An
externally described printer file prints the record format `fmt`
names from the record (option indicators first, one byte each, unless
the file has `INDARA`); a program-described one prints the record as
one line, its first byte the carriage control under `CTLCHAR(*FCFC)`.

A file is open per activation of the program; it closes with
`dbclose` or when the activation group is reclaimed. An open that
fails sends its message (`CPF9801` not found, `CPF4131` level check)
as an escape and returns the status; a failing operation only returns
it. The record place is filled on a read (padded with blanks, or
truncated) and taken whole on a write or update. A key operand holds
the key fields in record layout, as many as it has room for; `rrn`
positions by relative record number. A negative `rrn` positions at a
limit: -1 `*START`, -2 `*END`, -3 `*LOVAL`, -4 `*HIVAL`; `rrn(0)` on
`READE`/`READPE` compares with the key of the current record. `READC`
at the end of the subfile returns `00011`.

The information data structure, when given, is kept current after
every operation: file name at 0 (8), open flag at 8, end-of-file flag
at 9, the five-digit status at 10, the operation code at 15 (6), the
routine at 21 (8), the statement at 29 (8), the record format at 37
(8), the message identifier at 46 (7), the member at 128 (10), the
relative record number at 396 (`bin4`) and the encoded key from 400.
Only the fields that fit the place are written.

`dtaara.read` and `dtaara.write` copy a character area into or out
of a character place and convert a decimal area to or from a packed
or zoned place; a missing area is `CPF1015` as an escape. `dtaq.rcv`
returns 0 when the queue is empty; a missing queue is `CPF9801`.
`rslvsp` yields a system pointer whose identifier the object source
assigned, and `callp` through it calls the program it names.

**Debug.** `debug.stmt n` marks the statement the following code
belongs to; `debug.var NAME, place` names a place for `EVAL`;
`debug.scope` brackets nested scopes. The production translation only
records statements in its map and in `job.cur_stmt`; the debug
translation (`DBGVIEW` other than `*NONE`, `ILE.md` §2) also calls the
runtime at every statement and registers every `debug.var` with the
running call-stack entry, so a job under `STRDBG` stops at breakpoints
and steps, and `EVAL` reads the variables the entry registered.

**Messages.** Queues are `*SAME` (this call-stack entry), `*PRV`,
`*CTLBDY`, `*EXT`, `*SYSOPR`, or a count of entries up the stack.

| Instruction | Meaning |
|---|---|
| `sndpgmmsg msgid(x) msgf(x [, lib]) [data(x)] type(*T) to(*Q) [key(place)]` | send; `msgid` is a `char(7)`, `msgf` a `char(10)`, `key` receives the `char(4)` key |
| `%d = rcvmsg pgmq(*Q) [type(*T)] [key(x)] [rmv] [msg(p)] [msgid(p)] [data(p)] [keyout(p)] [sev(p)]` | receive the oldest message of the type (or the keyed one); an `ind` saying whether one was there |
| `rmvmsg pgmq(*Q) [key(x)] [clear(*ALL\|*OLD\|*NEW\|*KEEPUNANS)]` | remove one or clear |

Message types: `*INFO *COMP *DIAG *ESCAPE *STATUS *NOTIFY *RQS *INQ
*RPY *SCOPE`, plus `*ANY` to receive.

**Console, data areas, data queues.**

| Instruction | Meaning |
|---|---|
| `dsply src [reply(place)]` | echo on the console; wait for a reply into the place |
| `dtaara.lock\|unlock name, lib` | allocate or release a data area |
| `dtaara.read\|write name, lib, place` | the whole data area to or from the place |
| `dtaq.snd name, lib, data, len` | send an entry |
| `%d = dtaq.rcv name, lib, place, len, wait` | receive into the place; the length received (`i32`), 0 after `wait` seconds (negative waits forever) |

Names are `char(10)` places or literals; `"*LIBL"` is a library.

## 6. The verifier

Rejects a module when: a type cannot be laid out (§2); a name is
declared twice or does not resolve; a `based` variable is not on a
`ptr`; an initializer is not the type's size; an `*NOPASS` parameter is
followed by one that is not; an entry procedure is by value or returns;
a register holds a storage-only type, or is assigned two types; a block
lacks a terminator or has one before its end; a branch names no block;
a register may be read before it is assigned on some path (a forward
dataflow over the block graph, in which a handler sees the
intersection of the entry states of the blocks its region overlaps,
the opening block contributing its state at the `monitor`);
an operand's type does not fit the instruction (§5); a call passes the
wrong count, `*OMIT` where not allowed, a register by reference, or a
type the parameter is not; a decimal operation omits its precision;
monitors do not nest, a branch enters one, or a handler lies inside
its own region; `dbio` lacks the record or key its operation needs.

## 7. Literal encoding

A literal becomes the bytes of its type: integers and decimals through
`qdecimal`'s packed, zoned, integer and float encoders (truncated to the
scale, size error if the integer part does not fit); strings through
CP037 padded with blanks (`varchar` with its length, `ucs2` big-endian
padded with `0020`); hex bytes padded with zeros to the size;
`*BLANKS` is `40` (or `0020`), `*ZEROS` is `F0` for text and
indicators and numeric zero otherwise, `*HIVAL`/`*LOVAL` the largest
and smallest values (`FF`/`00` for text), `*NULL` sixteen zero bytes,
`*ON`/`*OFF` `F1`/`F0`.

## 8. Format version

`FORMAT_VERSION` is 3 (version 2 added `prtio`'s `ctl`, at the end of
the device I/O record, and `dec.mov`'s `trunc`, at the end of its;
version 3 the type `bin(d,s)`, tag 26, and `dspio`'s `CHAIN`, `UPDATE`
and `READC` as new tags with its `rrn` and `inds` at the end of the
device I/O record). `*MODULE` objects store the binary IR that
`qir::binary::write` produces: the magic `QIRM`, a `u16` version, a
`u16` of zero, then the module as tagged fields in a fixed order
(little-endian integers, `u32`-prefixed strings and byte arrays,
`u32`-counted lists, `u8` presence flags and enumeration tags; the
tag tables are in `runtime/qir/src/binary/`). Within V1R the format only
grows: a new instruction or type takes a new tag, a new field goes at
the end of its record behind a version bump, and a reader accepts
every version up to its own and refuses a later one.

## 9. `qirc`

```
cargo run -p qir --bin qirc -- asm IN.qir [-o OUT.qirm] [--no-verify]
cargo run -p qir --bin qirc -- dis IN.qirm [-o OUT.qir]
cargo run -p qir --bin qirc -- check IN.qir|IN.qirm
cargo run -p qir --bin qirc -- fmt IN.qir [-o OUT.qir]
```

`asm` verifies before writing; `dis` prints the canonical text, which
is what `fmt` prints for the source, so `fmt` is the normal form and a
module survives text → binary → text unchanged. The service console
gets the same functions as `qir asm` in a later checkpoint.
