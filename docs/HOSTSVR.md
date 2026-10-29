# Host servers

The host servers are what IBM i Access Client Solutions and JTOpen talk
to: a port mapper, a sign-on server, and one server per function
(central, database, data queue, file, network print, remote command).
`net/hostsvr` is the codec for their datastream; the servers themselves
come later in Phase 10. Everything here was learned from wire traces of
JTOpen talking to the primary oracle (IBM i 7.1), recorded by
`tn5250-driver hostsvr-trace` and committed under
`tools/conformance/hostsvr/traces/`, with JTOpen's open source (IBM
Public License) read as a description of the protocol. No JTOpen code is
in QS/400.

The tables marked for the spec test are checked against the code by
`net/hostsvr/tests/spec.rs`: a table after `<!-- consts: KEY -->` must
list exactly the constants of `KEY`, a table after `<!-- layout: KEY -->`
the fields of that template at the same frame offsets and sizes. The
tables are printed by `cargo test -p hostsvr --test spec -- --ignored
--nocapture` when the code changes.

## Datastream

### Servers and ports

| Server | Header server ID | Service name | Port | TLS port |
|---|---|---|---|---|
| Port mapper | (not framed) | `as-svrmap` | 449 | |
| Central | `0xE000` | `as-central` | 8470 | 9470 |
| File | `0xE002` | `as-file` | 8473 | 9473 |
| Network print | `0xE003` | `as-netprt` | 8474 | 9474 |
| Database | `0xE004` | `as-database` | 8471 | 9471 |
| Data queue | `0xE007` | `as-dtaq` | 8472 | 9472 |
| Remote command | `0xE008` | `as-rmtcmd` | 8475 | 9475 |
| Sign-on | `0xE009` | `as-signon` | 8476 | 9476 |

The database server also answers native database (`0xE005`) and
catalog (`0xE006`) server IDs in JTOpen's code; the traces only show
`0xE004`.

### Frames

Every frame after the port mapper starts with a 20-byte header, all
fields big endian:

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | Total length, header included |
| 4 | 2 | Header ID: byte 4 is client attributes in the seed exchange (`0x03`) and start server (`0x02`) requests, echoed in their replies; zero elsewhere |
| 6 | 2 | Server ID |
| 8 | 4 | CS instance (zero) |
| 12 | 4 | Correlation ID: the client counts requests; replies echo it |
| 16 | 2 | Template length |
| 18 | 2 | Request or reply ID |

The template (fixed fields, per request) follows, then LL/CP
parameters: a u32 length that counts itself and the code point, a u16
code point, the data. Every frame in every committed trace is header,
template and whole parameters, with nothing left over; the database and
network print servers nest further LL/CP lists inside some parameters.

A client sends a request and reads its reply, except end job (`0x7006`,
`0x1004`, `0x1FFF`), which closes without a reply, and database requests
whose ORS bitmap does not ask for a reply: in the JDBC traces create
RPB (`0x1D00`), change and delete descriptor (`0x1E00`, `0x1E01`) and
delete results (`0x1F01`) are sent without waiting, while delete RPB
(`0x1D02`) is answered.

### The sign-on family

Every server but the sign-on server starts with the same two requests:
exchange random seeds (`0x7001`, the client's 8-byte seed in the
template; the reply carries a return code and the server's seed) and
start server (`0x7002`, the authentication scheme and a send-reply byte
in the template, the password substitute in `0x1105` or a token in
`0x1115`, the user in `0x1104`; the reply carries a return code, the
user and the server job's name in `0x111F`). The client attributes in
the seed exchange header byte (`0x03`) say it can do SHA-1 and SHA-512
substitutes and additional authentication factors. The sign-on server
does the same with exchange attributes (`0x7003`) instead.

<!-- consts: common::id -->

| Value | Name |
|---|---|
| `0x7001` | `SEED_EXCHANGE` |
| `0xF001` | `SEED_EXCHANGE_REPLY` |
| `0x7002` | `START_SERVER` |
| `0xF002` | `START_SERVER_REPLY` |
| `0x7006` | `END_JOB` |

<!-- layout: common::SeedRequest -->

| Offset | Size | Field |
|---|---|---|
| 20 | 8 | `client_seed` |

<!-- layout: common::SeedReply -->

| Offset | Size | Field |
|---|---|---|
| 20 | 4 | `rc` |
| 24 | 8 | `server_seed` |

<!-- layout: common::StartServerRequest -->

| Offset | Size | Field |
|---|---|---|
| 20 | 1 | `auth_scheme` |
| 21 | 1 | `send_reply` |

<!-- layout: common::StartServerReply -->

| Offset | Size | Field |
|---|---|---|
| 20 | 4 | `rc` |

Authentication schemes (template byte of start server, sign-on
information, change password, generate token): `0x01` DES substitute,
`0x02` profile token, `0x03` SHA-1 substitute, `0x05` GSS token, `0x06`
identity token, `0x07` SHA-512 substitute.

Return codes (u32): `0` success; class `0x0001xxxx` request not valid
(`0x00010001`-`0x00010007` user ID length, `0x00010008` password length,
`0x00010009`-`0x0001000B` sign-on request, `0x0001000C` change request,
`0x0001000D` old password, `0x0001000E` new password, `0x0001000F` token
type, `0x00010010`/`0x00010012` token request, `0x00010011` token
length); `0x00020001` user unknown, `0x00020002` user disabled,
`0x00020003` user mismatch; `0x00030001`-`0x0003000A` new password rule
violations (too long, too short, repeated character, adjacent digits,
consecutive repeat, used before, no digit, character not valid,
disallowed, contains the user), `0x0003000B` password incorrect,
`0x0003000C` incorrect and now disabled, `0x0003000D` expired,
`0x00030010` password `*NONE`; `0x00040000` general security error.

### Password substitutes and password levels

The client never sends the password. Both sides know the two seeds and
compute a substitute from a token of the user and password
(`hostsvr::crypt`, built on `security/pwdsub`):

| QPWDLVL | Form | Substitute | Token |
|---|---|---|---|
| 0, 1 | DES | 8 bytes | DES of the user ID (folded to 8 bytes) under the password (EBCDIC, upper case, a leading digit gets a `Q`, at most 10 characters) XOR `0x55` and shifted left one bit; passwords of 9-10 characters encrypt both halves and XOR them |
| 2, 3 | SHA-1 | 20 bytes | SHA-1 of the user (10 characters upper case, UTF-16BE) and the password (as typed, trailing blanks removed, UTF-16BE) |
| 4 | SHA-512 | 64 bytes | PBKDF2-HMAC-SHA512 of the password (UTF-16BE), 10022 iterations, salt SHA-256 of the user (10 characters) and the password's last four characters (blank padded), UTF-16BE |

The DES substitute is the RFC 4777 chain: the sequence number (1) plus
the server seed, encrypted under the token, XORed with the client seed,
the user's two halves, and the sequence number, each step encrypted
again. The SHA forms are the digest of the token, server seed, client
seed, user (UTF-16BE, 10 characters) and the sequence number (8 bytes,
1). A change password request (`0x7005`) sends the old password's
substitute, the new password protected with the old token (`0x110D`)
and the old password protected with the new token (`0x110C`): the
password bytes XORed with substitutes at sequence numbers 2, 3, ... (8,
20 or 64 bytes per block; DES protects the first eight characters and
the last two left justified in blanks), with the byte lengths in
`0x111C`/`0x111D` and CCSID 13488 in `0x111E` for the SHA forms.

The primary oracle runs `QPWDLVL` 0, so its traces carry DES
substitutes; the recording proxy recomputes each one with
`hostsvr::crypt` and the traces record that they matched
(`note ... password substitute (Des) matches hostsvr::crypt`). The SHA-1
form was proven against pub400 by Phase 8's TN5250 auto-signon; the
SHA-512 form is from JTOpen's description, with its PBKDF2 checked
against the published HMAC-SHA512 vectors.

### Messages

The remote command server (`0x1106`) and the sign-on server (`0x112B`,
counted by `0x112A`) return job log messages in one layout: text CCSID
(u32), substitution data CCSID (u32), severity (u16), then u32
length-prefixed message type (two EBCDIC digits), message ID, message
file, library, first level text, substitution data and help text.

### RLE compression

Program call parameters (usage 21-23) and database replies (`0x3832`,
when the ORS bitmap asks for `REPLY_RLE`) are run-length encoded with
escape byte `0x1B`: escape-escape is a literal `0x1B`; escape, a 2-byte
pattern and a u16 count is the pattern repeated count times; everything
else is literal. A database `0x3832` holds the decompressed length (u32)
and the encoded bytes, which expand to the reply's LL/CP parameters.

### Traces

`tn5250-driver hostsvr-trace --target oracle --out DIR --scenario NAME
-- COMMAND...` listens on 127.0.0.1 at the port mapper and plain host
server ports, starts COMMAND with `QS_HS_HOST=127.0.0.1` and the
target's user and password in `QS_HS_USER`/`QS_HS_PASS`, forwards every
connection to the target unchanged, and writes `DIR/<server>/NAME.trace`
for each server the client used. A trace file is text:

```text
# QS/400 host server trace, format 1
server rmtcmd 8475
scenario pgmcall

conn 1
> 1 +5ms frame 28 e008/7001
  0000  00 00 00 1c 03 00 e0 08  00 00 00 00 00 00 00 00  |......\.........|
  0010  00 08 70 01 00 00 01 a1  6e 8c 08 cc              |.......~>...|
< 2 +2893ms frame 32 e008/f001
...
note +3082ms password substitute (Des) matches hostsvr::crypt
end 1 +8243ms
```

`>` is client to server, `<` back; the number counts chunks in the
connection, the time is milliseconds since accept, `frame` chunks are
cut at the header's length (with server and request ID shown), `raw`
chunks are bytes that are not framed (the port mapper, a partial frame
at close). The text column is EBCDIC and ignored when reading.
`hostsvr::trace::Trace` reads and writes the format.

Committed traces are masked as they are recorded
(`hostsvr::trace::mask`): in every frame of the sign-on family (request
IDs `0x70xx`/`0x71xx`, replies `0xF0xx`/`0xF1xx`) the user ID field
(`0x1104`; in replies after its CCSID) becomes the EBCDIC placeholder
`MASKUSER` blank padded to the field's length, and the password
substitute (`0x1105`), protected passwords (`0x110C`, `0x110D`) and
profile tokens (`0x1115`) become `0xFF` bytes. Lengths never change, so
masked traces still parse and re-encode. The proxy also refuses to
write anything if the password appears in plain text (ASCII, EBCDIC or
UTF-16, as typed or upper case) anywhere on the wire.

## Port mapper

Not framed. The client sends the service name in ASCII (`as-signon`,
`as-database`, ...; `-s` for the TLS ports) and the server answers five
bytes: `+` (`0x2B`) and the port as a u32. Any other first byte means
the service is unknown. Each lookup is its own connection.

## Sign-on server

Exchange attributes (`0x7003`) carries the client's version (`0x1101`,
1), datastream level (`0x1102`, 10 from current JTOpen) and seed
(`0x1103`); the reply has the server's version, level, seed, password
level (`0x1119`), job name and additional-factor support. Sign-on
information (`0x7004`, scheme byte, client CCSID in `0x1113`, substitute
and user, `0x1128` to return messages) answers with the current and
last sign-on dates and the password expiration (`0x1106`-`0x1108`: year
u16, month, day, hour, minute, second), warning days (`0x112C`), the
server CCSID (`0x1114`) and messages. Generate token (`0x7007`) returns
a 32-byte profile token at frame offset 30 (in `0x1115`), with the
token type (`0x1116`: `0xF1` single use, `0xF2` multiple use, `0xF3`
regenerable) and timeout (`0x1117`) in the request; `0x7008` does the
same for a user identity (`0x1126`, `0x1127`).

<!-- consts: signon::id -->

| Value | Name |
|---|---|
| `0x7003` | `EXCHANGE_ATTRIBUTES` |
| `0xF003` | `EXCHANGE_ATTRIBUTES_REPLY` |
| `0x7004` | `SIGNON_INFO` |
| `0xF004` | `SIGNON_INFO_REPLY` |
| `0x7005` | `CHANGE_PASSWORD` |
| `0xF005` | `CHANGE_PASSWORD_REPLY` |
| `0x7007` | `GENERATE_TOKEN` |
| `0xF007` | `GENERATE_TOKEN_REPLY` |
| `0x7008` | `GENERATE_TOKEN_IDENTITY` |
| `0xF008` | `GENERATE_TOKEN_IDENTITY_REPLY` |
| `0x7103` | `HOSTCNN_EXCHANGE_ATTRIBUTES` |
| `0xF103` | `HOSTCNN_EXCHANGE_ATTRIBUTES_REPLY` |

<!-- consts: cp::signon -->

| Value | Name |
|---|---|
| `0x1101` | `VERSION` |
| `0x1102` | `LEVEL` |
| `0x1103` | `SEED` |
| `0x1104` | `USER_ID` |
| `0x1105` | `PASSWORD` |
| `0x1106` | `CURRENT_SIGNON_DATE` |
| `0x1107` | `LAST_SIGNON_DATE` |
| `0x1108` | `EXPIRATION_DATE` |
| `0x110C` | `OLD_PASSWORD` |
| `0x110D` | `NEW_PASSWORD` |
| `0x1113` | `CLIENT_CCSID` |
| `0x1114` | `SERVER_CCSID` |
| `0x1115` | `PROFILE_TOKEN` |
| `0x1116` | `TOKEN_TYPE` |
| `0x1117` | `TOKEN_TIMEOUT` |
| `0x1119` | `PASSWORD_LEVEL` |
| `0x111C` | `OLD_PASSWORD_LENGTH` |
| `0x111D` | `NEW_PASSWORD_LENGTH` |
| `0x111E` | `PASSWORD_CCSID` |
| `0x111F` | `JOB_NAME` |
| `0x1126` | `IDENTITY_TYPE` |
| `0x1127` | `IDENTITY` |
| `0x1128` | `RETURN_MESSAGES` |
| `0x112A` | `MESSAGE_COUNT` |
| `0x112B` | `MESSAGE` |
| `0x112C` | `WARNING_DAYS` |
| `0x112E` | `AAF_INDICATOR` |
| `0x112F` | `AAF` |
| `0x1130` | `VERIFICATION_ID` |
| `0x1131` | `CLIENT_IP` |

<!-- layout: signon::Rc -->

| Offset | Size | Field |
|---|---|---|
| 20 | 4 | `rc` |

<!-- layout: signon::AuthRequest -->

| Offset | Size | Field |
|---|---|---|
| 20 | 1 | `auth_scheme` |

<!-- layout: signon::GenerateTokenRequest -->

| Offset | Size | Field |
|---|---|---|
| 20 | 1 | `auth_scheme` |
| 21 | 1 | `send_reply` |

## Central server

NLS exchange attributes (`0x1301`) returns the server job's CCSID and,
from NLS level 1, the CCSIDs it can convert (`0x0008`: count u32, CCSIDs
u32). A conversion table request (`0x1201`) names the from and to
CCSIDs and the mapping (1 round trip, 2 substitution) and sends every
code point to convert (`0x0004`: 256 bytes, or all u16 values for a
double-byte CCSID); the reply carries the converted table. License
requests (`0x1001`-`0x1003`) are listed by ID only. Replies reuse the
request ID; every template starts with a u16 chain field.

The primary oracle's central server (and its sign-on server) accepts
TCP connections and closes them at once without a reply, so these reply
layouts come from JTOpen's reading of them and are not trace-checked.

<!-- consts: central::id -->

| Value | Name |
|---|---|
| `0x1001` | `REQUEST_LICENSE` |
| `0x1002` | `RELEASE_LICENSE` |
| `0x1003` | `RETRIEVE_LICENSE_INFO` |
| `0x1201` | `CONVERSION_TABLE` |
| `0x1301` | `EXCHANGE_ATTRIBUTES` |

<!-- consts: cp::central -->

| Value | Name |
|---|---|
| `0x0004` | `CODE_POINTS` |
| `0x0008` | `CCSID_LIST` |

<!-- layout: central::ExchangeAttributes -->

| Offset | Size | Field |
|---|---|---|
| 20 | 2 | `chain` |
| 22 | 2 | `level` |
| 24 | 4 | `ccsid` |
| 28 | 4 | `client_version` |
| 32 | 2 | `license_level` |
| 34 | 2 | `system_level` |
| 36 | 2 | `nls_level` |

<!-- layout: central::ExchangeAttributesReply -->

| Offset | Size | Field |
|---|---|---|
| 20 | 2 | `chain` |
| 22 | 2 | `rc` |
| 24 | 2 | `secondary_rc` |
| 26 | 2 | `level` |
| 28 | 4 | `ccsid` |
| 32 | 4 | `server_version` |
| 36 | 2 | `license_level` |
| 38 | 2 | `system_level` |
| 40 | 2 | `nls_level` |

<!-- layout: central::ConversionTable -->

| Offset | Size | Field |
|---|---|---|
| 20 | 2 | `chain` |
| 22 | 4 | `from_ccsid` |
| 26 | 4 | `to_ccsid` |
| 30 | 2 | `mapping` |
| 32 | 2 | `parameter_count` |

<!-- layout: central::ConversionTableReply -->

| Offset | Size | Field |
|---|---|---|
| 20 | 2 | `chain` |
| 22 | 2 | `rc` |
| 24 | 2 | `secondary_rc` |
| 26 | 2 | `reserved` |

## Remote command and program call

After the seed exchange and start server, the client exchanges
attributes (`0x1001`: CCSID 13488, NLV `2924`, version 1, datastream
level 10); the oracle answers its job CCSID (37), NLV, version
`0x00070100` and level 10. A command (`0x1002`) travels in `0x1104` as
CCSID 1200 and UTF-16 text (`0x1101` in the job CCSID below level 10);
the reply's return code is 0 or `0x0400` (failed) and the message count
precedes the `0x1106` messages. A program call (`0x1003`) names the
program and library (EBCDIC, 10 each), the message option and the
parameter count; each parameter is a `0x1103` with the maximum length
(u32), the usage (u16) and the input bytes, trailing zeros trimmed. The
reply (`0x8003`) returns each output parameter as a `0x1103` with the
output length, usage 12 (or 22 compressed) and the bytes; a failed call
answers `0x9003` with return code `0x0500` and messages (`MCH3401` for
a missing program on the oracle). End (`0x1004`) closes.

Message options: 0 up to ten, 1 none, 2 all (levels 7-9); 3 up to ten
and 4 all with the extended layout at level 10; 5 and 6 from level 11.
Parameter usage: base usage (1 input, 2 output, 3 both) plus 10 plain,
20 RLE, or 30 for input-output at level 5 and up; `0xFF` null.

<!-- consts: rmtcmd::id -->

| Value | Name |
|---|---|
| `0x1001` | `EXCHANGE_ATTRIBUTES` |
| `0x8001` | `EXCHANGE_ATTRIBUTES_REPLY` |
| `0x1002` | `RUN_COMMAND` |
| `0x8002` | `RUN_COMMAND_REPLY` |
| `0x1003` | `CALL_PROGRAM` |
| `0x8003` | `CALL_PROGRAM_REPLY` |
| `0x1004` | `END` |
| `0x9003` | `CALL_PROGRAM_FAILED` |

<!-- consts: cp::rmtcmd -->

| Value | Name |
|---|---|
| `0x1101` | `COMMAND` |
| `0x1102` | `MESSAGE` |
| `0x1103` | `PARAMETER` |
| `0x1104` | `COMMAND_CCSID` |
| `0x1106` | `MESSAGE_EXTENDED` |

<!-- layout: rmtcmd::ExchangeAttributes -->

| Offset | Size | Field |
|---|---|---|
| 20 | 4 | `ccsid` |
| 24 | 4 | `nlv` |
| 28 | 4 | `client_version` |
| 32 | 2 | `level` |

<!-- layout: rmtcmd::ExchangeAttributesReply -->

| Offset | Size | Field |
|---|---|---|
| 20 | 2 | `rc` |
| 22 | 4 | `ccsid` |
| 26 | 4 | `nlv` |
| 30 | 4 | `server_version` |
| 34 | 2 | `level` |

<!-- layout: rmtcmd::RunCommand -->

| Offset | Size | Field |
|---|---|---|
| 20 | 1 | `message_option` |

<!-- layout: rmtcmd::ReplyCount -->

| Offset | Size | Field |
|---|---|---|
| 20 | 2 | `rc` |
| 22 | 2 | `count` |

<!-- layout: rmtcmd::CallProgram -->

| Offset | Size | Field |
|---|---|---|
| 20 | 10 | `program` |
| 30 | 10 | `library` |
| 40 | 1 | `message_option` |
| 41 | 2 | `parameter_count` |

## Database server

Every request has the same 20-byte template; the header's request ID is
the function. Every reply has ID `0x2800` and its own 20-byte template
naming the function it answers. JDBC starts with set attributes
(`0x1F80`: client CCSID, language feature, client functional level
(`V7R2M01`), date and time formats, naming, commitment control, use of
extended formats, data compression, interface name and level, ...); the
reply's server attributes (`0x3804`) give the job CCSID (37), language,
functional level (`V7R1M01400`), RDB name and the server job
(`QZDASOINIT QUSER nnnnnn`). A statement then runs as create RPB
(`0x1D00`, statement name `STMTnnnn` and cursor `CRSRnnnn`), prepare
and describe (`0x1803`, text in `0x3831` as UTF-16), change descriptor
(`0x1E00`, the parameter marker format), execute or open and describe
(`0x1805`/`0x1804`), fetches (`0x180B`, blocking factor `0x380C`) until
the SQLCA says end of data, close (`0x180A`), delete descriptor and RPB,
and send/delete results (`0x1F00`/`0x1F01`). Replies are RLE compressed
in `0x3832` when the request asks for it.

Strings are a CCSID (u16), a length (u16, u32 in `0x3831`) and bytes.
The SQLCA (`0x3807`) is the 136-byte DB2 layout; an empty `0x3807` means
none. Formats are the super extended kind (`0x3812` columns, `0x3813`
parameter markers): a 16-byte header, a 48-byte record per field, then
each field's name as `0x3840` (CCSID u16, name) at the record's offset.
Result data (`0x380E`) is a 20-byte header, the null indicators (rows ×
columns × 2, negative is null), then the rows (fixed `row_size`, or with
variable-length field compression a row information header and row
offsets).

<!-- consts: database::id -->

| Value | Name |
|---|---|
| `0x0000` | `TEST_CONNECTION` |
| `0x1800` | `PREPARE` |
| `0x1801` | `DESCRIBE` |
| `0x1802` | `DESCRIBE_MARKERS` |
| `0x1803` | `PREPARE_DESCRIBE` |
| `0x1804` | `OPEN_DESCRIBE` |
| `0x1805` | `EXECUTE` |
| `0x1806` | `EXECUTE_IMMEDIATE` |
| `0x1807` | `COMMIT` |
| `0x1808` | `ROLLBACK` |
| `0x1809` | `CONNECT` |
| `0x180A` | `CLOSE` |
| `0x180B` | `FETCH` |
| `0x180C` | `STREAM_FETCH` |
| `0x180D` | `PREPARE_EXECUTE` |
| `0x180E` | `OPEN_DESCRIBE_FETCH` |
| `0x180F` | `CREATE_PACKAGE` |
| `0x1810` | `CLEAR_PACKAGE` |
| `0x1811` | `DELETE_PACKAGE` |
| `0x1812` | `EXECUTE_OPEN_DESCRIBE` |
| `0x1813` | `END_STREAM_FETCH` |
| `0x1815` | `RETURN_PACKAGE` |
| `0x1816` | `RETRIEVE_LOB` |
| `0x1817` | `WRITE_LOB` |
| `0x1818` | `CANCEL` |
| `0x1819` | `FREE_LOB` |
| `0x1D00` | `CREATE_RPB` |
| `0x1D01` | `CREATE_RPB_BASED_ON` |
| `0x1D02` | `DELETE_RPB` |
| `0x1D03` | `CHANGE_RPB` |
| `0x1D04` | `RESET_RPB` |
| `0x1E00` | `CHANGE_DESCRIPTOR` |
| `0x1E01` | `DELETE_DESCRIPTOR` |
| `0x1F00` | `SEND_RESULTS` |
| `0x1F01` | `DELETE_RESULTS` |
| `0x1F80` | `SET_ATTRIBUTES` |
| `0x1F81` | `RETRIEVE_ATTRIBUTES` |
| `0x1FFF` | `END` |
| `0x2800` | `REPLY` |

<!-- consts: database::ors -->

| Value | Name |
|---|---|
| `0x80000000` | `RETURN_DATA` |
| `0x40000000` | `MESSAGE_ID` |
| `0x20000000` | `FIRST_LEVEL_TEXT` |
| `0x10000000` | `SECOND_LEVEL_TEXT` |
| `0x08000000` | `DATA_FORMAT` |
| `0x04000000` | `RESULT_DATA` |
| `0x02000000` | `SQLCA` |
| `0x01000000` | `SERVER_ATTRIBUTES` |
| `0x00800000` | `MARKER_FORMAT` |
| `0x00100000` | `PACKAGE_INFO` |
| `0x00080000` | `REQUEST_RLE` |
| `0x00040000` | `REPLY_RLE` |
| `0x00020000` | `EXTENDED_COLUMNS` |
| `0x00010000` | `VARIABLE_COMPRESSION` |
| `0x00008000` | `CURSOR_ATTRIBUTES` |

<!-- consts: cp::db_request -->

| Value | Name |
|---|---|
| `0x3801` | `LIBRARY_NAME` |
| `0x3802` | `LANGUAGE_FEATURE` |
| `0x3803` | `CLIENT_LEVEL` |
| `0x3804` | `PACKAGE_NAME` |
| `0x3805` | `TRANSLATE` |
| `0x3806` | `STATEMENT_NAME` |
| `0x3807` | `STATEMENT_TEXT` |
| `0x3808` | `PREPARE_OPTION` |
| `0x3809` | `OPEN_ATTRIBUTES` |
| `0x380A` | `DESCRIBE_OPTION` |
| `0x380B` | `CURSOR_NAME` |
| `0x380C` | `BLOCKING_FACTOR` |
| `0x380D` | `SCROLLABLE` |
| `0x380E` | `FETCH_SCROLL` |
| `0x380F` | `HOLD` |
| `0x3810` | `REUSE` |
| `0x3811` | `MARKER_DATA` |
| `0x3812` | `STATEMENT_TYPE` |
| `0x3813` | `PACKAGE_THRESHOLD` |
| `0x3814` | `MARKER_BLOCK` |
| `0x3817` | `QUERY_TIMEOUT` |
| `0x381F` | `EXTENDED_MARKER_DATA` |
| `0x3821` | `EXTENDED_FORMATS` |
| `0x3822` | `LOB_THRESHOLD` |
| `0x3823` | `DATA_COMPRESSION` |
| `0x3824` | `AUTOCOMMIT` |
| `0x3829` | `EXTENDED_COLUMNS` |
| `0x3830` | `HOLDABILITY` |
| `0x3831` | `EXTENDED_STATEMENT_TEXT` |
| `0x3832` | `COMPRESSED` |
| `0x3833` | `VARIABLE_COMPRESSION` |
| `0x3834` | `BUFFER_SIZE` |
| `0x383C` | `INTERFACE_TYPE` |
| `0x383D` | `INTERFACE_NAME` |
| `0x383E` | `INTERFACE_LEVEL` |
| `0x383F` | `CLOSE_EOF` |

<!-- consts: cp::db_reply -->

| Value | Name |
|---|---|
| `0x3801` | `MESSAGE_ID` |
| `0x3802` | `FIRST_LEVEL_TEXT` |
| `0x3803` | `SECOND_LEVEL_TEXT` |
| `0x3804` | `SERVER_ATTRIBUTES` |
| `0x3805` | `DATA_FORMAT` |
| `0x3806` | `RESULT_DATA` |
| `0x3807` | `SQLCA` |
| `0x3808` | `MARKER_FORMAT` |
| `0x380B` | `PACKAGE_INFO` |
| `0x380C` | `EXTENDED_DATA_FORMAT` |
| `0x380D` | `EXTENDED_MARKER_FORMAT` |
| `0x380E` | `EXTENDED_RESULT_DATA` |
| `0x380F` | `LOB_LOCATOR` |
| `0x3810` | `LOB_DATA` |
| `0x3811` | `EXTENDED_COLUMNS` |
| `0x3812` | `SUPER_DATA_FORMAT` |
| `0x3813` | `SUPER_MARKER_FORMAT` |
| `0x3814` | `CURSOR_ATTRIBUTES` |
| `0x3832` | `COMPRESSED` |
| `0x3840` | `FIELD_NAME` |

<!-- layout: database::RequestTemplate -->

| Offset | Size | Field |
|---|---|---|
| 20 | 4 | `ors_bitmap` |
| 24 | 4 | `reserved` |
| 28 | 2 | `return_ors` |
| 30 | 2 | `fill_ors` |
| 32 | 2 | `based_on_ors` |
| 34 | 2 | `rpb` |
| 36 | 2 | `marker_descriptor` |
| 38 | 2 | `parameter_count` |

<!-- layout: database::ReplyTemplate -->

| Offset | Size | Field |
|---|---|---|
| 20 | 4 | `ors_bitmap` |
| 24 | 4 | `compressed` |
| 28 | 2 | `return_ors` |
| 30 | 2 | `return_function` |
| 32 | 2 | `request_function` |
| 34 | 2 | `error_class` |
| 36 | 4 | `return_code` |

The server attributes record (offsets within the parameter's data;
the default schema name follows, `schema_length` bytes):

<!-- layout: database::ServerAttributes -->

| Offset | Size | Field |
|---|---|---|
| 0 | 2 | `reserved` |
| 2 | 2 | `date_format` |
| 4 | 2 | `date_separator` |
| 6 | 2 | `time_format` |
| 8 | 2 | `time_separator` |
| 10 | 2 | `decimal_separator` |
| 12 | 2 | `naming` |
| 14 | 2 | `ignore_decimal_errors` |
| 16 | 2 | `commit_level` |
| 18 | 2 | `package_size` |
| 20 | 1 | `translation` |
| 21 | 2 | `server_ccsid` |
| 23 | 2 | `nlss` |
| 25 | 3 | `language_id` |
| 28 | 10 | `language_table` |
| 38 | 10 | `language_table_library` |
| 48 | 4 | `language_feature` |
| 52 | 10 | `functional_level` |
| 62 | 18 | `rdb_name` |
| 80 | 10 | `default_library` |
| 90 | 26 | `job` |
| 116 | 2 | `schema_length` |

<!-- layout: database::Sqlca -->

| Offset | Size | Field |
|---|---|---|
| 0 | 8 | `sqlcaid` |
| 8 | 4 | `sqlcabc` |
| 12 | 4 | `sqlcode` |
| 16 | 2 | `sqlerrml` |
| 18 | 70 | `sqlerrmc` |
| 88 | 8 | `sqlerrp` |
| 96 | 4 | `sqlerrd1` |
| 100 | 4 | `sqlerrd2` |
| 104 | 4 | `sqlerrd3` |
| 108 | 4 | `sqlerrd4` |
| 112 | 4 | `sqlerrd5` |
| 116 | 4 | `sqlerrd6` |
| 120 | 11 | `sqlwarn` |
| 131 | 5 | `sqlstate` |

<!-- layout: database::FormatHeader -->

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | `consistency` |
| 4 | 4 | `field_count` |
| 8 | 4 | `date_time` |
| 12 | 4 | `record_size` |

<!-- layout: database::FieldRecord -->

| Offset | Size | Field |
|---|---|---|
| 0 | 2 | `ll` |
| 2 | 2 | `sql_type` |
| 4 | 4 | `length` |
| 8 | 2 | `scale` |
| 10 | 2 | `precision` |
| 12 | 2 | `ccsid` |
| 14 | 1 | `parameter_type` |
| 15 | 2 | `join_ref` |
| 17 | 4 | `lob_locator` |
| 21 | 1 | `flags` |
| 22 | 4 | `max_cardinality` |
| 26 | 4 | `lob_max` |
| 30 | 2 | `reserved` |
| 32 | 4 | `var_offset` |
| 36 | 4 | `var_length` |
| 40 | 8 | `reserved2` |

<!-- layout: database::DataHeader -->

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | `consistency` |
| 4 | 4 | `row_count` |
| 8 | 2 | `column_count` |
| 10 | 2 | `indicator_size` |
| 12 | 4 | `reserved` |
| 16 | 4 | `row_size` |

## Data queue server

Exchange attributes (`0x0000`) answers `0x8000` with the release
(`V7R1M0`). Requests name the queue and library (EBCDIC, 10 each) in the
template; entries travel in `0x5001`, keys in `0x5002`; most requests
answer the common reply `0x8002`, reads `0x8003`. The templates are kept
as bytes until the server is built; the traces show create, write
(keyed and not), attributes, peek, read, clear and delete.

<!-- consts: dtaq::id -->

| Value | Name |
|---|---|
| `0x0000` | `EXCHANGE_ATTRIBUTES` |
| `0x0001` | `ATTRIBUTES` |
| `0x0002` | `READ` |
| `0x0003` | `CREATE` |
| `0x0004` | `DELETE` |
| `0x0005` | `WRITE` |
| `0x0006` | `CLEAR` |
| `0x8000` | `EXCHANGE_ATTRIBUTES_REPLY` |
| `0x8001` | `ATTRIBUTES_REPLY` |
| `0x8002` | `COMMON_REPLY` |
| `0x8003` | `READ_REPLY` |

<!-- consts: cp::dtaq -->

| Value | Name |
|---|---|
| `0x5001` | `ENTRY` |
| `0x5002` | `KEY` |

## File server

Exchange attributes (`0x0016`) answers `0x8009`. JTOpen names paths in
UTF-16 (`0x0002`, CCSID 1200 in the template); a directory listing
(`0x000A`) answers one `0x8005` per entry; data is written in `0x0020`
and read back in `0x0005`. The templates are kept as bytes until the
server is built; the traces show listing, create directory, open,
write, read, attributes, rename and delete under `/home/QSTEST`.

<!-- consts: file::id -->

| Value | Name |
|---|---|
| `0x0001` | `COPY` |
| `0x0002` | `OPEN` |
| `0x0003` | `READ` |
| `0x0004` | `WRITE` |
| `0x0006` | `COMMIT` |
| `0x0007` | `LOCK` |
| `0x0008` | `UNLOCK` |
| `0x0009` | `CLOSE` |
| `0x000A` | `LIST_ATTRIBUTES` |
| `0x000B` | `CHANGE_ATTRIBUTES` |
| `0x000C` | `DELETE_FILE` |
| `0x000D` | `CREATE_DIR` |
| `0x000E` | `DELETE_DIR` |
| `0x000F` | `RENAME` |
| `0x0012` | `CREATE_DIR_HANDLE` |
| `0x0015` | `QUERY_SPACE` |
| `0x0016` | `EXCHANGE_ATTRIBUTES` |
| `0x001A` | `LOOKUP` |
| `0x001C` | `OPEN_NODE` |
| `0x001D` | `FREE_HANDLE` |
| `0x0021` | `GET_FILE_SYSTEM` |
| `0x8001` | `RETURN_CODE_REPLY` |
| `0x8002` | `OPEN_REPLY` |
| `0x8003` | `READ_REPLY` |
| `0x8004` | `CLOSE_REPLY` |
| `0x8005` | `LIST_ATTRIBUTES_REPLY` |
| `0x8006` | `CREATE_DIR_HANDLE_REPLY` |
| `0x8007` | `QUERY_SPACE_REPLY` |
| `0x8009` | `EXCHANGE_ATTRIBUTES_REPLY` |
| `0x800A` | `LOCK_REPLY` |
| `0x800B` | `WRITE_REPLY` |
| `0x800C` | `LOOKUP_REPLY` |
| `0x800E` | `OPEN_NODE_REPLY` |
| `0x800F` | `GET_FILE_SYSTEM_REPLY` |

<!-- consts: cp::file -->

| Value | Name |
|---|---|
| `0x0002` | `NAME` |
| `0x0005` | `READ_DATA` |
| `0x0020` | `WRITE_DATA` |

## Network print server

The request ID names the object type and the reply's is `0x8000` plus
the type; every frame has the same 12-byte template whose first word is
the action. Exchange attributes are action `0x000F` (retrieve
attributes) on the network print server object (`0x0008`); a spooled
file list is action `0x0016` on spooled files and answers one `0x8001`
per file. Attribute lists are nested code points, kept as bytes.

<!-- consts: netprt::id -->

| Value | Name |
|---|---|
| `0x0001` | `SPOOLED_FILE` |
| `0x0002` | `WRITER_JOB` |
| `0x0003` | `PRINTER_DEVICE` |
| `0x0004` | `OUTPUT_QUEUE` |
| `0x0005` | `PRINTER_FILE` |
| `0x0006` | `LIBRARY` |
| `0x0007` | `RESOURCE` |
| `0x0008` | `NP_SERVER` |

<!-- consts: netprt::action -->

| Value | Name |
|---|---|
| `0x0001` | `CREATE` |
| `0x0002` | `OPEN` |
| `0x0003` | `READ` |
| `0x0004` | `WRITE` |
| `0x0005` | `CLOSE` |
| `0x0006` | `HOLD` |
| `0x0007` | `RELEASE` |
| `0x0008` | `START` |
| `0x0009` | `END` |
| `0x000A` | `DELETE` |
| `0x000B` | `MOVE` |
| `0x000C` | `SEND` |
| `0x000D` | `CALL_EXIT_PROGRAM` |
| `0x000E` | `CHANGE_ATTRIBUTES` |
| `0x000F` | `RETRIEVE_ATTRIBUTES` |
| `0x0010` | `RETRIEVE_ATTRIBUTE_INFO` |
| `0x0011` | `RETRIEVE_MESSAGE` |
| `0x0012` | `ANSWER_MESSAGE` |
| `0x0013` | `WATCH` |
| `0x0014` | `CHECK_AUTHORITY` |
| `0x0015` | `PURGE` |
| `0x0016` | `LIST` |
| `0x0017` | `SEEK` |
| `0x0018` | `TELL` |
| `0x0019` | `SELECT_PAGE` |
| `0x001A` | `OPEN_MODIFIED` |
| `0x001B` | `COPY` |

<!-- layout: netprt::NpTemplate -->

| Offset | Size | Field |
|---|---|---|
| 20 | 2 | `action` |
| 22 | 4 | `flags` |
| 26 | 2 | `rc` |
| 28 | 4 | `error` |

## Trace inventory

Recorded 2026-10-24 against the primary oracle (IBM i 7.1, `QPWDLVL`
0) with JTOpen through `tools/conformance/jtopen/run.sh`; the client's
results are in `tools/conformance/jtopen/expected/oracle/`.

| Scenario | Servers | What it shows |
|---|---|---|
| `signon` | signon | Exchange attributes, then the server closes: validate, sign-on information, profile token and unknown user all fail with a dropped connection |
| `pwdlevel` | signon, central, rmtcmd, database | The sign-on and central servers close at once; remote command and database complete the seed exchange and start server (DES substitutes, verified) |
| `svrmap` | svrmap and every server | A port mapper lookup per service, then each server's start; sign-on and central close |
| `cmdcall` | rmtcmd | A command that succeeds, one not allowed in the server job (`CPD0031`), a missing object (`CPF9801`), an unknown command (`CPD0030`) |
| `pgmcall` | rmtcmd | `QWCRSVAL` (four system values) and `QUSRJOBI` with marshalled input, output and input-output parameters; a missing program (`0x9003`, `MCH3401`) |
| `sysval` | rmtcmd, central | Start only: JTOpen's `SystemValue` needs the release and job CCSID from the sign-on and central servers and fails |
| `central` | central | NLS exchange attributes and four conversion table downloads, each closed by the server |
| `jdbc` | database | Connect, metadata (`getTables`, `getColumns`), a blocked fetch of 115 catalog rows, create/insert 300 (batched)/select/update/delete/drop of `QSTEST.HSTRACE1`, null and type round trips, an SQL error (`SQL0204`), commit and rollback, a failed `CALL` |
| `dtaq` | dtaq, central | Create, write, attributes, peek, read, clear, delete of `QSTEST/HSTRACEQ`, and a keyed queue |
| `ifs` | file, central | List `/home/QSTEST`, create a directory, write, read, attributes, rename, delete |
| `splf` | netprt | Exchange attributes and a list of the user's spooled files |

Missing, and why:

- Sign-on server flows (sign-on information, profile tokens, change
  password, sign-on messages) and every central server reply: the
  oracle's sign-on and central servers accept connections and close
  them without answering. The suite starts the other servers directly
  (JTOpen's skip-sign-on path, `QS_HS_SKIP_SIGNON=1`) so their traces
  exist; QS/400 runs it with the sign-on server.
- SHA-1 and SHA-512 substitutes on the wire: the oracle is at
  `QPWDLVL` 0. pub400 (7.5) was not recorded in this pass (its single
  session was in use).
- TLS ports: out of scope for the recorder.
- Change password: never run against an oracle.
- The DDM/DRDA server (446/448) is not part of this recorder.
