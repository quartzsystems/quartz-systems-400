# NFS: the server and the client

Crates `net/xdrgen` (the XDR description compiler), `net/nfs` (ONC RPC,
the binder, MOUNT, NFS versions 3, 4.0 and 4.1, NLM, NSM, the export
table, the server and the client) and `tools/nfstest` (the test client).
`EXPORTFS` publishes IFS directories to NFS clients and `MOUNT
TYPE(*NFS)` mounts a remote export into the IFS, with IBM's commands,
daemon jobs and `/etc/exports`. Target release V1R2M0.

This document is the contract between the parts. Sections marked
**(design)** are decided and not yet built; the acceptance table at the
end says what is built and proven.

## 1. What the oracles settled

`tools/conformance/oracle/nfs/README.md` has the captures. In short:

- NFS is installed on both oracles and runs on neither; starting it is a
  configuration change this project does not make there. The commands,
  prompts, menus, message IDs, objects, daemon programs, `/etc` files and
  the two APIs come from the oracles and are followed exactly. The wire
  is built from the RFCs (5531, 4506, 1833, 1813, 7530, 7531, 8881,
  5662, and the Open Group's XNFS for NLM and NSM); every wire behaviour
  against a client or server that is not QS/400 is `NEEDS-CODY`
  (section 12).
- Things the handoff guessed that IBM does not have, and QS/400
  therefore does not add: `STRNFSSVR RETRY()`, `CHGNFSA`, `WRKNFSEXP`,
  `/etc/rpcbind`, `/etc/fstab`, `QIBM_QSTRTNFS`, a `QSYS2` NFS service.
  IBM has, and QS/400 adds, `STRNFSSVR RTVRPCREG()` and `SERVER(*GSS)`
  (refused, section 10), `RPCBIND`, `ENDRPCBIND`, `/etc/RPCBTAB`,
  `/etc/STATD`, `/etc/netconfig`, `/etc/vfs`, and the APIs `QZNFRTVE` and
  `QZNFNFSO`.
- `QSYS2.NFS_EXPORT_INFO` and `QSYS2.MOUNTED_FS_INFO` are QS/400's own
  (a documented difference): no release of IBM i has an SQL view of
  either.
- IBM starts NFS from the startup program or by hand. QS/400 does the
  same: nothing starts NFS at IPL unless `QSTRUP` is changed; exports
  made by `EXPORTFS` without `-A` last until the next IPL, as do mounts.

## 2. Pieces and where they live **(design)**

| Piece | Where | Holds |
|---|---|---|
| XDR compiler | `net/xdrgen` | the XDR language (RFC 4506 section 6 plus `program`) parsed into a model, and Rust types with `encode`/`decode` written from it. A build dependency only |
| Descriptions | `net/nfs/x/*.x` | `rpc.x`, `rpcb.x`, `mount.x`, `nfs3.x`, `nlm.x`, `nsm.x`, `nfs4.x` (4.0 and 4.1 in one description; 4.1 is a superset). Written in tree after the RFCs' interface definitions |
| XDR runtime | `net/nfs/src/xdr/` | `Encoder`, `Decoder` with a byte budget and a depth limit, the traits the generated code implements |
| RPC | `net/nfs/src/rpc/` | call and reply messages, record marking, `AUTH_NONE`, `AUTH_SYS`, the refusal of `RPCSEC_GSS`, a client transport (TCP and UDP, XIDs, retransmission) and a server loop over a `Program` trait, the duplicate request cache |
| Binder | `net/nfs/src/rpcb/` | portmap version 2 and rpcbind versions 3 and 4, the registration table, `/etc/RPCBTAB` |
| Protocols | `net/nfs/src/mount/`, `v3/`, `v4/`, `nlm/`, `nsm/` | each protocol's server procedures over the `Vfs` trait and its client calls |
| Export table | `net/nfs/src/exports/` | `/etc/exports`, `/etc/netgroup`, the option lists of `EXPORTFS` and `MOUNT`, host matching |
| Server | `net/nfs/src/server/` | the `Vfs` trait (what the protocols ask of a file system), file handles, the mapping of credentials, the v4 state tables. **No job, store or service code** |
| Client | `net/nfs/src/client/` | a mounted export as an `ifs::remote::RemoteFs`: version and transport negotiation, `soft`/`hard`/`intr`, reconnection, the NLM or v4 lock client |
| Registry | `net/nfs/src/registry/` | the v4 identity mapping (`user@domain` and numeric strings to UID/GID and back) |
| IFS groundwork | `ifs/ifs` | per-request identity for the daemons, lock owners within a job, file handles that survive an IPL, `ifs::remote`: a mount whose file system is remote (section 7) |
| Storage service side | `lic/slic/src/stgsvc/nfs/` | the export table and the daemon registry, the mounts' remote file systems and their worker threads, the commands' data |
| Jobs | `work/qsjob/src/work/nfs/` | `QNFSRPCD`, `QNFSNFSD`, `QNFSMNTD`, `QNFSNLMD`, `QNFSNSMD`, `QNFSBIOD`, `QNFSRGYD`, implementing `Vfs` over the job's IFS client |
| Commands | `command/sys-cpps/cmds/`, `lic/slic/src/cpp/nfscmd.rs`, `work/qsjob/src/command/nfs.rs` | section 8 |
| APIs | `runtime/sysapi/src/nfs.rs` | `QZNFRTVE`, `QZNFNFSO` |
| SQL | `database/sql/src/exec/catalog/services/nfs.rs` | `QSYS2.NFS_EXPORT_INFO`, `QSYS2.MOUNTED_FS_INFO`; the NFS ports in `NETSTAT_*` |
| Navigator, QAP | `net/navigator/src/routes/nfs/`, `work/qap-proto/src/ops/nfs.rs` | NFS Exports and Mounts pages; the `Nfs` service in the next draft version |
| Test client | `tools/nfstest` | scenarios against a server over loopback or a forwarded port |

Dependencies point one way: `xdrgen` knows nothing; `nfs` depends on
`ifs` (for `IfsClient`-shaped types and `ifs::remote`), `ccsid` and
`sockets`' resolver types only; `slic` and `qsjob` depend on `nfs`.

## 3. XDR and RPC **(design)**

- Every protocol type is generated from `net/nfs/x/` by `net/xdrgen` in
  `net/nfs/build.rs`. No call's arguments or results are encoded by
  hand; hand-written code exists only in `src/xdr/` (the primitives) and
  where a protocol leaves the description (`nfs4.x`'s `bitmap4` and
  `fattr4` attribute values, which are opaque in XDR and get their own
  table-driven codec in `v4/attr/`).
- Decoding never trusts a length: every variable-length item is checked
  against the description's bound and the bytes left in the record; a
  record is at most 1 MiB plus the largest header; recursion (linked
  lists such as `entry3`, `exportnode`) is decoded iteratively or under
  a depth limit.
- Record marking: fragments are joined up to the record limit; a larger
  record closes the connection. UDP datagrams carry one message.
- `AUTH_NONE` and `AUTH_SYS` are accepted. Any other flavor, with
  `RPCSEC_GSS` (6) named, is `MSG_DENIED`/`AUTH_ERROR` with
  `AUTH_BADCRED`. NFSv4 `SECINFO` and `SECINFO_NO_NAME` answer `AUTH_SYS`
  only.
- The duplicate request cache keys on (client address, port, XID,
  program, version, procedure) and keeps the answers of non-idempotent
  NFSv3 procedures and of NLM for 60 seconds, bounded in entries.
- Programs: `100000` (the binder, versions 2, 3, 4), `100003` (NFS,
  versions 3 and 4), `100005` (MOUNT, versions 1 and 3; version 1 only
  for `showmount`'s `DUMP` and `EXPORT`), `100021` (NLM, versions 1, 3
  and 4: 4 fully, 1 and 3 as the 32-bit forms of the same procedures),
  `100024` (NSM, version 1). A version outside a program's list is
  `PROG_MISMATCH` with the list's bounds; NFS version 2 is refused this
  way (IBM's V7R1M0 still registers it).

## 4. The daemons **(design)**

As on the oracle (`tools/conformance/oracle/net/primary/dspsbsd-qsyswrk.txt`),
`QSYSWRK` gets routing entries 2120 to 2127 and `QSYS` the job
descriptions and classes `QP0LNFSD` and `QP0LRPCD` (and the class
`QP0LBIOD`):

| Seq | Program | Compare value | Job | `SERVER` | Ports | What it runs |
|---|---|---|---|---|---|---|
| 2123 | `QZNFRPCD` | `QNFSRPCD` | `QNFSRPCD` | `*RPC` | 111 tcp, udp | the binder |
| 2121 | `QZNFBIOD` | `QNFSBIOD` | `QNFSBIOD` (`NBRBIO`) | `*BIO` | none | the client's read-ahead and write-behind (section 7) |
| 2127 | `QZNFRGYD` | `QNFSRGYD` | `QNFSRGYD` | `*RGY` | none | the v4 identity mapping for the server jobs and the client |
| 2120 | `QZNFNFSD` | `QNFSNFSD` | `QNFSNFSD` (`NBRSVR`) | `*SVR` | 2049 tcp, udp | NFS 3, 4.0, 4.1 |
| 2122 | `QZNFMNTD` | `QNFSMNTD` | `QNFSMNTD` | `*MNT` | any, registered | MOUNT |
| 2125 | `QZNFNSMD` | `QNFSNSMD` | `QNFSNSMD` | `*NSM` | any, registered | the status monitor, `/etc/STATD` |
| 2124 | `QZNFNLMD` | `QNFSNLMD` | `QNFSNLMD` | `*NLM` | any, registered | the lock manager |
| 2126 | `QZNFGSSD` | `QNFSGSSD` | none | `*GSS` | | refused (section 10) |

All run as `QSECOFR` (NEEDS-CODY: the user of IBM's daemon jobs;
section 12) and are submitted by the storage service with the routing
data of the table, as `QZLSSERVER` is. `STRNFSSVR SERVER(*ALL)` starts
them in IBM's order: `*RPC`, `*BIO`, `*RGY`, `*SVR`, `*MNT`, `*NSM`,
`*NLM`; `ENDNFSSVR` ends them in the reverse order. Each start waits
`STRJOBTIMO` for the daemon to report ready (it registers with the
binder first) and sends `CPIA1BB`; a daemon already running is
`CPDA1BA` (`CPDA1BD` for one of several). `*SVR`, `*MNT`, `*NSM` and
`*NLM` need the binder and start it if it is not running, as IBM's do.
`RPCBIND` and `ENDRPCBIND` are the binder alone.

**Several `QNFSNFSD` jobs.** The first job owns the listeners. It
serves UDP itself and gives each TCP connection to the job chosen by
the client's address (`sockets::relay::give`), so one client's NFSv4
state is in one job. What several clients share is in the storage
service: share reservations (the opens' share modes), delegations (the
lease table NetServer uses: a read delegation is a read lease, and a
break is `CB_RECALL`) and byte-range locks. Each job is multithreaded:
one thread per connection, each with its own link to the service.

**Restart and grace.** The storage service keeps the server's boot
verifier and, in `/QIBM/UserData/OS400/NFS/`, the stable-storage records
RFC 7530 section 9.6.3 asks for (client IDs with confirmed state). A
`QNFSNFSD` start after an end, a crash or an IPL begins a 90-second
grace period in which only reclaims are accepted (`NFS4ERR_GRACE`,
`NLM4_DENIED_GRACE_PERIOD`); NSM's state number in `/etc/STATD` goes up
by two and `QNFSNSMD` sends `SM_NOTIFY` to the hosts it monitored.

## 5. The server over the IFS **(design)**

`nfs::server::Vfs` is what the protocols need; the job implements it
over `IfsClient` with these additions to the IFS (built first):

- **Identity per request.** A daemon's request names the caller:
  `IfsClient::as_user(uid, gid, &gids)` returns a client whose calls run
  under that identity. The service honours it only for jobs named
  `QNFSNFSD`, `QNFSMNTD` and `QNFSNLMD`. The UID maps to the user profile
  with that UID (`CHGUSRPRF UID()`); authority is that profile's, with
  its group profiles (the wire's GID list is not trusted for authority,
  as on IBM i). UID 0 is `QSECOFR` only for a host in the export's
  `ROOT=` list; otherwise, and for a UID no profile has, the caller is
  the export's anonymous user (`ANON=`, default `QNFSANON`); `ANON=-1`
  refuses instead (`NFS3ERR_ACCES`, `NFS4ERR_ACCESS`). The service keeps
  principals by UID and drops them when a profile changes.
- **File handles** are 32 bytes: a format byte, the export's ID (4), the
  IFS `FileId` (file system 4, object 8, generation 4), the file ID of
  the export's root (8) and a checksum. Nothing in a handle depends on
  the IPL: the object number is the backend's, generations are stored
  with the object and are drawn from a counter seeded so that a reused
  object number never repeats a generation, and the service resolves a
  `FileId` to an object without walking the tree (`open_by_handle_at` on
  the xfs; `/QSYS.LIB` by the hashes of library, object and member). A
  handle whose object is gone, or whose generation differs, is
  `NFS3ERR_STALE`/`NFS4ERR_STALE`; one outside its export
  `NFS3ERR_BADHANDLE`. A handle is checked against the export table on
  every call, so `EXPORTFS -U` takes effect at once.
- **Lock owners.** `IfsClient::lock_range` gains an owner within the
  job, so NLM's (host, pid/owner handle) and NFSv4's lock owners hold
  POSIX locks that conflict with each other and with local jobs.
  Blocking locks wait in the service (`NLM_BLOCKED` then `NLM_GRANTED`
  callbacks; NFSv4 answers `NFS4ERR_DENIED` and the client polls).
- **Attributes.** Mode bits, owner and group from the object's
  authorities as `stat` shows them; `SETATTR` of mode maps to the data
  authorities the way `chmod` does. ACLs beyond mode bits are refused
  (section 10). NFSv4 `owner`/`owner_group` are `profile@domain` through
  the registry, the domain from `QZNFNFSO` (default: the TCP/IP domain
  name).
- **Exports.** `RO`, `RW=`, `ACCESS=`, `ROOT=`, `ANON=`, `NOSUID`,
  `SEC=sys` (others refused), `VERS=3:4`, `PUBLIC`, `ROOT` (the v4
  pseudo-root), `NFSROOT=`; per-host `HOSTOPT` (data CCSID, path name
  CCSID, synchronous writes). Host lists hold names, addresses, networks
  (`@net/len`) and netgroups. A host not allowed sees `MNT3ERR_ACCES`,
  `NFS3ERR_ACCES` or `NFS4ERR_ACCESS`.
- **`/QSYS.LIB`** exports show libraries, objects and members as
  directories and files (`.FILE` a directory of `.MBR` files, save files
  and user spaces as files). With a data CCSID other than `*BINARY` in
  `HOSTOPT`, members and stream files tagged with a text CCSID are
  converted on read and write (single-byte CCSIDs; a file size is the
  converted size). Path names are converted between the path name CCSID
  and the IFS's names. `/QOPT` and `/QDLS` cannot be exported
  (`CPDA1C5`); nor can `/QNTC`, `/QFileSvr.400` or an NFS mount.
- **NFSv4.** The pseudo-file system joins the exports under one root
  (read-only directories made of the exports' path components).
  Compound operations of 4.0 and 4.1; stateful opens with share
  reservations; byte-range locks; read delegations (write delegations
  are not offered); `SETCLIENTID` and callbacks for 4.0;
  `EXCHANGE_ID`, sessions, slots with exactly-once replies, the
  backchannel, `RECLAIM_COMPLETE`, `SEQUENCE` for 4.1. Referrals are
  refused (`fs_locations` is not a supported attribute), pNFS is refused
  (`EXCHGID4_FLAG_USE_NON_PNFS`, `LAYOUT*` and `GETDEVICE*` answer
  `NFS4ERR_NOTSUPP`).

## 6. `/etc` files **(design)**

| File | CCSID | Who writes it | Format |
|---|---|---|---|
| `/etc/exports` | the job's when made | the user (`EDTF`), read by `EXPORTFS -A` and `-I`'s absence | IBM's: one export a line, `path -option,option=value:value,...` then `HOSTOPT` in IBM's form; `#` comments; `\` continues a line. Section 6.1 |
| `/etc/netgroup` | the same | the user | `name (host,user,domain) ... othergroup ...`; only the host field is used |
| `/etc/RPCBTAB` | 37 | `QNFSRPCD` | 357-byte records `program version netid uaddr owner`, a new line, zeros. Read back by `RTVRPCREG(*YES)` |
| `/etc/STATD` | 37 | `QNFSNSMD` | the state number as text and a new line |
| `/etc/netconfig` | 819 | shipped | the transports `tcp`, `tcp_ord`, `udp` (visible, `inet`). Quartz-written comments |
| `/etc/vfs` | 1208 | shipped | the file system names and numbers of the oracle's, Quartz-written comments, `%defaultvfs udfs nfs3` |

The current export table is the storage service's (in memory, as IBM's
is kernel state); `/etc/exports` is only read by `EXPORTFS`. The
service's own records are under `/QIBM/UserData/OS400/NFS/` (the v4
stable storage, the monitored hosts of NSM, the NFS options of
`QZNFNFSO`): a format of this release, in `tools/fmtcheck`.

### 6.1 `/etc/exports` (fill: the commands' agent)

The syntax QS/400 reads and the option list of `EXPORTFS -O`, with
every option's meaning, default and message when wrong.

## 7. The client: an NFS mount in the IFS **(design)**

The storage service is one thread over one store and must never wait
for a network. A remote mount therefore works on a cache and never
blocks the serve loop:

- `ifs::remote::RemoteFs` is the trait a remote file system implements
  (blocking calls: `lookup`, `getattr`, `setattr`, `access`, `read`,
  `write`, `commit`, `create`, `mkdir`, `symlink`, `mknod`, `remove`,
  `rmdir`, `rename`, `link`, `readdir`, `readlink`, `statfs`,
  `pathconf`, `open`, `close`, `lock`). `nfs::client::Mount` is the
  implementation; tests use one in memory with delays and failures.
- `FsKind::Remote` is a mount whose nodes are remote handles. An IFS
  operation that reaches one answers from the mount's cache
  (attributes, names, directory pages, data blocks); on a miss, or for
  a change, it leaves a request in the mount's queue and a `Deferral`,
  exactly as an operation that waits for a lease break does. Worker
  threads of the service (one pool per mount, the "kernel" of the
  client) run the queued calls; when one finishes the parked request
  runs again and finds its answer. A change is queued once, under the
  request's token, and its second run takes the result.
- The attribute cache follows `acregmin`/`acregmax`/`acdirmin`/
  `acdirmax` (`noac` turns it off); data blocks of `rsize` are cached
  and validated by the file's change attribute or modification time;
  writes are cached to `wsize` blocks and sent `UNSTABLE` then
  `COMMIT`ted on `fsync` and close (`FILE_SYNC` with the export's
  synchronous option or without block I/O daemons). `QNFSBIOD` jobs,
  when started, take the read-ahead and write-behind work from the
  same queue; without them the service's threads do it at the request.
- `hard`: a call is retried for ever, `timeo` tenths of a second
  doubling to 60 seconds, a message to `QSYSOPR` when the server stops
  answering and when it answers again; the request stays parked and no
  other job is held. `intr`: ending the job, or System Request 2,
  takes the request back with `EINTR`. `soft`: after `retrans`
  retransmissions the call fails with `ETIMEDOUT`. `retry` is the
  number of times the mount itself is tried. `bg` is refused.
- `vers=3` (the default, as `/etc/vfs` says) or `vers=4` (`4.0`, `4.1`);
  `proto=tcp` or `udp` (v4 is TCP only); `rsize`, `wsize` up to 1 MiB;
  `sec=sys`; `rw`/`ro`; `suid`/`nosuid`; `port=`, `mountport=`.
  `CCSID`/`CODEPAGE` convert data and path names as the server's
  `HOSTOPT` does.
- The caller's credentials are `AUTH_SYS` with the job's user profile's
  UID, GID and group profiles' GIDs.
- Locks: `fcntl` byte-range locks on a remote file go to the server (NLM
  for version 3, with `QNFSNSMD` and `QNFSNLMD` running, else `ENOLCK`;
  `LOCK` for version 4) and are reclaimed after the server restarts.
- `UNMOUNT TYPE(*NFS)` refuses while anything is open or current in the
  mount; it flushes, tells the server's MOUNT daemon and ends the pool.
- Mounts last until the next IPL, as IBM's do. To have them back, put
  `MOUNT` in the startup program (`QSTRUP`).

## 8. Commands, APIs, SQL **(design)**

Definitions follow `tools/conformance/oracle/nfs/cmdd71/`; prompts are
compared with `tools/conformance/golden/nfs/`.

| Command | Runs in | What |
|---|---|---|
| `STRNFSSVR`, `ENDNFSSVR` | job | section 4. `*IOSYSCFG` (`CPFA1B8`) |
| `RPCBIND`, `ENDRPCBIND` | job | the binder alone |
| `EXPORTFS`, `CHGNFSEXP` | service | `OPTIONS` holds IBM's flags: `-A` every entry of `/etc/exports`, `-I` ignore `/etc/exports`' options for `DIR`, `-U` unexport, `-F` (the file is not read), `-O` the option list, `-E` escape message on failure, `-V` verbose; the messages of section 1's list |
| `MOUNT`, `ADDMFS` | service | `TYPE(*NFS)` section 7; `TYPE(*UDFS)` as before. `*NETWORK` is no longer a value (it is not one on V7R1M0) |
| `UNMOUNT`, `RMVMFS` | service | `TYPE(*NFS *UDFS *ALL)`, `MNTOVRDIR(*ALL)` |
| `DSPMFSINF`, `STATFS` | job (the display), service (`*PRINT`) | the display of `golden/nfs/dspmfsinf-*`, for every file system type |
| `RLSIFSLCK` | service | releases the byte-range locks a remote client holds through the server (`RMTLOCNAME`), or those on an object (`OBJ`): the NFS server's, the local ones, or the whole file system's |
| `CVTRPCSRC`, `RPCGEN` | | not shipped: no C compiler takes its output (section 10) |

`QZNFRTVE` (`EXPE0100`, `EXPE0200`) lists the export table; `QZNFNFSO`
retrieves and sets the security flavors (only `sys`), the NFSv4 domain
and the options (server threads, client handle cache, UTF-8
validation). `QSYS2.NFS_EXPORT_INFO` and `QSYS2.MOUNTED_FS_INFO` are
QS/400's views of the export table and the mount table.

## 9. Security mapping **(design)**

| On the wire | On QS/400 |
|---|---|
| `AUTH_SYS` UID | the user profile with that UID; none: the anonymous user |
| UID 0 | `QSECOFR` for `ROOT=` hosts; else the anonymous user (root squash is the default, as IBM's) |
| GID list | not used for authority; the profile's group profiles are |
| `AUTH_NONE` | the anonymous user |
| the anonymous user | `ANON=uid` (default the UID of `QNFSANON`); `-1` refuses |
| mode bits | the data authorities of owner, primary group and `*PUBLIC`, as `stat` and `chmod` map them |
| a set-user-ID bit | cleared on files made through an export with `NOSUID` |
| ports | calls from ports of 1024 and up are accepted (IBM's default); the client sends from a reserved port when it can |

Auditing follows the caller's profile: the `QAUDJRN` entries are those
the same change makes locally.

## 10. Refused

Nothing is accepted and ignored. What is not done is refused as below.

| What | How it is refused |
|---|---|
| NFS version 2 | `PROG_MISMATCH` 3 to 4; `MOUNT vers=2`: `CPFA1BB` |
| `RPCSEC_GSS`, Kerberos, `SEC=krb5*` in an export or a mount | `AUTH_ERROR`; `CPFA1BB`; `STRNFSSVR SERVER(*GSS)` and `ENDNFSSVR SERVER(*GSS)`: `CPFA1BC`/`CPFA1BE` with the reason in the job log (`*ALL` leaves it out) |
| pNFS | `NFS4ERR_NOTSUPP`; `EXCHGID4_FLAG_USE_NON_PNFS` |
| Referrals, `fs_locations`, migration | the attribute is not supported |
| ACLs beyond mode bits (`ACL`, `dacl`, `sacl`, NFSv3 `NFSACL` program 100227) | the attribute is not supported; `PROG_UNAVAIL` |
| Write delegations, directory delegations | never offered; `GET_DIR_DELEGATION` `NFS4ERR_NOTSUPP` |
| Named attributes (`OPENATTR`) | `NFS4ERR_NOTSUPP` |
| NFSv4.2 | minor version 2: `NFS4ERR_MINOR_VERS_MISMATCH` |
| Exporting `/QOPT`, `/QDLS`, `/QNTC`, `/QFileSvr.400`, an NFS mount | `CPDA1C5` |
| `MOUNT OPTIONS('bg')`, an option not in section 7 | `CPFA1B0` |
| `CVTRPCSRC`, `RPCGEN` | not shipped: the command is not found, as any command QS/400 lacks |
| Double-byte data CCSIDs in `HOSTOPT`/`CCSID` | `CPDA1CA` |

## 11. Tests **(design)**

- `cargo test -p xdrgen -p nfs`: the compiler; every generated type's
  round trip; the RFCs' examples; the server over a `Vfs` in memory and
  the client against it over loopback (v3, 4.0, 4.1; UDP and TCP; locks;
  grace and reclaim; `soft` and `hard` against a server that stops).
- `cargo test -p ifs`: remote mounts over a `RemoteFs` in memory; the
  daemons' identity; lock owners; handles.
- `tools/nfstest`: the scenarios a client runs against a server.
- QEMU (`build test`): two QS/400 guests, one exporting and one
  mounting, v3 and v4; locks across the guests; the server ended and
  started with locks reclaimed; a `/QSYS.LIB` export with text
  conversion; `nfstest` from the host against the guest's export; the
  guest mounting a Linux `nfsd` export where the host has one (skipped
  otherwise); 1 GB read and written both ways.
- Fuzz: `rpc`, `rpcb`, `mount`, `nlm`, `nfs3`, `nfs4` (the compound),
  `exports`.

## 12. NEEDS-CODY

Each line says what would settle it.

- The user and the job description of IBM's daemon jobs, the text of
  `WRKACTJOB`'s function column for them, and the order `STRNFSSVR
  SERVER(*ALL)` starts them in: on the primary, `STRNFSSVR SERVER(*ALL)`,
  `WRKACTJOB SBS(QSYSWRK) JOB(QNFS*)`, `DSPJOB` of each, `ENDNFSSVR
  SERVER(*ALL)`.
- `/etc/exports` as IBM writes and reads it: on the primary, `EXPORTFS
  OPTIONS('-I -O ''RO,ANON=0''') DIR('/home/QSTEST')
  HOSTOPT((host 850 850 *ASYNC))` after `STRNFSSVR`, then `EXPORTFS
  OPTIONS('-A')` with an `/etc/exports` holding the forms of section
  6.1, and `QZNFRTVE` `EXPE0100` of the result.
- `DSPMFSINF` of an NFS mount (the fields below "File system type"):
  on the primary, `MOUNT TYPE(*NFS)` of any export, `DSPMFSINF`.
- A Linux and a macOS client mounting a QS/400 export (v3, 4.0, 4.1),
  and QS/400 mounting a Linux export; an export to the Dell host.
