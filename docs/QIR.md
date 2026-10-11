# QIR: the QS/400 intermediate representation

The IR every compiled program is translated to. The `qir` crate
implements this document: the types with their byte layouts, the
module structure, the text form and the verifier. Code generation is
`codegen/`, the runtime it calls is `mi/`. Sections marked *Phase 4,
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
file NAME file(OBJECT) lib(*LIBL) usage(*INPUT|*OUTPUT|*UPDATE|*ALL) [keyed] [nolvlchk]
     fmt(FORMAT:"LEVELID")…
```

The level identifier is the 13-digit value from `DB_FORMAT.md`; the
check happens at open (`CPF4131`), not at activation.

**Prototypes** describe what a call reaches:

```
extern proc NAME(params) [-> T] [opdesc]          ; a bound procedure, resolved at bind time
extern pgm NAME(params) pgm(OBJECT) lib(*LIBL)     ; a program called dynamically
extern sysapi NAME(params) [-> T]                 ; a system API in QSYS
proto NAME(params) [-> T] [opdesc]                ; a signature for calls through a procedure pointer
```

**Parameters** are `[byval] [const] $name: T [*NOPASS] [*OMIT] [*VARSIZE]`.
By reference is the default. `*NOPASS` parameters come last; a
`*VARSIZE` parameter is text or an aggregate passed by reference.

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

*Phase 4, pending*: decimal arithmetic (`dec.*`), conversions (`cvt`
between storage types), strings (`str.*`), pointers (`addr`, `ptradd`,
`ptrcmp`, `nullptr`, `procaddr`), dates (`dt.*`), `monitor`, database
and display I/O, messages, `dsply`, data areas, data queues.

## 6. The verifier

Rejects a module when: a type cannot be laid out (§2); a name is
declared twice or does not resolve; a `based` variable is not on a
`ptr`; an initializer is not the type's size; an `*NOPASS` parameter is
followed by one that is not; an entry procedure is by value or returns;
a register holds a storage-only type, or is assigned two types; a block
lacks a terminator or has one before its end; a branch names no block;
a register may be read before it is assigned on some path (a forward
dataflow over the block graph); an operand's type does not fit the
instruction (§5); a call passes the wrong count, `*OMIT` where not
allowed, a register by reference, or a type the parameter is not.

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

`FORMAT_VERSION` is 1. `*MODULE` objects store the binary IR (*Phase 4,
pending*: `qirc`); within V1R the format may only grow, so a reader of
version *n* reads every version up to *n*.
