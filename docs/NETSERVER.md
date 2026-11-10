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

## 5. Security

Built: `security/sec` (`password::nthash`, `netsvr`), the storage
service's `lic/slic/src/stgsvc/nslogon.rs` and the job's requests
`netsvr_*` (`work/stg`, 1500-1503). Where IBM's documentation disagreed
with the first design it is followed, and each change is marked
**Changed**.

### Authentication

NTLMv2 inside SPNEGO (`SESSION_SETUP`), against user profiles. NTLMv1 and
LM responses are refused (`STATUS_LOGON_FAILURE`); the service answers
them `NotNtlmV2` and does not count them against `QMAXSIGN` (a client
configured for NTLMv1 would otherwise disable every user it tries).
Kerberos is refused: SPNEGO's `NegTokenInit` lists only NTLMSSP, and a
Kerberos token gets a reject. Anonymous sessions reach `IPC$` only
(share enumeration), as IBM allows; `netserver` handles them without
the service. The guest profile (when set) is used for an unknown user
name, never for a known user with a wrong password ([guest]: "used when
the requested ID ... is not found"). A guest must exist, be enabled and
have no special authority ([guest]: it "must not have any special
authorities"), else the unknown user is refused; a guest session has no
session base key, so it cannot sign (`SMB2_SESSION_FLAG_IS_GUEST`).

The sign-on is one request of the `QZLSFILE` job over its link (only a
job of that name may ask; any other gets `NoSuchUser`):

```rust
// work/stg/src/proto.rs
1500 => fn netsvr_signon / NetsvrSignon (logon: sec::netsvr::Logon, swap: bool)
    -> Result<sec::netsvr::LoggedOn, sec::netsvr::Refusal>;
1501 => fn netsvr_started / NetsvrStarted () -> ();
1502 => fn netsvr_enable_user / NetsvrEnableUser (user: String) -> Result<bool, String>;
1503 => fn netsvr_disabled_users / NetsvrDisabledUsers () -> Vec<(String, u64)>;
```

`Logon` carries the user and domain as the client sent them, the
server challenge, the NT response, the guest profile name (`None` for
none) and the client's name or address (for `CPIB682`). The guest name
is a parameter of the request until `net/netsvrcfg` holds the
attribute: then the job reads it from the configuration and passes it;
the service does not read the configuration itself. `LoggedOn` is the
profile, whether it is the guest, and the NTLMv2 session base key
(`None` for the guest); with `swap` the job then runs as the profile
(the host servers' swap, `PS` entries). The NT hash never leaves the
service: it is unsealed and checked inside the request
(`ntlm::verify_ntlmv2`, the domain tried as sent, upper case and empty).
`netserver` derives the signing and encryption keys from the session
base key (`ntlm::key_exchange_key`, `exported_session_key`).

| `Refusal` | When | Message (section 8) |
|---|---|---|
| `NoSuchUser` | no profile by the name (or longer than 10), and no usable guest | `CPIB68A` |
| `Disabled` | `STATUS(*DISABLED)` or past `USREXPDATE` | `CPIB68B` |
| `NetServerDisabled` | disabled for NetServer (below) | `CPIB68E` |
| `NoPassword` | `PASSWORD(*NONE)` | `CPIB68D` |
| `NoNtHash` | a password but no NT hash (set before V1R2M0, restored from another system, set through `QSYSUPWD`) | `CPIB68D` |
| `NotNtlmV2` | the response is shorter than an NTLMv2 response | none |
| `WrongPassword { disabled_now }` | the response does not prove the password; `disabled_now` when this one reached `QMAXSIGN` | none; `CPIB682` when disabled |
| `Expired` | the right password, but expired (`PWDEXP(*YES)` or the interval) | `CPIB68C` |

The order is the table's: the profile's own state is checked before the
response, so a disabled profile is never counted; the expiry after it,
so only someone who knows the password learns it has expired. Nothing
in the profile changes (no last sign-on date, no `SignOnInvalid`: that
counter is the 5250 sign-on's).

### The NT hash

NTLMv2 needs the NT one-way function of the password (MD4 of its
UTF-16LE form), which is password-equivalent. It is a new password form
`NtHash` (form byte 4) in the profile's password tail: the `*USRPRF`
payload is version 2; version 1 payloads read unchanged, with no NT
hash (`security/sec/tests/netsvr.rs` reads a payload V1R1M0 wrote). It
is sealed with the LIC partition's stash key (AES-256-GCM, the profile
name the additional data, the key's generation stored beside it) like
the `*SMTP` relay credentials; `docs/SECURITY.md` section 1 has the
bytes. A form byte the reader does not know is now kept and matches
nothing (version 1 read any unknown byte as the long form).

`sec::password::set_hashes` makes it wherever a password is set:
`CRTUSRPRF`, `CHGUSRPRF PASSWORD()`, `CHGPWD`, `QSYCHGPW` and the host
servers' change (all through `password::policy::set`), the installer's
`QSECOFR` and the shipped `QSECOFR`. It is dropped by `PASSWORD(*NONE)`,
never kept for a previous password, never returned by `QSYRUPWD` and
never taken from `QSYSUPWD` data. `RSTUSRPRF` restores the payload as
saved: on the system that sealed it the hash opens; on another (another
stash key) it does not, which reads as `NoNtHash`. A profile with no NT
hash cannot sign on to NetServer (`CPIB68D`); after an upgrade from
V1R1M0 each user changes the password once.

`QPWDLVL` and the NetServer password, from IBM's documentation:

| `QPWDLVL` | Passwords | NT hash kept (QS/400) | IBM |
|---|---|---|---|
| 0 | 1-10, upper case | of the password in upper case and in lower case | NetServer at 0 takes a one-case password only ([auth] table: "mono-case only"; [win7]: entered "in single case (all uppercase or all lowercase)"); the LAN Manager password is kept too |
| 1 | 1-10, upper case | as 0 | as 0; the LAN Manager password (Windows 95/98/ME) is removed ([chg1]: "to eliminate the IBM i NetServer LAN manager passwords") |
| 2 | 1-128, case sensitive | of the password as typed | "mixed-case with NTLM/NTLMv2" ([auth]); the LAN Manager password is kept, cleared for a password over 14 characters ([chg2]) |
| 3 | 1-128, case sensitive | as 2 | as 2; "all IBM i NetServer LAN manager passwords are cleared" ([chg3]) |
| 4 | 1-128, case sensitive | as 2 (`NEEDS-CODY`) | 7.5 only: "all password level 0 and 1 passwords and all password level 2 and 3 passwords are cleared" ([chg4]); the page names "IBM i NetServer Print support" among the clients that work at 4 and says nothing of the NT password |

QS/400 never keeps a LAN Manager hash (LM is refused). The rest follows
from IBM's pages:

- A change of `QPWDLVL` takes effect at the next IPL ([qpwdlvl], [chg1]
  to [chg4]: "A change to the QPWDLVL system value takes effect at the
  next IPL"; `QPWDLVL` is `restart_required` in `lic/sysval`). IBM
  removes only the LAN Manager passwords at a change, never the NT
  password, so QS/400's change of level does nothing to the NT hash.
- After a change, the NT hash is the one made at the old level until the
  password is changed: [win7] says a mixed-case Windows password "will
  only work if the IBM i QPWDLVL system value is changed to 2 and the
  NetServer users' passwords are changed to set the new hash value".
  `CHGUSRPRF PASSWORD()` (or `CHGPWD`) at the new level makes the hashes
  of the table's row for it and replaces the old ones.
- `QPWDLVL` 4: whether IBM keeps the NT password at 4 is not in its
  documentation. QS/400 keeps it (as 2 and 3). `NEEDS-CODY`: on a 7.5
  system at `QPWDLVL` 4 (pub400 is at 3), `DSPAUTUSR` or `QSYS2.USER_INFO`
  for a test profile, then map a drive to NetServer with its password:
  if that fails with the right password, IBM drops the NT password at 4
  and so must `variants` in `password::nthash`.

### Failed sign-ons and disabled users

A password not valid counts against `QMAXSIGN` in a NetServer counter of
its own. At the limit the user is **disabled for NetServer**, not the
profile ([disabled]: the profile is disabled "for NetServer use"):
`CPIB682` goes to `QSYSOPR` ([disabled]: "The IBM i QSYSOPR job log
records CPIB682"), and `QSYS2.USER_INFO.NETSERVER_DISABLED` shows
`YES`. **Changed** after IBM's documentation ([disabled], [disabled74]):

- The counter and the state are NetServer's own and are not persisted:
  they live in the storage service (`sec::netsvr::users()`), outside any
  frozen format. Restarting NetServer or an IPL enables every user
  ([disabled]: the counter is reset by "restarting NetServer, or an
  IPL"); the server job sends `netsvr_started` when it starts.
- The limit is `QMAXSIGN` as it was when NetServer started ([disabled]:
  NetServer "uses the QMAXSIGN value it had when it started");
  `netsvr_started` takes it. Before the first start the current value
  is used. `*NOMAX` never disables.
- A successful sign-on resets the counter ([disabled]: "A successful
  connection is made").
- The user is enabled again by `QZLSCHSI` (its reset of a disabled
  user), by Navigator's Disabled User IDs, by a NetServer restart or an
  IPL. **Not** by a password change: IBM lists the ways and a password
  change is not one of them; QS/400 does the same.
- `CPIB682`'s data, as sent: the profile name `CHAR(10)` and the client
  `CHAR(46)` (CP037); severity 0, as `messages.txt`. The message
  description in `QCPFMSG` is section 8's (`net/netsvrcfg` agent).

The functions `QZLSCHSI`, Navigator and QAP call, in the storage service:

```rust
sec::netsvr::enable_netserver_user(user: &str) -> bool   // whether it was disabled
sec::netsvr::netserver_disabled(user: &str) -> bool
sec::netsvr::users().disabled() -> Vec<(String, u64)>    // by name, with when
sec::netsvr::users().started(qmaxsign: Option<u32>)      // NetServer started
```

From a job they are `netsvr_enable_user` (which needs `*IOSYSCFG`, as
`QZLSCHSI` does), `netsvr_disabled_users` and `netsvr_started` (only the
`QZLSSERVER` job).

**Changed:** the audit entry. IBM writes no `PW` entry for NetServer; a
password not valid is a `VP` entry, "Network password error", selected
by `*AUTFAIL` (IBM's security auditing table, [audit]: "VP ... *AUTFAIL";
the layout [vp]: error type `P` password error, `D` NetServer user
disabled; server name `*NETSERVER`). QS/400 writes `VP` `P` for every
password not valid and `VP` `D` when that one disabled the user, in
IBM's `QASYVPJ5` layout of the primary
(`tools/conformance/oracle/netserver/audit-vp.txt`; `AuditVP` in
`docs/SECURITY.md` section 7; the audit layouts went to version 2). An
unknown user writes nothing (as on IBM i: no entry is written for a user
that does not exist). A refusal for the profile's state (`*DISABLED`,
expired, no password, disabled for NetServer) writes no entry either:
IBM's `VP` types are only `P` and `D`. `NEEDS-CODY`: on the primary,
`DSPAUDJRNE ENTTYP(VP)` after a wrong password from a test client, to
confirm the date format of `VPSDAT` (QS/400 writes `MMDDYY`, UTC) and
whether `D` is written once or for each attempt while disabled (QS/400:
once).

[qpwdlvl]: https://www.ibm.com/docs/en/i/7.5?topic=passwords-password-level-qpwdlvl
[chg1]: https://www.ibm.com/support/knowledgecenter/ssw_ibm_i_74/rzarl/rzarlconsdchg1.htm
[chg2]: https://www.ibm.com/support/knowledgecenter/ssw_ibm_i_74/rzarl/rzarlconsdchg2.htm
[chg3]: https://www.ibm.com/support/knowledgecenter/ssw_ibm_i_74/rzarl/rzarlconsdchg3.htm
[chg4]: https://www.ibm.com/docs/en/i/7.5.0?topic=changes-considerations-changing-qpwdlvl-from-2-3-4
[auth]: https://www.ibm.com/support/pages/netserver-authentication-and-security-considerations
[win7]: https://www.ibm.com/support/pages/windows-7-and-ibm-i-netserver
[disabled]: https://www.ibm.com/support/pages/user-profiles-disabled-use-ibm-i-netserver
[disabled74]: https://www.ibm.com/support/pages/node/684947
[guest]: https://www.ibm.com/support/knowledgecenter/ssw_ibm_i_71/rzahl/rzahltipguest.htm
[audit]: https://www.ibm.com/support/knowledgecenter/ssw_ibm_i_74/rzarl/rzarlsecaudje.htm
[vp]: https://www.ibm.com/support/knowledgecenter/ssw_ibm_i_74/rzarl/rzarlf68.htm

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
- `cargo test -p sec --test netsvr`: the V1R1M0 payload read under
  version 2, unknown password forms, the NT hash at each `QPWDLVL` and
  where it is set and dropped, every sign-on refusal, levels 0 and 1 in
  either case, the guest, disabling at `QMAXSIGN` with its `VP` entries;
  `cargo test -p stg --test netsvr_link`: `netsvr_signon` over a link;
  `cargo test -p sql --test services user_info`: `NETSERVER_DISABLED`.
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
