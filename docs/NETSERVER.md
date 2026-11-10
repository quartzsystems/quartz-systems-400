# NetServer (`*NETSVR`): SMB file and print serving

Crates `net/netserver` (the SMB2/3 server), `net/netsvrcfg` (the
attributes, the shares, their files and the `QZLS*` formats) and
`tools/smbtest` (the test client). A Windows, macOS or Linux client maps a
share, browses the IFS, opens and saves files and prints to an output
queue the way it does against IBM i NetServer. Target release V1R2M0.

This document is the contract between the parts. Sections marked
**(design)** are decided; the acceptance table at the end says what is
built and proven.

## 1. What the oracles settled

`tools/conformance/oracle/netserver/README.md` has the captures. In short:

- The primary (V7R1M0) runs NetServer but speaks **SMB1 only**: SMB2
  negotiates are dropped. pub400 (V7R5M0) does not answer on 445 from here.
  No oracle shows SMB2 or SMB3 on the wire, so the protocol is built from
  [MS-SMB2], [MS-NLMP], [MS-SPNG], [MS-SRVS] and [MS-WKST], and every
  wire behaviour is `NEEDS-CODY` against real clients (section 11).
- The jobs, the APIs, the job descriptions, the subsystem entries, the
  SQL view, the message IDs and the configuration files' location come
  from the oracles and are followed exactly.
- Things the handoff guessed that IBM does not have, and QS/400 therefore
  does not add: `QZLSOPEN`, `QSYS2.ADD_SERVER_SHARE` /
  `CHANGE_SERVER_SHARE` / `REMOVE_SERVER_SHARE`, `QSYS2.SERVER_SESSION_INFO`,
  `QIBM_QZLS_*` exit points, NetServer function usage IDs, a `*NETSVR`
  object type. `WRKNETSVR` does not exist on IBM i either.

## 2. Pieces and where they live (design)

| Piece | Where | Holds |
|---|---|---|
| SMB2/3 protocol | `net/netserver` | NetBIOS session framing, the SMB2 header and every command, negotiate contexts, signing, encryption, credits, compounding, sessions, trees, opens, leases and oplocks, durable and resilient handles, change notify; NTLMSSP and SPNEGO; DCE/RPC over named pipes for `srvsvc` and `wkssvc`; the NetBIOS name service. **No job, store or IFS code**: everything outside goes through the traits of `netserver::backend` |
| Configuration | `net/netsvrcfg` | the server attributes (current and pending), file and print shares, validation, the `QAZLSCFG` and `QAZLSSHR` codecs, the `ZLSL*` and `ZLSS*` formats of the APIs, the reason codes |
| Test client | `tools/smbtest` | an SMB2/3 client written separately from the server (it does not depend on `net/netserver`, so one codec bug cannot hide in both) and its scenarios |
| Profiles | `security/sec` | the NT hash password form, the `QPWDLVL` rules for it, the NetServer-disabled state, the guest profile check |
| Storage service side | `lic/slic/src/stgsvc/netsvr.rs`, `lic/slic/src/cpp/netsvr.rs` | the configuration files over the IFS, the session table, the API and SQL data |
| Jobs | `work/qsjob/src/work/netsvr/` | `QZLSSERVER` (the listener) and `QZLSFILE` (one client connection), implementing the backend traits over the job's IFS client, the security calls and the spooler |
| APIs | `runtime/sysapi/src/netsvr.rs` | `QZLSADFS`, `QZLSADPS`, `QZLSCHFS`, `QZLSCHPS`, `QZLSCHSG`, `QZLSCHSI`, `QZLSCHSN`, `QZLSENDS`, `QZLSENSS`, `QZLSLSTI`, `QZLSOLST`, `QZLSRMS`, `QZLSSTRS` |
| SQL | `database/sql/src/exec/catalog/services/` | `QSYS2.SERVER_SHARE_INFO` (the 20 columns of the 7.5 view), `USER_INFO.NETSERVER_DISABLED` filled |
| Navigator | `net/navigator/src/routes/netsvr/` | File Shares, Print Shares, Sessions, Properties |
| QAP | `work/qap-proto` | the NetServer operations in the next draft version |

## 3. Jobs and the subsystem (design)

As on the oracle (`dspsbsd-qserver.txt`, `prestart-jobs.txt`):

- `QZLSSERVER`: user `QPGMR`, subsystem `QSERVER`, job description
  `QSYS/QZLSSERVER`, started by routing entry 800 (compare value
  `QZLSSTART`, class `QPWFSERVER`). It listens on TCP 445, and on 139 and
  UDP 137/138 when NetBIOS is allowed, then hands each accepted connection
  through the descriptor relay (`net/sockets` `pass`, the way `QRWTLSTN`
  hands DDM connections to `QRWTSRVR`) to a `QZLSFILE` prestart job.
- `QZLSFILE`: prestart, user `QUSER`, job description `QSYS/QZLSPJ`,
  `STRJOBS(*YES) INLJOBS(1) THRESHOLD(1) ADLJOBS(5) MAXJOBS(*NOMAX)
  MAXUSE(200)`, class `QPWFSERVER`. One job serves one client connection;
  after `SESSION_SETUP` it runs as the signed-on profile (the host
  servers' swap), so every IFS call is checked and audited as that user.
- `QZLSFILET`: the entry is shipped (`STRJOBS(*NO)`, `ADLJOBS(0)`,
  `MAXUSE(1)`) as on V7R5M0; QS/400 does not use it.
- `STRTCPSVR SERVER(*NETSVR)` and `QZLSSTRS` start `QZLSSERVER`;
  `ENDTCPSVR SERVER(*NETSVR)` and `QZLSENDS` end it, and every `QZLSFILE`
  closes its sessions cleanly (an SMB2 `LOGOFF`-free disconnect after
  in-flight requests finish). The autostart attribute is the server
  table's (`*NETSVR` row of `net/servers/src/registry/table.rs`).
  `QZLSSTRS` with reset `*YES` applies pending attributes first.
- `WRKACTJOB` shows them as IBM does: `QZLSSERVER` type `BCH`,
  `QZLSFILE` type `PJ`.

## 4. Configuration (design)

### Persistence

As on IBM i, two stream files under `/QIBM/UserData/OS400/NetServer/`:
`QAZLSCFG` (server attributes) and `QAZLSSHR` (shares), owned by `QSYS`,
`*PUBLIC *EXCLUDE`, CCSID tag 37. They are saved and restored with `SAV`
and `RST` of `/QIBM/UserData` (`GO SAVE` 21 and 23), as IBM's are; not by
`SAVSECDTA` or `SAVCFG`. The content is QS/400's own format (IBM's is
undocumented), specified in section 4.4, a frozen format from its first
release, registered in `tools/fmtcheck/formats.toml`. Only the storage
service writes them.

### Server attributes

Each attribute has a current value (in effect) and a pending value
(taken at the next start with reset, or the next IPL), as `QZLSCHSI`
and `ZLSL0101` define:

| Attribute | Default | Notes |
|---|---|---|
| Server name | the system name (`QSYS2.SYSTEM_STATUS` `HOST_NAME`'s first 15 characters, upper case) | the primary shows the plain system name; `QZLSCHSN` changes it |
| Domain (workgroup) | the server name's workgroup default as IBM sets it | `QZLSCHSN` |
| Text | `QS/400` | |
| Allow NetBIOS | `*NO` | 139, 137, 138 |
| WINS | none | WINS server addresses and scope are refused (`CPFB68B`-style reason, section 10) |
| Guest profile | none | `QZLSCHSG`; off by default |
| Idle time-out | IBM's default | sessions with no open files end after it |
| CCSID | 0 (the job's) | for file-name translation of `QSYS.LIB` text |
| Message authentication (signing) | optional | required, optional, disabled |
| Encryption | not required | required per server or per share |
| Opportunistic locking | `*YES` | leases and oplocks |
| SMB2 minimum dialect | 2.0.2 | 2.0.2, 2.1, 3.0, 3.0.2, 3.1.1 |
| Log sign-on messages | off | `CPIB691` to `QSYSOPR` |

Fields IBM has that QS/400 refuses (WINS, Kerberos, browsing master,
LAN Manager authentication) are listed in section 10.

### Shares

File shares: name (12 characters), path, text, permission (read only,
read/write), maximum users (-1 no maximum), CCSID, text conversion and
its file extensions, encryption required, an authorization list.
Print shares: name, output queue (library, name), printer driver text,
spooled file type (`*AUTO`, `*USERASCII`, `*AFP`, `*SCS`), printer file,
publish. The shipped share is `QIBM` (`/QIBM`, with QS/400's own
text). `QDIRSRV` is not shipped (no Directory Services).

### 4.4 The files' format

`netsvrcfg::file` writes both files as: an 8-byte magic (`QSZLSCFG`
or `QSZLSSHR`), a 2-byte big-endian format version (1), a 2-byte record
count, then records of a 2-byte tag, a 4-byte length and the value,
UTF-8 text; unknown tags are kept and written back. The agent building
`netsvrcfg` fills in the tag table here.

## 5. Security (design)

### Authentication

NTLMv2 inside SPNEGO (`SESSION_SETUP`), against user profiles. NTLMv1 and
LM responses are refused (`STATUS_LOGON_FAILURE`, logged). Kerberos is
refused: SPNEGO's `NegTokenInit` lists only NTLMSSP, and a Kerberos
token gets a reject. Anonymous sessions reach `IPC$` only (share
enumeration), as IBM allows; the guest profile (when set) is used for
an unknown user name, never for a known user with a wrong password.

### The NT hash

NTLMv2 needs the NT one-way function of the password (MD4 of its
UTF-16LE form), which is password-equivalent. It is kept as a new
password form `NtHash` in the profile's password tail (the `*USRPRF`
payload goes to version 2; version 1 payloads read unchanged, with no
NT hash), sealed with the LIC partition's stash key (AES-GCM, the
profile name bound in) like the `*SMTP` relay credentials. It is set
whenever a password is set (`CRTUSRPRF`, `CHGUSRPRF PASSWORD()`,
`CHGPWD`, `QSYCHGPW`) at the levels where IBM keeps a NetServer
password, and dropped where IBM drops it. A profile with no NT hash
cannot sign on to NetServer (`CPIB68D`); after an upgrade from V1R1M0,
or a `QPWDLVL` change, each user changes the password once.

`QPWDLVL` table (from IBM's documentation of `QPWDLVL`; the security
agent fills the exact rows and cites them):

| `QPWDLVL` | Passwords | NetServer hash kept |
|---|---|---|
| 0 | 1-10, upper case | (fill) |
| 1 | 1-10, upper case | (fill) |
| 2 | 1-128, case sensitive | (fill) |
| 3 | 1-128, case sensitive | (fill) |

### Failed sign-ons and disabled users

A failed NetServer sign-on counts against `QMAXSIGN` in a NetServer
counter of its own. At the limit the user is **disabled for NetServer**
(`CPIB682` to `QSYSOPR`), not the profile itself: that is IBM's
behaviour, and `QSYS2.USER_INFO.NETSERVER_DISABLED` shows it. A
successful sign-on resets the counter; the user is enabled again by
`QZLSCHSI` (the enable-user request), by a password change, or by
Navigator. A `*DISABLED` profile is refused (`CPIB68B`), an expired
password too (`CPIB68C`). Every failure writes a `QAUDJRN` `PW` entry
when `QAUDLVL` has `*AUTFAIL`.

### Authority

After sign-on the `QZLSFILE` job runs as the user; share permission
(read only) narrows what the IFS allows, never widens it. The DOS
attributes and security descriptors clients see are an approximation of
the IFS authorities (section 6).

## 6. File semantics (design)

- Names: case-insensitive lookup with case-preserved storage on `/`
  and `/QSYS.LIB`, case-sensitive under `/QOpenSys`, as the IFS already
  rules (`ifs::path`). 8.3 short names are generated when a client asks
  (`FileAlternateNameInformation`, `FileIdBothDirectoryInformation`).
- File IDs: the IFS file ID, stable across the mount.
- DOS attributes: read only from the IFS's read-only attribute, hidden
  and system from the IFS's, archive from its archive flag; directory.
- Security descriptors: owner and group SIDs made from UID/GID
  (`S-1-22-1-uid`, `S-1-22-2-gid`, as Samba does), a DACL approximating
  `*R`, `*W`, `*X` of owner, group and `*PUBLIC`; `SET_INFO` of a
  security descriptor is refused (`STATUS_ACCESS_DENIED`), as changing
  authority goes through `CHGAUT`.
- Locks: byte-range locks through the IFS lock layer; share modes
  through its share modes.
- Leases (R, RH, RW, RWH on 2.1+) and oplocks (level II, exclusive,
  batch); break to the holder when another opener conflicts.
- Change notify on directories, from the IFS's change events.
- Alternate data streams, quotas, reparse points, extended attributes
  and object IDs are refused (section 10).
- `/QSYS.LIB`: libraries as `X.LIB`, files as `Y.FILE`, members as
  `Z.MBR`; a source member reads and writes as text converted by the
  member's CCSID when the share has text conversion (the `.MBR` extension
  among its extensions), else as records.

## 7. Print shares (design)

`CREATE` on a print share opens a spooled file in the share's output
queue as the signed-on user; `WRITE`s add its data; `CLOSE` ends it. The
type follows the share: `*USERASCII` as written, or through the host
print transform with the share's `*WSCST` (`work/print`).

Point and print: `\pipe\spoolss` serves the [MS-RPRN] subset Windows
uses to connect a printer and print through it (open, get printer and
driver, forms, start/write/end document, the user's own jobs). The share's
printer driver name is what the client is told; when the client has that
driver (the in-box class drivers), Windows connects with no download, and
otherwise takes the driver files from the read-only `print$` share.
Administration calls are refused: print shares are configured with
`QZLSADPS` and Navigator. Since Microsoft's 2021 point-and-print
hardening, Windows lets only administrators install a downloaded driver
unless the client's policy allows it.

## 8. Messages

NetServer's IDs in `QCPFMSG`, with QS/400's own text in
`runtime/msgf/msgs/qcpfmsg.toml`: `CPIB680` started, `CPIB681` ended,
`CPIB682` user disabled for NetServer, `CPIB683`/`CPIB686` unable to
start, `CPIB684` error ending, `CPIB685` request error, `CPIB687`/`CPIB689`
started with an error, `CPIB68A`-`CPIB68E` sign-on failures (no profile,
disabled, expired, no password, disabled for NetServer), `CPIB68F`
enabled, `CPIB690` password expires soon, `CPIB691` connected,
`CPIB692` Kerberos error; `CPFB680`-`CPFB695` the API errors, `CPDB680`.
Severities as `messages.txt`.

## 9. Protocol coverage (design)

| Feature | QS/400 |
|---|---|
| Dialects | 2.0.2, 2.1, 3.0, 3.0.2, 3.1.1 on TCP 445 (direct TCP); 139 with NetBIOS session service when allowed |
| SMB1 | an SMB1 `NEGOTIATE` that offers `SMB 2.002` or `SMB 2.???` is answered with an SMB2 `NEGOTIATE` response (the multi-protocol upgrade); a pure SMB1 negotiate is refused (the connection is closed after a `STATUS_NOT_SUPPORTED`) |
| 3.1.1 | preauthentication integrity (SHA-512), encryption and signing capabilities, `SMB2_NETNAME_NEGOTIATE_CONTEXT_ID` accepted, compression refused (not offered) |
| Signing | HMAC-SHA256 (2.x), AES-128-CMAC (3.0, 3.0.2), AES-128-GMAC refused unless negotiated (3.1.1) |
| Encryption | AES-128-CCM, AES-128-GCM, AES-256-CCM, AES-256-GCM |
| Credits, compounding, large MTU | yes |
| Leases and oplocks | yes (section 6) |
| Durable handles | v1 and v2: a reconnect within the timeout is routed back to the `QZLSFILE` job holding the opens (ClientGuid of the first `NEGOTIATE`, else the client address; section 3) |
| Resilient handles | `FSCTL_LMR_REQUEST_RESILIENCY` |
| Persistent handles, multichannel, RDMA, DFS, compression | refused |
| RPC | `srvsvc` (`NetShareEnumAll`, `NetShareGetInfo`, `NetServerGetInfo`), `wkssvc` (`NetWkstaGetInfo`); everything else `nca_s_op_rng_error` |

## 10. Refused, with reasons

| Refused | Why | What the client sees |
|---|---|---|
| SMB1/CIFS | removed by IBM too; insecure | connection closed after `STATUS_NOT_SUPPORTED` |
| Kerberos, EIM | QS/400 has no Kerberos (`RELEASE_NOTES_V1R1M0.md`) | SPNEGO offers NTLMSSP only |
| NTLMv1, LM | weak | `STATUS_LOGON_FAILURE` |
| Multichannel | not built | no `MULTI_CHANNEL` capability; a binding `SESSION_SETUP` gets `STATUS_REQUEST_NOT_ACCEPTED` |
| WINS | no WINS client | the attribute is refused by `QZLSCHSI` |
| DFS | no DFS | no `DFS` capability; referrals `STATUS_FS_DRIVER_REQUIRED` |
| Quotas | the IFS has none | `STATUS_NOT_SUPPORTED` |
| Alternate data streams | the IFS has none | no `FILE_NAMED_STREAMS`; `name:stream` is `STATUS_OBJECT_NAME_INVALID` |
| Setting security descriptors | authority is `CHGAUT`'s | `STATUS_ACCESS_DENIED` |
| `spoolss` administration (add, set or delete printers, drivers, ports, monitors) | print shares are configured with `QZLSADPS` and Navigator | `ERROR_ACCESS_DENIED` |

## 11. Tests

- `cargo test -p netserver`: codecs against [MS-SMB2] examples, crypto
  against the published vectors ([MS-SMB2] 3.1.4, RFC 4493, RFC 3610,
  SP 800-38C/D, [MS-NLMP] 4.2), state machines over a backend in memory.
- `cargo test -p netsvrcfg`: attributes, shares, file round trips,
  `ZLSL*` formats against the `QSYSINC` layouts.
- `cargo test -p smbtest`: the client against the server over loopback.
- `cargo test -p smbtest --test interop -- --ignored`: Samba's
  `smbclient` and the Linux kernel CIFS client against the server (WSL).
- QEMU steps from 166: the server started, every `smbtest` scenario
  against the guest, the APIs, the SQL view, a 1 GB copy (MB/s here).
- Fuzz targets: the SMB2 header and commands, NTLMSSP, SPNEGO, the
  NetBIOS session service and name service, DCE/RPC.
- `NEEDS-CODY`: Windows 11, macOS and a Linux kernel client map a share,
  open and save with Notepad, Word and Excel, print to a print share; a
  wrong password `QMAXSIGN` times disables the user for NetServer.

## Acceptance

| Item | Proof | Status |
|---|---|---|
| (filled as each part lands) | | |

[MS-SMB2]: https://learn.microsoft.com/openspecs/windows_protocols/ms-smb2
[MS-NLMP]: https://learn.microsoft.com/openspecs/windows_protocols/ms-nlmp
[MS-SPNG]: https://learn.microsoft.com/openspecs/windows_protocols/ms-spng
[MS-SRVS]: https://learn.microsoft.com/openspecs/windows_protocols/ms-srvs
[MS-WKST]: https://learn.microsoft.com/openspecs/windows_protocols/ms-wkst
[MS-RPRN]: https://learn.microsoft.com/openspecs/windows_protocols/ms-rprn
