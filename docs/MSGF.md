# Message files

The `msgf` crate implements this document: message descriptions, the
substitution of message data into text, the `*MSGF` payload, the
commands over it and the system files. `runtime/msgf/tests/spec.rs` compares
the code's layouts with the tables here. Every text is ours; the
message identifiers, the `ADDMSGD` parameters and the substitution
variable types are the compatibility interface.

## 1. Descriptions

A message description has an identifier (`AAAnnnn`: three letters and
four hexadecimal digits), a severity 0 to 99, first-level text,
second-level text, an ordered list of substitution variables, an
optional default reply with its valid replies, and a dump list. In
the text, `&n` names the n-th variable; an `&` that starts no number,
or a number past the list, stays as written.

Substitution variable types (`ADDMSGD FMT`), how each reads its bytes
of message data and how it renders:

| Type | Bytes | Rendering |
|---|---|---|
| `*CHAR n` | `n` | CP037 text, trailing blanks dropped |
| `*CHAR *VARY p` | `p` (2 or 4) + length | the text the length covers |
| `*DEC d s` | `d/2 + 1` | packed decimal as a number |
| `*HEX n` | `n` | hexadecimal digits |
| `*QTDCHAR n` | `n` | the text in quotes |
| `*BIN p` | `p` (2, 4 or 8) | a signed binary integer |
| `*DTS`, `*UTC` | 8 | microseconds since 1970 as `yyyy-mm-dd-hh.mm.ss` |
| `*CCHAR n`, `*ITYPE n` | `n` | as text |
| `*SPP`, `*SYP` | 16 | hexadecimal |

Message data shorter than the variables renders what is there; the
rest is empty.

## 2. Sources

The system files are authored in `runtime/msgf/msgs/*.toml`, embedded in the crate
at build time and installed into `QSYS` on first IPL when absent:
`QCPFMSG` (the `MCH`, `CPF`, `CPD` and `CPI` identifiers the runtime,
the binder and the database use), `QCEEMSG` (`CEE` identifiers) and
`QSQLMSG` (the `SQL0nnn` identifiers behind the SQLCODEs of Phase 3).
`QRNXMSG` and `QRPGLEMSG` are Phase 9.

```toml
name = "QCPFMSG"
text = "System messages"

[[msg]]
id = "MCH3401"
sev = 40
text = "Object &1 was not found."
help = "..."
vars = ["*CHAR 10"]
reply = { default = "C", valid = ["C", "D"] }   # inquiry messages
dump = [1]                                        # DMPLST
```

## 3. The `*MSGF` payload

One byte space: a `MsgfHdr`, then `count` `MsgEntry` records in
identifier order, each followed by its texts, its `MsgVar` records
and its dump list.

### `MsgfHdr` (32 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | magic | `QS400MSG` |
| 8 | 4 | version | 1 |
| 12 | 4 | count | descriptions |
| 16 | 2 | ccsid | the file's CCSID: 37 for the system files, the language's for a `QSYS29xx` file, `CRTMSGF CCSID` (65535 for `*HEX`) for a user's; 0 in a file stored before V1R1M0 reads as 37 |
| 18 | 14 | reserved | 0 |

### `MsgEntry` (32 bytes)

The fixed part of a description, followed by the first-level text,
the second-level text, the default reply and the valid replies joined
by commas (UTF-8, the lengths below), then `var_count` `MsgVar`
records and `dump_count` 2-byte variable numbers.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 7 | id | CP037 |
| 7 | 1 | reserved | 0 |
| 8 | 2 | severity | 0 to 99 |
| 10 | 2 | var_count | |
| 12 | 2 | dump_count | |
| 14 | 2 | reserved2 | 0 |
| 16 | 4 | text_len | |
| 20 | 4 | help_len | |
| 24 | 4 | dft_reply_len | |
| 28 | 4 | valid_len | |

### `MsgVar` (8 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | kind | 1 `*CHAR`, 2 `*CHAR *VARY`, 3 `*DEC`, 4 `*HEX`, 5 `*QTDCHAR`, 6 `*BIN`, 7 `*DTS`, 8 `*CCHAR`, 9 `*ITYPE`, 10 `*UTC`, 11 `*SPP`, 12 `*SYP` |
| 1 | 1 | prefix | bytes of the length prefix or the binary size |
| 2 | 1 | scale | decimal positions |
| 3 | 1 | reserved | 0 |
| 4 | 4 | len | characters or digits |

## 4. Commands

`CRTMSGF` creates an empty file. `ADDMSGD` adds a description; an
existing identifier is `CPF2412`. `CHGMSGD` replaces one and `RMVMSGD`
removes one; a missing identifier is `CPF2419`. `MRGMSGF` copies the
descriptions of one file into another, replacing existing ones only
when asked. `RTVMSG` resolves the file through the library list and
returns the description. `DSPMSGD` renders one description as text.

## 5. At run time

The runtime looks descriptions up through `MsgLookup`: the built-in
sources until a store supplies the objects. A message on a queue
keeps its identifier, file and data; its first- and second-level text
are substituted when it is sent, so a description changed afterwards
does not change messages already sent (`ILE.md` §5).

## 6. National language support

### Text comes from message files and panel sources

Every string a user reads at a display or in a printed report comes
from a message file, a panel group or a display file source, so that a
language library can carry it translated. `tools/textaudit` holds the
code to that: `scope.toml` names the display paths (the job's panel
drivers, display layer, developer tools and system displays; the
service's command processing programs and display data; the console's
screens; the hardware resource view; the database's and the IFS
commands' display text), and in them every string literal outside
tests, attributes, logging, assertions and `.expect(...)` that holds
words is a site. A site is a *message* (text written beside a message
identifier, which the message file already has), a *detail* (an error's
detail, which a function check puts in the job log as `CPD9898`) or
*display* text. `baseline.txt` lists the sites not yet moved; a literal
not in it fails `cargo test -p textaudit` and `build test`.
`cargo run -p textaudit` prints the counts per crate, `-- list [CRATE]`
the sites, `-- baseline` rewrites the baseline after moving some.

Display text that is not a message lives in `QSTXTMSG` ("Display
text", identifiers `TXTnnnn`): a driver asks for an identifier with its
values and gets the text of the first `QSTXTMSG` on the job's library
list. The hardware resource displays (`WRKHDWRSC`, `DSPHDWRSC`) were
the first moved there; `hwrsrc::view` takes a `Words` resolver and
works in identifiers.

### CCSIDs

A message file keeps its CCSID in its header, a panel group in its
(`Ccsid`, `docs/UIM.md` §6) and a display file in its (37 when the
source names none, `docs/DSPF.md`). A job's `CCSID` is 65535 or a
single-byte EBCDIC CCSID the conversion tables have (`QCCSID` and
`CHGJOB CCSID` refuse others); its default CCSID is its CCSID, or with
65535 its language's (`ccsid::nls`). `CHGJOB CCSID(*SYSVAL)`,
`(*USRPRF)`, `LANGID(*SYSVAL|*USRPRF)` and `CNTRYID(...)` take the
system value's or the profile's, and a job starts with its profile's
`LANGID`, `CNTRYID` and `CCSID` when they are not `*SYSVAL`. CCSID 1141
(273 with the euro) joined the tables.

### Language libraries

`QSYS2924` is the primary language's library (English), made at the
first IPL; the English objects themselves are in `QSYS`. A secondary
language is a library `QSYS29xx` holding the language's message files
and its sign-on display files. `INSLNG LNG(29xx)` installs one from the
text the system carries (`runtime/msgf/msgs/nls/29xx.toml`), as does
`RSTLICPGM LICPGM(QS00SS1) RSTOBJ(*LNG) LNG(29xx)`. IBM i has no
`INSLNG` command (the 7.1 oracle answers `CPD0030`,
`tools/conformance/golden/phase12/inslng/`); IBM installs a secondary
language with `RSTLICPGM RSTOBJ(*LNG)` or `GO LICPGM` option 21, and
QS/400's `INSLNG` is a front end to the same install.

The library is found as IBM finds it: ahead of `QSYS` in the system part
of the library list. `CHGSYSLIBL` or the `QSYSLIBL` system value put it
there; the sign-on display file is the first `QDSIGNON` (`QDSIGNON2`)
on `QSYSLIBL`, so a language library at its front gives the sign-on in
that language. As a QS/400 convenience, a job whose `LANGID` names an
installed secondary language gets its library ahead of `QSYS` when it
starts, and `CHGJOB LANGID` moves it (any other language library goes).

**The Spanish library is a scaffold.** `QSYS2931` (Spanish, CCSID 284)
proves the mechanism and is not a translation: its message files carry
machine-translated first-level text for 90 of the 1,245 messages of
`QCPFMSG` and `QSTXTMSG` (the sign-on's, the commonest object and
library messages, and all of `QSTXTMSG`), the English
first-level text for the rest, and no second-level text. The library's
text description starts with `SCAFFOLD` and each file's ends with
`(SCAFFOLD)`. Its sign-on display files have the Spanish constants.

### Dates, times and decimal numbers

One formatting path: `ccsid::nls::Formats` (from `DATFMT`, `DATSEP`,
`TIMSEP`, `DECFMT`, or the system values `QDATFMT`, `QDATSEP`,
`QTIMSEP`, `QDECFMT`). The job's display layer formats with the job's
(`JobAttrs::formats`): the display file runtime's `DATE` and `TIME`
constants and edited numbers (edit code `Y` with the job's date
separator, `DECFMT(J|I)` a decimal comma on output fields; an input
field still reads a point back), the panels' date and time columns
(`WRKJOB`, `WRKJOBSCDE`, the object and message displays, `DSPLOG`),
the `TOPSEP=DATETIME` stamp, `DSPWSUSR` and PDM's date column. The sign-on
display uses the system values. The storage service formats the dates
of its answers in the formats of the job that asked (`stgsvc/fmt.rs`
holds them for the request); a request with no job (the service
console) gets `*MDY`, `/` and `:`. PDM's subset date (an input field)
and the service tools' ISO stamps (the LIC log, SST, dumps) stay as
they are.
