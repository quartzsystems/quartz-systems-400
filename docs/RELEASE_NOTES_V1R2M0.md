# QS/400 V1R2M0 release notes

V1R2M0 adds NetServer (SMB file and print serving), NFS (the server and
the client) and the Directory Server (LDAP, the server and the client),
three of the features V1R1M0 left out (`RELEASE_NOTES_V1R1M0.md`,
"Deliberately not in V1R1M0"), and the fixes made since V1R1M0. The
marker inventory and the SQL refusals at the end are generated from the
tree (`cargo run -p kcbuild -- notes --update`).

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
  `CLRLIB`), and a directory, file or library renamed, moved or deleted
  above a leased file or member; a job's open for update waits for
  clients to give back cached writes.
- Files get Windows-style 8.3 short names (`XXHHHH~1.EXT`, made from the
  file's number, so they stay the same): in listings, in alternate-name
  queries, and to open files by.
- With "allow NetBIOS" on, the server registers and answers for its
  NetBIOS names on UDP 137 and announces itself to the workgroup's master
  browser on UDP 138 every browse interval.
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

**In a partition of QS/VS** the system has a service channel to the
hypervisor (`QSID.md`): its reference codes as they are posted, its
state, a controlled power down, control panel function 21 at a manual
keylock, and the console as a full screen on a console port.
The system says in its LIC record that it has the channel (TLV
`0x010C`), so the hypervisor need not wait for one an older system lacks.

**QSI**, the protocol a management console speaks to what it manages
(`QSI.md`), is three neutral crates QS/VS and QS/MC pin: its messages
(`lic/qsi`), its session (`lic/qsisess`) and its trust rules, mutual
TLS 1.3 with pinned certificates and pairing by one-time password
(`lic/qsitrust`). QSI also carries a managed system's problems, code
update of its A/B slots and its own dumps, each behind a capability bit
of its own; a host's physical I/O and physical tape drives and a running
partition's entitlement, behind two more; and, behind bits 19 to 27, a
host's switches and storage pools, a partition's settings, activation
times, a host's power-on settings, the panel functions a system has,
problem notes and severity, dump space and the fix level
(`QSI.md` section 5).

**A standalone QS/400 answers QSI.** The server `QSIDSRV` (`*QSI` in
`QSYSWRK`, port 2300, started with TCP/IP; `CHGQSIA` changes its port
and whether it starts, `STRTCPSVR` and `ENDTCPSVR` start and end it)
answers a paired management console with `caps::STANDALONE`: the system's
identity and state with the keylock and IPL side, power down by
`PWRDWNSYS`, this IPL's reference codes, events kept on disk and resumed
after an IPL, the time, the fix level, the system's dumps and their
space, the problem log with notes and severity, and the control panel
and the console at ID 0: a paired console views and types on the
system's console, one viewer at a time with take over, while the local
display keeps working. The system's Ed25519 identity is made at the
server's first start and sealed under the LIC's stash key; at most two
consoles are pinned. A one-time pairing password (15 minutes, five
attempts, kept only in memory) comes from `STRQSIPAIR` or from DST
(Work with DST environment, option 6, Management consoles); `WRKQSICSL`
and `RMVQSICSL` list and remove consoles. Every pairing, refusal,
removal and password made is a `QI` audit entry. These are QS/400's own:
IBM i has no QSI server, commands, DST option or audit entry
(`QSID.md` section 12, `SECURITY.md`; QEMU steps 201 to 210).

The web layer of QS/400's three web products (`net/qsweb`) and the
making and reading of certificates (`net/x509kit`, beneath the
certificate store) are neutral crates QS/MC builds its web UI on
(`BUILDKIT.md` section 17); QS/400's pages are unchanged. So are the
terminal's look, its script and the exchange the script speaks
(`net/termlook`), which draw Access for Web's 5250 session and a
partition's console alike, and the system's reference codes as data
(`lic/srcdata`), which the IPL posts from (`BUILDKIT.md` section 18):
every code now has its cause and recovery. The HTTP client (`net/httpc`)
and the update catalog's reader (`net/updcat`) are neutral too, the
client trusting what its caller gives and streaming a download
(`BUILDKIT.md` section 5).

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
- A slow request in the storage service is waited for, and step 200
  shows the system and every job keep the service across one held 30 s;
  when the service ends, `slic` logs its exit status before `B1001060`,
  and only `slic` may close it (`STORAGE_SERVICE.md`, "Slow requests").
- A tape never written was a device error (`CPF6708`, the drive's BLANK
  CHECK as `EIO`): `DSPTAP` now shows `*NL`, a save is refused with
  `CPF6772` and `INZTAP CHECK(*YES)` initializes it (`MEDIA.md`).
- The save, restore, tape, optical and image catalog commands sent words
  of their own beside a message identifier; each now sends its catalogue
  entry with IBM's identifier for the meaning and IBM's data layout, as
  captured from V7R1M0 (`tools/conformance/oracle/msgdata`). Programs
  that monitored the old identifiers must change: the next tape volume
  is `CPA4088`/`CPA4089` (was `CPA4075`), a write over an active file
  `CPA6754` and `INZTAP CHECK(*YES)` on one `CPF6754` (was `CPA4036`);
  the rest are listed in `MEDIA.md` and `SAVE_FORMAT.md`.
- A PKCS#12 file asking for more than 1,000,000 key derivation
  iterations is refused instead of being worked through.
- The FTP server refused every PTF sent into a `*SAVF`: it wanted whole
  528-byte records, which QS/400 save streams are not. It now takes a save
  stream of any length (`SAVE_FORMAT.md`, "Whole records"), and PTFs are
  padded to whole records so that V1R1M0's server takes them too.
- `CPYFRMSTMF` into `/QSYS.LIB/LIB.LIB/NAME.FILE` fills the save file NAME,
  as IBM i does (`IFS.md`).
- Every refusal of a save file says why (bad magic, the format version,
  unknown flags with their value, or the field at fault by offset): the FTP
  server's `426`, `RST*` and `DSPSAVF` (`CPF3743`), `LODPTF` and `INSPTF`
  (`CPF3598`, which a stream that does not read now sends in place of the
  signature message `CPF3586`) and `CPYFRMSTMF`.
- After `RMVPTF` of a LIC PTF IPLed an older LIC, `DSPPTF` showed no PTFs
  at all: the newer LIC wrote PTF database records the older one dropped.
  The records are as V1R1M0 reads them again; QS00002 to QS00004 were
  rebuilt with the fix under their own IDs.
- Navigator's Fixes page has Check now, and before any check says so
  instead of saying the check found nothing.
- NetServer: a lock on a whole file from a Linux client (`flock`, sent as
  a range to 2^63) was kept in the connection's own job only, so a client
  on another connection could take a conflicting lock. It now reaches the
  IFS, where every job sees it.

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

NetServer's are in `NETSERVER.md` §10. The main ones: the server
announces itself to the workgroup's master browser (UDP 138) but is no
browser itself, and serves no browse list (that needs SMB1); modern
clients, which no longer browse, connect by name or address. A file
renamed, moved or deleted under a client's open in another job breaks
handle caching but is not refused while the client keeps it open (the
IFS allows it; Windows refuses it within one job).

NFS's are in `NFS.md` §10 and §7. The main ones: a file not used
since the IPL is found from its handle by a walk of its file system's
directories (an xfs handle names no parent; xfs parent pointers would
need a new volume format); a mode an NFS client sets loses its group
bits when the owner has no primary group (the IFS's authority model);
version 3 locks on a mount need the server's binder (IBM has no option
naming the lock manager's port); NFSv4 blocking locks are polled, a
4.1 client being told by `CB_NOTIFY_LOCK` when a lock this server holds
goes, and QS/400's own client polls. A `*SYNC` export syncs every
write, as IBM documents: under QEMU a QS/400 client copying 1 GiB to a
QS/400 server with `CPY` writes at 15 to 37 MiB/s, the rest of the
time the host writing back the guests' disks (`NFS.md` §11).

The directory server's are in `DIRSRV.md` §14 and §16. The main one:
entries are added one at a time at 96 a second over 100 000 (195
over 10 000), slowing as the directory grows, so a million take nearly
three hours to load. (Binds with a new connection each, 2 200 a minute
before, are now about 100 000 a minute under QEMU.)

## Refused in SQL

The list of `SQL.md` §10, imported as it stands.

<!-- sql-refused:begin: imported from docs/SQL.md section 10 by `cargo run -p kcbuild -- notes --update`; do not edit -->
Every form QS/400 refuses or does not provide, as the rows above state it,
by section. A refusal is an error with the SQLCODE given and a message
that says what is not supported; nothing is accepted and ignored.
Generated from the tables above; the rows are the authority.

### 2. Statements

- `CREATE OR REPLACE VIEW`, `CREATE OR REPLACE ALIAS`: supported; refused SQL0084 when other views depend on the view
- `CREATE OR REPLACE TABLE` (`ON REPLACE PRESERVE/DELETE ROWS`): partial: refused SQL0084 when views or indexes depend on the table
- `CREATE OR REPLACE INDEX`: refused SQL0104, as IBM
- `ALTER TABLE` ADD/DROP/ALTER COLUMN (SET DATA TYPE, SET/DROP DEFAULT, SET/DROP NOT NULL, SET GENERATED, RESTART, DROP IDENTITY), ADD/DROP CONSTRAINT: partial: refused SQL0084 (BLOCKED(rel)) when indexes or views depend on the table, and for SET DATA TYPE of a constraint key column
- `ALTER TABLE ... ADD/DROP PARTITION`, materialized query tables: refused SQL0084
- `RENAME TABLE/INDEX` (`TO SYSTEM NAME`, `FOR SYSTEM NAME`): supported; refused SQL0084 for an object other views read
- `EXPLAIN` (a Quartz statement; pub400 answers SQL0104): partial: a query's plan text with `QQRID`, `QQRCOD`, `QVC13` per line; ACS Visual Explain (`QSYS.QQQDBVE`, undocumented) not provided
- `CREATE SEQUENCE` (long and delimited names with generated system names; `START WITH` below `MINVALUE` accepted, as the primary), `ALTER SEQUENCE`, `DROP SEQUENCE`, `NEXT VALUE FOR`, `PREVIOUS VALUE FOR`: supported, IBM's cache blocks matched; `ALTER SEQUENCE AS` refused SQL0084; a failed statement gives back the value it used, where IBM keeps it used
- `GRANT` on sequences: refused SQL0084
- `CREATE PROCEDURE ... EXTERNAL` (RPGLE, CL; `PARAMETER STYLE SQL`, `GENERAL`, `GENERAL WITH NULLS`): supported, also before the program exists (SQL7909); service program procedures up to 64 parameters in the call; `LANGUAGE C/COBOL/PLI/JAVA`, `DB2GENERAL`, `JAVA`, `DB2SQL` refused SQL0084
- `ALLOCATE`/`ASSOCIATE`/`DESCRIBE` in routines, `EXECUTE ... INTO`, cursors and handlers in functions, `AS LOCATOR`, `AUTONOMOUS`: refused SQL0084
- `CALL` from QAP: partial: the first result set only; OUT/INOUT refused SQL0084
- `CALL` and trigger-firing changes from `RUNSQL`, `RUNSQLSTM`: supported from a job's command line and programs; from the service console and QAP still refused SQL0084
- External table functions; `GENERAL` style external functions; `DBINFO`; external functions during an in-place scan or from the service console: refused SQL0084: BLOCKED(rel)
- Array parameters, array arguments in `CALL`: refused SQL0084: BLOCKED(rel)
- Distinct types as SQL PL variables or parameters, sequence types, in `ALTER TABLE`: refused SQL0084: BLOCKED(rel)
- `FOR EACH STATEMENT`, `OLD/NEW TABLE`, `INSTEAD OF`, several events (`INSERT OR ...`), `SECURED`: refused SQL0084 after IBM's own checks (SQL0628, SQL0696, SQL0159, SQL0156): BLOCKED(rel)
- `GRANT`, `REVOKE` on schemas, routines, sequences, variables, types; column privileges: refused SQL0084
- `LOCK TABLE`: refused SQL0084
- `PREPARE`, `EXECUTE`, `EXECUTE IMMEDIATE`, `DESCRIBE`, cursor statements, `WHENEVER`, `INCLUDE`, locator statements given to a dynamic interface (JDBC, STRSQL, RUNSQLSTM, `PREPARE` text): refused as IBM refuses them: SQL0104, SQL0084, SQL0312 for host variables, SQL0199
- `DESCRIBE CURSOR`, `DESCRIBE PROCEDURE`, `ALLOCATE/ASSOCIATE LOCATORS` in programs: refused SQL0084 (requesters describe `RS#n` cursors internally)
- `SET OPTION` (every program option, SQL5066 order): supported; values QS/400 cannot honour refused SQL0084
- LOB file reference variables: declared; any use refused SQL0084: BLOCKED(rel), the job's runtime has no IFS access
- `CONNECT TO [USER ... USING ...]`, `CONNECT RESET`, `SET CONNECTION`, `RELEASE`, `DISCONNECT` (DRDA client, DUW): partial: programs' embedded SQL and the job-side requesters; queries, cursors, immediate statements and `PREPARE`/`EXECUTE` run remotely; remote input values, scrolling `FETCH`, `DESCRIBE`, `GET DIAGNOSTICS` refused SQL0084; SQL0842, 0843, 0752, 0950, 30080, 30082
- `RDBCNNMTH(*RUW)`: refused SQL0084 (accepted and ignored before)
- Three-part names, `CREATE ALIAS` over a remote table: refused SQL0104
- `TRUNCATE` (all options): supported; SQL0100 on an empty table; `RESTART IDENTITY` on an identity table refused SQL0084 (BLOCKED(rel), joins identity state)
- `REFRESH TABLE`: refused SQL0084
- `CREATE TABLE/VIEW/INDEX/ALIAS` in QTEMP: supported; sequences and variables in QTEMP SQL0707; types, routines and triggers there refused SQL0084; a permanent view over a QTEMP table SQL7008

### 3. Queries

- `LATERAL`, `TABLE(fullselect)`, `TABLE(f(args)) AS x`: supported; a RIGHT or FULL lateral join, and a lateral or USING join as a MERGE source, refused SQL0084
- `FETCH FIRST n PERCENT`: refused SQL0104, as pub400
- `NULLS FIRST`, `NULLS LAST`: refused SQL0199, as pub400
- Common table expressions, chained and recursive, `CYCLE`: supported; `CYCLE ... USING` refused SQL0084
- OLAP: `ROW_NUMBER`, `RANK`, `DENSE_RANK`, `NTILE`, `PERCENT_RANK`, `CUME_DIST`, `LAG`/`LEAD` with `RESPECT/IGNORE NULLS`, `FIRST_VALUE`/`LAST_VALUE`/`NTH_VALUE`, `RATIO_TO_REPORT`, aggregates `OVER` with `ROWS` and `RANGE` frames: supported; `NTH_VALUE ... FROM LAST` refused SQL0084
- `LIKE_REGEX`: refused SQL0104, as pub400
- `ARRAY_EXISTS`, `XMLEXISTS`: refused SQL0084
- `SKIP LOCKED DATA` on queries: supported; on other statements refused SQL0084
- `USE AND KEEP ... LOCKS`, `WAIT FOR OUTCOME`, `USE CURRENTLY COMMITTED`: refused SQL0084
- `UPDATE ... FROM`: refused SQL0199, as pub400

### 4. DDL details

- `CHECK`: supported: comparisons, `AND/OR/NOT`, `IS NULL`, `BETWEEN`, `IN`, `LIKE`, arithmetic, concatenation, `UPPER LOWER LENGTH SUBSTR TRIM LTRIM RTRIM ABS MOD COALESCE`; other functions refused SQL0084
- `NOT ENFORCED`, `DEFERRABLE`, `ENABLE/DISABLE QUERY OPTIMIZATION`: refused SQL0084
- Identity columns: `START`, `INCREMENT`, `MINVALUE/MAXVALUE`, `CYCLE`, `CACHE`, `ORDER`, `BIGINT`/`DECIMAL`, `OVERRIDING SYSTEM/USER VALUE`, `IDENTITY_VAL_LOCAL()`: partial: a range past 19 digits refused SQL0084 (BLOCKED(rel)); `ORDER` kept, without effect on one system
- `IMPLICITLY HIDDEN`: refused SQL0084: BLOCKED(rel), queries have no hidden-column flag yet (IBM accepts it)
- `CREATE TABLE ... AS (query) WITH [NO] DATA`, `DEFINITION ONLY`: partial: an expression column is refused SQL0084 until expression types land
- `CREATE TABLE ... LIKE`, `(LIKE t)`: supported; `INCLUDING IDENTITY`, `EXCLUDING COLUMN DEFAULTS`, `USING TYPE DEFAULTS` refused SQL0084
- `WITH [LOCAL \| CASCADED] CHECK OPTION` (SQL0161): partial: on views that stay logical files; refused SQL0084 on others
- `CREATE INDEX ... WHERE` (sparse): partial: conditions select/omit can express; others refused SQL0084
- Expression (derived) index keys: refused SQL0084: BLOCKED(rel), access paths hold no key expressions
- `INCLUDE` on an ordinary index: refused SQL0104, as IBM
- `VOLATILE`/`NOT VOLATILE` [`CARDINALITY`] on CREATE and ALTER TABLE: kept with the table (`*SQLINF` tag 19 `TABLE`); the planner uses a key that serves regardless of statistics and asks none for the table; `QSYS2.SYSTABLESTAT.VOLATILE` and `SYSPARTITIONSTAT.VOLATILE` show `1`/`0` as 7.5 does (7.1 has no such column); a second VOLATILE clause is SQL0104. `IN schema` is ignored with warning SQ20367 (01680) and `IN ASP n` is SQL0104, as on 7.1 and 7.5. `UNIT ANY|SSD` (tables and indexes) and `PAGESIZE 8|16|32|64|128|256|512` (indexes) are no-ops, as IBM documents them for a store with fixed pages and one media tier; any other unit is SQL0104 and any other page size SQL1583 (428DE), as IBM. Kit: `ddl/VOLATIL1` (pub400), `ddl/TBLOPTS` (primary)

### 5. Functions

- `ENCRYPT_AES`, `ENCRYPT_RC2`, `ENCRYPT_TDES`, `DECRYPT_*`, `GETHINT`: refused SQL0084: IBM's encrypted layout is unpublished, so nothing made here could be read there
- `VERIFY_GROUP_FOR_USER`: refused SQL0084
- `XMLAGG`: refused SQL0084

### 7. Data types

- `CURRENT DECFLOAT ROUNDING MODE` and `SET` (7 modes): supported; a mode from a host variable refused SQL0084
- `FOR MIXED DATA`: refused SQL0604
- A LOB as an index or constraint key: refused SQL0350
- Changes through a read-only view: refused SQL0150
- `CLOB ... FOR BIT DATA`: refused SQL0683
- LOB locators, file reference variables, `FREE LOCATOR`: refused: BLOCKED(rel), the engine's locator table exists; the statements come with dynamic and embedded SQL
- `XML` as a column or `CAST` type, `XMLPARSE(CONTENT)`, `XMLQUERY`, `XMLTABLE`, `XMLEXISTS`, `XMLVALIDATE`, `XMLAGG` and the rest: refused SQL0084
- `ROWID`, `DATALINK`: refused SQL0084
- SQL/JSON path filters, item methods, `**`, `@`, variables, `PASSING`: refused SQL0084 (pub400 runs them)
- `JSON_UPDATE`: refused SQL0204, as pub400 (7.5 has none)

### 8. Catalogs

- `QSYS2.SYSCOLAUTH`: present, always empty: BLOCKED(rel), the store keeps no column authorities
- Other `QSYS2` catalogs, `SYSIBM` metadata views: not present: SQL0204

### 9. Services, tools and interfaces

- `HISTORY_LOG_INFO`: partial: `GENERATE_SYSLOG` and `EOF_DELAY` refused SQL0443
- `SPOOLED_FILE_INFO`, `SYSTEM_STATUS` (table function): partial: timestamp filters refused SQL0443; `RESET_STATISTICS` not applied
- `DISPLAY_JOURNAL`: partial: `GENERATE_SYSLOG`, `EOF_DELAY` refused SQL0443; thread, remote and syslog columns null
- `OBJECT_LOCK_INFO`, `MEMORY_POOL_INFO`, `MEMORY_POOL`, `SYSTEM_ACTIVITY_INFO`, `ASP_INFO`, `SYSDISKSTAT`, `HARDWARE_RESOURCE_INFO`, `MEDIA_LIBRARY_INFO`, `COLLECTION_SERVICES_INFO`, `EXIT_POINT_INFO`, `EXIT_PROGRAM_INFO`, `ENVIRONMENT_VARIABLE_INFO`, `GROUP_PTF_INFO`, `SYSTOOLS.GROUP_PTF_DETAILS`, `TCPIP_INFO`, `IFS_OBJECT_PRIVILEGES`, `IFS_OBJECT_LOCK_INFO`: partial: columns QS/400 keeps no value for are null (listed per service in `services/`); `RESET_STATISTICS => 'YES'` refused SQL0443; job-level environment variables not shown
- `DNS_LOOKUP`: partial: the system resolver; `DOMAIN_SERVER` refused SQL0084
- `SYSTOOLS.CHANGE_USER_PROFILE`: partial: preview or run; special-authority actions other than `*SAME` refused SQL0084
- `GENERATE_SQL`, `GENERATE_SQL_OBJECTS`: partial: tables, views, indexes, sequences, aliases with constraints, labels, grants and the drop, replace, unformatted, unqualified and system-name forms, laid out as pub400; other object types and options refused SQL0084; no member written in QTEMP
- `OVERRIDE_QAQQINI`: partial: tuning options kept as no-ops; options that change results or messages refused SQL0084 unless `*DEFAULT`
- `SET_SERVER_SBS_ROUTING`, `SET_PASE_SHELL_INFO` (SSH always starts QSH; no per-user shell is kept), `SYSTOOLS.LPRINTF`, `SYSTOOLS.GENERATE_SPREADSHEET`: refused SQL0084
- `STRSQL`: saved sessions (SQL6102, SQL6335, SQL7971), F13 options 1 to 4 (session attributes reshaping as pub400's, printer file, remove entries, save to a source file), SELECT output to display, printer (`QPQUPRFIL` report) and file, display formats (edited numbers, every date and time format, `*COMMA`), F19/F20 at pub400's column widths, F4 then F12: partial: output to a file supports options 1, 2, 4, 5; OUTMBR other than `*FILE`/`*FIRST` and OUTOPT(3) refused SQL0084; run at a real TN5250 session in QEMU steps 130 and 138
- `CONNECT`, `SET CONNECTION`, `RELEASE`, `DISCONNECT` in STRSQL: supported through the DRDA client; other statements go to the current remote server; a query there refused SQL0084 (BLOCKED(rel): no column descriptions come back)
- `RUNSQLSTM`, `RUNSQL`: partial: options checked (non-default `USRPRF`, `DYNUSRPRF`, `SAAFLAG`, `FLAGSTD` and values QS/400 cannot honour refused SQL0084; `ALWCPYDTA`, `ALWBLK`, `DLYPRP`, `SYSTIME`, `TGTRLS` without effect); a SELECT in RUNSQLSTM or RUNSQL refused SQL0084 (severity 30), as pub400; not yet run end to end
- `CRTSQLRPGI` options: settings it does not apply refused SQL0084 (`RPGPPOPT`, `CONACC`, `DECFLTRND`, `SQLCURRULE`, `DECRESULT`, `SRTSEQ`, `DFTRDBCOL`, `DYNDFTCOL`, `SQLPATH`, `COMPILEOPT`, `DYNUSRPRF`, `SAAFLAG`, `FLAGSTD`), accepted and ignored before
- DRDA server (JDBC, ACS): partial: `EXCSQLSET`, `CALL` with result sets and output values (`SQLDTARD`), bind into packages; LOB locators refused SQL0084
- Other `SYSIBM` views (`_S` forms, `ROUTINES`, `PARAMETERS`, ...): not present: SQL0204
- The other 81 `QSQCLI` exports (`SQLGetInfo`, `SQLMoreResults`, the wide forms, descriptors, locators): not present: a program that imports one fails to bind (CPD5D02)
- `DSPSQLPKG`: not present: no such command on V7R1M0 either (CPD0030)
- `CRTSQLCI`, `CRTSQLCBLI`: not present
- `STRDBMON`, `ENDDBMON` (output file of the primary's 276-column `QQQDBMN` format): partial: record types 3018, 1000, 3000, 3001, 3003, 3010, 3014; programs, host servers, DDM and QAP monitored, STRSQL, RUNSQL and RUNSQLSTM not; `TYPE(*DETAIL)` writes the same records as `*SUMMARY` (no 3019); `FTR*` filters, `FTRSQLCODE`, `JOB` operator `*NE`, `HOSTVAR` other than `*BASIC`, thresholds, `INCSYSSQL(*YES)` refused CPF9898
- `PRTSQLINF` of `*PGM`/`*SRVPGM`: supported for programs `CRTSQLRPGI` makes from now on, laid out as pub400's; of `*SQLPKG` and `*JOB` refused SQL9011 (BLOCKED(rel))
- `ADDPFTRG` (`RPLTRG`, generated names, `ALWREPCHG`, `TRGUPDCND`, `THDSAFE`, `MLTTHDACN`), `RMVPFTRG`, `CHGPFTRG`, `DSPFD TYPE(*TRG)` with `OUTPUT(*OUTFILE)` (`QAFDTRG`): supported; other `DSPFD` outfile types refused CPD0084
- `*READ` triggers on record I/O, logical files (one based-on member) and SQL queries (once per row the scan keeps): supported; in a join the inner table fires for every row its scan keeps where the primary fires on matches only; refused SQL0084 where no program can be called (the service console)
- SQL changes firing external and SQL triggers: supported for statements of programs, the host server, DRDA, QAP, STRSQL, RUNSQL and RUNSQLSTM; refused SQL0084 from the service console
- Triggers from `CPYF` (insert triggers; a failure stops the copy with CPF2972), DFU and the DDM server: supported; `CPYF MBROPT(*UPDADD)` into a file with update triggers refused CPF502B; FTP and the service console still refused
- `ALWREPCHG(*NO)` on after triggers: supported: the program's own change of the row is not refused, as on the primary

### 9b. Performance

- Plan cache (statement text and naming, 512 MiB or 10,000 entries); `QSYS2.DUMP_PLAN_CACHE`, `CLEAR_PLAN_CACHE`, `CHANGE_PLAN_CACHE_SIZE` (`*JOBCTL`): partial: the parsed statement is cached, the access plan chosen again each run; the entry-selection parameters refused SQL0084; `QIBM_DB_SQLADM` not accepted (no function usage registry)
<!-- sql-refused:end -->

## Marker inventory

Every marker left in the tree, generated from it.

<!-- inventory:begin: generated by `cargo run -p kcbuild -- notes --update`; do not edit -->
199 markers in the tree.

| Marker | Where | Reason |
|---|---|---|
| `BLOCKED(rel)` | `database/db/src/odp/mod.rs:767` | a logical path over more than one member |
| `BLOCKED(rel)` | `database/db/src/util/cpyf/mod.rs:538` | an update by *UPDADD does not fire update |
| `BLOCKED(rel)` | `database/sql/src/dbmon/write.rs:100` | records are written with INSERT, which |
| `BLOCKED(rel)` | `database/sql/src/exec/alter.rs:417` | dependent files are not rebuilt", |
| `BLOCKED(rel)` | `database/sql/src/exec/catalog/objects2.rs:13` | variables record no |
| `BLOCKED(rel)` | `database/sql/src/exec/catalog/objects2.rs:17` | ( |
| `BLOCKED(rel)` | `database/sql/src/exec/catalog/services/admin.rs:58` | adding or removing one special authority needs |
| `BLOCKED(rel)` | `database/sql/src/exec/catalog/services/admin.rs:133` | a statement runs in the storage service, which |
| `BLOCKED(rel)` | `database/sql/src/exec/catalog/services/gensql/format.rs:54` | the standards, obfuscation, temporal, additional |
| `BLOCKED(rel)` | `database/sql/src/exec/catalog/services/gensql/mod.rs:117` | routines, triggers, types, variables, schemas, |
| `BLOCKED(rel)` | `database/sql/src/exec/catalog/services/jobs.rs:246` | no table of ended jobs). |
| `BLOCKED(rel)` | `database/sql/src/exec/catalog/services/procs.rs:151` | PRINT => 'PRINT' writes a spooled file of the |
| `BLOCKED(rel)` | `database/sql/src/exec/catalog/services/qaqqini.rs:186` | these options change results or messages |
| `BLOCKED(rel)` | `database/sql/src/exec/catalog/services/sysconf.rs:291` | the system's resolver answers; asking one server |
| `BLOCKED(rel)` | `database/sql/src/exec/column.rs:93` | an 8-byte counter" |
| `BLOCKED(rel)` | `database/sql/src/exec/column.rs:116` | queries have no implicitly hidden column yet".into(), |
| `BLOCKED(rel)` | `database/sql/src/exec/ddl.rs:426` | DROP PRIMARY KEY leaves the path keyed and |
| `BLOCKED(rel)` | `database/sql/src/exec/ddl.rs:589` | ALTER TABLE keeps no COLTYPE entries yet. |
| `BLOCKED(rel)` | `database/sql/src/exec/func/services.rs:89` | the encoding modes pub400 accepts are not |
| `BLOCKED(rel)` | `database/sql/src/exec/grant.rs:63` | routines, sequences, schemas and packages as |
| `BLOCKED(rel)` | `database/sql/src/exec/grant.rs:72` | the store keeps no column authorities. |
| `BLOCKED(rel)` | `database/sql/src/exec/locator.rs:16` | locator host variables (`SQL TYPE IS CLOB_LOCATOR`), |
| `BLOCKED(rel)` | `database/sql/src/exec/mod.rs:1437` | statements run where no trigger program can |
| `BLOCKED(rel)` | `database/sql/src/exec/qtemp.rs:447` | routines, types and triggers in QTEMP need their |
| `BLOCKED(rel)` | `database/sql/src/exec/reads.rs:15` | the primary reads a join's inner table by the join's |
| `BLOCKED(rel)` | `database/sql/src/exec/reads.rs:45` | a context with no job (the service console, the |
| `BLOCKED(rel)` | `database/sql/src/exec/reads.rs:63` | a join or multi-format logical file's rows are |
| `BLOCKED(rel)` | `database/sql/src/exec/rename.rs:35` | the views over it keep its name in their query. |
| `BLOCKED(rel)` | `database/sql/src/exec/routine/call.rs:167` | this interface runs statements in the database |
| `BLOCKED(rel)` | `database/sql/src/exec/routine/create.rs:637` | distinct types need strong typing in the engine. |
| `BLOCKED(rel)` | `database/sql/src/exec/routine/create.rs:788` | an external table function is called for each row |
| `BLOCKED(rel)` | `database/sql/src/exec/routine/extfunc.rs:52` | PARAMETER STYLE GENERAL gives the result as the |
| `BLOCKED(rel)` | `database/sql/src/exec/routine/extfunc.rs:78` | an external table function is called once per row |
| `BLOCKED(rel)` | `database/sql/src/exec/routine/extfunc.rs:173` | a statement run where no program can be called |
| `BLOCKED(rel)` | `database/sql/src/exec/routine/extfunc.rs:181` | a scan holding the store open cannot give it back. |
| `BLOCKED(rel)` | `database/sql/src/exec/schema.rs:26` | IBM's SYSCHKCST, SYSCSTDEP, |
| `BLOCKED(rel)` | `database/sql/src/exec/table.rs:326` | IBM keeps the views and indexes over a replaced |
| `BLOCKED(rel)` | `database/sql/src/exec/trigger/mod.rs:120` | a statement trigger is called once per statement |
| `BLOCKED(rel)` | `database/sql/src/exec/trigger/mod.rs:155` | transition tables of a row trigger. |
| `BLOCKED(rel)` | `database/sql/src/exec/trigger/mod.rs:177` | an insert, update or delete of a view runs its |
| `BLOCKED(rel)` | `database/sql/src/exec/trigger/mod.rs:256` | a column whose storage is not a routine variable's. |
| `BLOCKED(rel)` | `database/sql/src/exec/udt/mod.rs:14` | a variable, routine parameter or result, sequence or |
| `BLOCKED(rel)` | `database/sql/src/exec/udt/typing.rs:387` | a quantified or `IN` subquery's column is not typed |
| `BLOCKED(rel)` | `database/sql/src/exec/view.rs:35` | IBM keeps views over a replaced view when |
| `BLOCKED(rel)` | `database/sql/src/exec/view.rs:128` | a view that is not a logical file is read only, so |
| `BLOCKED(rel)` | `database/sql/src/exec/view.rs:232` | an expression column has no type until the |
| `BLOCKED(rel)` | `database/sql/src/options.rs:110` | the job's connections follow the distributed unit |
| `BLOCKED(rel)` | `database/sql/src/parse/ddl.rs:296` | derived keys need key expressions in the |
| `BLOCKED(rel)` | `database/sql/src/parse/mod.rs:582` | an XML column needs a stored field type (DB_FORMAT |
| `BLOCKED(rel)` | `database/sql/src/parse/mod.rs:596` | a distinct type is a column's or a CAST's |
| `BLOCKED(rel)` | `database/sql/src/parse/trigger.rs:44` | one trigger for several events, with the |
| `BLOCKED(rel)` | `database/sql/src/pl/compile.rs:693` | a CALL passes values, and an array |
| `BLOCKED(rel)` | `database/sql/tests/trigger.rs:266` | in exec::reads); every row its scan keeps here. |
| `BLOCKED(rel)` | `docs/DB_OPEN_QUESTIONS.md:283` | the database |
| `NEEDS-CODY` | `docs/DIRSRV.md:1447` | §17 item 5). A failed bind as an OS profile also |
| `NEEDS-CODY` | `docs/DIRSRV.md:1923` | 17. |
| `NEEDS-CODY` | `docs/DIRSRV.md:1962` | item 5 |
| `NEEDS-CODY` | `docs/DIRSRV.md:1974` | Apache Directory Studio, the primary's administrator view, publishing to and from the primary |
| `NEEDS-CODY` | `docs/NFS.md:244` | IBM's user, section 12), as the service's own |
| `NEEDS-CODY` | `docs/NFS.md:1238` | IBM's own `HOSTOPT` form in `/etc/exports` and what its |
| `NEEDS-CODY` | `docs/NFS.md:1890` | the primary's `EXPE0100` |
| `NEEDS-CODY` | `docs/NFS.md:1932` | settled by mounting the primary's export as a UID whose profile is disabled); none: the anonymous user |
| `NEEDS-CODY` | `docs/NFS.md:2486` | 12. |
| `BLOCKED(rel)` | `docs/PHASE12.md:99` | SAVLICPGM writes no IFS records, RSTLICPGM restores none |
| `BLOCKED(rel)` | `docs/PHASE12.md:104` | at jobcmd.rs:153 |
| `BLOCKED(rel)` | `docs/PHASE12.md:138` | (b855a775) |
| `NEEDS-CODY` | `docs/PHASE12.md:160` | the primary has no QAUDJRN |
| `BLOCKED(rel)` | `docs/PHASE12.md:169` | `DB_OPEN_QUESTIONS.md` (Utilities) \| what an RPG program sees in a `*USA` time field's buffer \| §1: an RPG program on pub400 \| settled on pub400; the buffer form |
| `NEEDS-CODY` | `docs/PHASE12.md:171` | (a JTOpen probe) |
| `NEEDS-CODY` | `docs/PHASE12.md:173` | needs a test user profile on the primary |
| `NEEDS-CODY` | `docs/PHASE12.md:175` | physical tape acceptance |
| `NEEDS-CODY` | `docs/PHASE12.md:180` | `UI_OPEN_QUESTIONS.md` 4, 6, 8, 12 \| start-up response record; keys that carry data; the printer data stream; SST's audit code \| §1: the primary's telnet \| 4 closed; 6 open (a display-file probe); 8 a |
| `NEEDS-CODY` | `docs/PHASE12.md:253` |  |
| `NEEDS-CODY` | `docs/PHASE12.md:262` | ": Proxmox, ESXi, Workstation, VirtualBox, Hyper-V Gen2 and the Dell checklists; also try booting the ISO as a CD on real firmware \| open |
| `NEEDS-CODY` | `docs/PHASE12.md:366` |  |
| `NEEDS-CODY` | `docs/PHASE12.md:375` | 11 above) |
| `BLOCKED(rel)` | `docs/PHASE3.md:147` | `RCVJRNE`'s |
| `BLOCKED(rel)` | `docs/PHASE9.md:217` | the ABI items below |
| `BLOCKED(rel)` | `docs/PHASE9.md:218` | no ILE binding of `QSOSSLSR` (`gsk_*`) exists; `net/tls` has the semantics only (`docs/CERTS.md`), so `CONFIG_H` is built without `HAVE_SSLAPI` |
| `BLOCKED(rel)` | `docs/PHASE9.md:220` | `SIIOF` and `SIIMF` copy `QSYSINC/QRPGLESRC,FCNTL` and `SYSSTAT`, IBM's headers, which QS/400 does not ship; `JSONPARSE` is a `DATA-INTO` parser (see "Blocked" above) |
| `BLOCKED(rel)` | `docs/PHASE9.md:260` | (a change of the call ABI, `ABI.md` §3, which the V1R1M0 freeze of the module format holds to a version bump): a pointer taken to a by-reference parameter |
| `BLOCKED(rel)` | `docs/PHASE9.md:268` | (the same ABI change): a parameter passed by reference and a pointer |
| `NEEDS-CODY` | `docs/QUALIFIED_HARDWARE.md:7` | its own disk and powered down cleanly. Everything listed as |
| `NEEDS-CODY` | `docs/QUALIFIED_HARDWARE.md:22` | Proxmox VE 8 \| OVMF \| virtio-scsi (default), virtio-blk, SATA \| virtio-net \| `Default` VGA, serial \| QEMU guest agent option |
| `NEEDS-CODY` | `docs/QUALIFIED_HARDWARE.md:23` | VMware ESXi 8 \| EFI \| PVSCSI, LSI Logic SAS, SATA, NVMe \| vmxnet3, e1000e \| SVGA through the GOP \| none (VMware Tools is not offered); vsock |
| `NEEDS-CODY` | `docs/QUALIFIED_HARDWARE.md:24` | VMware Workstation 17 \| EFI \| as ESXi \| vmxnet3, e1000e \| as ESXi \| none; vsock |
| `NEEDS-CODY` | `docs/QUALIFIED_HARDWARE.md:25` | Oracle VirtualBox 7 \| EFI \| AHCI (SATA), virtio-scsi, NVMe \| Intel PRO/1000 (e1000), virtio-net \| VBoxVGA/VMSVGA through the GOP \| none |
| `NEEDS-CODY` | `docs/QUALIFIED_HARDWARE.md:26` | Microsoft Hyper-V, Generation 2 \| UEFI \| SCSI (storvsc) \| synthetic (netvsc) \| Gen2 EFI framebuffer \| none; vsock (hv_sock) |
| `NEEDS-CODY` | `docs/QUALIFIED_HARDWARE.md:27` | Dell PowerEdge with PERC H330 in HBA mode, 4 x 1 TB SSD (reference physical target, `HARDWARE_NOTES.md`) \| UEFI \| MegaRAID (`megaraid_sas`) \| the board's Broadcom or Intel ports \| iDRAC/Matrox VGA thr |
| `NEEDS-CODY` | `docs/QUALIFIED_HARDWARE.md:149` | hypervisor checklists |
| `NEEDS-CODY` | `docs/QUALIFIED_HARDWARE.md:204` | the Dell with the PERC H330 |
| `NEEDS-CODY` | `docs/SECURITY.md:606` | with directory auditing on for a test instance on the |
| `NEEDS-CODY` | `docs/SQL.md:57` | `sql::objects` |
| `BLOCKED(rel)` | `docs/SQL.md:60` | when indexes or views depend on the table, and for SET DATA TYPE of a constraint key column \| primary \| `integrity/CSTALTER`, `integrity/IDENTITY` |
| `BLOCKED(rel)` | `docs/SQL.md:88` | none \| |
| `BLOCKED(rel)` | `docs/SQL.md:92` | primary \| `sqlpl/ARRAYS` |
| `BLOCKED(rel)` | `docs/SQL.md:97` | primary \| |
| `BLOCKED(rel)` | `docs/SQL.md:99` | primary \| `trigger/TRGDEFS` |
| `BLOCKED(rel)` | `docs/SQL.md:113` | the job's runtime has no IFS access \| pub400 \| `rpg/b11sql/SQL060` |
| `BLOCKED(rel)` | `docs/SQL.md:118` | joins identity state) \| pub400 \| `query/CHANGES` |
| `BLOCKED(rel)` | `docs/SQL.md:163` | DROP PRIMARY KEY leaves the path, ALTER TABLE ADD PRIMARY KEY does not make one \| primary \| `integrity/CSTBASIC` |
| `BLOCKED(rel)` | `docs/SQL.md:169` | `ORDER` kept, without effect on one system \| primary \| `integrity/IDENTITY` |
| `BLOCKED(rel)` | `docs/SQL.md:175` | queries have no hidden-column flag yet (IBM accepts it) \| pub400 \| `integrity/AUDITCOL` |
| `BLOCKED(rel)` | `docs/SQL.md:185` | access paths hold no key expressions \| primary \| `ddl/INDEXEXT` |
| `BLOCKED(rel)` | `docs/SQL.md:253` | the engine's locator table exists; the statements come with dynamic and embedded SQL \| none \| |
| `BLOCKED(rel)` | `docs/SQL.md:279` | the store keeps no column authorities \| pub400 \| `sql::objects` |
| `BLOCKED(rel)` | `docs/SQL.md:329` | no column descriptions come back) \| none \| |
| `BLOCKED(rel)` | `docs/SQL.md:352` | pub400 \| `rpgc::prtsqlinf` |
| `BLOCKED(rel)` | `docs/SQL.md:432` | when indexes or views depend on the table, and for SET DATA TYPE of a constraint key column |
| `BLOCKED(rel)` | `docs/SQL.md:442` | External table functions; `GENERAL` style external functions; `DBINFO`; external functions during an in-place scan or from the service console: refused SQL0084: |
| `BLOCKED(rel)` | `docs/SQL.md:443` | Array parameters, array arguments in `CALL`: refused SQL0084: |
| `BLOCKED(rel)` | `docs/SQL.md:444` | Distinct types as SQL PL variables or parameters, sequence types, in `ALTER TABLE`: refused SQL0084: |
| `BLOCKED(rel)` | `docs/SQL.md:445` | `FOR EACH STATEMENT`, `OLD/NEW TABLE`, `INSTEAD OF`, several events (`INSERT OR ...`), `SECURED`: refused SQL0084 after IBM's own checks (SQL0628, SQL0696, SQL0159, SQL0156): |
| `BLOCKED(rel)` | `docs/SQL.md:451` | the job's runtime has no IFS access |
| `BLOCKED(rel)` | `docs/SQL.md:455` | joins identity state) |
| `BLOCKED(rel)` | `docs/SQL.md:476` | `ORDER` kept, without effect on one system |
| `BLOCKED(rel)` | `docs/SQL.md:477` | queries have no hidden-column flag yet (IBM accepts it) |
| `BLOCKED(rel)` | `docs/SQL.md:482` | access paths hold no key expressions |
| `BLOCKED(rel)` | `docs/SQL.md:499` | the engine's locator table exists; the statements come with dynamic and embedded SQL |
| `BLOCKED(rel)` | `docs/SQL.md:507` | the store keeps no column authorities |
| `BLOCKED(rel)` | `docs/SQL.md:522` | no column descriptions come back) |
| `BLOCKED(rel)` | `docs/SQL.md:531` | `PRTSQLINF` of `*PGM`/`*SRVPGM`: supported for programs `CRTSQLRPGI` makes from now on, laid out as pub400's; of `*SQLPKG` and `*JOB` refused SQL9011 ( |
| `NEEDS-CODY` | `docs/SQL.md:575` | JTOpen `DatabaseMetaData` diff clean; ACS Run SQL Scripts checklist signed off \| `DatabaseMetaData`: met, QEMU step 132 agrees with the primary's `metadata.out` line for line; ACS checklist: |
| `BLOCKED(rel)` | `docs/SQL.md:608` | in `rpg/sqlpre/src/stmt.rs`); JDBC and the CLI read |
| `BLOCKED(rel)` | `docs/SQL.md:613` | `COLUMN_NAME`); a LOB column of a table and `WRITE_LOB` are refused SQL0084 ( |
| `BLOCKED(rel)` | `docs/SQL.md:616` | in `net/hostserver/src/database/catalog.rs`). |
| `BLOCKED(rel)` | `docs/SQL.md:618` | in `work/qsjob/src/console/drda.rs`). |
| `NEEDS-CODY` | `docs/SQL.md:619` | ACS Run SQL Scripts checklist ( |
| `NEEDS-CODY` | `docs/SQL.md:621` | ( |
| `BLOCKED(rel)` | `docs/STORAGE_SERVICE.md:479` | until each can reach its job). DFU |
| `BLOCKED(rel)` | `docs/STORAGE_SERVICE.md:531` | the service console has no job runtime). `MODE DB2SQL` after |
| `BLOCKED(rel)` | `docs/STORAGE_SERVICE.md:542` | `ALWREPCHG(*NO)` on an after trigger does not yet |
| `BLOCKED(rel)` | `docs/STORAGE_SERVICE.md:543` | refuse the program's own change of the same record ( |
| `BLOCKED(rel)` | `docs/SYSAPI.md:568` | the DRDA requester), and a CLI call made by a procedure |
| `BLOCKED(rel)` | `docs/TCPIP.md:1168` | VT and 3270 sessions and |
| `BLOCKED(rel)` | `docs/WORKMGMT.md:1250` | a program running |
| `NEEDS-CODY` | `ifs/ifscmd/src/mfsinf.rs:61` | IBM's wording for an NFS mount (section 12). |
| `BLOCKED(rel)` | `lic/slic/src/cpp/compdiags.rs:11` | `OPTION(*EVENTF)` is accepted by every compiler command |
| `BLOCKED(rel)` | `lic/slic/src/cpp/dbmoncmd.rs:85` | a monitor watches one job or every job. |
| `BLOCKED(rel)` | `lic/slic/src/cpp/dbmoncmd.rs:307` | the planner makes no time or storage estimate a |
| `BLOCKED(rel)` | `lic/slic/src/cpp/dbmoncmd.rs:321` | query options come from OVERRIDE_QAQQINI, not a |
| `BLOCKED(rel)` | `lic/slic/src/cpp/jrncmd.rs:58` | CMPJRNIMG's entry selection (sequence numbers, |
| `BLOCKED(rel)` | `lic/slic/src/cpp/jrncmd.rs:64` | a receiver range other than *CURRENT is not taken. |
| `BLOCKED(rel)` | `lic/slic/src/cpp/rpg/mod.rs:352` | CRTPGM does not carry a module's statements. |
| `BLOCKED(rel)` | `lic/slic/src/cpp/savcmd.rs:1664` | SAVSAVFDTA's OUTPUT(*PRINT\|*OUTFILE) listing and |
| `BLOCKED(rel)` | `lic/slic/src/cpp/sqlinfcmd.rs:87` | the plan cache (`sql::exec::plan::cache`) keeps |
| `BLOCKED(rel)` | `lic/slic/src/cpp/sqlinfcmd.rs:98` | a package's statements are kept (`db::sqlpkg`) |
| `NEEDS-CODY` | `lic/slic/src/stgsvc/nfs/mod.rs:71` | IBM's, |
| `BLOCKED(rel)` | `lic/slic/src/stgsvc/sqlhost.rs:127` | the request holds the service until it answers |
| `BLOCKED(rel)` | `lic/slic/src/stgsvc/sqlhost.rs:314` | commands whose processing program runs in the |
| `BLOCKED(rel)` | `lic/slic/src/stgsvc/sqlhost.rs:479` | the job's own variables live in the job; the |
| `BLOCKED(rel)` | `lic/slic/src/stgsvc/sqlhost.rs:563` | the lookup holds the service until the resolver |
| `BLOCKED(rel)` | `lic/slic/src/svc/db/trgout.rs:180` | the other DSPFD outfile formats (QAFDBASI, QAFDMBR, |
| `NEEDS-CODY` | `lic/svctools/src/dst/partitions.rs:9` | the primary's own panel |
| `BLOCKED(rel)` | `licpgm/lpstore/src/savfile.rs:157` | the system's own directories exist already. |
| `BLOCKED(rel)` | `net/accessweb/src/routes/db/copy.rs:699` | a file on the system is read through the Files area's trait, not built at this level; refused with XWA0099. |
| `BLOCKED(rel)` | `net/accessweb/src/routes/db/mod.rs:104` | without QS00NAV installed the link has no page; the handoff's Columns page embedded in this product is not built. |
| `BLOCKED(rel)` | `net/drdar/src/session.rs:381` | the client sends no input values (SQLDTA) yet. |
| `BLOCKED(rel)` | `net/hostserver/src/database/catalog.rs:776` | distinct types (SQLUDTS) and routine parameters |
| `BLOCKED(rel)` | `net/hostserver/src/database/mod.rs:1130` | LOB locators of the engine's own result sets |
| `BLOCKED(rel)` | `net/httpd/src/serve/handle.rs:246` | CGI programs do not run; a ScriptAlias |
| `BLOCKED(rel)` | `rpg/qrpgrt/src/sql.rs:48` | the runtime has |
| `BLOCKED(rel)` | `rpg/sqlpre/src/stmt.rs:750` | a procedure's result sets (ALLOCATE CURSOR, |
| `BLOCKED(rel)` | `runtime/sqlcli/src/exec/conn.rs:79` | a connection to another relational database |
| `BLOCKED(rel)` | `runtime/sysapi/src/job.rs:532` | a job knows the shipped descriptions only; one made or |
| `BLOCKED(rel)` | `tools/conformance/rpg/known.txt:28` | in the database's buffer conversion |
| `BLOCKED(rel)` | `tools/conformance/rpg/known.txt:32` | SQL058 a SENSITIVE cursor is refused with SQL0243: QS/400 reads a cursor's rows when it opens ( |
| `BLOCKED(rel)` | `tools/conformance/rpg/known.txt:33` | SQL060 LOB file reference variables are refused with SQL0084: the runtime reads and writes no IFS files yet ( |
| `BLOCKED(rel)` | `tools/conformance/sql/known.txt:12` | integrity/AUDITCOL   # engine: IMPLICITLY HIDDEN refused (SQL0084, |
| `BLOCKED(rel)` | `tools/conformance/sql/known.txt:28` | ARRAY_EXISTS is 7.2+ (SQL0199 on 7.1); an array parameter of a function is SQ20441 there, SQL0084 here; UNNEST of a constructor is SQL0104 there, SQ20441 here |
| `BLOCKED(rel)` | `tools/conformance/sql/known.txt:31` | in alter) |
| `BLOCKED(rel)` | `tools/conformance/sql/known.txt:34` | in udt::typing::check) |
| `NEEDS-CODY` | `tools/pesign/src/keys/mod.rs:7` | the real one is made with `pesign |
| `BLOCKED(rel)` | `ui/uim/pnlgrp/QGWCDSKS.pnlgrp:8` | headings and decimal columns sit on a hand grid this renderer does not make (work2_panels::wrkdsksts); it needs a display file with a command line |
| `BLOCKED(rel)` | `ui/uim/pnlgrp/QGWCPOOL.pnlgrp:8` | the primary's display is a display file on a hand grid this renderer does not make; it needs a display file with a command line |
| `BLOCKED(rel)` | `ui/uim/pnlgrp/QGWCSSTS.pnlgrp:10` | the primary's display is a display file on a hand grid this renderer does not make (as WRKACTJOB's); it needs a display file with a command line |
| `BLOCKED(rel)` | `ui/uim/tests/object_panels.rs:380` | the golden runs the message text through column 80 into column 1 of the next row; output fields do not wrap rows yet"] |
| `BLOCKED(rel)` | `ui/uim/tests/system2_panels.rs:273` | the golden runs the 32-character signature past column 80 into column 1 of row 18; output fields do not wrap rows yet"] |
| `NEEDS-CODY` | `ui/uim/tests/system2_panels.rs:754` | no golden; the primary has no QAUDJRN (DSPSECAUD: journal exists NO), see docs/SEC_OPEN_QUESTIONS.md"] |
| `BLOCKED(rel)` | `work/qsjob/src/command/exits.rs:131` | a program running past the wait time it asked for |
| `BLOCKED(rel)` | `work/qsjob/src/command/ftp.rs:344` | no trigger programs called here; a file with |
| `BLOCKED(rel)` | `work/qsjob/src/command/rcvjrne.rs:89` | RCVJRNE passes *TYPE1 entries one at a time from |
| `BLOCKED(rel)` | `work/qsjob/src/command/telnet.rs:50` | the TELNET client does not make TLS sessions. |
| `BLOCKED(rel)` | `work/qsjob/src/command/telnet.rs:163` | a server that will not run 5250 (VT or 3270 |
| `BLOCKED(rel)` | `work/qsjob/src/console/drda.rs:151` | the requester's query brings no column descriptions |
| `BLOCKED(rel)` | `work/qsjob/src/console/jobcmd.rs:151` | another job's call stack needs a request the |
| `BLOCKED(rel)` | `work/qsjob/src/devtools/isql/prompts.rs:347` | F6, F10, F11, F14, F15, F19, F20 and F24 of |
| `BLOCKED(rel)` | `work/qsjob/src/devtools/isql/prompts.rs:458` | the WHERE display after this one is not |
| `BLOCKED(rel)` | `work/qsjob/src/qapsql/mod.rs:355` | QAP's Executed has no output values (a later |
| `BLOCKED(rel)` | `work/qsjob/src/qapsql/mod.rs:422` | `Executed` names one cursor, so a second set is not |
| `BLOCKED(rel)` | `work/qsjob/src/work/httpafw/cmd.rs:337` | the bridge waits 120 seconds for the job's thread; a longer command |
| `BLOCKED(rel)` | `work/qsjob/src/work/httpafw/msgs.rs:40` | the sender is the qualified job; a message a user typed here names |
| `BLOCKED(rel)` | `work/qsjob/src/work/httpafw/print.rs:192` | no command or service request a web job may use puts a spooled file at the top of a queue (QAP's move does, on its own connection job only); refused with XWA0099. |
| `BLOCKED(rel)` | `work/qsjob/src/work/httpafw/s5250.rs:323` | a session to another system goes through that system's own Access for Web; refused with QAD1002. |
| `BLOCKED(rel)` | `work/qsjob/src/work/httpafw/s5250.rs:727` | the display shows and types CCSID 37; the session's CCSID goes to the server as CODEPAGE only. |
| `BLOCKED(rel)` | `work/qsjob/src/work/httpafw/system.rs:186` | the password approval program (QPWDVLDPGM) is not called from a web job; refused with CPF22D3 while one is set. |
| `BLOCKED(rel)` | `work/qsjob/src/work/httpsvr.rs:67` | validation lists (*VLDL) do not exist; a PasswdFile |
| `BLOCKED(rel)` | `work/qsjob/src/work/qap/debug/own.rs:79` | stops are breakpoints, steps and entry points only; the |
| `NEEDS-CODY` | `work/wm/src/obj/shipped.rs:281` | DSPCLS of the primary's). |
<!-- inventory:end -->

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
