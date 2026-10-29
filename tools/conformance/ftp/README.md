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
