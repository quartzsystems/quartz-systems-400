# QS/400 V1R2M0 release notes

Draft for the next release. V1R2M0 adds NetServer, SMB file and print
serving, one of the features V1R1M0 left out
(`RELEASE_NOTES_V1R1M0.md`, "Deliberately not in V1R1M0"), and the fixes
made since V1R1M0. `VERSION` is still `1.1.0`: the generated blocks (the
marker inventory and the SQL refusals) stay in the V1R1M0 notes until
`ci/version.sh prepare 1.2.0` moves the release identity, when
`tools/kcbuild` is pointed at this file.

## Upgrading

- A V1R1M0 system upgrades in place with the slip install, as V1R0M0 did
  to V1R1M0 (`INSTALL.md` §4).
- Formats changed since V1R1M0, each with a version bump and read in its
  old form unchanged (`tools/fmtcheck/formats.toml`):
  - the `*USRPRF` payload is at version 2: a new password form holds the
    NT hash NetServer signs users on with, sealed with the LIC
    partition's stash key (`SECURITY.md`, `NETSERVER.md` §5). A version 1
    profile reads as one with no NT hash;
  - the audit entry layouts are at version 2: the `VP` entry (network
    password error) is written, under `*AUTFAIL`;
  - QAP is at version 10, a draft adding the NetServer service (`QAP.md`).
- One new format, frozen at V1R2M0: NetServer's configuration files
  `/QIBM/UserData/OS400/NetServer/QAZLSCFG` and `QAZLSSHR`
  (`NETSERVER.md` §4).
- **Each user changes the password once before using NetServer.** An NT
  hash can be made only from the password itself, so a profile carried
  over from V1R1M0 has none, and NetServer refuses it (`CPIB68D`) until the
  next `CHGPWD` or `CHGUSRPRF PASSWORD()`. The same holds after a
  `QPWDLVL` change (`NETSERVER.md` §5).
- Shipped objects are made only when missing, so a V1R1M0 system keeps
  its `QSERVER` subsystem description: add NetServer's routing entry 800
  (`ADDRTGE`), the `QPWFSERVER` job queue entry (`ADDJOBQE`) and the
  `QZLSFILE` and `QZLSFILET` prestart job entries (`ADDPJE`) as
  `NETSERVER.md` §3 lists them.

## What is new in V1R2M0

**NetServer** (`NETSERVER.md`). Windows, macOS and Linux clients map
shares of the IFS, open and save files, and print to output queues, as
with IBM i NetServer:

- SMB 2.0.2, 2.1, 3.0, 3.0.2 and 3.1.1 on port 445 (and 139 with the
  NetBIOS session service when allowed), with signing, encryption
  (AES-128/256, CCM and GCM), leases, durable handles that survive a
  dropped connection, change notification and server-side copy. SMB1,
  Kerberos, multichannel, DFS, quotas and alternate data streams are
  refused (`NETSERVER.md` §10).
- The SMB protocol is rust-smb-server 0.10.0 (MIT), carried in
  `net/smb-server` with QS/400's changes listed in its `QS400-PATCHES.md`.
- Sign-on is NTLMv2 against user profiles. The NT hash stays in the
  storage service; failed sign-ons count against `QMAXSIGN` and disable
  the user for NetServer only (`CPIB682`), as IBM does; `VP` audit
  entries; a guest profile when one is set.
- The jobs are IBM's: `QZLSSERVER` in `QSERVER`, and one `QZLSFILE`
  prestart job per client, which runs as the signed-on profile, so IFS
  authority and auditing apply as for any other access. A client's
  reconnect returns to the job that holds its files.
- Leases and change notifications are system-wide: a change from any job,
  FTP, QSH or a program breaks the leases other clients hold and reaches
  their change notifications.
- File shares over the IFS (`/QSYS.LIB` too), with text conversion by
  CCSID for the extensions a share lists and a maximum number of users.
  Print shares spool to an output queue as the user, as `*USERASCII` or
  through the host print transform; Windows point and print through the
  print spooler RPC and a read-only `print$` driver share.
- Administration as on IBM i: `STRTCPSVR` and `ENDTCPSVR SERVER(*NETSVR)`,
  the 13 `QZLS*` APIs, `QSYS2.SERVER_SHARE_INFO`,
  `QSYS2.USER_INFO.NETSERVER_DISABLED`, Navigator's NetServer pages (file
  and print shares, sessions, server properties, disabled users) and the
  QAP NetServer service.
- A 1 GB copy under QEMU runs at about 150 to 210 MB/s into the system
  and 90 to 115 MB/s out (`NETSERVER.md` §11).

**Fixes since V1R1M0**

- `ENDJOB`, `HLDJOB` and `RLSJOB` of a batch job a subsystem had just
  taken off its job queue failed with `CPF1321` (QS00004).
- The DDM and DRDA server ended its job on a request object with an
  extended length too large for the system (QS00004).
- `QSYS2` services (`NETSTAT_INFO` and the others) answered `SQL0443` from
  `STRSQL` and the service console; only embedded SQL reached them.
- The IFS volume's flushes no longer hold up its reads and writes: a slow
  flush on the host stalled every job's IFS requests behind it (`IFS.md`).
- A display varied off by `QMAXSGNACN` (too many sign-on attempts that
  were not valid) still showed VARIED ON, while every session that named
  it (`DEVNAME`) was closed, even after an IPL. The device now shows VARIED
  OFF, `QSYSOPR` gets `CPF1397`, `VRYCFG STATUS(*ON)` or an IPL (for
  `ONLINE(*YES)`) brings it back, and a device left in that state by an
  earlier release works again once this release is installed.
- The web products' listeners took up to 50 ms to accept each new
  connection; every response now carries a `Server-Timing` header
  (`SERVERS.md`).

## Deliberately not in V1R2M0

Each has its own handoff document for a later release.

- NFS server and client: `*UDFS` mounts only.
- LDAP server and client: user profiles are the directory.
- An SNMP agent: the definitions only.
- DNS, DHCP, BOOTP and TFTP servers: DHCP and DNS clients exist.
- POP3 and SMTP servers: the SMTP client sends mail (`SNDSMTPEMM`).
- RouteD (RIP): static routes only.
- DBCS display: SBCS and Unicode CCSIDs only.
- AFP and IPDS rendering: `*AFPDS` spooled files are stored, not
  rendered; PDF and the host print transform exist.
- Remote journaling: local journals only.
- Query/400 and Query Manager (`WRKQRY`, `RUNQRY`, QS00QU1): `STRSQL`,
  `RUNSQL` and `OPNQRYF` exist.
- The RDi/RSE server and the debug server.
- Virtual optical from HTTP or NFS: image catalogs on the IFS only.
- SNA, APPC and pass-through (`STRPASTHR` is refused): TCP/IP only.
- BRMS, save while active, parallel saves, optical media libraries.
- Independent ASPs: the system ASP only.
- Kerberos and EIM: password and RFC 4777 sign-on, SSH keys; NetServer
  signs on with NTLMv2 only.
- PASE for AIX programs and Java: QSH and Linux-native PASE programs.

## Known limits

NetServer's are in `NETSERVER.md` §10. The main ones: the NetBIOS name
service (UDP 137/138) and browsing are not built (clients connect by name
or address); `/QSYS.LIB` members changed through record I/O or SQL break
no lease and reach no change notification; the install ISO's boot image
has about 1 MB of room left after NetServer joined `qsjob`.

## Before shipping (Cody)

The NetServer `NEEDS-CODY` items (`NETSERVER.md`):

- Windows 11, macOS and a Linux kernel client map a share, open and save
  with Notepad, Word and Excel, and print; Windows point and print with
  an in-box class driver; a wrong password `QMAXSIGN` times disables the
  user for NetServer.
- `sudo apt install smbclient cifs-utils` and `sudo modprobe cifs` in WSL,
  then `cargo test --manifest-path tools/smbtest/Cargo.toml --test
  interop -- --ignored`.
- On an IBM i 7.5 system: whether the NetServer password is kept at
  `QPWDLVL` 4; the date format of a `VP` audit entry; the `QZLSCHSI`
  length bounds, the share-name characters, the `ZLSL0101` extension
  table, the `ZLSL0300` logon types and the `SERVER_SHARE_INFO` value
  spellings that need a change API or a live session; `CPFB68A` reason 6
  for a missing output queue; whether a share CCSID of 0 means 819.

Then `ci/version.sh prepare 1.2.0`, `tools/kcbuild` pointed at this file,
and the `v1.2.0` tag.
