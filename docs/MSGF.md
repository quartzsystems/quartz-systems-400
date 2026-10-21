# Message files

The `msgf` crate implements this document: message descriptions, the
substitution of message data into text, the `*MSGF` payload, the
commands over it and the system files. `msgf/tests/spec.rs` compares
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

The system files are authored in `msgs/*.toml`, embedded in the crate
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
| 16 | 2 | ccsid | of the texts' source, 37 |
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
existing identifier is `CPF2499`. `CHGMSGD` replaces one and `RMVMSGD`
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
