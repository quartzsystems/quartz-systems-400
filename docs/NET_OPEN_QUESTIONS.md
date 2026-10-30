# Phase 10 open questions

Questions Phase 10 answered provisionally, with what QS/400 does now
and how to settle each against the oracle.

## IFS

1. **`CPFA0xx` for errors without an obvious pair.** `QCPFMSG` has no
   `CPFA0xx` for `ENOTDIR`, `EBADF`, `EAGAIN`, `EDEADLK` and most
   socket values; QS/400 sends `CPFA0D4` (file system error, with the
   number) for them, `CPFA0B1` (access problem) for `EROFS` and
   `EROOBJ`, and `CPFA0A2` for `EINVAL`. Settle by provoking each
   condition with `WRKLNK`/`CPY`/`MOV` under `/home/QSTEST` on the
   primary oracle and reading the job log.
2. **Share-mode refusal errno.** An `open()` refused by another open's
   share mode is `EBUSY` here; IBM may answer `ETXTBSY` for some
   combinations. Probe with two JDBC sessions calling `QP0LOPEN`-style
   CL or with `CPY` while a file is held open by `EDTF`.
3. **Check-out refusals.** Writing, renaming or unlinking an object
   checked out by another user is `EBUSY` here. Confirm the errno and
   the `CPFA0xx` (`CPFA09E` assumed).
4. **Default umask.** A new owner's umask is 0o022. IBM documents the
   job's initial `umask` as 0; confirm with `umask` in QSH on pub400.
5. **Directory CCSID on `mkdir`.** A new directory takes its parent's
   CCSID; IBM may tag it with the job CCSID. Check `st_ccsid` of a
   directory made by `MKDIR` from a job with CCSID 37 and one with 273.
6. **Group authority after `CHGPGP` with no private authority.** QS/400
   gives the new group the authority the object already had for its
   group (the `rwx` triple), POSIX-style; IBM may give `*EXCLUDE`.
   Check `DSPAUT` after `CHGPGP` on a file created with mode 0o750.
7. **`CHGATR` authorities.** PC attributes, CCSID and text need the
   owner or `*W`; storage, save and scan attributes the owner or
   `*OBJALTER`. IBM's `CHGATR` documentation lists per-attribute needs
   that may differ (`*OBJMGT` for some).
8. **Source members as streams.** Settled on pub400 (7.5, 2026-10-29,
   a scratch source file in `{user}B`, removed after; JTOpen
   `IFSFileInputStream`/`IFSFileOutputStream` and QSH). A binary read
   of a member of four 80-byte records gives 320 bytes: the data
   records without sequence number and date, trailing blanks kept, no
   line ends. `IFSFile.length()` says 328, two bytes per record more.
   QSH `cat` (text mode) gives each whole record followed by `0x25`
   (324 bytes in CCSID 273). A text-mode write of `ONE\nTWO   \n`
   leaves two records, `ONE` and `TWO` padded to 80. A binary write of
   `ABC\r\nDEF` leaves one record holding those eight bytes: binary
   writes are not split at line ends. QS/400 does all of this. Open:
   the binary read after that write gave back only the eight bytes, not
   a padded record; QS/400 gives the record padded with blanks. A QSH
   write to a data member (`PF-DTA`) left it empty on IBM; QS/400 writes
   the bytes as fixed-length records.
9. **Fallback owner.** A node without metadata whose UID maps to no
   profile is owned by `QDFTOWN`. IBM's `RCLLNK` may assign `QDFTOWN`
   only for objects whose owner profile was deleted; nodes made outside
   the system have no IBM equivalent.
10. **`O_CREAT` on a dangling symbolic link.** QS/400 answers `EEXIST`
    (the link itself is the last name); POSIX creates the link's target.
    Check IBM with a link to a missing file under `/home/QSTEST`.

## TCP/IP configuration (tcpcfg, 2026-10-29)

- `QSYS/QTCPMSG` was not captured (the query read `QTCP/QTCPMSGF`). The
  configuration's own identifiers (`TCP1A00`-`TCP1A14`, `TCP1D03`-
  `TCP1D0D`) are ours in IBM's families; a capture of `QSYS/QTCPMSG`
  settles their numbers and substitution layouts.
- The keywords IBM keeps in `QATOCTCPIP` (`KEYWORD`/`DATA`) are not
  known; ours are the commands' keywords. A record dump of the oracle's
  file settles it (read-only: `DSPPFM QUSRSYS/QATOCTCPIP`).
- Where V7R5M0 keeps IPv6 interfaces, routes and host names is not in
  the capture (no `QATOC*6` file; the host table's `INTERNET` is 15
  characters). Ours are `QATOCIFC6`, `QATOCRTE6`, `QATOCHST6`.
- V7R1M0's TCP/IP jobs (`QTCPIP`, `QTCPMONITR` in `QSYSWRK`) were not
  seen; we follow V7R5M0 (`QTCPCTL`, `QTCPWRK`, type `SYS`, no
  subsystem).
- The host table lists IPv6 addresses before IPv4 (pub400: `::1`,
  `127.0.0.1`, `185.113.5.134`); within a family ours is numeric order,
  which a table with more entries would confirm.
- `CHGTCPDMN` on V7R5M0 has `DMNSVRRTY` (attempts, interval) where
  earlier releases had `INITQRYINT` and `QRYTIMOUT`; the resolver's
  `HostConfig` keeps those two names, filled from `DMNSVRRTY`.
- `IPSRCRTG` defaults to `*YES` as the oracle's definition and prompt
  show, which sets `accept_source_route` to 1 at `STRTCP`.
- `GO CFGTCP` is a `*PGM` menu on IBM (`ui/menus/inventory`); pub400
  refused it to the profile. Ours is the `CFGTCP` command alone.
- Whether `ADDTCPIFC` on a line that does not exist is refused at once
  (ours: `TCP9509`) or at `STRTCPIFC` is not captured (the profile could
  not run it).

## IFS APIs

Each is settled by running the probe under `tools/conformance/ifs/api/rpg/`
named with it through `rpg-oracle` and diffing its `QSYSPRT` lines with
`expected/` (the kit's test checks QS/400 prints the same as the IR).

1. **`perror` and C's `rename`.** Neither is in the captured exports;
   they are listed under `QC2IO` (`QUSAPIBD` binds it), with `rename`
   following `Qp0lRenameKeep` as the header maps it outside POSIX.
   Check `DSPSRVPGM QC2IO DETAIL(*PROCEXP)` on pub400 (IFSAPI04, IFSAPI14).
2. **`openat` and `AT_FDCWD`.** IBM i has neither; QS/400 exports
   `openat` with Linux's `AT_FDCWD` (-100) for ported code. The RPG probe
   uses `creat` in its place (IFSAPI05).
3. **`Qp0lGetAttr` buffer.** The next header is on a four-byte boundary;
   `QP0L_ATTR_CCSID` and `QP0L_ATTR_CODEPAGE` are two bytes;
   `QP0L_ATTR_TEXT` is a four-byte CCSID and 50 characters; a buffer too
   small answers 0 with bytes returned 0. Read the offsets and sizes the
   oracle gives (IFSAPI10).
4. **`d_reclen`.** Through the name's NUL on a four-byte boundary;
   `d_nlsinfo` holds the job's CCSID and zero country and language.
   (IFSAPI05 prints the names; add `d_reclen` to the probe if it differs.)
5. **`Qp0lProcessSubtree` calls.** The start directory is passed first,
   then each object parents first in name order, then a last call with
   `QP0L_SELECT_DONE`; a nonzero return value ends the walk. IBM's order
   and the final call are unconfirmed (IFSAPI12).
6. **Directory sizes and `nlink`.** `st_size` of a directory is the
   backend's (4096 in memory, xfs's on disk); `nlink` at least 2
   (IFSAPI02 prints both).
7. **`QP0LRRO` for another job.** Only `*` (this job) is answered;
   another job's name is `CPF3C58` until the service shares its lock
   table across jobs (IFSAPI13).
8. **`O_TEXT_CREAT` with a job CCSID data id (0).** Conversion from the
   job's CCSID into the new file's; check the bytes on disk (IFSAPI08).

- **`SRCSTMF` with `SRCFILE`.** The analyzer kit (`tools/conformance/cmd/`)
  accepts `CRTBNDCL`, `CRTCLMOD` and `CRTCMD` strings that give both
  `SRCSTMF` and `SRCFILE`/`SRCMBR`; `RUNSQLSTM` refuses the pair (SQL8019).
  Whether IBM's 7.3+ definitions refuse the pair at analysis time is not
  captured (the 7.1 primary has no `SRCSTMF` on these commands); settle it
  with the strings at pub400.

## Oracle facts (capture 2026-10-24)

What the oracle capture established, read-only, for the later Phase 10
sections. Screens and prompts came from pub400.com (V7R5M0) through the
driver on 2026-10-24 (`tools/conformance/golden/phase10/`, scripts in
`tools/conformance/probes/phase10/`); JDBC and remote-command facts and
the FTP transcripts from the same system on 2026-10-29
(`tools/conformance/oracle/net/pub400/`, `tools/conformance/ftp/`).
The primary oracle (V7R1M0) was unreachable both days: its sign-on
server dropped every connection on 2026-10-24 and every host-server
port was closed on 2026-10-29. Nothing of the primary's side (7.1 job
names, `QUSRSYS` file layouts, `WRKACTJOB`) is captured; the capture
script is ready for it (`oracle/net/README.md`).

### Server jobs and subsystems (V7R5M0)

| Server | Port(s) | Listening job | Subsystem | Worker jobs |
|---|---|---|---|---|
| telnet | 23, 992 | `QTVTELNET` (user `QTCP`, `*BCH`) | `QSYSWRK` | `QTVDEVICE` (`QTCP`), sessions as `QPADEVnnnn` interactive jobs |
| FTP | 21, 990 | `QTFTP00nnn` (`QTCP`, `*BCH`; the lowest-numbered instance listens, 8 were active) | `QSYSWRK` | each `QTFTP00nnn` serves one session |
| NTP | udp 123 | `QTOTNTP` (user `QNTP`, `*BCH`) | `QSYSWRK` | |
| DDM/DRDA | 446 (drda), 447 (ddm), 448 (ddm-ssl) | `QRWTLSTN` (`QUSER`, `*BCH`) | `QSYSWRK` | `QRWTSRVR` prestart in `QUSRWRK` (seen as `*BCI` in `QSYSWRK` too) |
| server mapper | 449 | `QZSOSMAPD` (`QUSER`) | `QSYSWRK` | |
| sign-on | 8476, 9476 | `QZSOSGND` | `QSYSWRK` | `QZSOSIGN` prestart in `QUSRWRK` |
| central | 8470, 9470 | `QZSCSRVSD` | `QSYSWRK` | `QZSCSRVS` prestart in `QUSRWRK` |
| database | 8471, 9471, 8478 (as-transfer) | `QZDASRVSD` | `QSERVER` | `QZDASOINIT`, `QZDASSINIT` prestart in `QUSRWRK` |
| data queue | 8472, 9472 | `QZHQSRVD` | `QSYSWRK` | `QZHQSSRV` prestart in `QUSRWRK` |
| file | 8473, 9473, 8477 (as-netdrive) | `QPWFSERVSD` | `QSERVER` | `QPWFSERVSO`, `QPWFSERVSS`, `QPWFSERVS2`, `QPWFSERV` prestart in `QSERVER` |
| network print | 8474, 9474, 8479 (as-vrtprint) | `QNPSERVD` | `QSYSWRK` | `QNPSERVS` prestart in `QUSRWRK` |
| remote command | 8475, 9475 | `QZRCSRVSD` | `QSYSWRK` | `QZRCSRVS` prestart in `QUSRWRK` |
| sshd | 2222 on pub400 | a `QP0ZSPWT` prestart job of `QUSRWRK` (user `QUSER`, function `PGM-sshd`), the PASE spawn job; sessions are further `QP0ZSPWT` jobs | `QUSRWRK` | |
| HTTP | 80, 443; admin 2001.. | `WWW`/`ADMIN` jobs (`QTMHHTTP`) | `QHTTPSVR` | |
| extended dynamic SQL | 4402 | `QXDAEDRSQL` (`QUSER`) | `QSYSWRK` | |
| debug | 4026 | `QTESDBGHUB` (`QPGMR`) | `QSYSWRK` | |

TCP/IP's own jobs: `QTCPCTL` and `QTCPWRK` (system jobs, no subsystem);
`QTCPIP`/`QTCPMONITR` of older releases do not appear on V7R5M0. The
`QUSRWRK` prestart job entries are `QDBMSRVR`, `QNPSERVS`, `QP0ZSPWT`,
`QRWTSRVR`, `QSCWCHPS`, `QZDASOINIT`, `QZDASSINIT`, `QZHQSSRV`,
`QZRCSRVS`, `QZSCSRVS`, `QZSOSIGN` (all `QSYS`, user `QUSER`);
`QSYSWRK`'s are `QIWVPPJT` (`QIWS`), `Q5BWHSRV`, `QSRRATBL`, `QANEAGNT`,
`QSQSRVR`, `QSRSYNCM` (`QSYS`), `QTMMSRVR`, `QTMSCLCP`, `QTMSSRCP`
(`QTCP`); `QSERVER`'s hold the file server's. `QCMN`/`QBASE` carry the
older `QZRCSRVR`, `QZSCSRVR`, `QNPSERVR`, `QACSOTP`, `QNMAPINGD`,
`QNMAREXECD`, `QOQSESRV`, `QLZPSERV` entries. Full detail (`MAXJOBS`,
`MAXUSE`, `INLJOBS`, `THRESHOLD`, classes) in `prestart-jobs.txt` and
the `DSPSBSD` prints. No LPD, SMTP, POP or REXEC server runs on pub400.

### Configuration objects

`QUSRSYS` holds the TCP/IP configuration as physical files with their
logical files: `QATOCIFC` (interfaces, format `QIFC`, 1024 bytes, 32
fields), `QATOCIFCN` and `QATOCISAVE` (same format), `QATOCRTE`
(`QROUTE`, 512/23) with `QATOCRTEN`, `QATOCHOST` (`QHOSTS`, 1152/8),
`QATOCTCPIP` (attributes, `QATTR`, 2048/2), `QATOCPORT` (port
restrictions), `QATOCRSI` (remote system information), `QATOCPN`/
`QATOCPNRG` (point-to-point), `QATOCRTD`/`QATOCRTDC` (RouteD),
`QATOCSTART`, `QATOCTFTP`, `QATOCBOOTP`, `QATOCPP`/`QATOCPPSCR`/`QATOCPS`/
`QATOCPSAVE`/`QATOCPVIP`/`QATOCRSAVE`; the servers' attributes in
`QATMFTP` (`QTMFTPR`, 100/1), `QATMLPD`, `QATMPOPA`, `QATMRXC`,
`QATMSMTP` (568), `QATMSMTPA`, `QATMSMTLA`, `QATMHTTP`/`QATMHTTPA`,
`QATMHINSTA`/`QATMHINSTC`, `QATMIFCLST`, `QATMIP6ADR`, `QATMIP6IFC`,
`QATMADRLST`, `QATMFTRLST`, `QATOTNTP`, `QATODBTP`; SNMP in `QAOK*`;
DDM/DRDA and SQL packages in `QAZSPP*`/`QAZSPL*`; telnet keeps no file
(its attributes live in system values and `QATOCTCPIP`). `QTMFCCHG`,
`QTMMCCHG`, `QTMSCCHG` are `*DTAARA`s, `QTMFTPS`/`QTMSMTPS` `*JOBD`s,
`QTCPASC`/`QTCPEBC` `*TBL`s. There is no `QTMS*` or `QATOD*` family
beyond `QATODBTP`. `QSYS` holds the templates (`QATOCTCPIP`,
`QATOCTCPI1`, `QATOCIFC`, `QATOCRTE`, ...) and `QATOFIPF`/`QATOFNAT` (IP
filters), `QATOQQOS`, `QATOSLOG`, `QATOVSOF`, `QAZD*` (DCM logs). Field
layouts are in `config-file-fields.txt`.

### Exit points and formats

`QIBM_QTMF_SERVER_REQ` `VLRQ0100`, `QIBM_QTMF_CLIENT_REQ` `VLRQ0100`,
`QIBM_QTMF_SVR_LOGON` `TCPL0100`/`TCPL0200`/`TCPL0300`,
`QIBM_QTMX_SERVER_REQ` `VLRQ0100`, `QIBM_QTMX_SVR_LOGON` `TCPL0100`/
`TCPL0300`, `QIBM_QTMX_SVR_SELECT` `RXCS0100`, `QIBM_QTOD_SERVER_REQ`
`VLRQ0100`, `QIBM_QTOD_DHCP_*`, `QIBM_QTG_DEVINIT` `INIT0100`,
`QIBM_QTG_DEVTERM` `TERM0100`, `QIBM_QZSO_SIGNONSRV` `ZSOY0100`,
`QIBM_QZSC_LM` `ZSCL0100`, `QIBM_QZSC_NLS` `ZSCN0100`, `QIBM_QZSC_SM`
`ZSCS0100`, `QIBM_QZDA_INIT` `ZDAI0100`, `QIBM_QZDA_SQL1` `ZDAQ0100`,
`QIBM_QZDA_SQL2` `ZDAQ0200`, `QIBM_QZDA_NDB1` `ZDAD0100`/`ZDAD0200`,
`QIBM_QZDA_ROI1` `ZDAR0100`/`ZDAR0200`, `QIBM_QZHQ_DATA_QUEUE`
`ZHQ00100`, `QIBM_QHQ_DTAQ` `DTAQ0100`, `QIBM_QPWFS_FILE_SERV`
`PWFS0100`/`PWFS0200`, `QIBM_QNPS_ENTRY` `ENTR0100`, `QIBM_QNPS_SPLF`
`SPLF0100`, `QIBM_QZRC_RMT` `CZRC0100`, `QIBM_QP0L_OBJ_OPEN`/`_CLOSE`
`OBOP0100`/`OBCL0100`, `QIBM_QP0L_SCAN_OPEN`/`_CLOSE` `SCOP0100`/
`SCCL0100`, the `QIBM_QSY_*` profile and password points. The exit
point's attributes (`WRKREGINF` option 5) show preprocessing programs
`QTMFADDP`/`QTMFRMVP` in `QTCP` with formats `ADDP0100`/`RMVP0100`,
maximum one program, change not allowed. 146 points in all
(`exit-points.txt`).

### IFS, SSH and certificates

- pub400's root: `_welcome.txt`, `bin`, `dev`, `etc`, `home`, `imgclg`,
  `lib`, `lib64`, `QDLS`, `QFileSvr.400`, `QIBM`, `QNTC`, `QOpenSys`,
  `QOPT`, `QSR`, `QSYS.LIB`, `QTCPTMM`, `QXDALISTEN`, `tmp`, `usr`, `var`
  and site directories. `/etc` holds `cit`, `ibm`, `magic`, `pip.conf`,
  `utmp`, `vfs`; `/` and `/etc` are owned by `QSYS`, CCSID 37,
  `*PUBLIC *RWX` on `/` with `QDIRSRV *X`.
- A stream file put by FTP in `TYPE A` gets CCSID 819, `*PUBLIC
  *EXCLUDE`, owner `*RWX` with all four object authorities, format
  `*TYPE2`, 8192 bytes allocated, auditing `*NOTAVL`, domain `*SYSTEM`.
- OpenSSH (5733-SC1): product under `/QOpenSys/QIBM/ProdData/SC1/OpenSSH`
  (`bin`, `sbin`, `libexec`, `etc_template`, `empty`); its `etc` is a
  symbolic link to `/QOpenSys/QIBM/UserData/SC1/OpenSSH/etc`, which holds
  `sshd_config`, `ssh_config`, `moduli`, the host keys (`ssh_host_rsa_key`,
  `_ecdsa_key`, `_ed25519_key`, `_dsa_key`, `_key`, with `.pub`) and
  `sshd.pid`. `/QOpenSys/usr/bin/ssh` and `/QOpenSys/usr/sbin/sshd` are
  symbolic links into the product. A second OpenSSH from yum lives in
  `/QOpenSys/pkgs/bin` with `/QOpenSys/etc/ssh/sshd_config`. pub400's
  running `sshd_config` sets `Port 2222`, `PermitRootLogin no`,
  `MaxAuthTries 5`, `LoginGraceTime 5m`, `GatewayPorts yes`, `Banner
  /_welcome.txt`, `Subsystem sftp /QOpenSys/QIBM/ProdData/SC1/OpenSSH/
  libexec/sftp-server`. SSH is not reachable from this PC.
- DCM: `/QIBM/UserData/ICSS/Cert/Server/DEFAULT.KDB` and `DEFAULT.RDB`,
  `/QIBM/UserData/ICSS/Cert/{CertAuth,Signing,Download,Upload}`,
  `/QIBM/UserData/ICSS/{.cssm,CertSvcs,dcm.properties}`,
  `/QIBM/ProdData/ICSS/Cert` (52 entries). `SignObject` and
  `/QIBM/UserData/OS400/DCM` do not exist.
- QSH: `/usr/bin` has 163 entries (names in `qsh-usr-bin.txt`);
  `/usr/bin/qsh` and `/usr/bin/sh` are symbolic links to
  `/QSYS.LIB/QSHELL.LIB/QZSHSH.PGM`; `/QOpenSys/usr/bin/sh`, `bsh`, `ksh`
  link into `/QOpenSys/QIBM/ProdData/OS400/PASE/bin`. `uname -a` prints
  `OS400 <node> 5 7 <serial>` (`-s OS400`, `-r 5`, `-v 7`); `id` prints
  `uid=NNNNN(USER) gid=0`; the default `env` holds `HOME`, `HOSTID`,
  `HOSTNAME`, `HOSTTYPE=powerpc`, `JAVA_HOME`, `LANG=/QSYS.LIB/EN_US.LOCALE`,
  `LOGNAME`, `MACHTYPE=powerpc-ibm-os400`, `OSTYPE=os400`,
  `PATH=/usr/bin:.:/QOpenSys/usr/bin`, `QIBM_DESCRIPTOR_STD{IN,OUT,ERR}`,
  `QIBM_MULTI_THREADED=Y`, `QIBM_PASE_USE_PRESTART_JOBS=Y`,
  `QIBM_USE_DESCRIPTOR_STDIO=Y`, `SHLVL`, `TERMINAL_TYPE=PIPELINE`,
  `TRACEOPT=UNLINK`. There is no `netstat` in `/QOpenSys/usr/bin`. The
  QSH 5250 session's keys: `F3=Exit F6=Print F9=Retrieve F12=Disconnect
  F13=Clear F17=Top F18=Bottom F21=CL command entry`, a `$` prompt line
  and the `===>` input line.
- `STRQSH` from Command Entry logs `New QSH session started` and
  `Current QSH session ended` (QSH messages are in `QSHELL/QZSHMSGF`,
  12 ids `QSH0001`..; the utilities' own `001-nnnn` diagnostics come
  from the utilities themselves).
- `PING` messages: `TCP3204` (verifying `*INTNETADR`), `TCP3203`
  (verifying a host at an address), `TCP3215` (one reply: number,
  address, ms, bytes, TTL), `TCP3211` (round-trip min/avg/max), `TCP3210`
  (statistics n of m, percent), `TCP3202` (unknown host, the escape).
  pub400's resolver appends its domain to an unknown name and a wildcard
  answers, so a bad host name still pings.

### FTP server dialect (V7R5M0)

- Banner `220-QTCP at <host>.` then `220 Connection will close if idle
  more than N minutes.`; `SYST` answers `215  OS/400 is the remote
  operating system. The TCP/IP version is "V7R5M0".`; `FEAT` lists
  `AUTH TLS`, `AUTH GSS`, `CCC`, `PBSZ`, `PROT`, `ADAT`, `ENC`, `MIC`,
  `CONF` (no `EPSV`, `UTF8`, `SIZE` or `MLST`, though `EPSV` works); `STAT` reports the client address, `NAMEFMT`,
  `TRIM`, `LISTFMT` and the session's settings.
- The session starts in the user's home directory with `NAMEFMT 1`;
  `SITE NAMEFMT 0` is refused there (`550 Current directory requires
  name format 1`): the current directory must first be a library
  (`CWD /QSYS.LIB/X.LIB`). `PWD` answers `257 "/home/{user}" is current
  directory.`, in a library `"/QSYS.LIB/X.LIB" is current library.`,
  and in `NAMEFMT 0` `"X" is current library.`
- `LISTFMT 0` on a library prints `owner size yy-mm-dd hh:mm:ss *FILE
  LIB/FILE` and `*MEM LIB/FILE.MBR` lines; `LISTFMT 1` prints Unix
  `drwx------ 1 owner 0 size Mon dd hh:mm NAME` lines where a file is a
  directory and a member a file of size 0. `LIST name` in `NAMEFMT 0`
  treats a bare name as a library (`550 Library H not found`); members
  are `FILE.MBR` and `FILE.*`.
- `SIZE`, `MDTM`, `MLST`, `MLSD` and `REST` are not implemented (`500
  Subcommand SIZE not valid.`, `504 subcommand not supported`).
- `STOR FILE` into a library creates the file (`150 Sending file to
  member FILE in file FILE in library X`), `STOR FILE.MBR` adds a member;
  `RETR` answers `150 Retrieving member M in file F in library X`;
  `TYPE E` sends EBCDIC bytes unconverted; `STOU` appends a digit to the
  name (`qsftp.txt1`); `RNFR`/`RNTO` renames members (`250 Member ...
  renamed`) and stream files (`250  File renamed as ...`); `MKD`/`RMD`
  answer `250  Created directory ...`/`250 Deleted directory ...`.
- `RCMD` answers `250 Command ... successful.` or `550-Error occurred on
  command ...` followed by the escape message text; `QUOTE RCMD` is the
  same subcommand.
- `EPSV` works but the advertised data port is not reachable from this
  client (pub400 fronts the server); `PASV` ports were.

### Screens (V7R5M0, the pub400 profile)

- `CFGTCP` shows options 1, 2, 3, 4, 10, 11, 12, 20, 21, 22, 23
  (`Load/Unload IP Filter`); there is no option 5 on V7R5M0. The lists
  offer only `5=Display` to this profile. `GO CFGTCP` is refused (no
  authority to the `*MENU`) while the `CFGTCP` command works.
- `NETSTAT` shows options 1 to 6 and 10 (stack status); `OPTION
  (*NBRCACHE)` is not a valid value. Every telnet session appears as a
  127.0.0.1 connection (a local proxy). The connection list's F11 cycles
  state, byte counts and connection type; F14 shows port numbers.
- `CFGTCPAPP`: 1 SNMP, 2 RouteD, 4 BOOTP, 5 DDM, 10 FTP, 11 TELNET, 12
  SMTP, 13 LPD, 16 POP, 17 REXEC, 19 SNTP (two pages).
- `WRKHDWRSC *CMN` nests `CMBnn` / `LINnn` / `CMNnn` with types `6B03`,
  `268C`, `6B26`; option 5 is refused on an IOP; `CMN04` is the Ethernet
  port behind `LINETH` (`*ELAN`, `MAXFRAME 1496`, `LINESPEED *AUTO`,
  `DUPLEX *AUTO`, network controller `LINETNET02`, device `LINETTCP00`
  run by `QTCPWRK`).
- `WRKLNK '/'` lists `/` alone; option 12 is refused on `/` and on a
  directory; `WRKLNK '/*'` lists the root's contents. `EDTF` leaves with
  F12 (F3 saves).
- 22 of the 108 prompts were refused for this profile (`Not authorized
  to command`): `ADDEXITPGM`, `RMVEXITPGM`, `CRTUDFS`, `MOUNT`,
  `UNMOUNT`, `RST`, `SNDSMTPEMM`, `DSPHDWRSC`, `STRTCP`, `ENDTCP`,
  `STRTCPIFC`, `ENDTCPIFC`, `STRTCPSVR`, `ENDTCPSVR`, `STRHOSTSVR`,
  `ENDHOSTSVR`, `STRCMNTRC`, `ENDCMNTRC`, `PRTCMNTRC`, `DLTCMNTRC`,
  `TRCCNN`, `TRCTCPAPP`; their parameter sets are in
  `command-definitions.txt` where the API could retrieve them (it could
  not for the same set).

### Not captured, and why

- Everything from the primary oracle (V7R1M0 job names, `WRKACTJOB`,
  its `QUSRSYS` layouts, its QSH inventory): unreachable on both days.
- `WRKHDWRSC *CMN` option 5 detail of a port's line, `WRKSRVTBLE`,
  `WRKPCLTBLE`, `WRKTCPHTE` prints: refused from the remote command
  server; the tables are captured as screens only.
- `NETSTAT` option 8 on this session's own connection: all telnet
  sessions share 127.0.0.1 behind pub400's proxy, so option 5 and 8 were
  taken on the listener (`QTVTELNET`) and option 5 on one proxied
  connection.
- `CFGTCP` options 3, 11, 12 and the `CFGTCPAPP` change options run a
  change command directly and were captured as F4 prompts instead.
- FTP `MLST`/`MLSD`/`SIZE`/`MDTM`: not implemented on V7R5M0.
- PING of `::1` fails on pub400 (`TCP3202 Unknown host`): no IPv6
  address is configured beyond the loopback interface.

## Certificates

1. **Application ID names.** The shipped list in `certs::apps::SHIPPED`
   is from IBM's documentation, not read from a system: DCM is web-only
   and neither oracle exposes its admin port. Least certain:
   `QIBM_QTMS_SMTP_CLIENT`, `QIBM_QRW_SERVER_DDM_DRDA` and
   `QIBM_QRW_CLIENT_DDM_DRDA` (IBM also writes `QIBM_QRW_SERVER_DDM`),
   `QIBM_QSO_LPD_SERVER`, `QIBM_QTOC_NTP_CLIENT`, `QIBM_QTOB_VPN_IKE`.
   Settle with `QSYS2.CERTIFICATE_USAGE_INFO` (7.4 and later) over JDBC
   on pub400, or `QsyListAppInfo`-style listing from the registration
   facility (`WRKREGINF` shows the exit points, not the IDs).
2. **Host server IDs.** `QIBM_OS400_QZBS_SVR_*` for central, database,
   data queue, network print, remote command, sign-on and file. IBM i
   7.5 may add `QIBM_OS400_QZBS_SVR_DDM`-style IDs; check the same view.
3. **The local CA's paths.** The CA store is
   `/QIBM/UserData/ICSS/Cert/CertAuth/DEFAULT.KDB` and its exported
   certificate `DEFAULT.TXT` beside it. The primary oracle shows the
   `CertAuth` directory but not its contents to a QSTEST user; confirm
   the file names with `QSYS2.IFS_OBJECT_STATISTICS` as a user with
   `*ALLOBJ`, or leave as is.
4. **Stash file extension.** A store's password is stashed beside it as
   `.RDB`, the name DCM uses for `DEFAULT.RDB`. Real `.RDB` files are
   IBM's request database, not the stash (IBM's stash is `.STH` on
   other platforms and internal on IBM i). The name is only ours to
   pick; nothing reads it but QS/400.
5. **Default store password expiry and rules.** DCM enforces a password
   expiration interval on stores; QS/400 does not expire them. Settle
   when the web admin UI is built.
6. **`gsk_environment_close` with sockets open.** Here
   `GSK_NORMAL_ENVIRONMENT_CLOSE` frees the handle at once and lets open
   sockets run on; `GSK_DELAYED_ENVIRONMENT_CLOSE` keeps the handle until
   the last socket closes and answers `gsk_secure_soc_open` with
   `GSK_PENDING_CLOSE_ERROR` (the header lists 109 among the `_open`
   codes). Settle with a small C or RPG program on pub400 calling
   `gsk_secure_soc_open` after each kind of close.
7. **`GSK_*_CIPHER_SPECS_EX` with unknown names.** IBM documents a
   comma-separated list of IANA names. QS/400 drops names it does not
   know or cannot negotiate and keeps the rest; IBM may answer
   `GSK_ATTRIBUTE_INVALID_LENGTH` or ignore the whole value. Probe with
   `gsk_attribute_set_buffer` on pub400.
8. **Client authentication from both the program and the application.**
   When a program names `GSK_SERVER_SESSION` but its application ID is
   set to require client certificates, QS/400 requires them (the stricter
   wins). IBM may let the application ID setting override the program's
   either way.
9. **`GSK_CONNECT_CIPHER_SPEC` for TLS 1.3 and ECDHE suites.** The old
   two-digit form cannot express `0x13nn` or `0xC0nn` codes; QS/400
   returns the low byte. IBM may return blanks or an error.

## Printers

1. **Writer messages for network failures.** A writer that cannot
   reach its printer or LPD sends `TCP370A` (no such host), `TCP3427`
   (refused), `TCP3436` (timed out), `TCP3409` (other connect failure),
   `TCP3430` (connection dropped) or `TCP3701` (refused or protocol
   error) to `QSYSOPR` and shows `MSGW`, then tries again after 30
   seconds without waiting for a reply. IBM's writers send `CPA33xx`
   inquiries and `PQT` messages for LAN printers; neither family was
   captured (`messages-cpf.txt` has no `CPx33xx`), and `TCP3701`'s
   reason code (`&2`) values are unknown. Settle by starting a remote
   writer on pub400 to an unreachable host (needs a writable outq in
   `{user}B`) and reading `QSYSOPR` and the writer's job log.
2. **`SYSDRVPGM` value for IPP.** V7R5's `CRTDEVPRT` lists
   `*IBMIPPDRV`, not the spec's `*IPPDRV`; both are accepted and
   `*IBMIPPDRV` is stored. Where the IPP resource goes (a URI in
   `RMTLOCNAME`, or `/ipp/print` by default) is ours; IBM's
   documentation of the IPP driver should say.
3. **`CRTDEVPRT` prompt order.** pub400 shows only `LANATTACH` after
   `F10` with `DEVCLS(*LAN)`, and only `PORT`, `ONLINE`, `FONT`,
   `FORMFEED`, `SEPDRAWER`, `SEPPGM` once `LANATTACH(*IP)` is given:
   IBM's prompt-control program reveals the parameters step by step.
   Ours follows `PMTCTL`: `LANATTACH` on `DEVCLS(*LAN)`, those six on
   `LANATTACH(*IP)`, the rest (`RMTLOCNAME`, `SYSDRVPGM`, `TRANSFORM`
   ...) once `PORT` has a value. The display after `PORT` was not
   captured; capture `CRTDEVPRT DEVCLS(*LAN) TYPE(3812) MODEL(1)
   LANATTACH(*IP) PORT(9100)` with F10 (F12 out, nothing created).
   `TYPE(*SCS)` from the spec is not a V7R5 value (the list ends at
   `*IPDS`); `MODEL` is free text here, IBM restricts it.
4. **Remote `CRTOUTQ` prompt.** pub400 shows `MSGQ` ("Queue for writer
   messages") after `AUTOSTRWTR` and `DESTTYPE` defaulting to `*OS400`
   under `CNNTYPE(*SNA)`; our `CRTOUTQ` keeps its Phase 5 order with
   `MSGQ`, `SEPPAGE` and `USRDRVPGM` added behind `F10`, `DESTTYPE`
   defaulting to `*OTHER`. `CNNTYPE` unspecified is stored as `*IP`
   (IBM's default is `*SNA`; SNA is not done). Capture the rest of the
   remote prompt (`*IP`, `*OTHER`) to order ours.
5. **`DESTOPT` words.** `XAIX` (copies as repeated print commands) and
   `XAUTOQ` are IBM's documented words as remembered, not traced;
   `*NOWAIT` is taken to skip the last acknowledgement and `*USRDFNTXT`
   does nothing. Other words are kept and not sent. A trace of
   `SNDTCPSPLF ... DESTOPT(XAIX)` to a harness LPD would settle the
   control file IBM sends, including the order of data and control
   files (ours: data first) and the `H`, `P`, `J`, `L`, `N` contents.
6. **IBM-to-IBM transfer.** `DESTTYPE(*OS400)` sends the SCS
   untransformed with our `Y` control lines (`docs/PRINT.md`). IBM's
   own `*OS400` LPR transfer keeps attributes by means that were not
   traced; a QS/400 receiving from a real IBM i gets only what the
   standard lines say.
7. **`RMTPRTQ(*SYSTEM)` and `*USER`.** `*USER` sends the file owner's
   name as the queue, `*SYSTEM` sends `QPRINT`. Unconfirmed.
8. **Host print transform output.** The PCL is written from the PCL5
   reference (`work/print/tests/golden/`) and not compared with IBM's
   `TRANSFORM(*YES)`: Cody's box has no remote outq aimed at a harness
   LPD and network settings there are read-only. The model capability
   table (PCL4 or 5, trays, duplex) is ours; IBM's `*WSCST` objects
   for each model (`RTVWSCST`) would settle it.
9. **`*WSCST` object type code.** `*WSCST` is type 0x0028 here, chosen
   clear of the codes other Phase 10 work may take; the coordinator
   should confirm no collision at merge.

## Registration facility

Built from the headers and the pub400 capture (`HOSTSVR.md`, § Exit
points); each item below waits for an oracle program or panel.

1. **`ADDEXITPGM` and `RMVEXITPGM` definitions.** pub400 refused
   `QCDRCMDD` for both (`CPF9802`). QS/400's follow IBM's documented
   keywords and order (`EXITPNT FORMAT PGMNBR PGM THDSAFE MLTTHDACN TEXT
   REPLACE CRTEXITPNT PGMDTA`; `EXITPNT FORMAT PGMNBR`). Retrieve them
   on the primary oracle with a profile that may.
2. **`QusRegisterExitPoint` control keys and `QusAddExitProgram`
   attribute keys.** No header lists them; the numbers in `SYSAPI.md`
   §11 are QS/400's reading of IBM's documentation. A CL program on the
   oracle calling `QUSRTVEI` after `QUSRGPT` with each key would settle
   them (registration in `QSTEST` names only, deregistered after).
3. **`EXTI0100` entry length, `EXTI0200`/`EXTI0300` offsets and the
   continuation handle.** QS/400 returns 204-byte point entries (the
   fields end at 201), program entries padded to four bytes with
   `NEXT` and `OFFDTA` from the receiver's start, and the next entry's
   number as text in the handle. Compare with `QUSRTVEI` on the oracle.
4. **Threadsafe and multithreaded job action codes.** `0`/`1`/`2` and
   `0`..`3` in QS/400; confirm from `EXTI0200` of a program added with
   `THDSAFE(*YES) MLTTHDACN(*NORUN)`.
5. **`VLRQ0100`, `TCPL0100`/`0200`/`0300`, `INIT0100`, `TERM0100`
   parameter lists.** No header carries them; QS/400 follows IBM's
   published descriptions. A logging exit program on the oracle (FTP
   server, telnet) would confirm order, sizes and the starting values
   of the outputs.
6. **`PWFS0100`/`PWFS0200` file name encoding** (CP037 here) and
   **`SPLF0100`'s layout after the server information** (the pieces of
   `ENPSEP` joined without padding here).
7. **Telnet connection description addresses.** `CHAR(20)` client and
   local addresses are dotted text here; the IPv6 fields are zeros.
8. **`Work with Exit Programs` layout.** IBM's column headings are not
   placed by UIM's column rules (the first row of the number heading is
   centered, the columns are wider than their fields) and the empty
   list's text sits a row lower; the panel test compares those rows by
   words. An input row for option 1 on an empty list is not shown:
   add with the command line or F4 there.
9. **Display Exit Point's second page.** Not captured; QS/400 shows the
   description message file, library, identifier and text there.
10. **Preprocessing exit programs** (`QTMFADDP`, `QUSEPAUT`, ...) are
    shown and stored but not called.

## Communications objects

1. **What a lost carrier sends.** QS/400 sends `CPI5917` (automatic
   recovery started, the line's network controller as `&1`) on a loss
   within `CMNRCYLMT` and the inquiry `CPA58EE` (line failed, cabling or
   hardware) past the limit or with no recovery; `CPF5908` when the
   carrier is back. The identifiers and layouts are pub400's `QCPFMSG`,
   but which ones IBM really sends for an Ethernet line, and whether the
   recovery attempts send anything, was not observed: both oracles
   forbid varying configuration. Settle on a test partition by pulling
   a cable with `CMNRCYLMT(2 5)` and reading `QSYSOPR`.
2. **`RCYPND` against `FAILED` during recovery.** A loss within the
   limit shows `RCYPND` until the carrier returns; past the limit
   `RCYCNL` at once (the inquiry's reply is not waited for or acted
   on: `C`, `G` and `R` all leave the line cancelled until a vary off
   and on); `CMNRCYLMT(0 ...)` shows `FAILED`. IBM may show `FAILED`
   while second-level recovery runs and `RCYPND` only while the
   inquiry waits. Settle with the experiment of question 1 and
   `WRKCFGSTS CFGTYPE(*LIN)` between steps.
3. **`QCMNRCYLMT` shipped value.** The catalog ships `0 0`, so a line
   with `CMNRCYLMT(*SYSVAL)` fails outright (`FAILED`). IBM's shipped
   value may be `2 5`; it was not in the system value capture. Check
   `DSPSYSVAL QCMNRCYLMT` on pub400.
4. **`CRTLINETH` parameters the capture omits.** The pub400 definition
   lists 22 parameters; `EXCHID`, `MAXCTL`, `COSTCNN`, `COSTBYTE`,
   `SECURITY`, `PRPDLY`, `USRDFN1` to `USRDFN3`, `AUTOCRTCTL`,
   `AUTODLTCTL`, `LINKSPEED`, `THRESHOLD` and `GENTSTFRM` are defined
   from IBM's documentation (`DSPLIND` shows `EXCHID`, `MAXCTL`,
   `THRESHOLD`, `GENTSTFRM`), at assumed positions and as `PMTRQS`. The
   second page of the F10 prompt was not captured
   (`prompt-crtlineth-f10-2` is missing), so the prompt's order past
   `ETHSTD` is not confirmed.
5. **The escape of a refused `CRTLINETH`.** The capture has no "line
   description not created" message (the `CPF26xx` range was captured
   whole); QS/400 escapes with the diagnostic itself (`CPD0084`,
   `CPD0085`, `CPF2606`). `CHGLINETH` escapes with `CPF261E` after its
   diagnostic.
6. **`EXCHID(*SYSGEN)`.** pub400 shows `056D610F`; QS/400 makes `056`
   and five hexadecimal digits from the system and line names. How IBM
   derives the last five digits is unknown.
7. **`DSPLIND` beyond `*BASIC`.** The capture shows `OPTION(*BASIC)`
   only, ending at `TEXT`; `AUTOCRTCTL`, `AUTODLTCTL` and `CMNRCYLMT`
   are in the text form only. The aggregate rows (`AGGPCY`, `AGGRSCL`
   after `RSRCNAME`, panel `DSPLINDA`) and the other `OPTION` values
   were not captured.
8. **`WRKLIND` F11 view.** Not captured; QS/400 shows the resource and
   status in place of the text.
9. **`WRKCFGSTS` jobs and keys.** pub400 shows `QTCPWRK QSYS 090579`
   beside the active TCP/IP device; QS/400 has no `QTCPWRK` job yet and
   leaves the column blank for network devices (displays show their
   signed-on job). The second key set (F24), `F23=More options` and the
   `*DEV CFGD(*CMN)` legend's `10=Location path` were not captured; the
   keys F5, F9, F11, F14 are QS/400's choice and F23 says there is
   nothing more.
10. **`RTVCFGSTS` codes.** 50 sign-on display, 80 recovery pending, 90
    recovery cancelled and 100 failed follow IBM's documentation, not
    a capture; the Phase 8 code 10 for a varied-on device was wrong and
    is 30 now.
11. **Naming when every suffix is taken.** `LINETNET` to `LINETNET99`
    exhausted is refused with `CPF2609` (`CPF2611` for the device);
    IBM's behavior was not seen.
12. **`WRKHDWRSC TYPE(*CMN)` tree.** Rows are indented under the
    nearest listed resource above them in sysfs (pub400's `CMB09`,
    `  LIN03`, `    CMN01`); a bridge shows only above an adapter or
    port. Option 5 ("Work with Configuration Descriptions",
    `wrkhdwrsc-cmn04-5`) and option 7 (resource detail) are captured
    but not built.
13. **VLANs on lines.** No IBM line parameter names a VLAN; the
    interface file `QATOFIPF` has `TFVLANID`, so the sub-interface is
    TCP/IP configuration's to make (`stgsvc::cmn::kernel::ensure_vlan`).

## TCP servers framework (2026-10-30)

1. **`STRTCPSVR` and `ENDTCPSVR` definitions.** `QCDRCMDD` refused both
   on pub400 (`CPF9802`), so their keywords are IBM's documented ones
   (`SERVER`, `RESTART`, `HTTPSVR`, `INSTANCE`); the special values and
   `MAX` are not compared with a capture. `*AUTOSTART` starts the
   servers whose autostart is on, `*ALL` every server this release runs.
2. **`CHGTELNA` on V7R5M0 has no `INACTTIMO` or `SSLCFG`.** The
   captured definition and prompt (`prompt-chgtelna`) end at `ALWSSL`;
   inactivity is `QINACTITV` and TLS is the application ID's in DCM. Ours
   follows the capture. Whether V7R1M0's command still has the older
   keywords is not captured (the primary oracle was unreachable).
3. **Where telnet's attributes live.** The capture says telnet keeps no
   file of its own ("its attributes live in system values and
   `QATOCTCPIP`"). Ours are a keyword bag in the server table
   (`QATOCSVRA`, ours) beside `QATOCSTART`'s autostart flag. A record dump
   of `QATOCTCPIP` would say which keywords IBM keeps there.
4. **Message identifiers.** `TCP1A04`-`TCP1A0A` and the new `TCP1A06`
   (instance not found) and `TCP1A10` (attributes changed) are ours in
   IBM's family; IBM's own `STRTCPSVR` completions (`TCP1A0x`/`TCP2xxx`
   in `QTCPMSG`) were not captured because `STRTCPSVR` may not be run on
   either oracle.
5. **Timing mark.** `TIMMRKTIMO` here is "silent this long, then a
   timing mark, then as long again for the answer". IBM documents the
   timeout as the wait for the answer; whether its server also sends the
   mark only after silence, or every interval, is not observable from a
   client without a long idle capture.
6. **Exit programs' job.** IBM calls `QIBM_QTG_DEVINIT` and `DEVTERM`
   programs in the `QTVDEVICE` job; ours run in a runtime job on
   `QTVTELNET`'s main thread (a session thread holds no runtime). A
   program that reads its job's name sees `QTVTELNET`. An `INIT0100`
   program's "allow automatic sign-on" is read and not acted on: it needs
   a storage service request that signs on a profile without a password.
7. **HTTP administration jobs.** pub400 shows `ADMIN` jobs (several, user
   `QTMHHTTP`) in `QHTTPSVR`; ours is one `ADMIN` job serving every
   connection on threads. The page's URL paths are `/`, `/index.html`
   and `/HTTPAdmin`; IBM's administration server redirects `/` to
   `/HTTPAdmin`, which a client capture of port 2001 would confirm
   (neither oracle exposes it).
8. **HTTPS certificate.** Port 2005 should present the certificate DCM
   assigns to `QIBM_HTTP_SERVER_ADMIN`; the job cannot open the `*SYSTEM`
   store (its stash key is in the LIC partition) and presents the
   system's self-signed telnet certificate until the storage service
   answers an application ID's identity (`servers::http::admin_tls` is
   ready for it).
9. **`QHTTPSVR`.** Shipped empty (no routing entries, no prestart jobs);
   `STRTCPSVR *HTTP` starts it when it is not active and `ENDTCPSVR`
   leaves it running, as IBM does.
10. **`*DDM` is blocked.** The DDM server needs `SqlBackend`,
    `FileBackend` and `Authenticator` over the storage service; until
    then `STRTCPSVR *DDM` answers `TCP1A04`. `CHGDDMTCPA AUTOSTART` is in
    `QUSRSYS/QRWDDMTCPA`, not the server table, so `STRTCP` does not
    consult it yet.
## SSH server (sshd, 2026-10-30)

SSH is not reachable from this PC on either oracle, so nothing here was
settled by a probe; pub400's configuration and paths came from the
2026-10-24 capture (`tools/conformance/oracle/net/pub400/`).

1. **Device name for the sign-on.** Password sign-on goes through
   `sec::signon::sign_on` with device `QSSHD`, so the `PW` audit entry
   names it and `QMAXSGNACN` 1 or 3 would try to vary off a device that
   does not exist (the profile part of 2 and 3 applies). IBM's PASE
   sshd authenticates through the PASE password check; what its `PW`
   entries show in `DEVN`, and whether `QMAXSIGN` counts SSH attempts at
   all on 7.5, needs a `DSPAUDJRNE`/`QSYS2.DISPLAY_JOURNAL` read on a
   system where SSH sign-on can be tried.
2. **`QLMTDEVSSN` and `QLMTSECOFR`.** The service's `sign_on` counts the
   user's interactive jobs for `QLMTDEVSSN` and checks `QLMTSECOFR`
   against a device description. Neither should apply to SSH (IBM's
   sshd ignores both); the link's sign-on needs a "not a display" form
   when it is wired.
3. **Expired passwords.** Refused, logged; OpenSSH's
   `SSH_MSG_USERAUTH_PASSWD_CHANGEREQ` dialog is not offered. On IBM i
   an expired password over SSH is also refused; the message the
   client shows is unknown.
4. **User name case.** Names are folded to upper case before the
   profile lookup and the `AllowUsers`/`DenyUsers` match (patterns are
   compared without case). IBM's sshd matches the patterns against the
   name as typed; a pattern `qstest` therefore matches `QSTEST` here
   and may not there.
5. **Host key file format.** New keys are written as PKCS#8 PEM, not
   OpenSSH's `openssh-key-v1`. Existing OpenSSH-format Ed25519 and
   ECDSA keys are read; OpenSSH-format RSA host keys from an IBM i
   system are not (the CRT parameters would have to be derived). Write
   `openssh-key-v1` instead if a tool on the system needs it.
6. **Session job.** pub400 shows sshd as a `QP0ZSPWT` prestart job
   (`QUSER`, `QUSRWRK`, port 2222). The jobs a session runs in (name,
   user, subsystem) were not seen; `WRKACTJOB` during an SSH session on
   pub400 would show them.
7. **`/QSYS.LIB` members over SFTP.** Read and written as records back
   to back, while `stat` reports IBM's size with two line-end bytes per
   source record. What IBM's sftp-server returns for a member (it runs
   in PASE over the IFS) is not known; clients that trust the size
   (resume, progress) see a short file.
8. **Ciphers.** Only AES-GCM is offered (graviola has no raw ChaCha20 or
   Poly1305 for `chacha20-poly1305@openssh.com`). Clients without
   AES-GCM (PuTTY before 0.76) cannot connect.
9. **Forwarding half-close.** A forwarded connection's channel closes
   when the far end closes its side; data the client still sends after
   that is dropped.

## Network tools (nettools, 2026-10-30)

Captured from pub400 for this section (read-only, one session): the
`TRACEROUTE`/`TRCTCPRTE` and `PING` messages
(`oracle/net/pub400/nettools-cl-log.txt`), the `QSYS/QTCPMSG` layouts
of `TCP32xx`-`TCP34xx` (`messages-qtcpmsg-tools.txt`) and the
communications trace's `CPF39xx` (`messages-cmntrc.txt`). The NETSTAT
displays and the prompts were in `golden/phase10` already.

1. **`PRTCMNTRC`'s layout.** The trace commands are refused to the
   pub400 profile (`*SERVICE`), so no listing was seen. Ours is IBM's
   shape from its documentation (description block, a line per record,
   decoded headers, hex and characters) in our words. Tools that parse
   IBM's formatted trace will not read it. Settle with a profile that
   has `*SERVICE` (the primary oracle, when it answers): trace a line,
   `PRTCMNTRC`, `CPYSPLF` the listing.
2. **The trace commands' parameters.** `QCDRCMDD` could not retrieve
   them (`CPF9802`). QS/400 defines `CFGOBJ CFGTYPE MAXSTG DTADIR
   TRCFULL USRDTA TEXT` (`STRCMNTRC`) and `CFGOBJ CFGTYPE CTLD CODE
   FMTTCP TCPIPADR SLTPORT FMTBCST` (`PRTCMNTRC`) from IBM's
   documentation; `RMTCTL`, `RMTMACADR`, `RMTSNAP`, `RMTIPADR`,
   `RMTL4PORT`, `BEGTIME`/`ENDTIME`, `FROMSTMF`/`TOSTMF` and `OUTFILE`
   are not there. `MAXSTG`'s values (`128K` ... `1G`, `*MAX`) and its
   default are a guess. The same capture settles them.
3. **What a trace of a non-line is.** IBM traces a line, network
   interface or network server description. QS/400 traces the kernel
   link a line is varied on over and, for a `CFGOBJ` with no varied-on
   line, a link of that name (`ETH0` is `eth0`), so a link without a
   line description can be traced; `*NWI` and `*NWS` are refused with
   `CPF39AD`.
4. **Which `CPF39xx` each case sends.** The identifiers and layouts
   are pub400's; which one IBM sends for a trace that exists
   (`CPF39AA`), is active at delete (`CPF39B2`), has no data
   (`CPF39B3`) or is not found (`CPF39B1`) follows the first-level
   texts, not an observation.
5. **`TRCCNN` and `TRCTCPAPP`** are refused to the profile and their
   definitions were not retrievable; they are not built.
6. **`PING` with F10.** The oracle's F10 display shows `INTNETADR`
   (conditioned on `RMTSYS(*INTNETADR)`) before "Additional Parameters"
   while `RMTSYS` is blank; QS/400's prompter keeps the condition, so
   the F10 goldens of `PING` and `VFYTCPCNN` are not matched (the test
   is ignored with this reason).
7. **`MSGMODE(* *ESCAPE)`.** The oracle sent the escape with 0 of 1
   answered. QS/400 makes `TCP3210` the escape whenever a packet went
   unanswered; whether IBM escapes on a partial loss was not tried.
8. **`TRACEROUTE` details.** Names come from the host table only (no
   reverse lookup through the name servers, which would slow every
   hop); `OUTPUT(*VERBOSE)` does not dump the packets the oracle shows
   as `CPF9897`/`CPF9898`, and `OUTPUT(*DTAQ)` prints as `*MSG`;
   `FRAGMENT` is accepted and ignored. The oracle writes a hop without
   a name as the address followed by three blanks; ours puts one.
9. **`NETSTAT` values the kernel does not have.** The connection
   details' sequence numbers, push and urgency numbers and send window
   (pages 2 and 3 of option 5) show 0; "Bytes out" is the bytes the
   peer acknowledged. A listener's idle time is 0. An interface the
   configuration does not hold has no change time. Option 14
   (multicast groups), F13 (sort), F15 (subset) and F22 say they are
   not available. The route display's second page (the oracle shows
   `More...`) was not captured; QS/400 puts the mask and the binding
   interface there. The IPv6 lists' F11 views were not captured.
10. **`WRKTCPSTS`** is not in pub400's command inventory; QS/400 does
    not define it (the panel driver answers to it if it is added).
11. **`ping_group_range`.** Opening ICMP datagram sockets to every
    group at IPL lets any process send echo requests, which Linux
    distributions commonly allow; nothing in QS/400 runs that is not
    Quartz's own. A job's `PING` therefore needs no special authority,
    as on IBM i.

## SNTP, LPD, LPR and mail out (smallsvr, 2026-10-30)

Neither oracle runs LPD or SMTP; pub400 runs `QTOTNTP` but its UDP 123
is not reachable from this PC, and `STRTCPSVR`/`CHG*A` may not be run
on either system. The `CHGNTPA`, `CHGLPDA`, `CHGSMTPA`, `SNDTCPSPLF`
and `LPR` definitions and prompts are the 2026-10-24 captures; the rest
is from IBM's documentation and unverified.

1. **`CHGNTPA` defaults.** The prompt could not retrieve current values,
   so the defaults are IBM's documented ones: `RMTSYS(*NONE)`,
   `AUTOSTART(*NO)`, `POLLITV(60)`, `MINADJ(20)` ms, `MAXADJ(20)` min,
   `ADJTHLD(*MAXADJ)`, `ACTLOG(*NONE)`, `SVRAUTOSTR(*NO)`,
   `SVRACTLOG(*NONE)`, `SYNCRQD(*YES)`. A `QATOTNTP` record dump would
   settle them and where IBM keeps them (ours: the server table's bag).
2. **`ADJTHLD` and `SYNCRQD`.** Read here as: up to `ADJTHLD` the clock
   is slewed, beyond it (within `MAXADJ`) set at once; `SYNCRQD(*YES)`
   keeps the server silent until the client has synchronized. Both
   readings are from the keyword texts; the help panels were not
   captured.
3. **What the server says unsynchronized.** With `SYNCRQD(*NO)` and no
   upstream the server answers stratum 10, reference `LOCL`. IBM's
   stratum in that state is unknown.
4. **`STRTCPSVR *NTP` parts.** The command starts both the client and the
   server; `STRTCP` starts those whose `AUTOSTART`/`SVRAUTOSTR` is on.
   IBM's `STRTCPSVR` may have a parameter choosing client or server (its
   definition was refused, `CPF9802`).
5. **SNTP messages.** `TCP1A40`-`TCP1A46` are ours in IBM's family; IBM's
   SNTP messages in `QTCPMSG` (beyond the `QTCPMSGF` capture) were not
   captured. `QSYSOPR` hears of a difference beyond `MAXADJ`, a clock set
   at once, the first poll with no answer after good ones, and another
   `QTIMADJ` owner.
6. **`QTIMADJ`.** The client claims it as `QIBM_OS400_SNTP` when it is
   `*NONE` and gives it back when it ends; any other value keeps it from
   adjusting. IBM's exact identifier string and whether its client gives
   the value back are not confirmed.
7. **The hardware clock.** A slew or step changes `CLOCK_REALTIME`; the
   client then tells the kernel the clock is synchronized, which lets a
   kernel with `RTC_SYSTOHC` copy it to the RTC every eleven minutes.
   Whether the 6.18 configuration has `RTC_SYSTOHC` (the default with
   `RTC_CLASS`) is to be confirmed in the built `.config`; without it a
   correction lasts until the next IPL reads the RTC.
8. **LPD jobs.** IBM's LPD runs `QTLPD00nnn` jobs, `NBRSVR` of them
   prestarted; ours is one `QTLPD00001` (user `QTMPLPD`) serving each
   connection on a thread. `NBRSVR` is kept.
9. **Spooled file owner.** IBM's LPD makes the file the property of the
   control file's user (`P`) when that profile exists, else `QTMPLPD`;
   here every file belongs to the LPD job, so to `QTMPLPD`, and the
   sending user is in `USRDTA`. Giving the file to `P` needs a storage
   service request that spools for another profile.
10. **What is spooled.** IBM keeps every received file as `*USERASCII`.
    Here text (`f`, `l`, `p`) and FORTRAN carriage control (`r`) become
    SCS, so `DSPSPLF` and the system's writers handle them; other print
    commands are kept as sent under `*USERASCII`. A file from a QS/400
    `DESTTYPE(*OS400)` sender keeps its SCS and attributes. The
    spooled file's name comes from the source name (`N`), `QPRTLPD`
    when there is none.
11. **Queue names.** `LIB/OUTQ`, or an output queue found through the
    LPD job's library list, then in `QUSRSYS`. IBM's LPD also takes a
    printer device's name; that is not done.
12. **Remove jobs (05) and the queue state's form.** Removal answers that
    files are removed with `DLTSPLF`; the queue state is our own `lpq`-like
    text. Neither was compared with IBM's LPD.
13. **`SNDTCPSPLF WSCST` and `USRDTATFM`.** Taken and not used: a
    `*WSCST` model prints as plain text, and no user transform program
    is called.
14. **`SNDSMTPEMM` definition.** Refused to the pub400 profile (`Not
    authorized to command SNDSMTPEMM in library QTCP`), so `RCP`,
    `SUBJECT`, `NOTE`, `ATTACH`, `CONTENT` and `SMIME` are IBM's
    documented keywords with our lengths; only `SMIME(*NONE)` sends.
15. **The sender's address.** IBM takes it from the system distribution
    directory (`WRKDIRE`, SMTP user and domain); there is none here, so
    it is `user@hostname.domain` from `CHGTCPDMN`.
16. **Mail router.** `FWDHUBSVR` first, then `MAILROUTER`, either as
    `host[:port]`; there is no MX lookup (client only). TLS is
    `STARTTLS` when offered (implicit on 465). When TLS is required or
    implicit the relay's certificate is checked against the shipped roots
    and the trusted CAs of `*SYSTEM`, by name; opportunistic `STARTTLS`
    takes any certificate (RFC 7435). Which application ID IBM's SMTP
    client checks with, and whether it checks opportunistic sessions, was
    not captured.
17. **`AUTH` credentials.** The client speaks `AUTH PLAIN` and `AUTH
    LOGIN`; the user and password come from the bag keys `AUTHUSER` and
    `AUTHPWD`, which no command sets and which any job reading the server
    table can see. IBM keeps SMTP client credentials elsewhere (an
    `ADDSMTPLE` entry type, perhaps a validation list); a `*VLDL` would
    be their home here.
18. **`QtmmSendMail`.** Bound procedure in service program `QTMMSNDM`
    (assumed: the export list was not captured). `ADDT0100`'s
    `Next_Addr_Structure_Offset` is taken from the start of the current
    structure (0: right after its address). Failures are `TCP1A4C` with
    the reason; IBM's identifiers were not captured. The file is sent as
    it is stored.


## FTP (2026-10-30)

From the 186 sessions recorded on pub400 (V7R5M0) for the replay
(`tools/conformance/ftp/`) and what they could not settle; docs/FTP.md
has the dialect as built.

1. **The job CCSID of the oracle's account.** `TYPE E` and files `STOR`
   makes map Latin-1 X'AC' to X'BA', which is CCSID 273 (or 500), not
   37; the replay runs with job CCSID 273. Which of the two pub400 uses
   (and whether `*CALC` follows the job or `QCCSID`) needs `DSPJOB` of a
   session job, which FTP cannot show.
2. **Binary files `STOR` makes.** A binary `STOR` into a file that is
   not there makes records of 512 bytes padded with nulls (256 and 70000
   bytes both show it). Whether IBM's rule is "512" or "the next power of
   two up to a limit" needs a longer upload than the account's library
   should take.
3. **Member sizes.** IBM's listings count two line-end bytes a record
   for data members too; the IFS's `stat` (docs/IFS.md) counts them for
   source members only, so the server converts. The IFS should decide
   which is right for `st_size`.
4. **Empty save file size.** IBM lists an empty save file as 24576 bytes
   (its allocation); ours is as long as its data (0). Left out of the
   replay (`gaps.txt`).
5. **Save file data.** IBM refuses data that is not a save file with
   `CPE3101` at close; ours checks only that the data is whole 528-byte
   records. A real save file's layout check waits for `SAVOBJ`/`RSTOBJ`.
6. **`MODE B` and `STRU R`.** IBM accepts both and then refuses a stream
   file transfer in them (`504`); whether block mode works for members
   could not be recorded (the recorder speaks stream mode only). Ours
   moves members in block mode.
7. **Profile swap.** IBM's session job runs as `QTCP` with the user as
   current user. Ours swaps with `QSYGETPH`/`QWTSETP` (the security
   API), so `WRKACTJOB` shows `QTCP` and the work is the user's. A
   profile a logon exit program names without a password needs `QTCP` to
   have `*USE` to it (`QSYGETPH *NOPWD`): IBM documents no such rule.
8. **`USER` after `REIN`, `PASS` without `USER`.** Recorded (`501`,
   `503`); a failed `PASS` was not, so as not to count sign-on failures
   against the account: `530` is RFC 959's and what IBM documents.
9. **Commands that closed pub400's connection.** `MKD`, `RMD`, `DELE`,
   `RNFR` and `RETR` without a name, and `PORT 1,2,3`, ended the session
   (pub400's front end, or the server); ours answers `501`. The replay
   stops comparing at the close.
10. **`PBSZ` without TLS.** IBM answers `421 ... connection closed` and
    keeps the session; ours answers `421` and keeps it too.
11. **Certificates.** `AUTH TLS` and port 990 should present the
    certificate DCM assigns to `QIBM_QTMF_FTP_SERVER`
    (`ftp::server::server_tls` reads it); the session job uses the
    system's self-signed certificate until the storage service answers an
    application ID's identity. The `FTP` client takes server
    certificates as they come until it can read the trust store
    (`QIBM_QTMF_FTP_CLIENT`).
12. **The client's own display.** IBM's `FTP` at a display runs a
    full-screen session; ours reads `INPUT` (batch form) only and says so
    when it is not overridden.
13. **`CHGFTPA` keywords.** The V7R5M0 command has no `SSLCFG`,
    `DTACNNPORT` or `RMTEXTPTH` (the Phase 10 spec names some): TLS is
    DCM's application ID; data ports are `DTACNNPORT` in the server
    table's bag, which no command sets yet.

## Certificate commands (certcmds, 2026-10-30)

The `QS*` commands, `cert_app` and the expiry scan (docs/CERTS.md). No
oracle has them: DCM is web-only and neither system's admin port is
reachable, so everything below is our choice until the web admin UI or a
DCM capture settles it.

1. **Who may manage certificates.** `*ALLOBJ` and `*SECADM`, what DCM's
   documentation asks of its users. Display-only access for `*AUDIT`
   users (DCM has none) is not offered.
2. **Who may have a server's key.** `cert_app` gives the private key to
   jobs running as `QSYS`, `QTCP`, `QUSER`, `QTMHHTTP`, `QTMHHTP1` or
   `QSPLJOB`, or as a user with `*ALLOBJ` and `*IOSYSCFG`. On IBM i the
   server programs reach the store inside the system and no user ever
   holds the key; a user program using `GSK_OS400_APPLICATION_ID` there
   also gets its certificate, which here would need its job's user on
   that list. Revisit when the GSK binding asks the service rather than
   opening the store in the job.
3. **Audit.** Phase 6 left DCM out of scope and IBM i has no audit
   entry type for certificate store changes (DCM changes show only as
   object changes of the `.KDB` files, `ZC`, when the files are
   audited). Refusals are `AF` entries (violation `A`, object
   `DEFAULT *STMF`, operation `manage certificates` or `certificate key`);
   the store files are written as `QSYS` through the IFS, so their own
   object auditing applies. A `GR`-style entry per change is not written.
4. **Expiry message identifiers.** `TCP1C20` (30, 7 and 1 days) and
   `TCP1C21` (expired) are ours. IBM i 7.4+ sends certificate expiration
   notices through the certificate expiration manager with identifiers
   we did not capture (DCM's "Certificate expiration" setting, the
   `QSYS2.CERTIFICATE_INFO` view); match them if a capture shows them.
5. **Which stores the scan watches.** `*SYSTEM` and the local CA's,
   because their stashes open without a person. Other stores (a path
   with its own password) are scanned only if they have a stash; the
   shipped root CAs are never watched (they change with the root set).
6. **The daily schedule.** The scan runs in `QSYSCOMM1` ten minutes
   after the IPL and every 24 hours, not as a job schedule entry: IBM i
   shows no such entry in `WRKJOBSCDE` on pub400. While `QSYSCOMM1` is
   not running only `QSCHKCRT` scans.
7. **When a server sees a new assignment.** At its next start (a host
   server prestart job keeps its configuration ten minutes). DCM's
   changes also apply only when the server restarts.
8. **An application without an assignment.** It takes the store's
   default certificate, as `certs::apps` already documents; on IBM i a
   server application with nothing assigned fails its TLS start. Our
   servers fall back further to the system's self-signed certificate so
   that 992 and 2005 keep working on a new system.
9. **Printers with self-signed certificates.** The IPP connector checks
   chain and name against the shipped roots and the `*SYSTEM` trusted
   CAs; a printer's own certificate has to be imported as a CA. IBM's
   `*IPPDRV` behavior with an untrusted printer certificate is not
   known (it may not check at all).
10. **Menus.** No IBM command menu holds the `QS*` commands (genmenus
    lists them as "ours only"); `GO CMDQS`-style access is not given.
