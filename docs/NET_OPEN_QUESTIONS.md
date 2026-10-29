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
