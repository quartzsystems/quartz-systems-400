# The CL compiler

`command/cl` compiles ILE CL source to the IR of Phase 4; `CRTCLMOD`,
`CRTBNDCL` and `CRTCLPGM` run it in the storage service against the
commands the source names. A CL program is an ordinary `*PGM` whose
module says `lang(CLLE)`: it is called, debugged, bound and dumped
like any other.

## Source

Statements are logical lines, `+` and `-` continuing, `/* */`
comments, an optional `LABEL:` first. A statement is a CL statement
(`PGM`, `ENDPGM`, `DCL`, `DCLF`, `DCLPRCOPT`, `CHGVAR`, `IF`, `ELSE`,
`DO`, `ENDDO`, `DOWHILE`, `DOUNTIL`, `DOFOR`, `LEAVE`, `ITERATE`,
`SELECT`, `WHEN`, `OTHERWISE`, `ENDSELECT`, `GOTO`, `CALL`, `TFRCTL`,
`CALLPRC`, `RETURN`, `MONMSG`, `SUBR`, `ENDSUBR`, `CALLSUBR`,
`RTNSUBR`, `RCVF`, `SNDF`, `SNDRCVF`, `WAIT`, `ENDRCV`, `CLOSE`,
`RTVDTAARA`, `CHGDTAARA`, `DMPCLPGM`) or a command, which the command
analyzer binds with the program's variables known (`docs/CMD.md`).
`SNDPGMMSG`, `RCVMSG`, `RMVMSG` and `SNDUSRMSG` are commands the
compiler turns into the IR's message instructions.

Expressions: `+ - * /`, `*CAT *BCAT *TCAT` (`||`, `|<`, `|>`), the
relational operators (`*EQ` and `=`, `*NE` and `¬=`, `*GT` `>`,
`*LT` `<`, `*GE` `>=`, `*LE` `<=`, `*NG`, `*NL`), `*NOT` (`¬`), `*AND`
(`&`), `*OR` (`|`), parentheses; the built-ins `%SST`/`%SUBSTRING`,
`%BIN`/`%BINARY`, `%CHAR`, `%DEC`, `%INT`/`%UINT`, `%LEN`, `%SIZE`,
`%SCAN`, `%CHECK`/`%CHECKR`, `%TRIM`/`%TRIML`/`%TRIMR`, `%UPPER`,
`%LOWER`, `%PARMS`, `%SWITCH`, `%ADDRESS`. A `%SST` or `%BIN` can be
the target of `CHGVAR`. Character results keep IBM's lengths: a
variable its declared length with its blanks, a literal and a
built-in's result their exact length, so `'a' *CAT %CHAR(&N)` has no
blank between.

Variables: `*CHAR`, `*DEC`, `*LGL`, `*INT`, `*UINT`, `*PTR`, with
`STG(*AUTO)`, `STG(*BASED) BASPTR(&P)` and `STG(*DEFINED) DEFVAR(&V
pos)`. `DCLF` declares a file's record formats' fields as variables
(`Files` describes them; the service's reads the store, the kit's a
fixed table).

## What the compiler produces

`cl::compile(source, options, env, files)` gives the IR text, the
listing, the diagnostics and the tables. The listing has the
prologue (program, source, options, debug view, activation group),
the source with statement numbers and nesting depth and each
diagnostic under its statement, the cross-reference of variables and
labels, the message summary and the end line. Errors stop creation;
`OPTION(*NOGEN)` stops it too.

Each statement is a block `Sn` with `debug.stmt n` (unless
`DBGVIEW(*NONE)`), so breakpoints and steps land on CL statements.
Control flow is a stack of frames: `DO` groups, loops (with their
`LEAVE` and `ITERATE` targets), `SELECT`s, subroutines. A `MONMSG`
after a command wraps the statement in a monitor region whose
handlers follow the statement; the program-level `MONMSG`s (before
the first command) are added to every command's region. A
subroutine is a block entered with a return index on a small stack;
`RTNSUBR` and `ENDSUBR` go through one `switch`.

A command takes the direct path when its processing program is a
system one and every value is a constant or a bare variable: the
block is a structure type with the constants in place, the variables
patched in before `call QSYS/<CPP>(segments...)`, and the return
values copied out after. Otherwise the run-time path: the command
string with `&n` placeholders and the values that fill them, handed
to `QCLEXEC`, which substitutes, analyzes and runs it in the job.
`CALL` and `TFRCTL` are the IR's calls; `RCVF` is `dbopen` on first
use and `dbio READ` with `CPF0864` at the end; a display file's
`SNDRCVF` is `dspio` and, until Phase 8 brings displays, ends in
`CPF4101`; `RTVDTAARA` and `CHGDTAARA` are the data area
instructions, `*LDA` included.

`MONMSG` semantics: `CPF0000` and `CPF9999`-style generic identifiers
are prefix selectors; an escape inside the region goes to the first
matching handler; `EXEC` runs the handler's command, without `EXEC`
the message is handled and the next statement runs; a `GOTO` in a
handler leaves the region. An escape no `MONMSG` takes becomes a
function check (`CPF9999`) with the inquiry `CEE9901`, as any
program's does. `CMPDTA` is accepted but not compared in this
release.

## The commands

`CRTCLMOD`, `CRTBNDCL`, `CRTCLPGM` read the source member, collect
the definitions of every word that could be a command through the
job's library list, compile, spool the listing to `QSYSPRT`, verify
the IR, and create the module and (not `CRTCLMOD`) the program, with
`REPLACE(*YES)` by default. `ALWRTVSRC(*YES)` keeps the source in the
module; `RTVCLSRC` writes it back to a source member. `LOG(*YES)`
and `*JOB` are recorded on the module (the job log's logging of each
command run is the job's `LOGCLPGM`). The startup program
`QSYS/QSTRUP` is CL source (`lic/slic/src/stgsvc/shipped/QSTRUP.clle`)
compiled at the storage service's first start.

## The corpus

`tools/conformance/cl/programs/*.clle` are compiled and run in the IR
kit's job with a test double for every processing program
(`conformance::cl`): the double logs the command in keyword form,
fills return values with marks, and escapes where the case's header
says (`/*; fail: CRTLIB CPF2111 */`). `expected/NAME.out` holds the
console, the messages raised, handled and pending, the parameters and
the external queue; `expected/NAME.lst` the listing. `cargo test -p
conformance --test cl_kit` compares; `QS400_UPDATE_CL=1` rewrites.

## Not in this release

`CMPDTA` on `MONMSG`; `DCLPRCOPT` beyond parsing; `%OFFSET`; `CALLPRC`
with a return value to a bound procedure of another module (the IR's
`callb` only); `WAIT` and `ENDRCV` on display files; `TFRCTL`'s
activation group semantics (a plain call that returns); `OPTION(*SRC)`
listings of included members; `TGTRLS`, `SRCSTMF`, `SQL` statements
inside CL.
