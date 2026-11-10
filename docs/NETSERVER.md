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
| SMB2/3 protocol | `net/smb-server` | rust-smb-server 0.10.0 (MIT, Palta.io), vendored with QS/400 patches (`net/smb-server/QS400-PATCHES.md`, each marked `QS/400:`): framing, every SMB2 command, negotiate contexts, signing, encryption, credits, compounding, sessions, trees, opens, leases, durable handles, change notify, NTLMv2 in SPNEGO, `srvsvc` and `spoolss` over named pipes. QS/400 does not write SMB codecs, NTLM or crypto |
| NetServer glue | `net/netserver` | the IFS backend (`ifs/`), sign-on (`signon.rs`), print shares (`print.rs`), one job's server (`server.rs`), the NetBIOS session request on 139 (`nbss/`), reconnect routing (`route.rs`). **No job, store or service code**: the job implements `signon::SignOn`, `print::Printer` and hands an `ifs::client::IfsClient` maker |
| Configuration | `net/netsvrcfg` | the server attributes (current and pending), file and print shares, validation, the `QAZLSCFG` and `QAZLSSHR` codecs, the `ZLSL*` and `ZLSS*` formats of the APIs, the reason codes |
| Test client | `tools/smbtest` | an SMB2/3 client written separately from the server (it does not depend on `net/netserver`, so one codec bug cannot hide in both) and its scenarios |
| Profiles | `security/sec` | the NT hash password form, the `QPWDLVL` rules for it, the NetServer-disabled state, the guest profile check |
| Storage service side | `lic/slic/src/stgsvc/netsvr.rs` (NS-2: `nslogon.rs`) | the configuration files over the IFS, the session table, the API and SQL data |
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

### Reconnects and durable handles

A client whose connection drops keeps its durable opens (version 1 and
2, with a lease that caches handles) for the durable timeout (the
library's default, 2 minutes, or what a version 2 request asks, up to 5
minutes) and takes them back on a new connection. The opens live in the
`QZLSFILE` job that served it, so the new connection must reach the same
job:

1. The job registers with `QZLSSERVER` the ClientGuids it serves or holds
   opens for (`NetServer::client_guids`) and the peer addresses of its
   connections, and updates them as they change.
2. `QZLSSERVER`, after accepting (and on 139 after answering the session
   request, `nbss::answer`), reads the ClientGuid of the first NEGOTIATE
   without consuming it (`route::client_guid`: `MSG_PEEK`, at most 2
   seconds and 64 KiB). An SMB2 NEGOTIATE offering 2.1 or later carries
   one; an SMB1 multi-protocol NEGOTIATE or a 2.0.2-only one gives none,
   and the peer address is used instead.
3. A ClientGuid (else a peer address) a job has registered sends the
   socket to that job, which serves it with `NetServer::serve_one` beside
   any connection it already serves; anything else goes to a new
   prestart job.
4. The library's own reconnect then runs within that job: same user, same
   client, same share, the lease key and, for version 2, the CreateGuid.
5. The job ends (back to the prestart pool) when
   `NetServer::idle_until_released` returns: no connection served and no
   durable open kept. It then withdraws its registration.

One job serves one profile: a sign-on as another profile in a job that
already has one is refused (`signon::Refusal::OtherProfile`, logged), so
a routed connection never runs under another user's authority. The
routing table and the job wiring are `QZLSSERVER`'s and `QZLSFILE`'s
(`work/qsjob/src/work/netsvr/`).

## 4. Configuration

Built in `net/netsvrcfg` (the model, the files, the formats, the APIs
over bytes) and `lic/slic/src/stgsvc/netsvr.rs` (the files over the
IFS, the session table, the requests).

### Persistence

As on IBM i, two stream files under `/QIBM/UserData/OS400/NetServer/`:
`QAZLSCFG` (server attributes) and `QAZLSSHR` (shares). The storage
service makes them at first use (any API call, the `netsvr` request's
`Config`, or `SERVER_SHARE_INFO`) with IBM's defaults and the shipped
`QIBM` share, as `QSYS`, mode `0600` (owner `QSYS`, `*PUBLIC *EXCLUDE`);
new directories on the way are `0700`. Each file is replaced whole
(written beside it, then renamed over it). They are saved and restored
with `SAV` and `RST` of `/QIBM/UserData` (`GO SAVE` 21 and 23), as IBM's
are; not by `SAVSECDTA` or `SAVCFG`. The content is QS/400's own format
(IBM's is undocumented: the primary's are binary records with UTF-16
strings), specified in 4.4. Only the storage service writes them; the
`QZLSSERVER` and `QZLSFILE` jobs read them through the `netsvr` request
(`Config`, the two files' bytes, decoded with `netsvrcfg::file::read`).

### Server attributes

Each attribute has a current value (in effect) and a pending value.
`QZLSCHSN`, `QZLSCHSG` and `QZLSCHSI` (`ZLSS0100`) change only the
pending value; it becomes current at a `QZLSSTRS` with reset `1` that
the server control accepts, or at the first read of the configuration
after an IPL (the service applies pending values once per IPL). The
defaults are the primary's (`zlsl/olst-ZLSL0201-ALL.txt`).

| Attribute | Default | `ZLSL0201` / `ZLSS0100` | Notes |
|---|---|---|---|
| Server name | the system name (`QSYSNAME`), upper case, 15 characters | `serverName` | `QZLSCHSN`; the primary's equals its system name |
| Domain (workgroup) | blank | `domainName` | `QZLSCHSN`; the primary has one set |
| Text | `QS/400` | `textDescription` | the primary's is `OS/400` |
| Guest profile | none | `guestLogonUserProfile`, `guestLogonSupport` 0/1 | `QZLSCHSG` |
| Code page | 0 (the job's) | `codePage` | the OEM code page of NetBIOS names; 0 or a code page with a CCSID QS/400 converts |
| Idle time-out | 6000 seconds | `idleTimeOut` | -1 never; above 0 |
| Oplock time-out | 30 seconds | `oplockTimeOut` | -1 turns leases and oplocks off; 1 to 600 |
| Browse interval | 720000 ms | `browsingInterval` | 0 to 720000; for the NetBIOS announcements when NetBIOS is allowed |
| WINS primary, secondary, scope, proxy | none | as named | refused when set (section 10) |
| Allow system name | `0` | `systemNameAccess` | the server also answers to the system name |
| Authentication method | `0` (passwords) | `authMethod` | `1`, `2` (Kerberos) refused |
| Logon server role | 0 | `serverRole`, `domainLogonSupport` | 1 refused |
| Message authentication | 0 | `messageAuthMode` | 0 and 1: signing offered, not required (SMB2 cannot turn it off); 2: required |
| Minimum message severity | -1 | `minMessageSeverity` | only -1 (no administrative alerts in SMB2) |
| LAN Manager authentication | 0 | `LanmanAuth` | 1 refused |
| Encrypted connections | 0 | `encryptedConnectionEnforcement` | 0 not required, 1 required for every connection |
| Server authorization list | none | `serverAuthorizationList` | users need `*USE` to it to connect; must exist |
| Allow NetBIOS | no | (QS/400 only) | 139, 137, 138 |
| SMB2 minimum dialect | 2.0.2 | (QS/400 only) | 2.0.2, 2.1, 3.0, 3.0.2, 3.1.1 |
| Log sign-on messages | no | (QS/400 only) | `CPIB691` to `QSYSOPR` |

The QS/400-only attributes have no API yet; Navigator and the file set
them. Autostart is the server table's (`*NETSVR`).

### Shares

File shares: name (12 characters, upper case), path (begins with `/`,
up to 1024 bytes, trailing `/` dropped), text (50), permission (1 read
only, 2 read/write), maximum users (-1 no maximum, else above 0), text
CCSID (0 or a CCSID QS/400 converts), text conversion, its extensions
(up to 100, each up to 46 bytes, upper case, a leading `.` dropped),
encryption required, an authorization list (must exist). Print shares:
name, output queue (library, name), printer driver text (50), spooled
file type (1 `*USERASCII`, 2 `*AFP`, 3 `*SCS`, 4 `*AUTO`), printer
file, publish (always off: QS/400 has no directory server), encryption,
authorization list. Share changes take effect at once.

Share names may not begin with a blank or hold a control character or
`\ / : * ? " < > | [ ] ; = + ,` (`CPFB68B`); server and domain names
take only A-Z, 0-9, `#`, `$`, `@` and embedded blanks, as `CPFB68B`
states. `NEEDS-CODY`: whether IBM accepts more in share names (only an
add call shows it: `CALL QZLSADFS ('A-B_C' '/tmp' 4 0 ' ' 1 -1 errc)`
on a test system, then `QZLSRMS`).

The shipped share is `QIBM` (`/QIBM`, read/write, no maximum, QS/400's
own text), as on the primary. `QDIRSRV` is not shipped (no Directory
Services).

### 4.4 The files' format

Frozen at V1R2M0 (`tools/fmtcheck/formats.toml`, format
`netserver-config`; version `netsvrcfg::file::FORMAT_VERSION`).

Both files: an 8-byte ASCII magic (`QSZLSCFG` or `QSZLSSHR`), a 2-byte
big-endian format version (1), a 2-byte record count, then records of
a 2-byte tag, a 4-byte length and the value. Values are UTF-8 text;
integers are decimal text, flags `0` or `1`. A newer version is refused;
unknown tags are kept and written back (in `QAZLSSHR`, an unknown tag
inside a share stays with that share; one before the first share stays
at the head). Bytes after the last record, a short record or a value
not valid for its tag make the file unreadable (the API fails with
`CPFB681`).

`QAZLSCFG`: each attribute twice, the current value under its tag and
the pending value under the tag with bit 15 set (`0x8000`). A missing
attribute takes the default.

| Tag | Attribute | Value |
|---|---|---|
| `0x0001` | server name | text |
| `0x0002` | domain | text |
| `0x0003` | text | text |
| `0x0004` | guest profile | text, empty for none |
| `0x0005` | code page | integer |
| `0x0006` | idle time-out | integer |
| `0x0007` | oplock time-out | integer |
| `0x0008` | browse interval | integer |
| `0x0009` | WINS primary | text |
| `0x000A` | WINS secondary | text |
| `0x000B` | scope | text |
| `0x000C` | WINS proxy | flag |
| `0x000D` | allow system name | flag |
| `0x000E` | authentication method | integer 0-2 |
| `0x000F` | logon server role | integer |
| `0x0010` | message authentication | integer |
| `0x0011` | minimum message severity | integer |
| `0x0012` | LAN Manager authentication | integer |
| `0x0013` | encrypted connections | integer |
| `0x0014` | server authorization list | text |
| `0x0015` | allow NetBIOS | flag |
| `0x0016` | SMB2 minimum dialect | `2.0.2`, `2.1`, `3.0`, `3.0.2`, `3.1.1` |
| `0x0017` | log sign-on messages | flag |

`QAZLSSHR`: each share is its name record followed by its attributes,
until the next name record, in name order.

| Tag | Attribute | Value |
|---|---|---|
| `0x0100` | share name (starts a share) | text |
| `0x0101` | type | `FILE` or `PRINT` |
| `0x0102` | text | text |
| `0x0103` | encryption required | flag |
| `0x0104` | authorization list | text, empty for none |
| `0x0110` | file: path | text |
| `0x0111` | file: permission | integer 1, 2 |
| `0x0112` | file: maximum users | integer |
| `0x0113` | file: text CCSID | integer |
| `0x0114` | file: text conversion | flag |
| `0x0115` | file: an extension (one record each, in order) | text |
| `0x0120` | print: output queue | `LIB/NAME` |
| `0x0121` | print: printer driver | text |
| `0x0122` | print: spooled file type | integer 1-4 |
| `0x0123` | print: printer file | `LIB/NAME`, empty for none |
| `0x0124` | print: publish | flag |

### The session table

The storage service holds one table per IPL (`netsvrcfg::SessionTable`):
sessions (identifier, workstation name, expanded name, user, the
serving job, guest, start, last request, the end mark) with their tree
connections (identifier, share, type 0 disk, 1 print, 3 IPC, open
files, start), and the `ZLSL0400` counters. The jobs report through the
`netsvr` request (tag 1511): `Started`, `Register` (the reply is the
session's identifier; the service fills in the job), `Unregister`
(normal or idle end), `Connect`, `Disconnect`, `Opens`, `Touch`,
`Count` (a counter and an amount), `JobEnded` (the job's sessions go,
counted as errors). Only jobs named `QZLSSERVER` or `QZLSFILE` may
change it; any job may `List` and ask `Ending` (whether `QZLSENSS`
marked a session to end, which the serving job then ends and
unregisters).

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

## 6. File semantics (built: `net/netserver/src/ifs/`)

- Names: looked up and stored by the IFS's own rules (`ifs::path`):
  case-insensitive with case preserved on `/` and `/QSYS.LIB`,
  case-sensitive under `/QOpenSys`; a share reports the case sensitivity
  of its root's file system. The library rejects `..`, empty components,
  `<>:"|?*` and control characters before the backend sees a name. No 8.3
  short names: listings carry an empty ShortName (clients use the long
  name).
- File IDs: the IFS file ID (file number and generation, `Qp0lGetAttr`'s
  `*FILEID`), the same in listings, `FileInternalInformation` and the
  library's tables; an open finds its object by it after a rename
  (`path_from_file_id`).
- DOS attributes: `FILE_ATTRIBUTE_READONLY`, `HIDDEN`, `SYSTEM` and
  `ARCHIVE` are the IFS's `*READONLY`, `*HIDDEN`, `*SYSTEM` and
  `*PCARCHIVE`, read and set (`SET_INFO` FileBasicInformation, the
  attributes of a CREATE); `TEMPORARY`, `OFFLINE` and
  `NOT_CONTENT_INDEXED` are not stored (dropped, and reported clear).
  A directory is never read-only to clients.
- Times: last access and last write are the IFS's and can be set; the
  creation time is the IFS's and cannot be set (a set of it alone
  succeeds and changes nothing); the change time is the IFS's.
- Share modes: the library checks them among the opens of the job; an
  open with data access also opens its IFS descriptor with
  `O_SHARE_RDONLY`, `O_SHARE_WRONLY`, `O_SHARE_RDWR` or `O_SHARE_NONE`
  from FILE_SHARE_READ and FILE_SHARE_WRITE, so other jobs (NetServer or
  not) are refused as the IFS refuses them (`STATUS_SHARING_VIOLATION`).
  FILE_SHARE_DELETE has no IFS counterpart. Attribute-only opens take
  no descriptor and deny nothing.
- Byte-range locks: the library keeps them and also takes each in the
  IFS (`F_SETLK` on the open's descriptor), so another job's conflicting
  lock gets `STATUS_LOCK_NOT_GRANTED`. IFS locks are advisory: another
  job's plain reads and writes are not refused. An exclusive lock through
  an open without write access is taken shared in the IFS.
- Leases: on by default (`FileShare::leases`); they let a client cache
  and are needed for durable handles. The library breaks them for opens
  in the same job only: a change made by another job (another client, a
  program, FTP) breaks no lease, and the caching client keeps stale data
  until it next opens the file. Shares where several clients change the
  same files should turn leases off (which also turns off durable
  handles there). Oplocks are not offered (the library has none).
- Change notify: the IFS has no change events, so clients hear only of
  changes made through their own job's connections.
- Security descriptors: queries answer the library's default (no owner or
  DACL from the IFS); setting one is refused (section 10). Authority is the
  IFS's: every call runs as the job's profile and the share's read-only
  permission narrows it.
- Named streams and extended attributes are off on every share
  (`Share::named_streams(false)`, `extended_attributes(false)`).
- `/QSYS.LIB`: libraries are `X.LIB`, files `Y.FILE`, members `Z.MBR`
  directories and files as the IFS's `/QSYS.LIB` view shows them; a
  member's data is what that view reads and writes (no per-share text
  conversion in V1R2M0: the share's CCSID and text-conversion fields are
  refused by `QZLSADFS`, section 10).

### Backend

`ifs::IfsShare` implements the library's `ShareBackend` over the job's
`ifs::client::IfsClient`. An `IfsClient` is not `Send` (the job's link
belongs to one thread), so `ifs::IfsWorker` makes it on a thread of its
own and runs every call there; the async side sends closures and awaits
their answers. Each `Handle` holds the object's file ID and path, and an
IFS descriptor: opened at CREATE for an open with data access or a
truncate (with its share mode), else on first read. CREATE dispositions
map to `open()` flags: `FILE_OPEN` an existing object,
`FILE_CREATE` `O_CREAT|O_EXCL`, `FILE_OPEN_IF` `O_CREAT`,
`FILE_OVERWRITE(_IF)` `O_TRUNC`; directories are `mkdir()` (mode 777
under the umask), files `O_INHERITMODE` (data authorities from the
directory, as IBM NetServer does). Reads and writes move at most 1 MiB
per IFS call. Listings are `read_dir` filtered by the search pattern
([MS-FSA] 2.1.4.4: `*`, `?`, `<`, `>`, `"`, without regard to case) and a
stat of each entry. Volume information is `statvfs()` of the share root.
Errors map as: `ENOENT` not found (path not found when the directory is
missing too), `EACCES`/`EPERM`/`EROFS`/`EROOBJ` access denied, `EEXIST`
collision, `ENOTEMPTY` not empty, `EBUSY`/`EAGAIN` sharing violation,
`ENOSPC` disk full.

`print::PrintShare` is the backend of a print share over the job's
`print::Printer` (section 7). `NetServer::build` makes one job's
`SmbServer`: the shares as the configuration gives them, every share
granted to any signed-on profile (`Share::any_user`, read or read/write
by the share permission), remarks from the share text, encryption per
share or for all, signing required or not, the idle timeout, read and
write sizes, the minimum dialect, the domain and server text, and the
library's own logon throttle **off** (`logon_throttle(0, ..)`): failed
sign-ons count against `QMAXSIGN` in the storage service (section 5),
and the library's per-name and per-address lockouts would lock users out
on rules IBM does not have. The server runs on a current-thread tokio
runtime on a thread of its own; `serve_one` hands it one socket.

## 7. Print shares (design)

`CREATE` on a print share opens a spooled file in the share's output
queue as the signed-on user; `WRITE`s add its data; `CLOSE` ends it.
Built as a library patch (`Share::print`: TREE_CONNECT share type PRINT
0x03, listings `STYPE_PRINTQ`, every CREATE a new print file named
`PRINTnnnnnnnn` whatever the name the client gave, no leases, durable
handles, change notify, streams or server-side copy) and
`net/netserver/src/print.rs` (`Printer::open(user, queue, document)`, then
`PrintJob::write` and `PrintJob::close`, which the job implements over its
spooler). Reads of a print file are refused; a directory or named-stream
CREATE on a print share is `STATUS_INVALID_PARAMETER`. The
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

### Point and print (built: `net/smb-server/src/rpc/spoolss/`)

`\pipe\spoolss` exists on IPC$ when the server has a print service
(`SmbServerBuilder::print_service`, the library's `print::PrintService`
trait: the printers a session sees, a job's start, writes and end, the
user's jobs). It binds winspool 1.0 over NDR 2.0 only, takes request
stubs up to 1 MiB + 4 KiB (a `RpcWritePrinter` of 1 MiB), and answers
output buffers (`cbBuf`) up to 512 KiB. INFO structures are
custom-marshaled ([MS-RPRN] 2.2.2): fixed portions, then variable data
after them, offsets from each fixed portion, strings aligned to 2,
DEVMODE and security descriptor to 4.

| Call (opnum) | What is served |
|---|---|
| `RpcOpenPrinter` (1), `RpcOpenPrinterEx` (69) | `\\NAME` or none is the server, `\\NAME\SHARE` or `SHARE` a printer (the server part is not checked); `,Job n`, `,XcvPort` and other suffixes `ERROR_INVALID_PRINTER_NAME`; a data type other than `RAW` `ERROR_INVALID_DATATYPE`. Granted: `SERVER_ACCESS_ENUMERATE` or `PRINTER_ACCESS_USE`, with `READ_CONTROL`; generic read, write and execute and `MAXIMUM_ALLOWED` map to that; anything more (administer, `DELETE`, `WRITE_DAC`, `GENERIC_ALL`) `ERROR_ACCESS_DENIED`. 64 handles a pipe |
| `RpcClosePrinter` (29) | ends a document still open, as Windows does; a handle the pipe did not open faults `nca_s_fault_context_mismatch` (every call) |
| `RpcEnumPrinters` (0) | levels 1, 2, 4, 5; name none or `\\NAME` with `PRINTER_ENUM_LOCAL`, `_NAME` or `_SHARED`; other flags list none; a printer's name `ERROR_INVALID_NAME` |
| `RpcGetPrinter` (8) | levels 0 (`PRINTER_INFO_STRESS`), 1, 2, 3, 4, 5, 7, 8; a server handle `ERROR_INVALID_HANDLE`; a share gone since the open `ERROR_PRINTER_DELETED` |
| `RpcGetPrinterDriver` (11), `RpcGetPrinterDriver2` (53) | levels 1, 2, 3, 4, 6, 8 for `Windows x64`, `Windows NT x86`, `Windows ARM64`, `Windows IA64` (none means x64); other environments `ERROR_INVALID_ENVIRONMENT`; server version 3.0 (or the files' version) |
| `RpcGetPrinterData` (26), `RpcGetPrinterDataEx` (78), `RpcEnumPrinterData` (72) | server: `MajorVersion` 3, `MinorVersion` 0, `Architecture`, `DsPresent` 0, `W3SvcInstalled` 0, `DNSMachineName`, `OSVersion`, `OSVersionEx` (10.0, a server, as srvsvc says); printer: `ChangeID`; keys none or `PrinterDriverData`; anything else `ERROR_FILE_NOT_FOUND` |
| `RpcEnumPrinterDataEx` (79), `RpcEnumPrinterKey` (80) | a printer's `PrinterDriverData` key, empty; other keys `ERROR_FILE_NOT_FOUND` |
| `RpcEnumForms` (34), `RpcGetForm` (32) | levels 1 and 2 of Letter, Legal, Tabloid, Executive, A3, A4, A5, B5 (JIS), Envelope #10, Envelope DL (built-in, whole sheet imageable); other names `ERROR_INVALID_FORM_NAME` |
| `RpcStartDocPrinter` (17) | level 1, `RAW` or no data type, no output file; one document a handle (`ERROR_INVALID_PRINTER_STATE` for a second); answers the job id the print service gives |
| `RpcStartPagePrinter` (18), `RpcEndPagePrinter` (20), `RpcWritePrinter` (19), `RpcEndDocPrinter` (23), `RpcAbortPrinter` (21) | pages are counted and given to the job's end; without a document `ERROR_SPL_NO_STARTDOC`; a pipe that goes with a document open abandons it |
| `RpcGetJob` (3), `RpcEnumJobs` (4) | levels 1 and 2 of the user's own jobs on the printer; an unknown job `ERROR_INVALID_PARAMETER` |
| `RpcRemoteFindFirstPrinterChangeNotification` (62), `...Ex` (65) | `ERROR_NOT_SUPPORTED`: the server never opens the reply channel to the client's spooler, and the client falls back to polling the printer (`NEEDS-CODY`: confirm a Windows 11 client's queue window keeps working) |
| `RpcFindClosePrinterChangeNotification` (56) | `ERROR_INVALID_HANDLE`: no notification is ever open |

Driver names: the print share's printer driver (`ZLSL0101`'s field) is
what `GetPrinter` and `GetPrinterDriver2` report. For a driver every
client has in its driver store (the in-box "Microsoft PCL6 Class
Driver", "Microsoft PS Class Driver"), the share names no files and
every environment is answered with empty file names: Windows finds the
driver locally and connects with no download. A share whose driver has
files names them per environment (`PrinterInfo::driver_files`, paths
inside `print$`, e.g. `x64\3\unidrv.dll`), and the DRIVER_INFO levels
point the client to `\\NAME\print$\...`; an environment with no files
is then `ERROR_UNKNOWN_PRINTER_DRIVER`.

In the job (`net/netserver/src/print/`): `spoolss::Spoolss` is the
library's print service over the job's `Printer`, the same one the
share's CREATE and WRITE use, so a document printed through
`\pipe\spoolss` becomes a spooled file on the share's output queue as
the signed-on profile, exactly as one copied to the share does. Its
printers are the job's print shares (`PrintShareCfg`, with `driver`
from the share's printer driver attribute), for every signed-on profile
(none for anonymous sessions); the port column shows the output queue,
`LIB/NAME`; documents start on Letter. `RpcAbortPrinter`, and a pipe
closed with a document open, call `PrintJob::abort`, which deletes the
spooled file. The jobs `RpcEnumJobs` shows are the profile's documents
being written through this job (`JOB_STATUS_SPOOLING`); once ended they
are on the output queue, where `WRKSPLF` and Navigator show them.

The driver store (`print::drivers`): the read-only `print$` disk share
serves the IFS directory `/QIBM/UserData/OS400/NetServer/print$`
(`NetServerConfig::driver_store`), when it exists and the job has print
shares. IBM i has no such directory (the primary's
`/QIBM/UserData/OS400/NetServer` holds only `QAZLSCFG` and `QAZLSSHR`,
`tools/conformance/oracle/netserver/README.md`), so the path is QS/400's
choice, beside IBM's files. Its layout is Windows': `x64`, `W32X86` and
`ARM64`, each with `3` and `4` for the driver version. For a driver
named `NAME`, the administrator copies the driver's files into, say,
`x64/3` and writes `x64/3/NAME.qsdrv` beside them, lines of
`key=value`: `driver`, `data` and `config` once each, `help` at most
once, `file` for each dependent file, every value a file name in that
directory (`#` lines are comments). The newest version with a manifest
is the one an environment is answered with; a manifest that does not
parse is left out and logged. The files are
read when the job starts. Since Microsoft's 2021 point-and-print
hardening (KB5005652, `RestrictDriverInstallationToAdministrators`),
Windows lets only administrators install a driver downloaded from a
server unless the client's policy allows it; that is client policy, and
the class drivers avoid it.

Data: printers are `RAW`-only (`PRINTER_ATTRIBUTE_RAW_ONLY`), so the
client renders and sends the printer's language; the print processor
named is `winprint`. The DEVMODE given (levels 2 and 8) is the public
part only: the printer's name, portrait, one copy, the share's default
form.

## 8. Messages

Every NetServer identifier of `QCPFMSG` (`tools/conformance/oracle/netserver/messages.txt`)
is in `runtime/msgf/msgs/qcpfmsg.toml` with QS/400's own first- and
second-level text, IBM's severity, and IBM's substitution layout (the
primary's `QMHRTVM RTVM0300` gave the variables' types and lengths;
its text was read to learn what each reason code means and is not
kept). `CPIB682` carries the profile (`*CHAR 10`) and the client's
address (`*CHAR 46`), as NS-2's sign-on sends it to `QSYSOPR`.

| ID | Sev | Meaning | Data |
|---|---|---|---|
| `CPIB680`, `CPIB681` | 0 | NetServer started, ended | server name 15 |
| `CPIB682` | 0 | user disabled for NetServer | profile 10, client address 46 |
| `CPIB683`, `CPIB684` | 40 | unable to start, error ending | reason, return code, ... |
| `CPIB685` | 40 | a request's conversion failed | CCSIDs, return code, item, share |
| `CPIB686` | 40 | a share's extensions unreadable | share 12 |
| `CPIB687`, `CPIB689` | 0 | started with an error | reason, ... |
| `CPIB688` | 0 | a user shared a directory (home share) | user, workstation, share, path, client |
| `CPIB68A`-`CPIB68E` | 40 | sign-on refused: no profile, profile disabled, password expired, no password form, disabled for NetServer | user 37 |
| `CPIB68F` | 10 | user enabled again | user 37 |
| `CPIB690` | 20 | password expires in &2 days | user 37, days `*BIN 2` |
| `CPIB691` | 10 | user signed on | user 37 |
| `CPIB692` | 40 | Kerberos error | user 37, code |
| `CPDB680` | 40 | not authorized to enable a user | profile 10 |
| `CPFB680` | 40 | `QZLSSERVER` not active | none |
| `CPFB681` | 40 | internal error | none |
| `CPFB682`-`CPFB695` | 40 | the API errors | the APIs section |

The help audit (`tools/helpaudit`) reads the help of every message; each
of these has its own.

## APIs

Natives of `QSYS` (`runtime/sysapi/src/netsvr.rs`): each gathers its
parameters' bytes in `netsvrcfg::api`'s order and sends them to the
storage service (`netsvr_api`, tag 1510), which runs
`netsvrcfg::api::call` over the configuration files, the caller's
principal, the object store and the session table; the receiver, list
information, user space and error code (`ERRC0100`/`ERRC0200`, or an
escape when the error code is 0 bytes) are filled in the job. The error
data are laid out as the primary's: every message's data starts with the
API's name (10).

| API | Parameters (after IBM's header) | Authority | Errors (reasons) |
|---|---|---|---|
| `QZLSADFS`, `QZLSCHFS` | share 12, path, path length, path CCSID (0, 37, 1200, 13488, 1208 or any QS/400 converts), text 50, permission, maximum users, errc; optional text CCSID, text conversion, extension table, extension count, encryption required, authorization list | `*IOSYSCFG`, or ownership of the path (`CPFB684` 3 missing, 4 not owner; `QZLSCHFS` checks the old and the new path) | `CPFB68A` 1 duplicate, 2 missing, 3 permission, 4 maximum users, 7 path, 8 a print share, 10 text conversion or CCSID; `CPFB68B` name characters; `CPFB68D` path length (1 to 1024, 2048 in UTF-16), extension count or length; `CPFB683` path CCSID; `CPF2283` no such authorization list |
| `QZLSADPS`, `QZLSCHPS` | share 12, output queue 20, text 50, spooled file type, driver 50, errc; optional printer file 20, publish, encryption, authorization list | `*IOSYSCFG`, or ownership of the output queue | `CPFB68A` 1, 2, 5 spooled file type, 8 a file share, 11 publish value; publish `1`: the share is added or changed unpublished and `CPFB682` 6 is returned |
| `QZLSRMS` | share 12, errc | `*IOSYSCFG`, or ownership of the path or queue | `CPFB68A` 2 |
| `QZLSCHSN` | server name 15, domain 15, text 50, errc; optional allow system name | `*IOSYSCFG` (`CPFB684` 1) | `CPFB68B` parameter 1, 2 or 4 |
| `QZLSCHSG` | guest 10 (blanks or `*NONE`: no guest), errc | `*IOSYSCFG` and `*SECADM` (`CPFB684` 2) | `CPFB687` (no profile, or no `*USE` to it), `CPFB68C` (it or a group has a special authority) |
| `QZLSCHSI` | request, length, format, errc | `ZLSS0100`: `*IOSYSCFG`; `ZLSS0200`: `*OBJMGT` and `*USE` to each profile | `CPFB690` 1 format; `CPFB68D` length (`ZLSS0100` 296 to 310: the 7.1 layout, then encryption, then the authorization list; fields past the length keep their pending values); `CPFB686` 5, 6, 7, 8, 9, 10, 12, 13, 14, 15, 16, 17, 18 (4 above); `CPF2283`; `ZLSS0200`: `CPFB694` with the count not enabled (enabling calls `sec::netsvr::enable_netserver_user`) |
| `QZLSSTRS` | reset 1 (`0` or `1`), errc | `*IOSYSCFG` | `CPFB685` 5 reset value, 2 active already; then the server control's answer |
| `QZLSENDS` | errc | `*IOSYSCFG` | the server control's answer |
| `QZLSENSS` | workstation 15 (a name, `*SESSID`), errc; optional session identifier 8 | `*IOSYSCFG` | `CPFB68E` 1 no session, 2 not active, 4 identifier without `*SESSID`, 5 no such identifier, 6 `*EXPANDED` (the prototype has no expanded name parameter) |
| `QZLSLSTI` | qualified user space 20, format, qualifier 15, errc; optional session user 10, session identifier 8 | none | `CPFB68F` 1 format, 3 user space, 7 qualifier, 9 `*SESSID` rules, 10 `ZLSL0900` needs a blank qualifier, 11 `*EXPANDED` |
| `QZLSOLST` | receiver, length, list information 64, format, qualifier, errc; optional session user, session identifier | none | `CPFB688` 1 format, 2 `QZLSLSTI`'s refusal; `CPFB68D` negative length |

Qualifiers, as the primary takes them: `ZLSL0100`, `ZLSL0101`,
`ZLSL0800` a share name, `*ALL` or blanks (a missing share is an empty
list); `ZLSL0200`, `ZLSL0201` anything; `ZLSL0300`, `ZLSL0600` a
workstation, `*ALL`, blanks or `*SESSID`; `ZLSL0700` a share, `*ALL`
or blanks; `ZLSL0400` `*ALL` only; `ZLSL0900` blanks only.

Formats: QS/400 returns the 7.5 layouts (`ZLSL0101` 209 fixed bytes,
`ZLSL0201` 800 bytes); `netsvrcfg::fmt::zlsl::Level::V7R1` writes the
primary's (198, 772), and `net/netsvrcfg/tests/captures.rs` compares
both APIs' receivers, user spaces (generic header of 576 bytes, level
`0300`, the input section at 576, the list at 619) and error data with
the captures byte for byte. Path offsets: `ZLSL0100` in a receiver
from the record, otherwise from the buffer's start; paths are in the
job's CCSID. `ZLSL0101`'s extension table follows the path (offset 0
when there is none; `NEEDS-CODY`: the primary has no share with
extensions to show the placement). `ZLSL0300`'s logon type is `1` for a
user and `0` for the guest, the encrypted-password flag always `1`,
the workstation `*EXPANDED` (type 1) when its name passes 15
characters (`NEEDS-CODY`: no live session was captured).

The server control (`netsvrcfg::api::ServerControl`): `QZLSSERVER`
counts as active when the job table has it in `QSERVER`. Start, end and
ending a session belong to the `QZLSSERVER` job and answer `CPFB680`
until it lands (`BLOCKED(rel)` in `lic/slic/src/stgsvc/netsvr.rs`). A
reset applies the pending values only after the server control accepts
the start.

`NEEDS-CODY` (change APIs are never called on the oracles): the exact
`CPFB68D` bounds of `QZLSCHSI`
(`CALL QZLSCHSI (x'00' 1 'ZLSS0100' errc)` shows them without changing
anything only if IBM checks the length first), and IBM's checks of
`QZLSADFS` on a share name with `-` and `_` (above).

## 9. Protocol coverage (built: the library's)

| Feature | QS/400 |
|---|---|
| Dialects | 2.0.2, 2.1, 3.0, 3.0.2, 3.1.1, at least the minimum dialect attribute (a library patch; a NEGOTIATE with only lower ones gets `STATUS_NOT_SUPPORTED`); TCP 445 direct; 139 after the NetBIOS session request (`nbss`), keepalives skipped (a library patch) |
| SMB1 | an SMB1 `NEGOTIATE` that offers `SMB 2.???` or `SMB 2.002` (the latter only when the minimum is 2.0.2) is answered with an SMB2 `NEGOTIATE` response (the multi-protocol upgrade); any other SMB1 negotiate closes the connection without a reply |
| 3.1.1 | preauthentication integrity (SHA-512), encryption and signing capabilities; `CipherCount` 0 is `STATUS_INVALID_PARAMETER`, as Windows answers; compression and RDMA transform contexts ignored (not offered) |
| Signing | HMAC-SHA256 (2.x), AES-128-CMAC (3.x; the 3.1.1 answer whatever the client lists; AES-GMAC not offered); required for every session when the signing attribute says so |
| Encryption | 3.1.1: AES-128-CCM, AES-128-GCM, AES-256-CCM, AES-256-GCM; 3.0 and 3.0.2: AES-128-CCM; per share or for every session; clients that cannot encrypt are refused a tree on an encrypted share |
| Credits, compounding, large MTU | yes; READ and WRITE up to the configured size (64 KiB to 8 MiB, 1 MiB by default) |
| Leases | R, RH, RW, RWH on files (2.1+); no directory leases and no oplocks; broken only by opens in the same job (section 6) |
| Durable handles | v1 and v2 with a handle-caching lease; reconnects routed back to the job (section 3) |
| Resilient and persistent handles, multichannel, RDMA, DFS, compression | refused (section 10) |
| Change notify | yes, for changes through the job's own connections |
| Server-side copy | COPYCHUNK and COPYCHUNK_WRITE, done by reads and writes in the job; no block cloning |
| Sign-on | NTLMv2 in SPNEGO through `signon::SignOn` (a library patch: the NT hash stays in the storage service); the guest profile gets an `IS_GUEST` session with no key (unsigned, unencrypted; a client that requires signing gets no guest session); the library's logon throttle off |
| Limits and timeouts | the library's: 64 sessions per connection, 128 trees per session, 16 384 opens per connection, 1024 locks per open; handshake 30 s, idle as the attribute (15 minutes by default), I/O 60 s, TCP keepalive 60 s |
| RPC | `srvsvc`: NetrShareEnum, NetrShareGetInfo, NetrServerGetInfo (100, 101 with the server text); `spoolss` (section 7); everything else `nca_s_op_rng_error`; no `wkssvc` |

## 10. Refused, with reasons

| Refused | Why | What the client sees |
|---|---|---|
| SMB1/CIFS | removed by IBM too; insecure | the connection is closed without a reply (an SMB1 negotiate offering SMB2 dialects is upgraded) |
| Kerberos, EIM | QS/400 has no Kerberos (`RELEASE_NOTES_V1R1M0.md`) | SPNEGO offers NTLMSSP only |
| NTLMv1, LM | weak | `STATUS_LOGON_FAILURE` |
| Multichannel | not built | no `MULTI_CHANNEL` capability; a binding `SESSION_SETUP` gets `STATUS_REQUEST_NOT_ACCEPTED` |
| WINS | no WINS client | the attribute is refused by `QZLSCHSI` |
| DFS | no DFS | no `DFS` capability; referrals `STATUS_FS_DRIVER_REQUIRED` |
| Quotas | the IFS has none | `STATUS_NOT_SUPPORTED` |
| Alternate data streams | the IFS has none | no `FILE_NAMED_STREAMS`; `name:stream` is `STATUS_OBJECT_NAME_INVALID` |
| Setting security descriptors | authority is `CHGAUT`'s | `STATUS_ACCESS_DENIED` |
| `spoolss` administration: `RpcAddPrinter`(Ex), `RpcSetPrinter`, `RpcDeletePrinter`, `RpcAddPrinterDriver`(Ex), `RpcDeletePrinterDriver`(Ex), print processors, forms, ports, monitors, printer data and keys, per-machine connections, `RpcXcvData` (opnums 5, 6, 7, 9, 13, 14, 27, 30, 31, 33, 39, 46, 47, 48, 61, 70, 71, 73, 77, 81, 82, 84, 85, 86, 88, 89) | print shares are configured with `QZLSADPS` and Navigator | a fault with status 5, `ERROR_ACCESS_DENIED` |
| Opening a printer for administration (`PRINTER_ACCESS_ADMINISTER`, `DELETE`, `WRITE_DAC`, `WRITE_OWNER`, `GENERIC_ALL`, `SERVER_ACCESS_ADMINISTER`) | as above | `ERROR_ACCESS_DENIED` |
| `spoolss` change notifications (opnums 62, 65) | the server would have to call back into the client's spooler | `ERROR_NOT_SUPPORTED`; the client polls |
| `RpcSetJob`, `RpcScheduleJob`, `RpcResetPrinter` (2, 25, 52) | jobs are held, released and ended on the output queue (`WRKSPLF`, Navigator) | `ERROR_NOT_SUPPORTED` |
| Every other `spoolss` operation (`RpcAddJob`, `RpcReadPrinter`, `RpcEnumPrinterDrivers`, `RpcGetPrinterDriverDirectory`, ports and monitors lists, ICs, `RpcFlushPrinter`, bidi, `RpcGetCorePrinterDrivers`, `RpcGetPrinterDriverPackagePath`, named job properties, IPP, branch office) | not needed to connect a printer and print | a fault `nca_op_rng_error` (`RPC_S_PROCNUM_OUT_OF_RANGE`); Windows falls back from the package-aware calls |
| Data types other than `RAW` (EMF, XPS, TEXT) | the output queue takes the printer's own data | `ERROR_INVALID_DATATYPE`; printers are `PRINTER_ATTRIBUTE_RAW_ONLY` |
| A document's output file | the job goes to the output queue | `ERROR_INVALID_PARAMETER` |
| `,Job n`, `,XcvPort`, `,XcvMonitor` printer names | jobs are not opened by handle; ports and monitors are administration | `ERROR_INVALID_PRINTER_NAME` |
| Resilient handles (`FSCTL_LMR_REQUEST_RESILIENCY`), persistent handles | the library has none | `STATUS_NOT_SUPPORTED`; the PERSISTENT flag of a DH2Q is ignored and the open is durable only |
| Compression, RDMA | the library has none | not offered in NEGOTIATE |
| Oplocks, directory leases | the library has none | an oplock request gets no oplock |
| Extended attributes | off on every share | no `FILE_SUPPORTS_EXTENDED_ATTRIBUTES`; `ExtA` contexts `STATUS_EAS_NOT_SUPPORTED` |
| Reparse points, object IDs, 8.3 short names, snapshots | the IFS has none | `STATUS_NOT_SUPPORTED`; no short names in listings; no previous versions |
| `wkssvc` and other named pipes | not built | `STATUS_OBJECT_NAME_NOT_FOUND` on the pipe |
| `srvsvc` over NDR64 | the library binds NDR 2.0 only (Windows binds NDR 2.0) | the NDR64 context is rejected in the bind ack |
| The NetBIOS name service (UDP 137) and datagram service (138), browsing | not built | no answer on 137/138; clients connect by DNS name or address, or `*SMBSERVER` on 139 |
| Per-share maximum users, CCSID and text conversion | not built in V1R2M0: one job per client cannot count other jobs' users, and the backend serves data as the IFS stores it | `QZLSADFS`/`QZLSCHFS` refuse a value other than the default |
| A second profile in one job | every IFS call runs as the job's profile | `STATUS_LOGON_FAILURE` (`OtherProfile` logged) |
| The library's logon throttle | `QMAXSIGN` is counted in the storage service | off |
| Leases broken by other jobs | the IFS has no change events and the lease table is per job | none; turn leases off on shares several clients change |
| WINS servers, scope, WINS proxy | no WINS client | `QZLSCHSI` `CPFB686` reason 6 or 7, return code 3440 |
| Kerberos authentication method (`1`, `2`) | no Kerberos | `QZLSCHSI` `CPFB686` reason 15 |
| Logon server role, domain logon | no domain logons | `QZLSCHSI` `CPFB686` reason 13 |
| Administrative alerts (minimum severity not -1) | SMB2 has no messenger service | `QZLSCHSI` `CPFB686` reason 17 |
| LAN Manager authentication | weak | `QZLSCHSI` `CPFB686` reason 18 |
| Publishing print shares | no directory server | the share is kept unpublished; `CPFB682` reason 6 |
| Home (temporary) shares | not built | none are made; `CPIB688` is never sent |
| `CPDB680` diagnostics per user from `ZLSS0200` | the API answers over the link with one error | only `CPFB694` with the count |

## 11. Tests

- `cargo test -p netserver`: the backend, sign-on, print shares, the
  NetBIOS session request and routing end to end: SMB clients (the
  library's own test helpers, `net/netserver/tests/smbc/`) against
  `NetServer` jobs over one IFS in memory through handed-over loopback
  sockets: files round trip, case-insensitive lookup, rename and delete,
  DOS attributes and file IDs, share modes and byte-range locks between
  two jobs, a read-only share and the IFS authority, sign-on refusals and
  one profile per job, a print share spooling as the profile,
  `/QSYS.LIB` listings and member reads, a durable reconnect into the
  same job and the job's release, port 139.
- `cd net/smb-server; cargo test --test qs400_hooks --test
  qs400_patches`: the library patches.

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
- `tools/smbtest` (the test client and its scenarios, below).
- QEMU steps from 166: the server started, every `smbtest` scenario
  against the guest, the APIs, the SQL view, a 1 GB copy (MB/s here).
- `cd net/smb-server; cargo test --lib spoolss` and `cargo test --test
  qs400_spoolss`: the spoolss calls, their NDR and INFO structures, forms,
  refusals; OpenPrinterEx, GetPrinter 2, GetPrinterDriver2, StartDoc, a
  20 000-byte WritePrinter in fragments, EndDoc and ClosePrinter over a
  connection, the bytes arriving at a test print service.
- `cargo test -p netserver --test shares`: a spoolss document lands on
  the print share's queue as the profile, as a CREATE does; `print$`
  serves the driver store read-only and GetPrinterDriver2 names its
  files; no store, no `print$`. `cargo test -p netserver --lib`: the
  driver manifests.
- Fuzz targets: the SMB2 header and commands, NTLMSSP, SPNEGO, the
  NetBIOS session service and name service, DCE/RPC, the spoolss request
  decoder (`smb_server::fuzzing::spoolss_request`).
- `NEEDS-CODY`: Windows 11, macOS and a Linux kernel client map a share,
  open and save with Notepad, Word and Excel, print to a print share; a
  wrong password `QMAXSIGN` times disables the user for NetServer.
- `NEEDS-CODY`: point and print from Windows 11: Add Printer by
  `\\SERVER\SHARE` on a share with the "Microsoft PCL6 Class Driver"
  connects with no download, prints a test page and an Office document
  into the share's output queue, and the printer's queue window shows the
  user's jobs.

### The test client (`tools/smbtest`)

The client is the published pure-Rust `smb` crate 0.12.1 (afiffon/smb-rs,
MIT), other authors than the vendored server library, so one misreading of
[MS-SMB2] cannot hide in both; `smbtest` does not depend on
`net/netserver`. The crate sits **outside the workspace with its own
`Cargo.lock`** (root `Cargo.toml` `exclude`): `smb` pulls `sspi` 0.21.3,
which pins `curve25519-dalek` `=5.0.0-rc.1` for macOS targets, and that
cannot share a lockfile with `tools/build`'s `ed25519-dalek` 3.0.0.
`cargo run -p build -- deny` checks its graph as well as the workspace's.

```
cargo test  --manifest-path tools/smbtest/Cargo.toml        # unit tests and tests/loopback.rs
cargo clippy --manifest-path tools/smbtest/Cargo.toml --all-targets
cargo run   --manifest-path tools/smbtest/Cargo.toml -- HOST PORT USER PASSWORD SHARE SCRATCH \
    [--only A,B] [--copy-bytes N[K|M|G]] [--large-bytes N] [--print-share NAME] [--no-bad-signon] [--list]
cargo run   --manifest-path tools/smbtest/Cargo.toml -- probe HOST PORT   # NEGOTIATE only, no sign-on
```

`PASSWORD` `-` reads `SMBTEST_PASSWORD`. Each scenario prints `ok NAME`
with indented detail lines, `fail NAME: why`, or `skip NAME: why` (the
client crate cannot express it; never faked). The exit status is 1 when
any scenario failed. Every writing scenario works under a run directory
`SCRATCH\smbtest-PID-MILLIS` that `cleanup` removes.

| Scenario | What it proves |
|---|---|
| `negotiate-2.0.2` .. `negotiate-3.1.1` | each dialect offered alone is chosen; signing algorithm and maximum sizes printed |
| `negotiate-multiprotocol` | the SMB1 negotiate with the SMB2 dialect strings is upgraded to 3.1.1 |
| `signon-good`, `signon-wrong-password`, `signon-unknown-user` | NTLMv2 over SPNEGO; a wrong password (once) and an unknown user are both `STATUS_LOGON_FAILURE` (`--no-bad-signon` leaves the two out) |
| `share-enum` | `NetShareEnumAll` over `IPC$` lists the share (and `--print-share`) |
| `signing` | a signed round trip on every dialect: HMAC-SHA256 on 2.x, AES-CMAC on 3.x (the client signs every request and checks every response) |
| `encrypt-3.0`, `encrypt-3.0.2`, `encrypt-3.1.1` | encryption required by the client: a sealed round trip; the cipher (AES-128-CCM implied on 3.0 and 3.0.2, the server's choice on 3.1.1) |
| `file-round-trip`, `dir-list`, `rename`, `delete` | text and every byte value through a new open; a listing of mixed-case names and a directory; rename on an open handle (old name `STATUS_OBJECT_NAME_NOT_FOUND`); delete on close |
| `change-notify` | a watch from one connection sees `Added` for a file another connection creates |
| `large-io` | several requests at the negotiated maximum read and write sizes, checked |
| `copy` | `--copy-bytes` (64 MiB by default) written and read with 8 requests of up to 1 MiB in flight; `in:` and `out:` in MB/s (10^6 bytes) |

What smb 0.12.1 cannot express, printed as `skip`: byte-range locks
(`lock-conflict`: no LOCK request), leases and oplocks (`lease-break`,
`oplock-break`: every CREATE asks for none), one cipher at a time
(`encrypt-each-cipher`: all four are offered together), print shares
(`print-write`: the crate panics opening one). `share-enum` is also a
`skip` against a server that, like the vendored library, accepts only NDR
2.0 for `srvsvc`: the crate binds with NDR64 only (Windows, macOS and Samba
clients offer NDR 2.0). These are covered by `tests/interop.rs`.

Facts the client found:

- With its encryption disabled, the crate sends 3.1.1 an
  `SMB2_ENCRYPTION_CAPABILITIES` context with `CipherCount` 0. Windows'
  own server (`tools/conformance/oracle/netserver/windows-negotiate.txt`,
  NEGOTIATE only) and the vendored server both answer
  `STATUS_INVALID_PARAMETER`, so the scenarios leave encryption at the
  server's choice except in `encrypt-*`; `tests/loopback.rs` holds the
  server to Windows' answer.
- Windows gives 2.0.2 64 KiB maximum sizes and later dialects 8 MiB; the
  vendored server gives 1 MiB on every dialect.

`tests/loopback.rs` runs every scenario against the vendored server in the
test process on 127.0.0.1, over a temporary directory, with a
`LogonVerifier` checking NTLMv2 as the storage service will and signing
required; a second server requires encryption (2.1 is refused, 3.x sealed
round trips pass); the probe answers as Windows does; bad sign-ons are
`STATUS_LOGON_FAILURE`.

`tests/interop.rs` (`#[ignore]`, Linux): Samba's `smbclient` (`-L` over
NDR 2.0, a put, get, rename, list and delete on every dialect signed and
sealed, `print` to `SMBTEST_PRINT_SHARE`, a wrong password) and the kernel
CIFS client (two `nosharesock` mounts per dialect: one mount's writes,
renames and deletes seen from the other; `flock` on one mount refused on
the other, which the kernel sends as an SMB2 LOCK). WSL's Ubuntu has none
of it; `NEEDS-CODY`: `sudo apt install smbclient cifs-utils` and `sudo
modprobe cifs`, then

```
SMBTEST_HOST=127.0.0.1 SMBTEST_PORT=4450 SMBTEST_USER=NSUSER SMBTEST_PASSWORD=... \
SMBTEST_SHARE=SMBTEST SMBTEST_SCRATCH=smbtest [SMBTEST_PRINT_SHARE=PRT01] \
cargo test --manifest-path tools/smbtest/Cargo.toml --test interop -- --ignored --test-threads 1
```

(the kernel tests mount, so as root or with `sudo -n`).

QEMU steps 166-168 (`tools/conformance/src/netsvrstep.rs`) are
registered only with `QS400_NETSVR_STEPS=1` until `QZLSSERVER` and
`QZLSFILE` exist: 166 `STRTCPSVR SERVER(*NETSVR)` and both jobs in
`WRKACTJOB` (`BCH`, `PJ`); 167 a user `NSUSER`, a read/write share
`SMBTEST` over `/home/NSUSER` added with `QZLSADFS` from the console,
and every scenario through a forward to the guest's 445 (16 MiB copy);
168 `--only copy,cleanup --copy-bytes 1G`, MB/s in and out in the step's
detail. They run the `smbtest` binary from `QS400_SMBTEST`, or build it
(`cargo build --release --locked --manifest-path tools/smbtest/Cargo.toml`
into `$QS400_TARGET_DIR/smbtest`).

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
