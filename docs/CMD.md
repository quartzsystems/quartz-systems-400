# Command definitions and the command analyzer

Phase 7 makes commands objects. A `*CMD` holds a definition compiled
from command definition source; the analyzer reads a command string
against it, binds every parameter, converts the values and builds the
parameter block the processing program receives; the prompter shows
the definition as the F4 displays. The IBM names are the interface:
the statements and keywords of the source, the parameter types and
their encodings, the message identifiers, the screen geometry. The
text is ours.

## 1. Command definition source

The source is the statements `CMD`, `PARM`, `ELEM`, `QUAL`, `DEP` and
`PMTCTL`, one per logical line, `+` and `-` continuing a line, `/* */`
comments. Labels (`Q1:`) name an `ELEM` or `QUAL` group and a `PMTCTL`
statement; the group runs to the next labelled statement.

`CMD` carries `PROMPT`, `MAXPOS`, `ALLOW`, `MODE`, `PMTOVRPGM`,
`PMTOVRLIB`, `MSGF`, `HLPPNLGRP`, `HLPID`, `THDSAFE`, `TEXT`. `PARM`,
`ELEM` and `QUAL` carry `KWD` (`PARM` only), `TYPE`, `LEN`, `RTNVAL`,
`RSTD`, `DFT`, `VALUES`, `REL`, `RANGE`, `SPCVAL`, `SNGVAL`, `MIN`,
`MAX`, `ALWUNPRT`, `ALWVAR`, `PGM`, `DTAARA`, `FILE`, `FULL`, `EXPR`,
`VARY`, `PASSATR`, `PASSVAL`, `CASE`, `CCSID`, `LISTDSPL`, `DSPINPUT`,
`CHOICE`, `CHOICEPGM`, `PMTCTL`, `PMTCTLPGM`, `KEYPARM`, `INLPMTLEN`,
`PROMPT`. `DEP` carries `CTL`, `PARM`, `NBRTRUE`, `MSGID`; `PMTCTL`
carries `CTL`, `COND`, `NBRTRUE`, `LGLREL`.

Types: `*DEC`, `*LGL`, `*CHAR`, `*NAME`, `*SNAME`, `*CNAME`,
`*PNAME`, `*GENERIC`, `*DATE`, `*TIME`, `*HEX`, `*ZEROELEM`,
`*INT2`, `*INT4`, `*UINT2`, `*UINT4`, `*VARNAME`, `*X`, `*CMD`,
`*CMDSTR`, `*NULL`, or the label of a group. `LEN` is the digits and
decimal positions of a `*DEC`, the characters of the others; its
defaults are IBM's (`*DEC` 15 5, `*CHAR` 32, the name types 10,
`*VARNAME` 11, `*X` 5000 and so on).

The compiler (`cmd::source::compile`) checks the source with the rules
of `command/cmd/rules.toml`; each rule has a message identifier, so a
refusal reads as `CPD02nn` with the rule's text. The rules: one `CMD`
statement (CPD0237); statements and keywords known (CPD0243, CPD0242);
values of the right form (CPD0244); at most 99 parameters (CPD0212);
keywords well-formed, unique and present (CPD0213, CPD0214, CPD0211);
`MAXPOS` within the parameters (CPD0215); types and labels resolved
(CPD0216, CPD0217, CPD0218); lists at most three deep, qualified names
at most five parts, lists at most 300 elements (CPD0219, CPD0220,
CPD0221); `LEN` valid for the type and decimal positions within the
digits (CPD0222, CPD0223); `RTNVAL(*YES)` only on `*CHAR`, `*DEC`,
`*LGL` and without defaults or restrictions (CPD0224, CPD0225);
`*CMDSTR` and `*CMD` without lists or restrictions (CPD0226, CPD0227);
`MIN` at most `MAX`, `MAX` at most 300 (CPD0228); no default on a
required parameter (CPD0229); `RSTD(*YES)` with something to restrict
to (CPD0230); `REL` or `RANGE`, not both, and ordered (CPD0231,
CPD0232); `SNGVAL` on lists and groups only (CPD0233); `PMTCTL` and
`DEP` naming what exists (CPD0234, CPD0235, CPD0236); a group without
a parameter using it (CPD0253).

## 2. The `*CMD` object

The payload is a 256-byte header followed by three sections: the
definition (the `CmdDef` model, Wire-encoded), the source text, and
the `CHGCMDDFT` overlay (keyword and new default, one per changed
parameter). `effective()` applies the overlay to the definition. A
proxy command (`CRTPRXCMD`) has `FLAG_PROXY` set and names its target
in `TargetName`/`TargetLib`; the analyzer follows proxies up to five
deep.

### `CmdHeader` (256 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | Magic | `QS400CMD` |
| 8 | 4 | Version | 1 |
| 12 | 4 | Flags | bit 0 proxy, bit 1 overlay present |
| 16 | 10 | CppName | processing program |
| 26 | 10 | CppLib | its library |
| 36 | 10 | TargetName | proxy target |
| 46 | 10 | TargetLib | its library |
| 56 | 2 | Allow | `ALLOW` bits |
| 58 | 1 | Mode | `MODE` bits |
| 59 | 1 | MaxPos | `MAXPOS` |
| 60 | 1 | ThdSafe | `THDSAFE` |
| 61 | 1 | AlwLmtUsr | `ALWLMTUSR` |
| 62 | 2 | Reserved | |
| 64 | 8 | Created | seconds since the epoch |
| 72 | 4 | DefOffset | definition section |
| 76 | 4 | DefLen | |
| 80 | 4 | SrcOffset | source section |
| 84 | 4 | SrcLen | |
| 88 | 4 | DftOffset | overlay section |
| 92 | 4 | DftLen | |
| 96 | 160 | Reserved2 | |

## 3. The analyzer

`cmd::analyze(env, text)` tokenizes the string (`cmd::token`: words,
numbers, quoted strings, `X'..'`, `&VAR`, operators, `%BUILTIN(...)`,
`KWD(args)`, parentheses, the selective prompting prefixes `?`, `??`,
`?*`, `?<`, `?-`, `?&`, `?%`, `?/`), finds the command through the
job's library list (`CmdSource`), and binds the parameters: keywords
first, then the positional values up to `MAXPOS`. Each value is
converted for its type (names upper-cased and checked, numbers
decoded with the job's decimal format, dates and times with its
formats, restrictions `VALUES`, `REL`, `RANGE`, `SPCVAL`, `SNGVAL`
applied, `&VAR` accepted only where a CL program's variables are
known). `DEP` statements then run over the bound values. The result,
`Resolved`, carries the definition, the bound values in definition
order, the label and the prompt flag.

Every refusal is a diagnostic `CPD00nn` followed by the escape
`CPF0001`: CPD0010 a positional value beyond `MAXPOS`; CPD0012 an
unknown keyword; CPD0013 a value where a list is not allowed; CPD0014
and CPD0015 a required parameter missing; CPD0016 too many values;
CPD0020 a keyword given twice; CPD0030 the command not found;
CPD0031 the command not allowed in this setting (`ALLOW`); CPD0032
not allowed in this mode; CPD0043 not allowed to a limited user;
CPD0061 a value outside `RANGE`; CPD0074 a value too long; CPD0078 a
name not valid; CPD0084 a value not valid for the type or not of the
restricted set; CPD0093 a value outside `REL`; CPD0094 a `DEP`
relation not met. CPF0005 refuses a string over 32 702 characters,
CPF0006 anything the analyzer itself cannot do.

Where IBM's definitions say `EXPR(*YES)` on nearly every parameter
and ours leave it out, a character, numeric, logical or name-typed
scalar takes an expression anyway: constants are evaluated at
analysis (`'a' *CAT 'b'`), variables defer the expression to the CL
compiler (`Value::Expr`). Where a `RTNVAL(*YES)` character parameter
is longer than 100, a CL variable of any length receives it (message
data, replies); otherwise the variable must be at least as long.

## 4. The parameter block

The processing program is called with one address per parameter, in
definition order (`block::build`). A parameter of `TYPE(*NULL)`, or
omitted with `PASSVAL(*NULL)`, is passed as address 0. Otherwise the
segment is:

| Parameter | Layout |
|---|---|
| `PASSATR(*YES)` | one byte first: `0x80` specified, `0x40` the default taken, `0x20` null |
| `*DEC` | packed decimal of `LEN` digits and positions |
| `*INT2`, `*UINT2` | 2 bytes, big-endian |
| `*INT4`, `*UINT4` | 4 bytes, big-endian |
| `*LGL` | one byte, `F0` or `F1` |
| `*X` | `LEN` characters, a number as its digits |
| character types | `VARY` prefix (2 or 4 bytes big-endian) when asked, then `LEN` characters of CP037, blank padded |
| qualified name | the parts in definition order, each a fixed field |
| group (`ELEM`) | a 2-byte element count, then each element as its type says; an omitted group is a count of 0 |
| list | a 2-byte count, then the elements; a list of groups puts a 2-byte offset table (from the count) between |

`block::read` decodes a block back into values; `block::segment_len`
tells a program called with raw addresses how far a segment reaches.
The CL compiler lays the block out as a structure type with every
constant in place and the variables patched in at run time
(`Patch`); return values (`RTNVAL(*YES)`) come back the same way.

### `Block` constants

| Name | Value |
|---|---|
| ATR_SPECIFIED | 0x80 |
| ATR_DEFAULT | 0x40 |
| ATR_NULL | 0x20 |

## 5. The prompter

The prompt display is 24 by 80: the title (`PROMPT` text and the
command name in parentheses) centred on row 1, `Type choices, press
Enter.` on row 3, the parameters from row 5, one per row (a group's
elements and a qualified name's parts indented under it), the label
from column 2 dotted to column 33, a `>` marker in column 35 for a
value the caller gave, the field from column 37, the choices (`Name`,
`*YES, *NO`, `1-99`) from column 51 or just past the field; 16 rows
per page, `More...` or `Bottom` on row 22, the keys on rows 23 and 24.
Parameters with `PMTCTL(*PMTRQS)` appear after `F10=Additional
parameters`; `PMTCTL(label)` parameters appear when their condition
holds, the display re-evaluated after Enter. F3 and F12 cancel, F4 on
a field prompts its choices, F5 refreshes, F9 shows every parameter,
F11 shows the keywords, F13 the help, F14 the command string, F15
the error, F16 and F24 the other keys. Enter binds the fields; an
error puts its message on row 24, the cursor on the field and the
field in reverse image. Selective prompting (`?KWD`, `??KWD`, `?*`,
`?<`, `?-`) hides, protects or omits parameters as IBM's does.

The goldens of `command/cmd/tests/golden/` and
`command/sys-cpps/tests/golden/` keep the displays.

## 6. Where commands run

`command/sys-cpps` holds the shipped definitions (`cmds/*.cmd`, one
per command) and the inventory (`inventory.toml`): a command runs in
the storage service unless the inventory makes it the job's (calls,
the debugger, the job's own displays, `RTVJOBA`, `RTVSYSVAL`) or a
stub (`SAVLIB`, `RSTLIB`, `CHKTAP`, `STRSST` send the message of the
phase that will do them). In the service, `lic/slic/src/cpp/direct.rs`
implements a command on the library functions directly, or the
bridge hands the keyword form to the console word of Phases 2 to 6.
A user's `*CMD` names a `*PGM` the job calls with the block.

The dev console's old words live on behind `svc.` (`svc.obj ls`,
`svc.xfer put`), and `QCMDEXC`, `QCMDCHK` and `QCAPCMD` run the
analyzer in the job.
