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
