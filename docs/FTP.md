# FTP

Phase 10 section 5: the `*FTP` server and the `FTP` CL client. The
protocol is IBM i's server dialect (V7R5M0, recorded on pub400): its
reply codes everywhere, its exact text where clients parse it (`PWD`
and `CWD`, `SYST`, the `LISTFMT 0` listing, `PASV`), Quartz text where
IBM's is prose. A session runs in a job of its own, signs the user on
through the storage service and takes the user's profile, reaches the
IFS (and `/QSYS.LIB`) over its link, and runs `RCMD` in its own
runtime.

## Layout

| Path | What |
|---|---|
| `net/ftp/src/session/` | `Session`: one control connection's state and the answer to each command (`mod.rs` dispatch; `login.rs` sign-on and `QIBM_QTMF_SVR_LOGON`; `dirs.rs` names, `PWD`/`CWD`/`MKD`/`RMD`/`DELE`/`RNFR`/`RNTO`; `xfer.rs` `RETR`/`STOR`/`APPE`/`STOU`, block mode, `ABOR`; `listing.rs` `LIST`/`NLST`/`MLSD`/`MLST`/`SIZE`/`MDTM`; `site.rs` `SITE`, `RCMD`, IBM's object commands; `info.rs` `SYST`/`FEAT`/`HELP`/`STAT`/`TYPE`/`MODE`/`STRU`/`PASV`/`EPSV`/`PORT`/`EPRT`/`REST` and TLS) |
| `net/ftp/src/host.rs` | `Host`: what a session needs from its job |
| `net/ftp/src/names.rs` | paths, `/QSYS.LIB` nodes, `NAMEFMT 0` names (`LIB/FILE.MBR`), generic names |
| `net/ftp/src/objects.rs` | what a path is; whole objects read and written through the IFS; members as records |
| `net/ftp/src/store.rs` | objects in a transfer's form, shared by server and client: `fetch`, `put`, the files `STOR` makes, member moves |
| `net/ftp/src/xlate.rs` | `TYPE A`/`E`/`I`/`C`: conversion, line ends, records, trimming |
| `net/ftp/src/list.rs` | the `LISTFMT 0` and `LISTFMT 1` line layouts, dates |
| `net/ftp/src/net/` | `Net`: replies, data connections, TLS; `tcp.rs` over sockets, `mem.rs` in memory |
| `net/ftp/src/server.rs` | a session over a socket; `server_tls` (the application ID's certificate) |
| `net/ftp/src/client/` | the `FTP` command's client: `conn.rs` connections, `sub.rs` subcommands, `mod.rs` the session |
| `net/ftp/src/block.rs` | block mode framing |
| `net/ftp/src/memhost.rs` | a `Host` in memory for tests and the replay |
| `net/servers/src/attrs/ftp.rs` | `CHGFTPA`'s attributes |
| `net/servers/src/framework/builtin.rs` | `Ftp`, the framework's hooks |
| `work/qsjob/src/work/ftp.rs` | roles `FTP` (the listener) and `FTPSESS` (a session job) |
| `work/qsjob/src/command/tcpsvr.rs`, `command/ftp.rs` | `CHGFTPA`; `FTP` and `STRTCPFTP` |
| `command/sys-cpps/cmds/` | `CHGFTPA.cmd`, `FTP.cmd`, `STRTCPFTP.cmd` |
| `tools/conformance/ftp/`, `src/ftp/` | the transcripts, the fixture and the replay |
| `tools/conformance/src/ftpsrv.rs` | QEMU step 67 |

## Jobs

`STRTCPSVR SERVER(*FTP)` starts the listener `QTFTP00001` (user `QTCP`,
subsystem `QSYSWRK`, or the one `CHGFTPA SBSD` names) with role `FTP`.
It listens on 21 (unless `ALWSSL(*ONLY)`) and on 990 with TLS from the
first byte (unless `ALWSSL(*NO)`). For each connection it starts a
session job `QTFTP00nnn` (user `QTCP`, role `FTPSESS`) and hands it the
socket over the job's control channel (`SCM_RIGHTS`); the session job
does the TLS handshake of port 990 itself. `ENDTCPSVR SERVER(*FTP)` ends
the listener, which ends its sessions with it.

A session job signs the user on with the storage service's `sign_on`
(no display device: `QLMTSECOFR` is about displays and does not apply,
as on IBM i), then takes the profile with `QSYGETPH` and `QWTSETP` (the
security API's `GetHandle` and `SetHandle`): the job stays `QTCP`'s and
everything it does after (IFS, `/QSYS.LIB`, `RCMD`) is the user's. The
session starts in the user's current library (`CURDIR(*CURLIB)`, `QGPL`
when the profile has none) or home directory (`*HOMEDIR`); `NAMEFMT 0`
needs a library, so a session that starts in a directory starts in
format 1.

## `CHGFTPA`

The oracle's keywords, ranges and defaults (V7R5M0 `QCDRCMDD`, checked by
`command/sys-cpps/tests/tcp_defs.rs`); kept in the TCP/IP server table in
`QUSRSYS` (`QATOCSVRA`) like `CHGTELNA`'s.

| Keyword | Default | Effect |
|---|---|---|
| `AUTOSTART` | `*YES` | whether `STRTCP` starts the server (the table's `QATOCSTART`) |
| `NBRSVR` | 3 | kept; a session job starts per connection |
| `INACTTIMO` | 300 | a session silent this many seconds ends (`421`); 0 never |
| `CCSID` | 819 | the ASCII CCSID of `TYPE A` |
| `TBLFTPOUT`, `TBLFTPIN` | `*CCSID` | kept; conversion is by CCSID |
| `NAMEFMT` | `*LIB` | the naming format a session starts in |
| `CURDIR` | `*CURLIB` | the current library, or `*HOMEDIR` |
| `LISTFMT` | `*DFT` | the listing format a session starts in (`*UNIX` is 1) |
| `CRTCCSID` | `*CALC` | the CCSID of database files `STOR` makes |
| `SBSD` | `*DFT` | the subsystem of the jobs (`QSYSWRK`) |
| `ALWSSL` | `*YES` | `*NO`: no TLS; `*ONLY`: port 990 and `AUTH TLS` only |

`PORT` (21), `TLSPORT` (990) and `DTACNNPORT` (`lo-hi`, the passive data
ports; any when empty) ride in the same bag; no command sets them.

## The server's dialect

### Naming

- `NAMEFMT 1` names are paths, relative to the current directory; `~`
  is the home directory. In `/QSYS.LIB`, a relative `X.LIB` is a library
  beside the current one. Names in `/QSYS.LIB` are upper-cased.
- `NAMEFMT 0` names are `[LIB/]FILE[.MBR]`: the current library when no
  library is given, the file's name as the member when no member is
  (for a save file, the save file). `DELE`, `RNFR` and `RNTO` of a name
  without a member mean the file itself.
- `SITE NAMEFMT 0` needs a library (or `/QSYS.LIB`) as the current
  directory. `CWD LIB` in format 0 sets the current library (and
  `RCMD CHGCURLIB` changes it too); after a format-0 `CWD` a path is
  refused (`501`), else `CWD /path` switches to format 1 and says so on
  a first reply line. `CDUP` in format 0 goes to `QSYS`.
- `PWD` and `CWD` answer exactly as IBM's: `257 "/home/u" is current
  directory.`, `"/QSYS.LIB/X.LIB" is current library.`, in format 0
  `"X" is current library.` A file can be the current directory; a
  member cannot.

### Listings

- `LISTFMT 0`: `owner(10) size(11) yy-mm-dd hh:mm:ss type(10) name`; a
  member line leaves size and dates blank (`*MEM`). `LISTFMT 1`: `ls -l`
  (`mode nlink(3) owner(8) group(8) size(7) date name`, the date `Mon dd
  hh:mm` within half a year, else `Mon dd yyyy `), a file a directory, a
  symbolic link `name -> target`.
- In format 1 a name in `LISTFMT 0` and in `NLST` is the path relative to
  the current directory when it is under it, else the whole path, a
  directory's with `/`; `LISTFMT 1` shows the last part. `LISTFMT 1`
  drops `ls` options (`LIST -la`); `LISTFMT 0` takes them as names.
- In format 0 a bare name is a library (its files with their members,
  `LIB/` before names of another library in `LISTFMT 0` and `NLST`);
  `FILE.MBR` and `FILE.*` list members only; a generic name (`QSF*`)
  picks files of the current library; `NLST` lists `FILE.MBR` (a file
  without members by its name).
- An empty directory, a member not in its file, and generic names that
  match nothing answer `125` with the reason, no data, then `250`; a
  listing that cannot start answers `550`. Listings end in `250`, as
  IBM's do. A member's size counts two line-end bytes a record.
- Listings travel in the transfer's form: in `TYPE E` they are EBCDIC
  with `NL` line ends.

### Transfers

| | Stream file | Member |
|---|---|---|
| `TYPE A` out | converted from the file's CCSID to the ASCII CCSID | each record converted, trailing blanks dropped (`SITE TRIM 1` or `2`), `CRLF` |
| `TYPE A` in | a new file is tagged with the ASCII CCSID, bytes as sent; an existing one gets them converted | lines (at `LF`, a `CR` before it dropped) converted, cut or padded to the record length |
| `TYPE E` | the job's EBCDIC CCSID; line ends become `NL` (X'15') out, `CR LF` in | records trimmed and ended by `NL` |
| `TYPE I` | the bytes | records back to back (a source member's data part); in: cut at the record length, the last padded with nulls |
| `TYPE C n` | as `A`, in CCSID `n` (as `E` for an EBCDIC CCSID) | as `A` in CCSID `n` |

- `STOR` into a file that is not there makes it: `CRTPF RCDLEN(n)
  MBR(m) MAXMBRS(*NOMAX)`, `n` the longest line (512 for binary data),
  in the CCSID `CRTCCSID` says (`*CALC` is the job's EBCDIC CCSID). A
  member not in an existing file is added. Lines longer than the
  records are cut and the transfer ends in `426`.
- Save files go in `TYPE I` only (`504` otherwise); data that is not
  whole 528-byte records is refused (`426`).
- `STOU` adds 1, 2, ... to a name in use. `APPE` adds records or bytes.
- A missing member of an existing file answers `426`, a missing file or
  library `550`; a directory parent that is not there `426`.
- `MODE B` and `STRU R` are taken; a stream file refuses them at the
  transfer (`504`), members move in block mode.
- The preliminary reply (`150`, `125`) goes before the data connection
  opens, so clients that connect after it and clients that connect
  before it both work. `ABOR` during a transfer answers `426` then
  `226`; without one, `226`.

### Other commands

- `SITE NAMEFMT`, `LISTFMT`, `TRIM` (0, 1, 2), `NULLFLDS`, `CRTCCSID`
  (`*CALC`, `*USER`, `*SYSVAL`, 1-65533): with no value they report;
  words after the value are not read; others answer `550`.
- `RCMD cmd` runs the CL command in the session job: `250`, or a
  two-line `550` with the escape message's text. `ADDM`, `ADDV`, `CRTL`,
  `CRTP`, `CRTS`, `DLTF`, `DLTL` run `ADDPFM`, `CRTLIB`, `CRTPF`,
  `CRTSRCPF`, `DLTF`, `DLTLIB`.
- `TYPE`: `A`/`E` with `N` or nothing, `I`, `L 8`, `B`, `F`, `C n`;
  another format `504`, another type `501`. `MODE S|B`, `STRU F|R`
  (`250`), `C` and `P` `504`, others `501`.
- `PORT` and `EPRT` take any address; the data connection itself goes
  only to the client's own (RFC 2577), else `425`. `EPSV ALL` is `EPSV`.
- `USER` while signed on answers `501` (`REIN` first). Before sign-on a
  session takes `SITE`, `TYPE`, `MODE`, `STRU`, `PASV`/`EPSV`/`PORT`/
  `EPRT`, `ALLO`, `STAT`, `SYST`, `FEAT`, `HELP`, `NOOP` and the TLS
  commands; the rest answer `530`.
- `HELP` of any name answers `214` (the list for an unknown one).
  `STAT` with a name answers `504`, as IBM's does. `ACCT` `503`, `ALLO`
  `202`.

### Deviations

| Command | IBM i 7.5 | Here |
|---|---|---|
| `SIZE`, `MDTM`, `MLST`, `MLSD` | `500` | RFC 3659: the size `RETR` would send in the current type, the change time in UTC, facts `type`, `size`, `modify`, `perm` |
| `REST` | `504` | RFC 3659 stream restart for `RETR`, `STOR` |
| `FEAT` | Kerberos features | `AUTH TLS`, `CCC`, `EPRT`, `EPSV`, `MDTM`, `MLST`, `PBSZ`, `PROT`, `REST STREAM`, `SIZE` |
| `OPTS` | `501` always | `OPTS MLST` sets the facts; others `501` |
| `PBSZ` without TLS | `421` (the session stays) | the same code; the session stays |
| `PORT` to another host | accepted | accepted; the data connection is refused |

### TLS

`AUTH TLS` (and `TLS-C`, `SSL`, `TLS-P`; the last two make data private
at once) answers `234` and does the handshake; `PBSZ 0`, `PROT C|P`
(`S`, `E` answer `536`), `CCC` back to clear text. Port 990 is TLS from
the first byte. The certificate is the one assigned to the application
ID `QIBM_QTMF_FTP_SERVER` in `*SYSTEM`, which the session job asks the
storage service for (`cert_app`, docs/CERTS.md) as `QTCP` before the
user signs on; with none assigned, no store or a refusal it uses the
system's self-signed certificate (`work/qsjob/src/work/tls.rs`), as
`*HTTP` does.

### Exit points

- `QIBM_QTMF_SERVER_REQ` (`VLRQ0100`, application 1): before the
  session (operation 0, user `*N`; a refusal answers `421` and closes),
  `MKD` (1), `RMD` (2), `CWD` (3), `LIST`/`NLST` (4), `DELE` (5),
  `RETR` (6), `STOR`/`APPE`/`STOU` (7), `RNFR` (8), `RCMD` and the
  object commands (9). A refusal answers `550`.
- `QIBM_QTMF_SVR_LOGON` (`TCPL0300`, `TCPL0200` or `TCPL0100`, whichever
  has programs): return code 0 refuses; 1 signs on as sent; 2 with the
  profile and password the program returns; 3 as the returned profile
  without a password; 4, 5, 6 as 1, 2, 3 with the returned current
  library (and home directory). A program that names a profile without a
  password is how anonymous FTP is done: the server takes that profile
  with `QSYGETPH *NOPWD`, so `QTCP` needs `*USE` to it.

## The `FTP` command

`FTP` (and `STRTCPFTP`) `RMTSYS INTNETADR CCSID TBLFTPOUT TBLFTPIN PORT
SECCNN DTAPROT APPID`, the oracle's definition. The client reads its
subcommands from the member the file `INPUT` is overridden to, as IBM's
client does in batch: the first line after a connection is the user,
then the password after a blank; what the session prints goes to the
member `OUTPUT` is overridden to, added to it (the command's output when
`OUTPUT` is not overridden). Lines read are echoed with the password
starred.

`SECCNN(*SSL)` sends `AUTH TLS`, `*IMPLICIT` (or `PORT(*SECURE)` with
`*DFT`) connects with TLS; `DTAPROT` makes data private (the default
with TLS). The server's certificate must chain to the shipped roots or a
trusted CA of `*SYSTEM` and name the host connected to; the client
presents none (the key of `QIBM_QTMF_FTP_CLIENT` goes only to server
profiles), and `APPID` is not read.

Subcommands (shortest forms in `ftp::client::sub`): `OPEN`, `USER`,
`PASS`, `CLOSE`, `QUIT` (`BYE`, `END`), `CD`, `CDUP`, `PWD`, `LCD`,
`LPWD`, `LS`, `DIR`, `GET` (`(REPLACE`), `PUT`, `APPEND`, `MGET`,
`MPUT`, `DELETE`, `RENAME`, `MKDIR`, `RMDIR`, `BINARY`, `ASCII`,
`EBCDIC`, `TYPE`, `MODE`, `STRUCT`, `NAMEFMT` (local and `SITE`),
`QUOTE` (and `RCMD`), `SITE`, `SYSCMD` (`!`), `LOCSITE` (`NAMEFMT`,
`TRIM`, `CRTCCSID`), `SENDSITE`, `SENDPASV`, `SUNIQUE`, `DEBUG`,
`STATUS`, `SYSTEM`, `HELP`, `NOOP`. Local names follow the local naming
format (format 0 when the local current directory is a library), with
the server's transfer rules (`ftp::store`); `QIBM_QTMF_CLIENT_REQ`
(`VLRQ0100`, application 0) is called before each operation. After
sign-on the client asks `SYST` and, in format 1, sends `SITE NAMEFMT 1`
to an IBM i server.

## Conformance

- `cargo test -p ftp`: names, conversions, listing layouts against the
  oracle's lines, block mode, subcommand parsing; sessions in memory
  (sign-on, `REIN`, the attributes a session starts with, files `STOR`
  makes, truncation, renames within and across files, `RCMD`, anonymous
  FTP through the logon exit, the request exit refusing, EBCDIC
  listings); the client against the server over loopback sockets in the
  clear, active and block mode, with `AUTH TLS` and `PROT P`, and on an
  implicit TLS port.
- `cargo test -p conformance --test ftp_replay`: the 192 recorded
  sessions (`tools/conformance/ftp/README.md`) against the server in
  memory. Each command's reply codes must match, and `PWD`/`CWD`/`SYST`
  text, data bytes, text lines and listing fields (names, owners, types,
  the sizes and date forms of what the session made). The fixture is the
  oracle's account world (`QSFTPUSR` stands in for the account: same
  length, same masks, job CCSID 273 as the oracle's `TYPE E` bytes
  show). Result on 2026-10-30:

  | Category | Sessions | Passed | Commands | Matched | Left out |
  |---|---|---|---|---|---|
  | list | 21 | 21 | 411 | 411 | 3 |
  | member | 41 | 41 | 732 | 732 | 0 |
  | namefmt | 21 | 21 | 240 | 240 | 0 |
  | proto | 27 | 27 | 231 | 231 | 6 |
  | rcmd | 17 | 17 | 169 | 169 | 0 |
  | seed | 6 | 6 | 256 | 256 | 15 |
  | stmf | 35 | 35 | 698 | 698 | 5 |
  | xfer | 24 | 24 | 425 | 425 | 0 |
  | all | 192 | 192 | 3162 | 3162 | 29 |

  "Left out" are the deviations above (25) and four commands the fixture
  cannot set up (`ftp/gaps.txt`: listings of other profiles' objects, an
  empty save file's allocation).
- QEMU step 67 (`tools/conformance/src/ftpsrv.rs`): `CHGFTPA`;
  `STRTCPSVR *FTP` starts `QTFTP00001`; a host client through a port
  forward signs on as a new user, stores a member into a file the
  server makes and reads it back, lists it, runs `RCMD`, moves a stream
  file in binary and asks `SIZE`; a session job shows in `WRKACTJOB`;
  the `FTP` command on the console runs a script member against the
  server with `INPUT` and `OUTPUT` overridden; `ENDTCPSVR *FTP` ends it.
