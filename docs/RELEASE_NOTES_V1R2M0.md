# QS/400 V1R2M0 release notes

Draft for the next release. V1R2M0 adds NetServer (SMB file and print
serving), NFS (the server and the client) and the Directory Server
(LDAP, the server and the client), three of the features
V1R1M0 left out (`RELEASE_NOTES_V1R1M0.md`, "Deliberately not in
V1R1M0"), and the fixes made since V1R1M0. `VERSION` is still `1.1.0`: the generated blocks (the
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
  - the audit entry layouts' version 2 also holds the directory
    server's `DI` entry (`SECURITY.md` §7), written under `*SECDIRSRV`,
    `*AUTFAIL`, `*CREATE`, `*DELETE`, `*OBJMGT` and object auditing as
    IBM selects it;
  - the object store's type registry is at version 2: it adds the
    internal type `*DIRDB` (0x00FC), the directory server's entry store
    (`STORE_FORMAT.md` §9). A V1R1M0 system never opens a V1R2M0 ASP, so
    no older code meets the type;
  - QAP is at version 12: version 10 a draft adding the NetServer
    service, version 11 a draft adding the `Nfs` service, version 12 a
    draft adding the `Dirsrv` service (`QAP.md`).
- New formats, frozen at V1R2M0: NetServer's configuration files
  `/QIBM/UserData/OS400/NetServer/QAZLSCFG` and `QAZLSSHR`
  (`NETSERVER.md` §4); and under `/QIBM/UserData/OS400/NFS/` the NFS
  options, the status monitor's monitored hosts and the NFSv4 stable
  records (`NFS.md` §6); the directory server's `*DIRDB` entry store
  (`dirdb`, `DIRSRV.md` §5), its publishing configuration
  `/QIBM/UserData/OS400/DirSrv/publish.conf` (`dirsrv-publish`, §10.1)
  and an instance's schema changes `etc/V3.modifiedschema`
  (`dirsrv-modified-schema`, §6.4).
- **Shipped user profiles get IBM's UIDs.** Until now they had none, so
  UID 0 named no one. At the first IPL of this release a shipped
  profile without a UID takes IBM's number when it is free (`QSECOFR` 0,
  `QSYS` 101, `QNFSANON` 4294770710, ...); a profile that has one keeps
  it. IFS objects name their owner by profile, so nothing changes hands.
  `CRTUSRPRF` and `CHGUSRPRF` now refuse a UID or GID another profile
  has (`CPF22CE`), and `QSECOFR`'s UID cannot change (`CPF224C`).
- The upgrade adds the NFS daemons' `QSYSWRK` routing entries 2120 to
  2127 with their job descriptions and classes, as it adds NetServer's
  entries.
- The kernel configuration names `CONFIG_FHANDLE` (the IFS opens an
  object by its file ID through its xfs handle).
- **Command prompts are laid out as IBM's**: `More...` or `Bottom` on
  row 21, the keys on rows 22 and 23, the message on row 24; long
  fields wrap. A script that reads a prompt by row sees the new rows.
- **Each user changes the password once before using NetServer.** An NT
  hash can be made only from the password itself, so a profile carried
  over from V1R1M0 has none, and NetServer refuses it (`CPIB68D`) until the
  next `CHGPWD` or `CHGUSRPRF PASSWORD()`. The same holds after a
  `QPWDLVL` change (`NETSERVER.md` §5).
- The upgrade adds NetServer's `QSERVER` entries for you: routing entry
  800, the `QSYS/QPWFSERVER` job queue entry, and the `QZLSFILE` and
  `QZLSFILET` prestart job entries. They go next to the system's own
  entries, once; an entry whose sequence number is taken is left out and
  named in the install log. A system that gets the code by PTF gets them
  at its next IPL, with the shipped message descriptions its message
  files lack.
- The upgrade adds the directory server's objects: the library
  `QUSRDIRDB`, the job descriptions `QSYS/QDIRSRV`, `QGLDPUBE`,
  `QGLDPUBA` and `QDIRJOBD`, `QSYSWRK`'s autostart entries for the
  publishing jobs `QGLDPUBE` and `QGLDPUBA` (routing entries 2140 and
  2141), the message file `QSYS/QGLDMSG`, the `QSYSINC` library with
  QS/400's LDAP members, and at the first IPL the shipped instance list
  and `QUSRDIR`'s `ibmslapd.conf` under `/QIBM/UserData/OS400/DirSrv/`.
  The directory server does not start until `STRTCPSVR SERVER(*DIRSRV)`
  (its autostart is off, as IBM ships it).
- **More prompts are laid out as IBM's V7R1M0 draws them**: `+ for more
  values` sits at column 17 with a field where the next value is typed;
  a group whose default is a single value shows only that value, its
  other elements blank; a choice text with no whole item that fits is
  left out. `STRTCPSVR` and `ENDTCPSVR` prompt `RESTART`, `HTTPSVR` and
  ENDTCPSVR's `OPTION` and `DELAY` only for `SERVER(*HTTP)`, and take
  IBM's `INSTANCE` element for `*DIRSRV`.
- A slip install now keeps the system's work management objects as they
  are: subsystem entries, the messages on `QSYSOPR`, the jobs on job
  queues, what output queues hold, the job schedule and the reply list.
  Before, the operating system's load replaced them (`INSTALL.md` §4).
- Install step 10 proves the upgrade from V1R1M0 media under QEMU,
  NetServer included.

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
  their change notifications. That includes `/QSYS.LIB` members, files
  and libraries changed through the database and the object commands
  (record I/O, SQL, `CLRPFM`, SEU, `ADDPFM`, `RMVM`, `RNMM`, `DLTF`,
  `CLRLIB`); a job's open for update waits for clients to give back
  cached writes.
- Files get Windows-style 8.3 short names (`XXHHHH~1.EXT`, made from the
  file's number, so they stay the same): in listings, in alternate-name
  queries, and to open files by.
- With "allow NetBIOS" on, the server registers and answers for its
  NetBIOS names on UDP 137.
- File shares over the IFS (`/QSYS.LIB` too), with text conversion by
  CCSID for the extensions a share lists and a maximum number of users.
  Print shares spool to an output queue as the user (a spooled file of
  the signed-on profile's `QPRTJOB`, as IBM's are), as `*USERASCII` or
  through the host print transform; Windows point and print through the
  print spooler RPC and a read-only `print$` driver share.
- Administration as on IBM i: `STRTCPSVR` and `ENDTCPSVR SERVER(*NETSVR)`,
  the 13 `QZLS*` APIs, `QSYS2.SERVER_SHARE_INFO`,
  `QSYS2.USER_INFO.NETSERVER_DISABLED`, Navigator's NetServer pages (file
  and print shares, sessions, server properties, disabled users) and the
  QAP NetServer service.
- A 1 GB copy under QEMU runs at about 150 to 210 MB/s into the system
  and 90 to 115 MB/s out (`NETSERVER.md` §11).

**NFS** (`NFS.md`). `EXPORTFS` publishes IFS directories to NFS clients
and `MOUNT TYPE(*NFS)` mounts a remote export into the IFS, with IBM's
commands, daemon jobs and `/etc/exports`:

- The server: NFS versions 3, 4.0 and 4.1 (version 2 is refused), MOUNT
  1 and 3, the lock manager (NLM 1, 3, 4) and the status monitor, over
  ONC RPC with the binder on port 111 (`/etc/RPCBTAB` as IBM writes
  it). Every protocol type is generated from an XDR description in the
  tree.
- IBM's exports: `RO`, `RW=`, `ACCESS=`, `ROOT=`, `ANON=`, `NOSUID`,
  `HOSTOPT` with text conversion by CCSID, `/etc/netgroup`; `/QSYS.LIB`
  exported with members as files. A caller is the user profile with
  the wire's UID; root is the anonymous user except for `ROOT=` hosts;
  authority is the profile's, never the wire's group list.
- File handles survive an IPL. NFSv4 has the pseudo-file system over
  the exports, stateful opens with share reservations enforced against
  local jobs and NetServer, byte-range locks shared with NLM and local
  jobs, read delegations, 4.1 sessions with the backchannel, and a
  grace period with reclaim after a restart.
- The daemons are jobs in `QSYSWRK` (`QNFSRPCD`, `QNFSNFSD`, `QNFSMNTD`,
  `QNFSNLMD`, `QNFSNSMD`, `QNFSBIOD`, `QNFSRGYD`), started and ended by
  `STRNFSSVR`, `ENDNFSSVR`, `RPCBIND` and `ENDRPCBIND`. Nothing starts
  them at IPL unless the startup program does, as on IBM i.
- The client: versions 3, 4.0 and 4.1 with IBM's mount options (`hard`,
  `soft`, `intr`, `timeo`, `retrans`, `retry`, `rsize`, `wsize`, the
  attribute cache times, `vers`, `proto`, `port`, `mountport`). A mount
  never holds the storage service: its requests wait on a cache that
  worker threads fill. A server that stops answering is reported to
  `QSYSOPR` (`CPDA1B9`) once, and again when it answers. Byte-range
  locks go to the server and are reclaimed after it restarts.
- NLM version 2 is registered with the binder, as IBM i registers it,
  for clients that ask for it before version 4. Daemons that cannot
  start say why in the job log and on the serial console.
- Writes: a `*SYNC` export (the default) syncs each write before it
  answers, as IBM documents, but the sync runs off the storage
  service's thread and one sync covers every write queued behind it.
  The server prefers 1 MiB reads and writes; a client writes back on
  eight workers. Under QEMU a 1 GiB version 3 write in 1 MiB blocks
  ran at 62 to 80 MiB/s (`*ASYNC`: 136), reads at 95 to 103 (`NFS.md`
  §11).
- `DSPMFSINF`/`STATFS` (display and listing), `RLSIFSLCK`, `UNMOUNT
  TYPE(*NFS *ALL)`, the APIs `QZNFRTVE` and `QZNFNFSO`, the menus
  `CMDNFS`, `CMDMFS` and `CMDRPC`. `MOUNT` and `UNMOUNT` are now IBM's
  V7R1M0 definitions (`TYPE(*NETWORK)` is gone; it was never a value
  there).
- QS/400's own, which no release of IBM i has: `QSYS2.NFS_EXPORT_INFO`
  and `QSYS2.MOUNTED_FS_INFO`; Navigator's Network > NFS pages
  (exports, servers, mounts); the QAP `Nfs` service.
- `tools/nfstest`, a test client written apart from the server, with
  about 90 scenarios, `probe` and `bench`.
- Refused (`NFS.md` §10): NFS version 2 and 4.2, `RPCSEC_GSS` and
  Kerberos (`SERVER(*GSS)`), pNFS, referrals, ACLs beyond mode bits,
  write and directory delegations, named attributes, exporting `/QOPT`,
  `/QDLS`, `/QNTC`, `/QFileSvr.400` or an NFS mount, `CVTRPCSRC`.

**Directory Server** (`DIRSRV.md`, `DIRSRV_GUIDE.md`). An LDAPv3
server in the base operating system, as IBM i's (option 3 of IBM's
5770SS1), started with `STRTCPSVR SERVER(*DIRSRV)` and changed with
`CHGDIRSVRA`, the LDAP client API for ILE programs, and the QSH tools.
Written from the RFCs; the primary's running IBM Directory Server 6.1
settled the wire answers, read anonymously
(`tools/conformance/oracle/dirsrv/`):

- The instance `QUSRDIR` runs as the job `QUSRDIR`, user `QDIRSRV`, in
  `QSYSWRK`, on 389, and with a certificate assigned to the directory
  server's application and `ibm-slapdSecurity` `SSL` or `SSLTLS` on 636
  as well; StartTLS on 389. Its configuration is IBM's `ibmslapd.conf`
  in IBM's path; an attribute QS/400 does not honour stops the start
  with a `GLD` message naming it. The administrator is
  `cn=Administrator`, with no password until `CHGDIRSVRA` sets one.
- Entries live in the object store (`*DIRDB` in `QUSRDIRDB`): indexed,
  changed in the store's transactions, saved with `SAVLIB QUSRDIRDB`,
  checked by `RCLSTG`. A subtree moves in one change.
- Every LDAPv3 operation; paged results, server-side sort, manageDsaIT
  and referrals, persistent search, proxied authorization, tree delete,
  assertion, pre- and post-read, the password policy control; StartTLS,
  whoami and password modify; the change log (`cn=changelog`) when it
  is configured. Assertion, pre- and post-read, whoami, password modify
  and the change log are QS/400's additions: IBM's 6.1 server lacks
  them.
- The schema of RFC 4512, 4519, 4524, 2798, 2307 and 4530 with IBM's
  operational and directory attributes and every name and OID the
  primary lists, so an LDIF exported from IBM's server loads; changes
  through `cn=schema` are kept in `V3.modifiedschema`.
- IBM's access control (`aclEntry`, `entryOwner`, their propagation,
  access classes, pseudo DNs) and password policy (global, group and
  individual); passwords stored `{SSHA512}` by default.
- The user profiles appear read-only under `os400-sys=` and the system's
  name, as IBM projects them. A bind as a profile checks its password
  as sign-on does: a disabled profile is refused, `QMAXSIGN` and
  `QMAXSGNACN` act, a failure writes a `PW` audit entry. A profile with
  function usage `QIBM_DIRSRV_ADMIN` binds as an administrator. SASL
  `EXTERNAL` takes a TLS client certificate.
- `DI` audit entries in `QAUDJRN`; `QSSECCHK` reports anonymous binds,
  binds without TLS and an administrator password unset or easily
  guessed (the shipped instance gives the three warnings).
- Publishing: the agents `QGLDPUBE` and `QGLDPUBA` publish the user
  profiles and the printers to this system's directory or another,
  and keep them in step as profiles change (`DIRSRV.md` §10.1).
- `QGLDCLNT`: IBM's LDAP C API, 159 functions built and 56 refused with
  `LDAP_NOT_SUPPORTED` (`DIRSRV.md` §14), with QS/400's own
  `QSYSINC/H` members `LDAP`, `LBER` and `LDAPSSL` and an RPG copy
  member `QSYSINC/QRPGLESRC(LDAP)`, which IBM does not ship. A program
  is compiled against QS/400's members.
- The QSH tools `ldapsearch`, `ldapadd`, `ldapmodify`, `ldapdelete`,
  `ldapmodrdn`, `ldapchangepwd`, `ldapexop`, `ldapcompare` and
  `ldapdiff`, with IBM's option letters.
- Navigator's Network > Directory server pages (instances, properties,
  suffixes, indexes, schema, access, publishing, counters) and the QAP
  `Dirsrv` service.
- `tools/ldaptest`, a test client written apart from the server, with
  67 scenarios, an `oracle` mode that recorded the primary's anonymous
  answers, and `bench`. Under QEMU: about 480 000 binds a minute on one
  connection, 2 200 a minute with a new connection each; a paged search
  of 100 000 entries in 6 seconds (`DIRSRV.md` §16).
- Refused (`DIRSRV.md` §14): replication, SASL `DIGEST-MD5`,
  `CRAM-MD5`, `GSSAPI` and `OS400-PRFTKN`, filter-based ACLs,
  pass-through authentication, the `aes` and `md5` password
  encryptions, and IBM's own controls and extended operations
  (`1.3.18.0.2.10.*`, `1.3.18.0.2.12.*`), whose encodings IBM does not
  publish.

**Install media.** The media UKI is 13 MB, down from 31 MB at the edge
of the 32 MiB an El Torito boot image can name: its initramfs holds only
a loader, which takes `slic` and the job program from the media once
the signed manifest and their hashes check out (`INSTALL.md`, "The media
UKI"). The boot image now grows with the kernel only.

**Tooling.** The build pipeline is a library, `tools/buildkit`, that
another repository pins and drives with its own product description
(`BUILDKIT.md`); `tools/build` is QS/400's over it, with the same
commands and the same artifacts. The console, the screen kit of the
attended IPL and the installer (`lic/scrkit`), the service tools user
IDs (`lic/svcusers`), mirrored and parity protection
(`storage/protect`), the ublk server and the netlink client are crates
with no QS/400 in them, which the same repositories take; QS/400's
screens and the bytes on its disks are as they were.

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
- `VRYCFG STATUS(*OFF)` of a display changed its `ONLINE` attribute, so
  it stayed off after the next IPL whatever its description said.
- Navigator's Time management page has Sync now, which has the NTP
  client poll at once; Power down / IPL keeps its delay on one line.
- The web products' listeners took up to 50 ms to accept each new
  connection; every response now carries a `Server-Timing` header
  (`SERVERS.md`).

## Deliberately not in V1R2M0

Each has its own handoff document for a later release.

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
- Virtual optical from HTTP or an NFS server named in the device
  description: image catalogs on the IFS only.
- SNA, APPC and pass-through (`STRPASTHR` is refused): TCP/IP only.
- BRMS, save while active, parallel saves, optical media libraries.
- Independent ASPs: the system ASP only.
- Kerberos and EIM: password and RFC 4777 sign-on, SSH keys; NetServer
  signs on with NTLMv2 only; the directory server takes simple binds
  and SASL `EXTERNAL`.
- Directory server replication (suppliers, consumers, gateways).
- PASE for AIX programs and Java: QSH and Linux-native PASE programs.

## Known limits

NetServer's are in `NETSERVER.md` §10. The main ones: the NetBIOS
datagram service (UDP 138) and browsing are not built (clients connect by
name or address); renaming a file or library above a leased member breaks
no lease.

NFS's are in `NFS.md` §10 and §7. The main ones: a file not used
since the IPL is found from its handle by one walk of its file system;
a mode an NFS client sets loses its group bits when the owner has no
primary group (the IFS's authority model); version 3 locks on a mount
need the server's binder; NFSv4 blocking locks are polled. A QS/400
client copying a large file to a QS/400 server with `CPY` writes at
only 4 to 11 MiB/s under QEMU (`NFS.md` §11).

The directory server's are in `DIRSRV.md` §14 and §16. The main ones:
a bind on a new connection costs about 27 ms under QEMU (2 200 a
minute, short of the 10 000 the handoff asked; one connection binds
far faster); entries are added at about 80 a second, so a million take
over three hours to load; LDAP clients resolve `localhost` but not other
host-table names; Navigator's and QAP's directory pages have not been
driven under QEMU yet.

## Before shipping (Cody)

The NFS `NEEDS-CODY` items (`NFS.md` §12): a Linux and a macOS client
mounting a QS/400 export (versions 3, 4.0, 4.1) and QS/400 mounting a
Linux export; an export to the Dell host; on the primary with NFS
started: the daemon jobs' user and `WRKACTJOB` appearance, `/etc/exports`
as IBM reads and writes it with `HOSTOPT`, `DSPMFSINF` of an NFS mount,
the classes of the routing entries, how a disabled profile's UID maps.
`ifs/ifs/tests/linux-xfs.sh` needs root.

The NetServer `NEEDS-CODY` items (`NETSERVER.md`):

- Windows 11, macOS and a Linux kernel client map a share, open and save
  with Notepad, Word and Excel, and print; Windows point and print with
  an in-box class driver; a wrong password `QMAXSIGN` times disables the
  user for NetServer.
- The kernel CIFS mounts of QEMU step 171 need root: confirm they pass on
  CI (Samba's `smbclient` passes locally).
- On an IBM i 7.5 system: whether the NetServer password is kept at
  `QPWDLVL` 4; the date format of a `VP` audit entry; the `QZLSCHSI`
  length bounds, the share-name characters, the `ZLSL0101` extension
  table, the `ZLSL0300` logon types and the `SERVER_SHARE_INFO` value
  spellings that need a change API or a live session; `CPFB68A` reason 6
  for a missing output queue; whether a share CCSID of 0 means 819.

The directory server `NEEDS-CODY` items (`DIRSRV.md` §17): a profile
bind on the primary to settle the projected DN form; the primary's
administrator view (`cn=configuration`, an ACL on a test subtree), only
if Cody agrees to set the administrator's password there; Apache
Directory Studio against QS/400 (browse, edit, a 50 000-entry LDIF, a
profile bind); publishing between the primary and QS/400; the `DI`
audit entry's code values. Also: the slip install from V1R1M0 media
(install step 10 checks the directory server's objects after the
upgrade but has not run: it needs the V1R1M0 development ISO built
first); and whether `dirsrv-publish` may stay re-locked without a
version bump (only a default that is never written changed; the file's
syntax did not).

Then `ci/version.sh prepare 1.2.0`, `tools/kcbuild` pointed at this file,
and the `v1.2.0` tag.
