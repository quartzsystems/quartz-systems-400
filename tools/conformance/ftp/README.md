# FTP server transcripts

Recorded against pub400.com's FTP server (IBM i V7R5M0, port 21,
explicit-TLS not used) on 2026-10-24 by a scripted client from the
harness host, one control connection per session, 400 ms between
commands. They seed the Phase 10 FTP replay (~200 sessions): each is the
oracle's dialect for one area, with the reply codes and texts as sent.

Format, one line per event:

| Prefix | Meaning |
|---|---|
| `# session:`, `# source:` | the session name, the oracle and the date |
| `C: ` | a command the client sent (`PASS ********`) |
| `S: ` | a reply line from the server, multi-line replies line by line |
| `D: N bytes` | a data connection's payload follows: `D|` lines (text, line ends removed) or hex (`(hex)`, 32 bytes a line) |
| `D> N bytes sent` | the client sent N bytes on the data connection (`STOR`, `APPE`, `STOU`) |

Masks: the account name in any case is `{user}` (its libraries are
`{user}B` and so on, its home `/home/{user}`), the password never
appears, and the client's address is `{client}`. Data connections were
made to the control host with the port from the `227` reply (`EPSV`
ports were not reachable through pub400's front end).

| Transcript | Covers |
|---|---|
| `transcripts/basic.txt` | banner, `USER`/`PASS`, `SYST`, `FEAT`, `HELP`, `HELP SITE`, `SITE HELP`, `HELP STOR`, `STAT`, `PWD`/`XPWD`, `NOOP`, `OPTS UTF8`, an unknown command, `SITE` alone, a bad `SITE NAMEFMT` |
| `transcripts/namefmt.txt` | `PWD`/`CWD`/`CDUP`/`XCWD`/`XCUP` in `NAMEFMT 0` and `1`, `SITE NAMEFMT`/`LISTFMT` queries, missing library and directory |
| `transcripts/list.txt` | `LIST`/`NLST` in `LISTFMT 0` and `1` on `QSYSINC` (the current library), a file, a member, a missing member, `{user}B`, the home directory, a `/QSYS.LIB` file and `/QOpenSys`; listings longer than 40 lines are cut with a count, and a listing of `QGPL` (other users' objects) was removed |
| `transcripts/types.txt` | `TYPE A`/`A N`/`I`/`E`/`L 8`/bad, `MODE`, `STRU`, `PASV`, `EPSV`, `REST`, `ALLO`, `ACCT`, `PBSZ`/`PROT` without TLS, `SITE CRTCCSID` and `SITE TRIM` |
| `transcripts/stmf.txt` | stream files in the home: `STOR` text and binary, `SIZE`, `MDTM`, `MLST`, `MLSD`, `RETR` in `TYPE A`/`I`/`E`, `APPE`, `STOU`, `RNFR`/`RNTO`, `MKD`/`XMKD`/`RMD`/`XRMD`, `DELE`, and the replies for a missing file |
| `transcripts/member.txt` | members in `{user}B` (the session starts in the home directory, where `SITE NAMEFMT 0` is refused until a library is the current directory): `RCMD` (`CHKOBJ` found and not, `DSPLIBL`, an unknown command, `CRTSRCPF`), `STOR` into a new source member and into files that do not exist (auto-create), `RETR` in `TYPE A`/`E`/`I`, `APPE`, `SIZE`, `MDTM`, `RNFR`/`RNTO` of a member, `DELE`, missing objects; everything created was deleted in the same session |

The recorder is a small Python client kept with the capture notes (not
shipped); a replay needs only these files.

## The replay corpus (second round)

186 more sessions, recorded on 2026-10-30 by the same client in
categories, one folder each under `transcripts/`:

| Folder | Sessions | Covers |
|---|---|---|
| `proto/` | 27 | `HELP` of every command, case, missing and bad arguments, `PORT`/`EPRT`, `ABOR`, `REIN`, `USER` twice, `OPTS`, TLS commands without TLS, every `SITE` value, `STAT`, commands before sign-on |
| `namefmt/` | 21 | `CWD`/`CDUP`/`PWD` in both naming formats: relative and `..`, `/QSYS.LIB`, libraries by name and path, names that are not valid, a file and a member as directory, the switch to `NAMEFMT 1` |
| `list/` | 21 | `LIST`/`NLST` in both listing formats: the home, stream files and wildcards, an empty directory, members in `NAMEFMT 0` and as `/QSYS.LIB` paths, a save file, a data area, `/QOpenSys/usr` with a symbolic link, `ls` options |
| `stmf/` | 35 | stream files: ten text payloads in `TYPE A` read back in `A`/`I`/`E`, binary, `TYPE E` and `TYPE C` stores, replace, `APPE`, `STOU`, renames, directories, errors, `MODE B`/`STRU R` |
| `member/` | 41 | members: source and data files, every payload into a file `STOR` makes, record lengths 1 to 240, `TRIM`, `CRTCCSID`, truncation, `APPE`, `STOU`, renames within and across files, `DELE`, qualified and path names, save files, a 70000-byte binary member |
| `rcmd/` | 17 | `RCMD`: found and missing objects, syntax errors, the library list, data areas, files and members, CL-only and interactive commands, calls, overrides, quoting |
| `xfer/` | 24 | each type into stream files and members, line ends alone, every payload into a source member read back in `E`/`I`/`A`, a path into `/QSYS.LIB` |

Two more prefixes: `D> N bytes sent` is followed by `U|` lines, the
payload in hex, 32 bytes a line; listings longer than 60 lines are cut
with a count. Text the oracle sent in EBCDIC (a listing in `TYPE E`) is
in `D|` lines read as Latin-1, as the recorder took it. The account's
name appears in no encoding: text has `{user}`, and bytes (hex blocks
and EBCDIC text) have the stand-in `QSFTPUSR`, which is as long, so
listings keep their columns. Two listings of other profiles' home
directories were dropped (`# redacted`). Every session removed what it
made; the account's home and library were checked clean after the run.

`fixture.txt` is the world the sessions met (the home's `qs400/` and
`qstest/`, the library's files, `QSYSINC`'s files and members,
`/QOpenSys/usr`), taken from the transcripts' own listings; rows under
`[session]` are for that session alone. `gaps.txt` lists the few
commands the fixture cannot reproduce. `cargo test -p conformance
--test ftp_replay` replays every session against the server in memory
(`tools/conformance/src/ftp/`) and fails when fewer pass than
`baseline.txt` says; the table and every difference go to
`target/conformance-ftp/report.md`.
