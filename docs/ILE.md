# ILE: objects, binder, activation, exceptions

The `ile` crate implements the object payloads and the binder in this
document; `mi` implements the runtime model. Byte layouts are
little-endian unless a field is marked CP037; `ile/tests/spec.rs`
compares the code's layouts with the tables here, so both change
together. Sections marked *Phase 4, pending* describe parts that are
specified but not yet in the crates.

## 1. Object payloads

Every ILE object's payload is one byte space (`STORE_FORMAT.md` §10):
a fixed header, a section table, then the sections. A section table
entry is a `Section`; offsets are from the start of the payload.
Strings inside sections carry a `u32` length and are UTF-8; names are
CP037, 10 bytes, blank padded.

### `ModuleHdr` (64 bytes)

The start of a `*MODULE` payload.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | magic | `QS400MOD` |
| 8 | 4 | version | 1 |
| 12 | 4 | codegen_version | the code generator that made the native output |
| 16 | 2 | ir_version | the IR format version of the IR section |
| 18 | 1 | lang | 0 QIR, 1 RPGLE, 2 CLLE |
| 19 | 1 | dbgview | 0 `*NONE`, 1 `*STMT`, 2 `*SOURCE`, 3 `*LIST`, 4 `*ALL` |
| 20 | 8 | created | Unix seconds |
| 28 | 4 | section_count | entries in the table that follows |
| 32 | 4 | flags | bit 0 `*SRCSTMT`, bit 1 `*NODEBUGIO`, bit 2 `NOMAIN` |
| 36 | 28 | reserved | 0 |

### `Section` (24 bytes)

One per section, right after the header.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 4 | tag | which section |
| 4 | 4 | reserved | 0 |
| 8 | 8 | offset | from the payload start |
| 16 | 8 | len | bytes |

Module sections: `1` the IR (`QIR.md` §8), `2` the native output
(code, relocations, procedures with statement maps, the static frame
layout and its initial bytes, data exports; `ile/src/object/code.rs`),
`3` creation data (compiler, options, source file, library and member),
`4` the source view, as CP037 lines, when the compiler supplied it.
The module's exports and imports are not stored twice: they are read
from the IR (exported procedures and statics, `extern proc` and
`import` declarations).

### `PgmHdr` (128 bytes)

The start of a `*PGM` or `*SRVPGM` payload.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | magic | `QS400PGM` |
| 8 | 4 | version | 1 |
| 12 | 1 | kind | 0 `*PGM`, 1 `*SRVPGM` |
| 13 | 1 | actgrp_kind | 0 `*NEW`, 1 `*CALLER`, 2 named, 3 `DFTACTGRP(*YES)` |
| 14 | 1 | usrprf | 0 `*USER`, 1 `*OWNER`; `MIGRATE(phase6)` |
| 15 | 1 | useadpaut | `MIGRATE(phase6)` |
| 16 | 1 | user_state | 1 user state, 0 system state |
| 17 | 1 | user_domain | 1 user domain, 0 system domain |
| 18 | 1 | alwupd | `ALWUPD(*YES)` |
| 19 | 1 | alwlibupd | `ALWLIBUPD(*YES)` |
| 20 | 1 | observable | the IR is kept for retranslation |
| 21 | 1 | lvlchk | callers check the signature |
| 22 | 2 | reserved | 0 |
| 24 | 10 | actgrp_name | CP037, for a named activation group |
| 34 | 6 | tgtrls | CP037, `V1R0M0` or `*CURRENT` |
| 40 | 8 | created | Unix seconds |
| 48 | 4 | codegen_version | the code generator the native code came from |
| 52 | 4 | module_count | |
| 56 | 4 | srvpgm_count | |
| 60 | 4 | import_count | |
| 64 | 4 | export_count | |
| 68 | 4 | signature_count | |
| 72 | 8 | static_size | bytes of the program static area, a multiple of 16 |
| 80 | 4 | entry_module | bound module index of the entry procedure, `0xFFFFFFFF` for none |
| 84 | 4 | entry_proc | procedure index in that module |
| 88 | 4 | section_count | |
| 92 | 36 | reserved2 | 0 |

Program sections: `1` the bound modules (each: library name, frame
offset, and the complete `*MODULE` payload, so a program carries its
own copies as IBM's do and `UPDPGM` replaces one), `2` the service
programs as `SrvPgmRef` records, `3` the import resolution (importing
module, symbol, kind, target: a bound procedure, a static area offset,
a service program export, or unresolved), `4` the exports of a service
program (symbol, kind, module, procedure index or static area offset),
`5` the signatures, 16 bytes each, current first, `6` the binder
source.

### `SrvPgmRef` (36 bytes)

A service program a program is bound to.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 10 | name | CP037 |
| 10 | 10 | library | CP037, `*LIBL` when bound through the library list |
| 20 | 16 | signature | the signature bound against |

### `BndDirHdr` (16 bytes)

The start of a `*BNDDIR` payload, followed by `count` entries.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | magic | `QS400BND` |
| 8 | 4 | version | 1 |
| 12 | 4 | count | entries |

### `BndDirEntry` (24 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 10 | name | CP037 |
| 10 | 10 | library | CP037 or `*LIBL` |
| 20 | 1 | kind | 0 `*SRVPGM`, 1 `*MODULE` |
| 21 | 1 | activation | 0 `*IMMED`, 1 `*DEFER` |
| 22 | 2 | reserved | 0 |

`ADDBNDDIRE` appends or inserts at `POSITION`, ignoring an entry that
is already there; `RMVBNDDIRE` removes by name and kind, and by
library when one is given.

## 2. The binder

`CRTPGM` and `CRTSRVPGM` bind in this order:

1. The modules listed. Every exported procedure and static of every
   listed module becomes a candidate; a symbol exported twice is
   `CPD5D04`, a warning, and the first module's export is used.
2. For each import still unresolved, the binding directories in the
   order given, entry by entry. A `*SRVPGM` entry whose service
   program exports the symbol binds that service program: the
   program records its name, the library (`*LIBL` when the entry said
   so) and the service program's current signature, and the import
   resolves to the export by name. A `*MODULE` entry whose module
   exports the symbol copies the module in at the end of the module
   list; its own imports are resolved in turn.
3. `QUSAPIBD` in `QSYS`, when it exists, as the last directory.

An import nothing resolves is `CPD5D02`; the object is not created
unless `OPTION(*UNRSLVREF)`, in which case the import is recorded as
unresolved and a call to it fails at run time. A program needs a
module with a program entry procedure, `ENTMOD` or the first that has
one; none is `CPD5D06`. A service program has no entry.

The program static area is every bound module's static frame in
module order, each rounded up to 16 bytes; a module's frame offset is
fixed at bind time and reaches its code through the `StaticFrame`
relocation (`ABI.md` §5). A data import resolves to an offset in the
static area.

**Exports of a service program.** `EXPORT(*ALL)` exports every
exported procedure and static of every bound module, in module order
then declaration order. `EXPORT(*SRCFILE)` takes the binder source
(§3): the `*CURRENT` block's symbols in its order, each of which must
exist (`CPD5D03`).

**Signatures.** A signature is 16 bytes. A generated signature is the
first 16 bytes of the SHA-256 of the ordered export names, each
followed by a newline (the algorithm is ours; the behaviour, that the
signature changes when the export list does, is IBM's). An explicit
`SIGNATURE('text')` is the text, CP037, blank padded to 16; `X'…'` is
32 hexadecimal digits. The service program keeps the `*CURRENT`
signature first and the `*PRV` blocks' signatures after it, and with
`LVLCHK(*YES)` accepts a program bound to any of them. *Phase 4,
pending*: at activation a program whose recorded signature the service
program does not accept gets `MCH4431`.

**`UPDPGM` and `UPDSRVPGM`** replace the named bound modules with the
current `*MODULE` objects of the same names and bind again with the
object's own attributes and binder source; modules not named keep
their bound copies, and directories are not consulted.

## 3. The binder language

A source member of `STRPGMEXP` blocks:

```
STRPGMEXP PGMLVL(*CURRENT) SIGNATURE('ACCOUNTS V2') LVLCHK(*YES)
  EXPORT SYMBOL('OPENACCT')
  EXPORT SYMBOL("closeAcct")
ENDPGMEXP
STRPGMEXP PGMLVL(*PRV)
  EXPORT SYMBOL('OPENACCT')
ENDPGMEXP
```

Keywords are case-insensitive; `/* */` comments are skipped; symbol
names keep their case. `PGMLVL` defaults to `*CURRENT`; exactly one
`*CURRENT` block is required. `SIGNATURE(*GEN)` is the default.

## 4. Activation

**Loading.** A program is loaded once per job: every bound module's
code is placed in memory, the service programs it is bound to are
loaded first (recursively) and each must accept the signature the
program was bound with, else the call fails with `MCH4431`; then the
relocations are applied: runtime entries from the call table, the
module's frame offset, bound procedures by address, service program
exports by name, data imports by static area offset. An import the
binder left unresolved points at a runtime entry that raises
`MCH3401` when called.

**Groups.** A job starts with the default activation group. A dynamic
program call chooses the group from the program's attribute: `*NEW`
creates a group for the call and destroys it on return; `*CALLER` is
the caller's group; a named group is found or created; `DFTACTGRP(*YES)`
is the default group. A bound call into a service program activates
it, on first use, in the group its own attribute names (`*CALLER`
being the calling program's group).

**Activations.** A program's activation in a group is its static
area: every module's frame at its bind-time offset, initialized from
the module's initial bytes. The activation lives until the group is
destroyed. One environment record serves the job; the runtime's
`enter` points its static area at the activation of the program that
owns the procedure being entered, and `leave` restores the caller's,
so a call across programs needs no stub and every static access in
generated code goes through the current activation (`ABI.md` §2).

**`RCLACTGRP`.** A named group is destroyed when no call-stack entry
runs in it; its static areas are freed, and a pointer into them fails
afterwards with `MCH3402`. `*ELIGIBLE` destroys every such named
group. *Phase 4, pending*: files opened and commit definitions scoped
to the group are closed and rolled back first, heaps freed last.

**Heaps.** Every activation group has a default heap, where `alloc`
(`%ALLOC`) takes storage, and `CEECRHP` adds user heaps to it. Each
block is its own space: a pointer to a freed block, or to the old
storage after `realloc`, fails with `MCH3402`; `dealloc` of a pointer
that is not a heap block is `MCH3601`. The storage never moves until
`realloc` replaces it. `CEEDSHP` discards a user heap with everything
in it, and `RCLACTGRP` frees every heap of the group.

**User spaces.** `QUSPTRUS` maps a `*USRSPC` into the job once, as a
copy of its bytes that is a space of its own, and returns a space
pointer to it; the copy goes back to the object when the job syncs
its user spaces. Deleting the object unmaps it, so a pointer into it
fails with `MCH3402` afterwards. The object store behind the mapping
is a trait (`UserSpaceSource`); slic supplies the store-backed one.
*Phase 4, pending*: pinning the object's pages instead of copying,
with writes through the WAL.

**Entry procedures.** A program's entry procedure receives a parameter
list: the count and an array of parameter addresses (`ABI.md` §3).
`%PARMS` is the count; a parameter not passed reads as a null pointer
and `MCH3601` when touched. Native programs (system APIs implemented
in Rust) are registered by name and found before any object when the
library is `QSYS` or `*LIBL`.

## 5. Exceptions

The runtime model so far is in `ABI.md` §7: percolation by return
with a poll after every call, `monitor` dispatch blocks, resumption
after `raise`. *Phase 4, pending*: condition handlers, cancel
handlers, the function check and the inquiry message, with a state
diagram here.
