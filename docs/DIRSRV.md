# Directory Server: the LDAP server and client

QS/400's directory server is an LDAPv3 server in the base operating
system (IBM: IBM Tivoli Directory Server for i, option 3 of 5770SS1,
"Extended Base Directory Support"), started with `STRTCPSVR
SERVER(*DIRSRV)`, with the LDAP client API that ILE programs bind to
(`QGLDCLNT`), the QSH tools, and publishing of the system's user
profiles. Everything is written from the RFCs (4510-4519, 4522-4533,
2849, 2696, 2891, 3296, 3062, 4370, 4532, 3876, 4527, 4528) and IBM's
public documentation; no IBM text, header or schema description is
reproduced. It targets V1R2M0.

The oracle facts are in `tools/conformance/oracle/dirsrv/README.md`.
Unlike NFS, the primary runs IBM's server, so its wire answers can be
diffed, anonymously. Section numbers below are what the code and the
agents' commits cite.

## 1. What the oracles settled

| Question | Answer (primary V7R1M0, IBM Directory Server 6.1) |
|---|---|
| The server's job | named after the instance: `QUSRDIR`, user `QDIRSRV`, batch, in `QSYSWRK`, job description `QSYS/QDIRSRV` |
| Instances | `/QIBM/UserData/OS400/DirSrv/idsinstinfo/idsinstances.ldif`; the shipped instance is `QUSRDIR`, autostart 1 |
| Configuration | `/QIBM/UserData/OS400/DirSrv/idsslapd-QUSRDIR/etc/ibmslapd.conf`, LDIF, CCSID 1208, `cn=Configuration` and children |
| Administrator | `ibm-slapdAdminDN: cn=Administrator` (not `cn=root`) |
| Commands | `CHGDIRSVRA INSTANCE DN((dn *ADMINDN *UPDATEDN) password)` only; `STRTCPSVR`/`ENDTCPSVR SERVER(*DIRSRV) INSTANCE(*DFT *ALL *AUTOSTART name)` |
| Publishing | jobs `QGLDPUBE` and `QGLDPUBA` in `QSYSWRK` as `QDIRSRV`; `QGLDPUBE` is not a command |
| Suffixes | `cn=schema`, `cn=localhost`, `cn=ibmpolicies`, `dc=...` (user), `os400-sys=SYSTEM` (projected); `cn=configuration`; no `cn=changelog` configured |
| Projected backend | `os400-sys=SYSTEM`, class `os400-root`; anonymous users see only that entry |
| SASL | advertised `DIGEST-MD5`, `OS400-PRFTKN` (plug-ins for `CRAM-MD5`, `GSSAPI` configured) |
| Extensions | StartTLS and IBM's set; **no** whoami, password modify, pre/post read, assertion |
| StartTLS, no certificate | protocolError (2) |
| Messages | `GLDxxxx` in `QSYS/QGLDMSG` (481 at 7.1) |
| SQL | none: no `QSYS2` service covers LDAP at 7.5 |
| Function usage | `QIBM_DIRSRV_ADMIN` |
| Client | `QSYS/QGLDCLNT` (205 exports at 7.5); `QSYSINC/H` `LDAP`, `LBER`, `LDAPSSL`; no RPG copy member |
| QSH | `ldapsearch`, `ldapadd`, `ldapmodify`, `ldapdelete`, `ldapmodrdn`, `ldapchangepwd`, `ldapexop`, `ldapcompare`, `ldapdiff` |

Where the handoff guessed otherwise, the oracle wins: the administrator
is `cn=Administrator`; the job is `QUSRDIR`; there is no `CHGLDAPA`,
`QGLDPUBE` command, `ldap.conf` or `QSYS2.LDAP_*` service, so QS/400 has
none either. Where the handoff asks for more than IBM 6.1 offers
(whoami, password modify, pre/post read, assertion, the change log),
QS/400 implements it and says so in the root DSE; each is a documented
difference (§15).

## 2. Pieces and where they live

| Piece | Where | What |
|---|---|---|
| The protocol | `net/ldap` | BER (`ber`), LDAPv3 messages (`msg`), DNs (`dn`), filters (`filter`), controls (`control`), extended operations (`ext`), LDIF (`ldif`), LDAP URLs (`url`), schema descriptions (`schema`), a blocking client (`client`), result codes (`code`). No store, no job; the server, the client API, the QSH tools and the publisher all use it |
| The server's logic | `net/dirsrv` | the schema model and matching rules (`schema`), the entry store over the object store (`store`), access control (`acl`), the backends (`backend/btree`, `backend/projected`, `backend/config`, `backend/schema`, `backend/monitor`, `backend/changelog`), operations (`ops`), password policy and storage schemes (`pwd`), the configuration file (`config`), publishing (`publish`), the connection loop (`server`) |
| The service's side | `lic/slic/src/stgsvc/dirsrv/` | the instances' stores and backends behind `stg` requests 1560-1579, the projected backend over the profiles, binds against `sec` |
| The job | `work/qsjob/src/work/dirsrv/` | the instance job (`QUSRDIR`): listeners on 389 and 636, a thread per connection, TLS, requests to the service; the publishing jobs `QGLDPUBE`, `QGLDPUBA` |
| Commands | `command/sys-cpps/cmds/CHGDIRSVRA.cmd`, `STRTCPSVR.cmd`, `ENDTCPSVR.cmd`; `lic/slic/src/cpp/dircmd.rs` | `CHGDIRSVRA`, `*DIRSRV` with `INSTANCE` (§2.2) |
| Messages | `runtime/msgf/msgs/qgldmsg.toml` | `QSYS/QGLDMSG`, IBM's IDs, severities and formats, Quartz text |
| The client API | `runtime/ldapclnt` | `QGLDCLNT`'s exports as bound procedures over `net/ldap::client`; QS/400's own `QSYSINC/H(LDAP, LBER, LDAPSSL)` and `QRPGLESRC(LDAP)` |
| QSH tools | `qsh/qsh-utils/src/net/ldap/` | the nine tools over `net/ldap::client` |
| Test client | `tools/ldaptest` | outside the workspace with its own lockfile, wire code by other authors than `net/ldap` (§13) |
| Navigator, QAP | `net/dirsrv/src/admin/` (`DirAdmin`, the interface both use, its `ibmslapd.conf` side `conf`, `MemAdmin` in memory), `work/stg/src/diradmin.rs` (request 1568), `net/navigator/src/routes/dirsrv/`, `work/qap-proto` version 12 (`Dirsrv`, service 14), `net/qapd/src/dirsrv.rs` | instances, server properties, suffixes, indexes, the administrator, schema, access, publishing, counters |

The server runs as the job; the data lives in the storage service. Every
operation is a request from the job to the service (§4), as the NFS
daemons' file operations are.

### 2.1 Messages

`QSYS/QGLDMSG` (`runtime/msgf/msgs/qgldmsg.toml`, installed with the
other system message files and added to an older system's at its next
IPL) has every identifier of the primary's (481, `GLD0101` to
`GLD0956`), with IBM's severities and substitution layouts
(`messages.txt`); a `*QTDCHAR` or `*HEX` with `*VARY` is `*CHAR *VARY`
there (the same data; the frozen `*MSGF` payload has no other form).
IBM's text was not kept, so an identifier QS/400 does not send has a
plain text by severity and a help naming its data; the 7.5 additions
(`messages-pub400.txt`) are left out until QS/400 sends one. The ones
QS/400 sends, chosen by their layouts (an identifier QS/400 sends with
no IBM text known keeps IBM's severity and data layout, the text
Quartz's):

| ID | Severity, data | When |
|---|---|---|
| `GLD010B` | 0, `*CHAR 10` | the publishing agents `QGLDPUBE` and `QGLDPUBA` end at once (publishing is not built yet) |
| `GLD0122` | 40, `*CHAR 8` | a request that needs the instance's job (`dir_conns`; `dir_admin`'s counters and entry access): the instance is not active |
| `GLD0123` | 0, `*CHAR 8` | `CHGDIRSVRA` changed the instance (completion) |
| `GLD0126` | 40, two `*CHAR *VARY` | an attribute (`name: value`) or entry of `ibmslapd.conf` QS/400 does not honour, not at IBM's default (§10), and the file |
| `GLD0127` | 40, `*CHAR *VARY` | a configuration file missing or not LDIF |
| `GLD0128` | 40, none | the service has no object store for a directory request |
| `GLD014B` | 40, `*CHAR *VARY` | a session number the job does not hold (1561-1567) |
| `GLD014D` | 40, `*CHAR *VARY` | the instance's `*DIRDB` could not be opened, made or changed: the store's reason |
| `GLD0166` | 40, `*CHAR *VARY` | `dir_admin`: no entry with the DN |
| `GLD0167` | 40, `*CHAR *VARY` | `dir_admin`: a schema definition refused, and why |
| `GLD0168` | 40, `*CHAR *VARY` | `dir_admin`: an index of an attribute type the schema lacks |
| `GLD016E` | 40, none | `dir_admin`: an entry's own ACL or owners with no value |
| `GLD0201` | 40, `*CHAR 8` | `CHGDIRSVRA INSTANCE`: no such instance |
| `GLD0202` | 40, none | `CHGDIRSVRA DN` without a password |
| `GLD0203` | 40, `*CHAR 32` | `STRTCPSVR`/`ENDTCPSVR SERVER(*DIRSRV) INSTANCE`: no such instance |
| `GLD0204` | 40, none | `dir_admin`: a publishing agent's port not 1 to 65535 |
| `GLD0215` | 40, `*CHAR 8` | `CHGDIRSVRA DN(*UPDATEDN)`: the instance names no update DN |
| `GLD0224` | 40, `*CHAR *VARY` | `CHGDIRSVRA DN`: not a distinguished name |
| `GLD0232` | 40, two `*CHAR *VARY` | a value of an honoured attribute QS/400 does not take (`ibm-slapdPwEncryption: aes256`, `ibm-slapdSchemaCheck: V2`) |
| `GLD0239` | 40, `*CHAR *VARY` | `dir_admin`: a publishing agent's server not a host name or address |
| `GLD0244` | 40, `*CHAR 10` | a directory request (1560-1567) not from an instance's job as `QDIRSRV` (the user) |
| `GLD0246` | 40, `*CHAR 8`, two `*BIN 4` | the instance job could not listen on `ibm-slapdPort` (the instance, the port, the errno); the job ends |
| `GLD0259` | 40, `*CHAR *VARY` | a request that does not decode as an LDAP message, and why |
| `GLD0265` | 0, two `*CHAR 10` | the instance job ended (the instance) |
| `GLD0268` | 40, `*CHAR 8` | the instance job could not open its instance in the service (after the service's own message) |
| `GLD0269` | 0, `*CHAR 8`, `*CHAR 10` | the instance job started, and its ports |

`QTCPMSG` gains `TCP1617` (30), `STRTCPSVR` and `ENDTCPSVR`'s dependency
message for `INSTANCE` with `SERVER(*ALL)` (or `*AUTOSTART`).

### 2.2 Commands

- **`CHGDIRSVRA`** is the primary's definition attribute for attribute
  (`command/sys-cpps/tests/dirsrv_prompts.rs`), shipped `*PUBLIC
  *EXCLUDE` (pub400 refuses it), processing program `QGLDCHGATR` in the
  service (`cpp/dircmd.rs`); `*ALLOBJ` and `*IOSYSCFG` (`CPF222E`).
  `INSTANCE` is looked up in the instance list (`GLD0201`). `DN(*SAME)`
  changes nothing. Otherwise the second element is the new password
  (`GLD0202` when blank), stored as `{SSHA512}` (`dirsrv::pwd`):
  `DN(*ADMINDN pw)` sets `ibm-slapdAdminPW` and keeps the administrator;
  `DN(name pw)` sets `ibm-slapdAdminDN` to the name (`GLD0224` when it is
  not a DN) and its password; `DN(*UPDATEDN pw)` sets
  `ibm-slapdMasterPW` of `cn=Master Server, cn=Configuration`, and
  `GLD0215` when that entry names no `ibm-slapdMasterDN` (the shipped
  file names none: replication is refused, §14). The file is rewritten
  with its order and comments kept (`dirsrv::config::edit`), `GLD0123`.
- **`STRTCPSVR`/`ENDTCPSVR`**: `*DIRSRV` among `SERVER`'s values;
  `INSTANCE` is IBM's (on `STRTCPSVR` the group of server instance and
  instance startup values with the single values `*DFT`, `*ALL`,
  `*AUTOSTART`, prompted unless `SERVER` is `*ALL` or `*AUTOSTART`; on
  `ENDTCPSVR` a name or `*DFT`/`*ALL`, prompted unless `SERVER(*ALL)`),
  `TCP1617` the dependency, `MSGF(QTCPMSG)` as IBM's. `RESTART`,
  `HTTPSVR` (and `ENDTCPSVR`'s QS/400 `OPTION` and `DELAY`) are prompted
  for `*HTTP` (`RESTART` also `*DNS`, `*QOS`) as IBM's are, so the
  `*DIRSRV` prompts are the primary's character for character
  (`tools/conformance/golden/dirsrv/`). The prompter learned two of
  IBM's rules for it: a list of values shows `+ for more values` at
  column 17 with a field as wide as a value's, where the next value can
  be typed, and a group whose default is a single value shows it in the
  first element with the others blank; a choice text not one item of
  which fits is not shown.

## 3. The protocol crate (`net/ldap`)

- `ber`: definite-length BER as LDAP uses it (RFC 4511 §5.1): a reader
  over a byte slice with a depth limit (32) and a length limit (the
  caller's, 16 MiB by default), a writer; tags, lengths in short and
  long form, BOOLEAN, INTEGER (to i64), ENUMERATED, OCTET STRING, NULL,
  SEQUENCE, SET, context and application tags. Indefinite lengths,
  constructed strings and non-minimal lengths are refused on input.
- `msg`: `LdapMessage { id, op, controls }` and every `protocolOp`
  (RFC 4511 §4): bind, unbind, search request, entry, reference, done,
  modify, add, delete, modify DN, compare, abandon, extended request and
  response, intermediate response. Encode and decode, both directions,
  so the server, the client and the tests share one codec.
- `dn`: RFC 4514 strings to `Dn` (RDNs of attribute type and value,
  multi-valued RDNs, hex and escaped values, OIDs as types) and back;
  `Dn::normalize` against the schema's matching rules is `dirsrv`'s.
  IBM's case and spacing (`OS400-SYS=...`, `dc=a, dc=b`) parse.
- `filter`: RFC 4515 strings to `Filter` and the BER form both ways;
  extensible match; a depth limit (64). A filter's own nesting is
  counted against this limit, not against BER's 32, so a search request
  may carry a filter 64 deep.
- `control`: the controls of §7 as typed values with their OIDs.
- `ext`: the extended operations of §7 (request and response values).
- `ldif`: RFC 2849 content and change records, folding, base64, `< url`
  values from a file the caller resolves, `version: 1`; IBM's
  configuration files are LDIF and parse with it.
- `url`: RFC 4516.
- `schema`: RFC 4512 §4.1 descriptions (attribute types, object
  classes, syntaxes, matching rules, matching rule uses, DIT content
  rules, name forms) and IBM's `IBMAttributeTypes` extension values,
  parsed and printed.
- `client`: a blocking client over a `TcpStream` with `net/tls`
  (LDAPS and StartTLS), synchronous and asynchronous operations by
  message ID, controls, paging and sorting helpers, referral chasing
  off by default (IBM's `ldap_set_option(LDAP_OPT_REFERRALS)`), a size
  and time limit, and the rebind hook the C API needs.
- `code`: the result codes of RFC 4511 Appendix A with IBM's additions
  IBM documents (`ldap_err2string` text is Quartz's).

## 4. The server's shape

**Instances.** `idsinstances.ldif` lists the instances (name, location,
autostart, version). QS/400 ships `QUSRDIR` as IBM does. Each instance
has its configuration file, its job and its store (§5). `STRTCPSVR
SERVER(*DIRSRV) INSTANCE(*DFT)` starts `QUSRDIR`; `*AUTOSTART` starts
those with `ids-instanceAutostart: 1`; `*ALL` all; a name that one.
`ENDTCPSVR` the same. `STRTCPSVR SERVER(*AUTOSTART)` at IPL starts the
autostart instances when `*DIRSRV` is an autostart server.

`net/servers` registers `*DIRSRV` (`registry/table.rs`): the job
`QUSRDIR` as `QDIRSRV` in `QSYSWRK` (another instance's job has its
name), ports 389 and 636 (TLS), `QS400_DIRECTORY_SERVER`, readiness
`Built`. `framework::dirsrv::DirSrv` resolves `INSTANCE` against the
job's instance list (read by `work/qsjob/src/command/tcpsvr.rs` through
`dirsrv::config`; `GLD0203` for an unknown name) and starts one job per
instance not already running (`TCP1A07` for each, `TCP1A08` for one
running): named after the instance, user `QDIRSRV`, job description
`QSYS/QDIRSRV`, role `DIRSRV`, request data `INSTANCE(name)`. `ENDTCPSVR`
ends the running ones it names (`TCP1A0A`; `TCP1A09` when none runs);
`ENDTCP` ends every one. `STRTCP` (`*AUTOSTART`) starts the instances with
`ids-instanceAutostart: 1` only when `*DIRSRV` autostarts in the TCP/IP
server table, where it is off as shipped. `STRTCPSVR`'s instance startup
values (`INSTANCE`'s second element) are refused for `*DIRSRV` (§14).

**Objects.** The profile `QDIRSRV` (`*USER`, no special authorities,
password `*NONE`, UID 4294770712, `sec::profile::shipped`) and the
function usage `QIBM_DIRSRV_ADMIN` (`sec::fcnusg`) were already shipped.
V1R2M0 adds the job descriptions `QSYS/QDIRSRV` (the instance jobs),
`QSYS/QGLDPUBE` and `QSYS/QGLDPUBA` (routing data of their names),
`QSYS/QDIRJOBD` (`QCMDB`), all as `QDIRSRV` on `QSYSNOMAX`; in
`QSYSWRK` the autostart job entries `QGLDPUBE` and `QGLDPUBA` and the
routing entries 2140 and 2141 that run programs `QGLDPUBE` and
`QGLDPUBA` (the sequence numbers are QS/400's: the primary's were not
captured), which log `GLD010B` and end (publishing is not built yet,
§10.1); and the library `QUSRDIRDB` (blank text, as the primary's),
made at each IPL when missing (`lic/slic/src/stgsvc/dirconf.rs`). A
V1R1M0 system gets the job descriptions and library at its first
V1R2M0 IPL and the `QSYSWRK` entries through `wm::obj::upgrade`
(marker `QSYS/QGLDPUBE`), as NFS's entries came.

**The job** (`work/qsjob/src/work/dirsrv/instance.rs` over
`net/dirsrv::server`): one job per instance, named after it, user
`QDIRSRV`, job description `QSYS/QDIRSRV`, in `QSYSWRK`. The loop is
`dirsrv::server`, behind a `Service` trait of the eight requests so the
tests run it against a directory in memory (`net/dirsrv/tests/server.rs`);
the job's part is the link, the certificate and the job log.

- *Start.* The job opens (and closes) a session to read its settings
  (`dir_open`: ports, `ibm-slapdSecurity`, the idle and write time-outs,
  the application ID, the message limit); a refusal is the service's
  message and `GLD0268` in the job log, and the job ends. It listens on
  `ibm-slapdPort` (389) on every interface; a port it cannot bind is
  `GLD0246` and the job ends. When a certificate is assigned to
  `QIBM_GLD_DIRSRV_SERVER` (QS/400's ID `QS400_DIRECTORY_SERVER`,
  `net/certs/src/apps.rs`; the `*SYSTEM` store through `cert_app`, as
  every server's) it also listens on `ibm-slapdSecurePort` (636, TLS
  from the first byte) when `ibm-slapdSecurity` is `SSL` or `SSLTLS`, and
  takes StartTLS on 389 when it is `TLS` or `SSLTLS`. Otherwise StartTLS
  is protocolError (2), as IBM's server answers it without a certificate
  (the primary: `none`, no certificate); a value asking for TLS without a
  certificate is `TCP1A63` to `QSYSOPR` and the job log, and the job runs
  in the clear. `GLD0269` names the ports.
- *Connections.* A thread per connection, with a link of its own to the
  service (`hello_thread`, as the NFS daemons' threads). Messages are cut
  from the stream with `ldap::ber::frame_len` (16 MiB, or the
  settings' limit) and decoded with `ldap::msg`; one that does not
  frame or decode, or is not a request, ends the connection with a
  notice of disconnection (protocolError). The service gets each
  request's bytes: `dir_bind` with what the job knows of the connection
  (client address, TLS, the TLS client certificate's subject for SASL
  EXTERNAL), `dir_search` for a search, `dir_op` for the rest, and the
  job writes the answer's messages as they are. A bind first closes the
  connection's paused and persistent searches. A request the service
  refuses is answered with other (80) and the service's text.
- *Searches* stream: `dir_search` then `dir_next` with a budget (100
  entries, 256 KiB) until the cursor is done, paused (paged results:
  the client's next page is a new `dir_search`) or watching. Between
  batches the job reads what the client sent without waiting, so an
  abandon of the search (or an unbind) stops it at once with
  `dir_close` of its message ID and no done; an abandon of a paused or
  persistent search closes it the same way. Size and time limits are
  the service's (the cursor's deadline).
- *Persistent searches.* While a connection has any, its thread asks
  `dir_changes` every 500 ms (at most 100 entries) between reads and
  writes the entries, each with its entry change control when asked
  for; a search the service ended gets a done with other (80).
- *StartTLS* is operationsError (1) under TLS or with searches
  outstanding; otherwise the service answers, and after the success
  response is written the job does the handshake (bytes the client sent
  after the request end the connection).
- *Time-outs.* `ibm-slapdIdleTimeOut`: a connection with no request for
  that long, and no persistent search, is closed. `ibm-slapdWriteTimeout`:
  a write that does not finish in time ends the connection. Neither
  sends a notice.
- *Counts.* Open, accepted, StartTLS and secure-port connections go to
  the service with `dir_conns` when they change (every 2 s at most), for
  `cn=monitor`.
- *End.* `ENDTCPSVR`, `ENDJOB` or the subsystem's end: the listeners
  close, each connection writes a notice of disconnection (unavailable)
  and closes its session (`dir_close`), the job waits up to 10 seconds
  for them and logs `GLD0265`. Unbind and a closed connection close the
  session too.

**The service** (`lic/slic/src/stgsvc/dirsrv/`) holds, per instance,
the backends and the schema, and answers:

| Request | What |
|---|---|
| 1560 `dir_open` | a connection to the job's instance: a session ID and the settings the job runs by (ports, `ibm-slapdSecurity`, size and time limits, idle and write time-outs, the TLS application ID, the largest request); the first open by a job opens the instance (its file checked, `GLD0126`/`GLD0127`/`GLD0232`; its store made at the first) |
| 1561 `dir_bind` | a bind request with the connection's facts (client, TLS, the client certificate's subject): simple or SASL EXTERNAL; the identity (DN, groups, administrator or not) kept with the session; committed whatever the result; failures counted and audited (§9) |
| 1562 `dir_op` | add, modify, delete, modify DN, compare, an extended operation; one store transaction, committed when it succeeded, then its changes go to the persistent searches; `start_tls` in the answer when StartTLS succeeded |
| 1563 `dir_search` | a search (or the next page of a paged one) and its first entries within the job's budget |
| 1564 `dir_next` | the next entries of a cursor (at most N entries or B bytes per answer, capped at 1000 and 4 MiB, so the single-threaded service never runs one long search) |
| 1565 `dir_close` | the search of a message ID abandoned (its cursor or persistent search), or the connection ended (unbind, close, idle time-out: the session and all it holds, and the `UB` audit entry) |
| 1566 `dir_conns` | the job's connection counts (open, total, StartTLS, secure port), for `cn=monitor` |
| 1567 `dir_changes` | what the session's persistent searches saw since the last call, answered at once (the job's watcher thread polls), and the searches that no longer exist |
| 1568 `dir_admin` | Navigator's and QAP's questions (`DirAdmin`, below): the configuration, the instance list, start and end, the schema, access, publishing, counters |
| 1569 | reserved |
| 1570 `dirpub_config` | the publishing configuration for `QGLDPUBE` and `QGLDPUBA`: the file's text and the agents' passwords unsealed (only to `QDIRSRV`'s jobs) (§10.1) |
| 1571 `dirpub_events` | an agent's changes after its cursor, or a resync; the agent's status reported with the ask (§10.1) |
| 1572 `dirpub_list` | every user or printer to publish and the cursor they are as of (§10.1) |
| 1573-1579 | reserved |

**Administration** (`net/dirsrv/src/admin/`): `DirAdmin` is what
Navigator's Directory server pages and QAP's `Dirsrv` service ask, one
method per question (instances, start, end, restart, autostart;
properties; suffixes; indexes; the administrator; schema; an entry's
access; publishing and its status; `cn=monitor`'s counters), each with
the signed-on profile: reads need `*IOSYSCFG`, changes `*ALLOBJ` and
`*IOSYSCFG` (`CPF222E`), start and end `*IOSYSCFG`. A refusal is an
`AdminError`: its kind, the message ID (`GLD` where `QGLDMSG` has one,
`CPF9898` with Quartz text otherwise) and data. `admin::conf` answers
the configuration's questions over `config::SlapdConf`,
`admin::schema` the schema's (a page of definitions, an addition, a
removal), `admin::publish` the agents' over `publish.conf` (§10.1),
all three shared by the service and `MemAdmin`; `admin::access` checks
`aclEntry` and `entryOwner` values; `MemAdmin` is a directory server
in memory for tests and fixtures. Request 1568 carries each method as a
`stg::diradmin::DirAdminOp` answered by a `DirAdminReply`
(`stg::diradmin::serve` on the service's side, `Remote` over a
`Sender` on the job's: Navigator's asks on the job's thread swapped to
the signed-on profile, QAP's on the connection's secondary thread,
which runs as it); the job sets the user before it asks, so no request
names one.

The service's side (`lic/slic/src/stgsvc/dirsrv/admin.rs`) answers as
the job's current profile, and every refusal's text is its message's
first-level text with the data:

- *The configuration* (properties, suffixes, indexes, the
  administrator): the instance's `ibmslapd.conf` read, changed through
  `admin::conf` and written (`dirconf::write`), then
  `serve::config_changed` has a running instance apply what takes
  effect at once (§10); the rest at its next start. `CHGDIRSVRA` does
  the same. An index needs an attribute type of the instance's schema
  (`GLD0168`). Suffix entry counts are not given (`None`).
- *The instance list*: `idsinstances.ldif`; autostart is its
  `ids-instanceAutostart`, edited in place
  (`config::instances::with_autostart`). An
  instance runs when a job of its name runs as `QDIRSRV` (`Ending` once
  its end was asked); its certificate is the label assigned to
  `QS400_DIRECTORY_SERVER`.
- *Start, end, restart* are `STRTCPSVR`'s and `ENDTCPSVR`'s, since the
  instance jobs are started by a job, not the service: a start checks
  the file (`GLD0126`, `GLD0232`) and submits a batch job `QGLDSTRSVR`
  (`QSYS/QDIRJOBD`, `QSYSNOMAX`) running `STRTCPSVR SERVER(*DIRSRV)
  INSTANCE(name)` as the profile; an end is the instance job's
  `ENDJOB OPTION(*CNTRLD)`; a restart ends it and schedules the start
  15 seconds later, past the job's 10 seconds to end. Each answers once
  asked; the outcome is the instance's state and the jobs' logs.
- *The schema*: a running instance's, changed in place and its
  additions written to `V3.modifiedschema` as a `cn=schema` modify
  writes them; otherwise the shipped schema with the file's additions.
- *An entry's access and the counters* are a running instance's
  (`GLD0122` otherwise): the access attributes read as the
  administrator (`aclEntry`, `aclPropagate`, `aclSource`,
  `entryOwner`, `ownerPropagate`, `ownerSource`; an entry is explicit
  when its source is itself, `GLD0166` when there is no entry); a
  change replaces the entry's own values (none: removed, so it
  inherits) in one modify whose changes reach the persistent searches.
- *Publishing*: `publish.conf` (`publish::read_config`, the shipped
  state without one) seen through `admin::publish`; a change is
  checked there (`GLD0204`, `GLD0239`, `GLD0224`, `GLD0232`), then
  written with the new password sealed (`publish::write_config`). The
  status is each agent's last report (1571) and its job.

ACL evaluation, schema checking and password policy run in the
service, beside the data, with the session's identity.

The protocol is `work/stg/src/dir.rs`. LDAP messages travel as their
BER bytes (`ldap::msg` at both ends): a request is the message the
client sent; each answer's `replies` are whole messages with that
message's ID, written to the connection in order, and its `state` says
what follows (`Done`, `More { cursor }`: ask 1564, `Paused { cursor }`:
a page of paged results is over, `Watching { watch }`: a persistent
search's changes come from 1567). Unbind and abandon are the job's,
told with 1565. Only the instance's job asks (user `QDIRSRV`, job name
the instance's; `GLD0244` otherwise), and a session answers only the
job that opened it (`GLD014B` otherwise); refusals are `DirError`
(message ID, data, text: the message file's first-level text with the
data, filled in as the answer leaves the service).
The service (`stgsvc/dirsrv/serve.rs`) keeps the instance open while
its job runs; when the job ends its sessions, cursors and persistent
searches go, and the next start reads the configuration again. A
change through `cn=configuration` is written to the file after the
request (passwords hashed as `CHGDIRSVRA` keeps them) and what takes
effect at once is applied; `serve::config_changed` does the same for a
change made outside (`CHGDIRSVRA`, `dir_admin`). `ibm-slapdIndex`
values (on the entry store's backend entry) are attribute names,
comma or space separated; at each open the store gets an index for
each and loses the configured ones no longer listed.

## 5. The entry store

A new internal object type `*DIRDB` (type code 0x00FC), one per
instance, `QUSRDIRDB/name` (IBM's library name for its database; the
library is created at the first IPL of V1R2M0). Internal like `*MBR`: it
is in no listing a user sees, `RCLSTG` walks its trees, `SAVLIB
QUSRDIRDB` saves it whole (that is the online backup, as IBM's is a save
of its libraries). Its header's attribute area is `DirdbAttr` and its
four tree roots are:

| Slot | Tree | Key | Value |
|---|---|---|---|
| 0 | entries | entry ID (u64), chunk (u16) | the entry: its attributes and values, operational attributes included, encoded as `EntryRec` (§5.2), in chunks of 1024 bytes |
| 1 | names | parent ID (u64), normalized RDN | the child's entry ID |
| 2 | index | index number (u16), kind, normalized key (equality value, or a substring gram), entry ID | empty |
| 3 | change log | change number (u64), chunk (u16) | the change record (`cn=changelog`, §8), in chunks of 1024 bytes |

A DN is found by walking `names` from the suffix's root entry, one RDN
at a time; a subtree's move is one `names` re-key, so modify DN with a
new superior is cheap at any size. Entry IDs come from `DirdbAttr`'s
counter and are never reused. `entryUUID` (RFC 4530) and IBM's
`ibm-entryuuid` are the same value.

Indexes: `objectClass`, `cn`, `uid`, `sn`, `mail`, `entryUUID` always;
more by the configuration (`ibm-slapdIndex` QS/400 attribute on the
backend entry, §10). Equality keys are the matching rule's normalized
value; substring keys are trigrams; presence is the index number with
an empty key. A search picks the most selective indexed conjunct and
filters the rest; with no indexed conjunct it walks the base's subtree
through `names`.

`DirdbAttr` holds the format version, the suffix roots (up to 16
suffixes, each its own root entry), the next entry ID and change
number, entry and index counts, and the change log's first number, its
count and its size limit. The indexes' definitions are entry 0 of the
entries tree (entry IDs start at 1).

A long entry or change record is several tree values (chunks under one
ID) rather than overflow pages: every page of a `*DIRDB` is then a node
of one of its four trees, so `RCLSTG`, delete and `SAVLIB`/`RSTLIB`
handle it through the header's tree roots with no code of their own
(the save writes the trees' entries and the attribute area, which holds
no page numbers; the restore bulk-loads them).

Listings leave `*DIRDB` out (`store::obj::types::hidden`, applied in
`ops::list`), though it has a library directory entry in `QUSRDIRDB`:
`DSPLIB`, `WRKOBJ`, `QUSLOBJ`, `/QSYS.LIB` and the SQL catalogs never
show it, while the save and `RCLSTG`, which read the library directory
itself, do. `CLRLIB QUSRDIRDB` therefore leaves the stores in place.

### 5.1 The format and the freeze

`*DIRDB` is new at V1R2M0: its layouts (`DirdbAttr`, `EntryRec`, the
key forms) are specified here before they are coded (as
`STORE_FORMAT.md` requires) and frozen at V1R2M0 in
`tools/fmtcheck/formats.toml` as the `dirdb` format with its own
version. The store's type registry gains 0x00FC: `types.rs` changes, so
the store format's version list gains `TYPE_REGISTRY_VERSION` (2 at
V1R2M0; V1R1M0's registry is version 1). A V1R1M0 system never opens a
V1R2M0 ASP (`release::store_may_open`), so nothing older meets the new
type; a slip install from V1R1M0 creates `QUSRDIRDB` and the empty
`QUSRDIR` store at its first IPL. The proof is the slip install under
QEMU (install step 10) plus a directory step after it.

The byte tables (`DirdbAttr`, `EntryRec`, keys) are §5.2, written by the
store's agent before the code, with `net/dirsrv/tests/spec.rs` comparing
them as `storage/store/tests/spec.rs` does.

### 5.2 Layouts

Conventions are `STORE_FORMAT.md` §0's: integers in the attribute area
and in records are little-endian, integers in tree keys big-endian so
that byte order is key order. Text (RDNs, DNs, attribute descriptions)
is UTF-8. Frozen at V1R2M0 (`tools/fmtcheck/formats.toml`, format
`dirdb`, the files `net/dirsrv/src/store/format.rs` and `record.rs`);
the version is `DIRDB_VERSION`, 1.

**Limits.** A tree key is fixed size per tree (`STORE_FORMAT.md` §3);
the names and index trees use 128 bytes, so a long normalized RDN or
value keeps its first bytes and ends in a SHA-256 of the whole (the
*hashed tail*). The other limits: an `EntryRec` or `ChangeRec` at most
16 MiB (16 384 chunks); an RDN's text and its normalized key at most
65 535 bytes each; an attribute description at most 1024 bytes; at most
65 535 attributes per entry; 16 suffixes; index numbers 1 to 65 535,
an index's attribute name at most 255 bytes. A record past a limit is
refused (`adminLimitExceeded` at the service).

#### `DirdbAttr` (256 bytes)

At offset 0 of the header's attribute area (body offset 1024); the rest
of the area is zero. No field is a page number: tree roots are the
header's `tree_roots` (slot 0 entries, 1 names, 2 index, 3 change log).

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 2 | version | `DIRDB_VERSION` |
| 2 | 2 | suffix_count | entries used in `suffixes` |
| 4 | 2 | index_count | definitions in entry 0 |
| 6 | 2 | next_index | number the next configured index gets (7 at creation) |
| 8 | 8 | next_id | the next entry ID; 1 at creation, never reused |
| 16 | 8 | next_change | the next change number; 1 at creation |
| 24 | 8 | entry_count | entries, entry 0 not counted |
| 32 | 8 | index_keys | keys in the index tree |
| 40 | 8 | log_first | the oldest change number held, 0 for an empty log |
| 48 | 8 | log_count | change records held |
| 56 | 8 | log_max | records kept, 0 for no limit; an append past it drops the oldest |
| 64 | 64 | reserved | 0 |
| 128 | 128 | suffixes | 16 × entry ID (u64) of a suffix's root entry, 0 for unused |

**The entries tree (slot 0).**

#### `EntryKey` (10 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | entry | entry ID; 0 is the index definitions |
| 8 | 2 | chunk | 0, 1, ...: the record's bytes `chunk * 1024 ..` |

The value of chunk *n* is bytes `n * 1024` to `n * 1024 + 1023` of the
record (the last chunk shorter). A record is read by a range over its
ID; a rewrite that needs fewer chunks deletes the rest.

#### `EntryRecHeader` (24 bytes)

An `EntryRec` is this header, then `rdn_len` bytes of the RDN's text as
the client wrote it (for a suffix root, the whole suffix DN), then
`rdn_key_len` bytes of its normalized key (the names tree's RDN bytes
before any hashing; for a suffix root, the suffix key), then
`attr_count` attributes. An attribute is an `AttrHeader`, `name_len`
bytes of its description (type and options as stored, operational
attributes included: `createTimestamp`, `creatorsName`, `entryUUID`,
`ibm-entryuuid` and the rest are attributes like any other), then
`value_count` values, each a `ValueHeader` and `len` bytes.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | version | 1 |
| 1 | 1 | flags | bit 0: a suffix root |
| 2 | 2 | attr_count | |
| 4 | 2 | rdn_len | |
| 6 | 2 | rdn_key_len | |
| 8 | 8 | parent | the parent's entry ID; 0 for a suffix root |
| 16 | 4 | total_len | bytes of the whole record, header included |
| 20 | 4 | reserved | 0 |

#### `AttrHeader` (8 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 2 | name_len | 1..1024 |
| 2 | 2 | reserved | 0 |
| 4 | 4 | value_count | |

#### `ValueHeader` (4 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 4 | len | bytes of the value |

**Entry 0** holds the index definitions: an `IndexDefsHeader`, then
`count` definitions, each an `IndexDef` and `name_len` bytes of the
attribute's name as the normalizer identifies it (`Normalizer::attr_id`:
`objectclass`, `cn`, ...), in index number order.

#### `IndexDefsHeader` (4 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | version | 1 |
| 1 | 1 | reserved | 0 |
| 2 | 2 | count | |

#### `IndexDef` (4 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 2 | number | the index number in index keys |
| 2 | 1 | kinds | bit 0 presence, bit 1 equality, bit 2 substring, bit 7 always on (shipped, never removed) |
| 3 | 1 | name_len | 1..255 |

The six always-on indexes are created with the store: 1 `objectclass`
(presence, equality), 2 `cn`, 3 `uid`, 4 `sn`, 5 `mail` (presence,
equality, substring), 6 `entryuuid` (presence, equality).

**The names tree (slot 1).**

#### `NameKey` (128 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | parent | the parent's entry ID; 0 for a suffix |
| 8 | 118 | rdn | the RDN bytes, zero padded; or, when longer than 118, their first 86 bytes and the SHA-256 of all of them |
| 126 | 2 | form | the RDN bytes' length (0..118), or 0xFFFF for the hashed form |

The RDN bytes of a child are `Normalizer::rdn_key` of its RDN. Those of
a suffix (parent 0) are its *suffix key*: each of the suffix's RDNs,
most specific first, as a two-byte big-endian length and its
`rdn_key`. The value is the entry's ID, 8 bytes, little-endian
(`NameValue`). Children of one parent are adjacent and in key order,
which is the order of a one-level search and of a subtree walk.

#### `NameValue` (8 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | entry | the child's entry ID |

**The index tree (slot 2).**

#### `IndexKey` (128 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 2 | index | the index number |
| 2 | 1 | kind | 0 presence, 1 equality, 2 substring |
| 3 | 115 | key | the key bytes, zero padded; or, when longer than 115, their first 83 bytes and the SHA-256 of all of them |
| 118 | 2 | form | the key bytes' length (0..115), or 0xFFFF for the hashed form |
| 120 | 8 | entry | the entry's ID |

The value is empty. Presence has empty key bytes. Equality's are the
attribute's normalized value under its equality rule
(`Normalizer::value_key` with `Rule::Equality`). Substring's are the
grams of the value normalized under its substring rule: every run of
three bytes of `0x02`, the value, `0x03` (so an initial piece's grams
start with `0x02` and a final piece's end with `0x03`); a lookup of a
substring filter intersects the grams' entry lists and the service
filters the candidates. A hashed key matches only its exact value; the
candidates of a hashed key are checked by the service as all
candidates are. An attribute value yields one presence key per
attribute (not per value), one equality key per distinct normalized
value, one substring key per distinct gram.

**The change log (slot 3).**

#### `ChangeKey` (10 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | number | the change number |
| 8 | 2 | chunk | as `EntryKey`'s |

#### `ChangeRecHeader` (32 bytes)

A `ChangeRec` is this header and five fields, each a four-byte length
and its bytes, in order: the target DN, the new RDN, the new superior
(both empty unless a modify DN), the modifier's DN, and the changes
(draft-good-ldap-changelog's `changes`: LDIF of the added entry or the
modifications, empty for a delete or a modify DN).

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | version | 1 |
| 1 | 1 | kind | 1 add, 2 delete, 3 modify, 4 modify DN |
| 2 | 1 | flags | bit 0 `deleteOldRDN`, bit 1 a new RDN, bit 2 a new superior |
| 3 | 1 | reserved | 0 |
| 4 | 4 | total_len | bytes of the whole record |
| 8 | 8 | time | Unix milliseconds |
| 16 | 8 | entry | the entry's ID |
| 24 | 8 | reserved2 | 0 |

## 6. Schema

The schema is the RFC 4512 subschema (`cn=schema`) with: RFC 4512's
operational attributes, RFC 4519 (the user schema), RFC 4524 (COSINE),
RFC 2798 (`inetOrgPerson`), RFC 2307 (`posixAccount`, `posixGroup`,
`shadowAccount`), RFC 4530 (`entryUUID`), RFC 2589-free; IBM's
operational and directory classes and attributes that QS/400 honours
(`aclEntry`, `aclPropagate`, `aclSource`, `entryOwner`,
`ownerPropagate`, `ownerSource`, `ibm-effectiveAcl`, `ibm-entryuuid`,
`ibm-pwdPolicy`, `pwdPolicy` and the `pwd*`/`password*` set,
`ibm-capabilitiessubentry`, `ibm-replicationThisServerIsMaster`
(always TRUE), `ibm-replicationIsQuiesced` (always FALSE), the
`ibm-slapd*` configuration attributes of §10, the projected `os400-*`
classes and attributes of §9); and, for compatibility, every other name
and OID the primary's schema lists, defined so that an LDIF exported
from IBM's server loads (`tools/conformance/oracle/dirsrv/
schema-primary.txt` has the OIDs, syntaxes and matching rules). The
descriptions (`DESC`) are Quartz's. The code is `net/dirsrv/src/schema/`
(the model, checking, DN keys, changes) and
`net/dirsrv/src/backend/schema.rs` (the `cn=schema` entry).

### 6.1 The shipped files

QS/400's files, in `net/dirsrv/schema/`, embedded in the build
(`schema::shipped::FILES`) and written to
`/QIBM/ProdData/OS400/DirSrv/etc/`. Each is lines of `attr: value`
(RFC 4512 descriptions; `#` comments; a line that starts with a space
continues the one before, as in LDIF), loaded in this order:

| File | What |
|---|---|
| `qs.ldapsyntaxes` | the syntaxes QS/400 validates (§6.2) |
| `qs.matchingrules` | the matching rules QS/400 implements (§6.2) |
| `qs.system.at`, `qs.system.oc` | RFC 4512's operational attributes and `top`, `alias`, `subschema`, `extensibleObject`; RFC 3045, 3296 (`ref`, `referral`), 4530; IBM's directory attributes and classes QS/400 honours (ACLs, owners, password policy, audit, capabilities, `ibm-slapd*` configuration, `IBMAttributeTypes`) |
| `qs.user.at`, `qs.user.oc` | RFC 4519, 4524, 2798, 2307, 2079 (`labeledURI`), `uidObject` |
| `qs.os400.at`, `qs.os400.oc` | the projected backend's `os400-*` (§9) |
| `qs.ibm.at`, `qs.ibm.oc` | every other definition of the primary's `cn=schema` (IBM's and third parties'), so IBM exports load; QS/400 defines them and gives them no behaviour |

The definitions of every file come from the primary's names, OIDs,
supertypes, syntaxes, matching rules, flags and usages, so an entry
written for IBM's server checks the same here. The system, user and
os400 definitions carry Quartz's `DESC`; those of `qs.ibm.*` carry none
(IBM's were stripped and are not reproduced). Where QS/400 differs from
the primary's definitions:

- added (absent on the primary): `entryUUID` (RFC 4530, the same value
  as `ibm-entryUuid`), `structuralObjectClass`,
  `governingStructureRule`, `entryDN` (RFC 5020), the class `uidObject`;
- `supportedControl` is `USAGE dSAOperation` (RFC 4512; the primary
  has it as a user attribute);
- kept as IBM has them, against the RFCs: `displayName` and
  `userPKCS12` under IBM's OIDs (1.3.18.0.2.4.715 and .456, not RFC
  2798's), `uid` with caseIgnoreIA5Match over Directory String, `mail`
  as Directory String;
- `homeFax` and `middleName` (IBM's, in RFC 4512's OID arc) are in
  `qs.ibm.at`.

**Access classes** (`IBMAttributeTypes ACCESS-CLASS`, §12): the
primary's values are not in the facts, so QS/400's are its own:
`critical` for `userPassword`, the `ibm-slapd*PW`/`*Pw`
configuration passwords, `ibm-replicaKeypwd`, `passwordHistList`,
`pwdHistory`, `os400-password`, `os400-docpwd`; `restricted` for
`aclEntry`, `aclPropagate`, `entryOwner`, `ownerPropagate`,
`ibm-filterAclEntry`, `ibm-filterAclInherit`; `system` for every other
operational attribute; `normal` for every other user attribute. No
attribute ships `sensitive`, and none ships a `LENGTH` (values are
bounded only by a syntax's `{n}` and the store's limits).

**Protected definitions**: those of `qs.system.*` and `qs.os400.*`
cannot be deleted or replaced through `cn=schema` (53); the user and
compatibility definitions can.

### 6.2 Syntaxes and matching rules

Every syntax of RFC 4517 §3.3 the shipped attributes name, the older
ones the primary lists (Audio, the description syntaxes, Supported
Algorithm), RFC 4530's UUID and IBM's 1.3.18.0.2.8.1 (taken as a
Directory String) has a validator (`schema::syntax`). An attribute
whose syntax QS/400 does not know takes any value. Boolean accepts any
case (IBM's configuration writes `false`). Generalized Time requires
its time zone on input; matching treats a value without one as UTC.

Every matching rule the shipped attributes and the primary name is
implemented (`schema::rule`): each has a normalizer, so equality is
equality of normalized bytes and an ordering rule's normalized bytes
sort as its values do (integers as sign, length and digits; times as
`YYYYMMDDHHMMSS.nnnnnnnnnZ` in UTC). The string rules prepare values
as RFC 4518 does, with these simplifications:

- mapping: the characters of §2.2 mapped to nothing or to a space;
- case folding: Unicode lower-casing (`char::to_lowercase`), not the
  full case-folding table (`ß` stays `ß`);
- no NFKC normalization: values that differ only in normalization
  form do not match;
- prohibited output (§2.4: private use, non-characters, U+FFFD) makes
  the value Undefined to the rule;
- insignificant spaces: leading and trailing dropped, inner runs one
  space; a substring assertion's initial piece meets the value's start
  and its final piece the value's end, its spaces kept within.

Other rules: numericString drops spaces; telephoneNumber drops spaces
and hyphens and ignores case; caseIgnoreList compares the lines of a
postal address; objectIdentifierMatch resolves descriptors to OIDs;
the first-component rules take a description's first OID, integer or
string; wordMatch and keywordMatch match a word (a run of words) of the
value. The IA5 rules apply to any UTF-8 value (IBM gives them to
Directory String attributes). IBM's rules without an RFC:
ibm-entryUuidMatch compares UUIDs ignoring case;
distinguishedNameOrderingMatch orders DNs by their normalized bytes;
presentationAddressMatch and protocolInformationMatch are
caseIgnoreMatch. An attribute with no equality rule compares octet by
octet.

**DNs** (distinguishedNameMatch, `schema::dn`): each AVA's type
becomes its OID and its value is normalized by the type's equality
rule; the AVAs of an RDN sort by their bytes; the form is
`oid=value(+oid=value)*(,...)*`, values escaped as RFC 4514 §2.4. A
type the schema does not know keeps its lower-cased name and its value
is compared as caseIgnoreMatch. So `cn=Foo  Bar,DC=Example, dc=COM`
and `cn=foo bar,dc=example,dc=com` are one DN.

**The store's keys** (`store::normalize::Normalizer`, §5.2):
`attr_id` is the attribute's primary name lower-cased; `rdn_key` is the
RDN's normalized form above, refusing unknown types and invalid values
(34); `value_key` is the equality rule's normalized value, or for
substrings the value prepared by the substring rule without its end
spaces (so a matching piece's grams are always among the value's).

### 6.3 Checking entries

`ibm-slapdSchemaCheck: V3_lenient` by default, `V3` strict; `V2` and
anything else are refused. Both levels check: `objectClass` present and
every class defined (65); every attribute type defined (17) and
options other than `;binary` refused (53: language tags, §14); values
valid for the syntax (21); single-valued attributes (19); a value twice
under the equality rule (20); every MUST of every class present (65);
every user attribute allowed by some class's MUST or MAY, itself or a
supertype of it, unless `extensibleObject` is present (65; operational
attributes are always allowed); the RDN's values in the entry (64).
NO-USER-MODIFICATION attributes are refused in a user's add or modify
(19, `check_user_modification`, which the operations call).

| | `V3` | `V3_lenient` |
|---|---|---|
| superclasses of a listed class | must be listed (`top` is implied) | implied, and stored |
| structural classes | exactly one chain | none, one or several |
| obsolete classes | refused | taken |

The check returns the entry's full class list (listed and implied) and
its structural class for `structuralObjectClass`.

### 6.4 `cn=schema` and `V3.modifiedschema`

The subschema entry (`backend::schema::entry`) holds `objectClass`
(`top`, `subschema`, `ibmSubSchema`), `cn: schema`, `attributeTypes`,
`objectClasses`, `ldapSyntaxes`, `matchingRules`, `matchingRuleUse`
(each rule's attribute types: those naming it, and for equality and
ordering rules those of its assertion syntax) and `IBMAttributeTypes`
(every attribute type's access class, and length when it has one).

A modify of `cn=schema` (the administrator's; ACLs are §12) changes
`attributeTypes`, `objectClasses` and `IBMAttributeTypes`, all its
changes or none, the result resolved and checked as a whole:

- `add`: a new definition; an OID or name already defined is 20;
- `replace`: each value redefines its OID's definition, or defines it;
- `delete`: each value's OID (the value may be the whole description or
  just `( oid )`) is removed; one still used by another definition (a
  supertype, a superclass, a MUST or MAY) is 19, so a class goes before
  its attributes; a delete without values is 53;
- a description that does not parse is 21; a reference to something
  undefined, a cycle, a wrong rule kind or no syntax is 19; a protected
  definition, `ldapSyntaxes`, `matchingRules`, `matchingRuleUse` or any
  other attribute of the entry is 53.

What differs from the shipped schema is the instance's
`etc/V3.modifiedschema` (`Schema::modified_text`, read back with
`Schema::load_modified`; the service does the file I/O). Its format
(UTF-8, the shipped files' line form):

```
# comment
attributeTypes: ( ... )          every attribute type added or replaced
IBMAttributeTypes: ( ... )       after its attribute type, when it has one
objectClasses: ( ... )           every object class added or replaced
deletedAttributeTypes: OID       every shipped attribute type deleted
deletedObjectClasses: OID        every shipped object class deleted
```

Loading applies the deletions, then the definitions (each replacing its
OID's or added), then resolves; a file that does not load stops the
instance's start.

## 7. Operations, controls, extended operations

The engine is `net/dirsrv/src/ops/` (`ops::Directory`), a library the
service calls per request (§4); it has no job, socket or thread of its
own.

**The interface.** A `Directory` is one instance: its `Schema`, its
`Settings` (§10's values, filled by `config`), its backends by suffix
(the btree backend over the `*DIRDB` holds the user suffixes,
`cn=localhost` and `cn=ibmpolicies`; `cn=changelog` is added when
`ibm-slapdChangeLogMaxEntries` is set; the service adds the projected
backend and `cn=configuration` with `add_backend`), a clock and random
source (`Env`), and two hooks the service implements: `Identity` (binds
of DNs it answers for: OS profiles, §9) and `Audit` (§12). Every call
runs in a store transaction the caller begins and ends, and takes the
connection's `Session` (anonymous, or a DN with its groups, the
administrator flag and the must-change flag of a reset password):

| Call | Request | What |
|---|---|---|
| `Directory::new`, `seed` | `dir_open` | the instance; `seed` adds the entries of `cn=localhost` and `cn=ibmpolicies` the primary has (`oracle/dirsrv/anon-entries.ldif`) when missing |
| `bind(txn, msg, conn)` | 1561 | the new `Session` in `Outcome::session`; `Conn` carries TLS and the client certificate's subject; commit whatever the result (policy failures are recorded) |
| `op(txn, session, msg)` | 1562 | add, modify, delete, modify DN, compare, extended; commit when `Outcome::succeeded`, then pass `Outcome::changes` to `publish`; `start_tls` and `schema_changed` tell the job and the service what follows |
| `search(txn, session, msg, cursors)` | 1563 | `Started::Done(replies)` or `Started::Cursor(id)` in the connection's `Cursors` |
| `next(txn, cursors, id, budget)` | 1564 | at most `Budget { entries, bytes }`; `PageState::More`, `Done`, `Paused` (a page of paged results: the cursor waits for the next page's request, whose cookie is its number) or `Watching(w)` |
| `Cursors::close`, `close_watch` | 1565 | an abandoned or ended search |
| `publish(txn, changes)` | (after a commit) | changes matched against every persistent search, as each watcher may see them |
| `changes(w, max)` | 1567 | a persistent search's queued entries, with entry change notifications when asked |

Responses are `Reply { op, controls }` with `ldap::msg` operations.

**Operations.** All of RFC 4511: bind, search (every scope, aliases
dereferenced per `derefAliases` within `ibm-slapdDerefAliases`, size
and time limits min'd with the server's for non-administrators,
`typesOnly`, `*`, `+`, `1.1` and IBM's `+ibmaci`, `+ibmentry`,
`+ibmpwdpolicy`, `+ibmrepl` classes of `ibm-opAttrClasses`), compare,
add, modify (add, delete, replace, increment from RFC 4525), modify DN
(new RDN, delete old RDN, new superior: a subtree move is one key in
the store), delete (a leaf, or a subtree with tree delete), extended.
Unbind and abandon are the job's (the service closes the cursors).
Referral objects (`referral`, `ref`) answer referrals with the DN
carried into each URL and continuation references in searches (RFC
3296) unless manageDsaIT is sent; `ibm-slapdReferral` answers for a DN
under no suffix, noSuchObject (32) otherwise.

**Search candidates** are the btree backend's (§5): an indexed
equality, presence or substrings assertion; the smallest list of an
`&`; the union of an `|` of indexed terms; a walk of the base's subtree
otherwise. The operations evaluate the filter on each candidate with
the schema's matching rules (`ops::filter`): through subtypes; an
attribute defined without a rule takes the first rule of its syntax
(IBM's `createTimestamp` orders without one); Undefined for an unknown
attribute, a missing rule, or an attribute the requester may not
search (`s`). `~=` is QS/400's approximate rule on the
equality-normalized values: equal, a common prefix of at least four
characters and three quarters of the shorter value, or an edit distance
of at most a quarter of it (the primary matches `(cn~=ibmpolicy)` to
`IBMpolicies`; IBM's algorithm is not documented). Extensible match
runs the named rule (an ordering rule matches values below the
assertion, a substrings rule takes `*` in the value) and `:dn:` the
DN's AVAs: a QS/400 addition, the primary's matches nothing. `(&)` and
`(|)` match nothing, as the primary answers (RFC 4526 is not
advertised).

**Operational attributes** stored by the server: `creatorsName`,
`createTimestamp`, `modifiersName`, `modifyTimestamp` (IBM's form,
`YYYYMMDDHHMMSS.ffffffZ`), `entryUUID` and `ibm-entryuuid` (one random
version 4 UUID), `structuralObjectClass`, the implied superclasses in
`objectClass` (`V3_lenient`, §6.3), the password policy state (§11).
Computed when asked for: `entryDN` (RFC 5020), `hasSubordinates`,
`numSubordinates`, `subschemaSubentry`, `aclEntry`, `aclPropagate`,
`entryOwner`, `ownerPropagate` (the effective values when the entry has
none of its own, as the primary shows them), `aclSource`,
`ownerSource` (a DN or `default`), `ibm-effectiveAcl`,
`ibm-replicationThisServerIsMaster` (TRUE), `ibm-replicationIsQuiesced`
(FALSE).

**What follows the primary** (`oracle/dirsrv/ldaptest-primary.txt`):
a size limit is exceeded (4) only when more entries match than it
allows; noSuchObject's matched DN is upper-cased; a one-level search of
`""` is 53 and a subtree search of `""` searches every suffix of the
entry store; `cn=schema` one-level is 2; `cn=monitor` one-level is 32
and anything below it 53; a filter naming the ACL attributes
(`aclEntry`, `entryOwner` and their kin) is 53 for anyone but an
administrator; a compare of an attribute the schema does not define is
16; a paged search past `ibm-slapdPagedResLmt` open on the connection
is 53, a cookie the server did not give 53, a page size of 0 with a
cookie ends the search; sorting with more keys than
`ibm-slapdSortKeyLimit` is sortResult 11 and with an unknown attribute
16 — the search fails with 12 when the control is critical and is
answered unsorted otherwise; proxied authorization by an anonymous
user is 50 (a bound one that is not an administrator 123, RFC 4370); a control the operation does not take is 12 when
critical (paged results on a compare included). Deliberately not
followed: the primary's 80 for the second paged search opened while
one is open (the limit is what counts), and its 53 for the request
after an abandoned persistent search (a race in IBM's server).

Controls (`ops::controls`; each in `supportedControl`):

| OID | Control | Operations | |
|---|---|---|---|
| 1.2.840.113556.1.4.319 | paged results (RFC 2696) | search | built; non-administrators only with `ibm-slapdPagedResAllowNonAdmin` |
| 1.2.840.113556.1.4.473 / .474 | server-side sort (RFC 2891) | search | built; the least value ascending, the greatest descending, entries without one last; non-administrators only with `ibm-slapdSortSrchAllowNonAdmin` |
| 2.16.840.1.113730.3.4.2 | manageDsaIT (RFC 3296) | all but bind and extended | built |
| 2.16.840.1.113730.3.4.3 | persistent search | search | built when `ibm-slapdEnablePersistentSearch`, at most `ibm-slapdMaxPersistentSearches` (11 past it), entry change notifications, 10 000 queued changes per watch |
| 2.16.840.1.113730.3.4.18 | proxied authorization v2 (RFC 4370) | all but bind (whoami and password modify run as the identity) | built; administrators only (50 for anonymous users, as the primary; 123 for a bound user), `dn:` or empty identities (123 for `u:`), must be critical (2) |
| 1.2.840.113556.1.4.805 | tree delete | delete | built; `object:d` on every entry |
| 1.3.6.1.1.12 | assertion (RFC 4528) | all but bind and extended | built (QS/400 addition: the primary answers 12); an add's assertion is tested against the entry to be added; 122 when false |
| 1.3.6.1.1.13.1 / .2 | pre-read, post-read (RFC 4527) | modify, delete, modify DN / add, modify, modify DN | built (QS/400 addition), with the requester's read rights |
| 1.3.6.1.4.1.42.2.27.8.5.1 | password policy | bind, add, modify, password modify | built (§11): the response control when the request carried it |
| 1.3.18.0.2.10.5, .8, .9, .15, .18, .21, .22, .23, .24, .25, .26, .27, .28, .30 | IBM's (transactions, server administration, omit group referential integrity, group authorization, limit number of attribute values, replication, audit) | — | not built: 12 when critical, ignored when not (RFC 4511 §4.1.11); their values are not in the facts and QS/400 does not guess IBM's encodings (§14) |

Any other control: 12 when critical, ignored when not; a control the
operation takes whose value does not decode is 2.

Extended operations (`ops::ext`):

| OID | Operation (`ldapexop -op`) | |
|---|---|---|
| 1.3.6.1.4.1.1466.20037 | StartTLS (RFC 4511 §4.14) | built: success and `Outcome::start_tls`; the job does the handshake, or answers 2 itself without a certificate, as the primary does; a value is 2 |
| 1.3.6.1.4.1.4203.1.11.3 | whoami (RFC 4532) | built (QS/400 addition): `dn:DN`, empty for anonymous |
| 1.3.6.1.4.1.4203.1.11.1 | password modify (RFC 3062) | built (QS/400 addition): a modify of `userPassword` under ACLs and §11; the old password checked when sent (49); one generated when no new one is |
| 1.3.18.0.2.12.5, .6, .64 | begin, end, prepare transaction | refused: a request is one store transaction; IBM's transaction encodings are not in the facts |
| 1.3.18.0.2.12.15, .16, .17, .19, .54, .56, .79 | `cascrepl`, `controlrepl`, `controlqueue`, `quiesce`, `repltopology`, `controlreplerr`, `resumerole` | refused: replication (§14) |
| 1.3.18.0.2.12.20, .22, .24 | `clearlog`, `readlog`, `getlogsize` | refused: QS/400's logs are the job log and `QAUDJRN` |
| 1.3.18.0.2.12.28 | `readconfig` | refused: configuration takes effect as §10 says |
| 1.3.18.0.2.12.35, .44 | `unbind` (end a connection), `stopserver` | refused: connections and the job are `ENDTCPSVR`'s |
| 1.3.18.0.2.12.40 | dynamic trace | refused: `TRCTCPAPP` |
| 1.3.18.0.2.12.73 | `onlineBackup` | refused: `SAVLIB QUSRDIRDB` is the backup (§5) |
| 1.3.18.0.2.12.30, .37, .46, .50, .58, .75, .80 | DN normalization, `getusertype`, `getattributes`, `evaluategroups`, `acctstatus`, `effectpwdpolicy`, `locateEntry` | refused: their request and response values are not in the facts; QS/400 does not invent encodings under IBM's OIDs (the password policy response control and `+ibmpwdpolicy` give account status) |

Anything not built answers as IBM's server does an unknown request:
protocolError (2), "unsupported extended operation" in QS/400's own
words.

**The root DSE** (`ops::rootdse`) answers, as on the primary, as user
attributes (no list and `*` return them all, `+` none):
`namingContexts` (`CN=SCHEMA`, `CN=LOCALHOST`, `CN=IBMPOLICIES`, the
user suffixes, `cn=changelog` when on and the projected suffix,
upper-cased as IBM writes them; not `cn=configuration`),
`subschemaSubentry: cn=schema`, `supportedLDAPVersion` 2 and 3,
`supportedControl`, `supportedExtension`, `supportedSASLMechanisms`
(`EXTERNAL` only), `vendorName: Quartz Systems`, `vendorVersion` (the
release), `ibmdirectoryversion: 6.1`, `ibm-serverId`,
`ibm-slapdServerInstanceName`, `ibm-slapdServerBackend: QS400`,
`ibm-osregistrycontext`, `ibm-osregistrytype`, `ibm-ldapservicename`,
`security`, `port`, `ibm-slapdSizeLimit`, `ibm-slapdTimeLimit`,
`ibm-slapdDerefAliases`, `ibm-supportedacimechanisms`
(1.3.18.0.2.26.2), `ibm-opAttrClasses`, `ibm-slapdisconfigurationmode`.
`tests/ops_read.rs` diffs the attribute names with `rootdse.ldif`; the
primary's `ibm-sasldigestrealmname` and `ibm-supportedAuditVersion` are
absent (§15).

## 8. Backends

The operations reach entries through `backend::Backend`: a backend
holds suffixes, resolves a DN to an ID (and the deepest existing entry
above it), gives an entry, its parent and its children's count, lists
search candidates in pages (`Scan`, resumable across calls and
transactions), and adds, replaces, renames and deletes. Access control,
schema checks, operational attributes and filters are the operations'.
A backend may hide an entry from a session: it resolves the DN but
gives no entry and leaves it out of its candidates, and the search
succeeds with no entries (the projected backend answers anonymous
users so, as the primary does). `read_only` backends refuse writes
(53); `admin_only` ones answer 50 to anyone else.

- `btree` (`backend/btree`): the user suffixes, `cn=localhost` and
  `cn=ibmpolicies`, over the entry store of §5, candidates from its
  indexes; with the change log on it appends a `ChangeRec` in the same
  transaction as every change.
- `config` (`backend/config`): `cn=configuration`. The interface only:
  the service implements `ConfigSource` (the file's entries; a modify
  written to the file at once) and adds `ConfigBackend` with
  `add_backend`. Administrators only; others get 50, as on the primary.
- `schema` (`backend/schema`): `cn=schema`, answered by the operations
  (base and subtree; one-level is 2); its modify is the
  administrator's.
- `monitor` (`backend/monitor`): `cn=monitor`, one read-only entry from
  `Counters` (operations requested and completed by kind, entries sent,
  persistent searches, the connection counts the job reports, the start
  and current time as Generalized Time). The caches, work queues,
  transactions and event notification of IBM's entry are absent, not
  zero.
- `changelog` (`backend/changelog`): `cn=changelog` when
  `ibm-slapdChangeLogMaxEntries` is set (QS/400's addition; IBM 6.1 on
  IBM i has it only with replication), read-only:
  `changeNumber=N,cn=changelog` per change (draft-good-ldap-changelog:
  `changeNumber`, `targetDN`, `changeType`, `changeTime`, `changes` as
  LDIF without `userPassword` values, `newRdn`, `deleteOldRdn`,
  `newSuperior`, `ibm-changeInitiatorsName`), trimmed to the size.
- `projected` (`backend/projected`): §9, the service's.

## 9. Users: the administrator, the projected backend, binds

**The administrator** is `ibm-slapdAdminDN` (`cn=Administrator`) with
`ibm-slapdAdminPW`; it bypasses access control. Its password is unset
at install: until `CHGDIRSVRA DN(*ADMINDN 'password')` sets it, binds
as the administrator fail with 49. `QSSECCHK` reports an instance with
an unset or default administrator password (§12). `*UPDATEDN` is the
replication update DN, kept because the command takes it but used by
nothing (replication is refused, §14).

**The projected backend** shows the system's user profiles under
`os400-sys=SYSTEM` (the system's DNS name, as IBM): the root
(`os400-root`), `cn=accounts` (container) and one `os400-usrprf` entry
per profile, `os400-profile=NAME,cn=accounts,os400-sys=SYSTEM`, with
the `os400-*` attributes the schema defines, filled from the profile.
Group profiles are entries the same way, members listed
(`os400-groupmember`). The password and document password are never
shown. It is read-only through LDAP (`ibm-slapdOs400UsrprjRead: TRUE`;
`FALSE` stops the start with `GLD0232`; a write is 53
unwillingToPerform). Who sees what: an administrator session and a
profile with `*ALLOBJ` or `*SECADM` every entry; a session bound as a
profile `cn=accounts`, itself and the profiles it has `*READ` to;
anyone else (anonymous, a DN of the entry store) the root only, whose
entry shows only its object classes (as the primary shows). A hidden
entry resolves but is not given, so a search of it succeeds with no
entries, as the primary answers anonymous users; for a session that
does not see every profile a profile that does not exist answers the
same, so profile names cannot be found out by asking. The profiles come
through `backend::projected::Profiles`, which the service implements
over `sec` (`stgsvc/dirsrv/profiles.rs`).

**Binds:**

- simple, with a DN in a btree suffix: the entry's `userPassword` under
  its storage scheme (§11) and the password policy;
- simple, with a projected DN: the profile's password through `sec`
  (the same check sign-on makes): `*DISABLED` profiles, expired
  passwords (49 with the password policy response control's
  `passwordExpired`), `QMAXSIGN` counting and `QMAXSGNACN` acting on the
  invalid attempts, `QAUDJRN` `PW` entries for failures as for any
  sign-on (`SECURITY.md`). The service makes the check with the
  call every server's sign-on makes, `sec::signon::sign_on(txn, qsys,
  &Attempt { user, password, device: String::new(), console: false,
  sessions: 0, now, proof: None, ticket: false }, &Values::from_sysvals(..),
  &|_, _| None)`, in a transaction it commits whatever the answer (the
  count and a disabling are kept): it records the `PW` entry (`U` no
  such profile, `P` wrong password, `Q` disabled) with
  `sec::audit::record`, which the service drains after the request as
  for any other, counts `QMAXSIGN` and disables the profile under
  `QMAXSGNACN` 2 or 3 (with no device there is nothing to vary off).
  `Err(Refused)` answers 49 (`CPF1393`, `CPF1394` and `CPF1118` too);
  `Ok` with `Next::ChangePassword` is the expired password. The bound
  identity is the DN, and the
  profile's groups are its groups for access control. A profile with
  function usage `QIBM_DIRSRV_ADMIN` allowed binds as an administrator
  (IBM's administrative group of OS profiles);
- SASL `EXTERNAL` over TLS with a client certificate: the
  certificate's subject DN is the identity only when it names something
  the server knows: a projected DN of a profile that is enabled and has
  a password (the profile's groups and function usage as for a simple
  bind; no password is checked and nothing is counted), or else an
  entry with exactly that DN (no certificate mapping); anything else is
  49;
- anonymous, when `ibm-slapdAllowAnon: TRUE` (the default).

Refused (§14): `DIGEST-MD5`, `CRAM-MD5`, `GSSAPI` (Kerberos),
`OS400-PRFTKN` (QS/400 has no profile tokens), LDAP version 2 SASL.

## 10. Configuration

The instance's `ibmslapd.conf` is LDIF in IBM's path and IBM's entries
(`ibmslapd-shape.ldif` lists them); QS/400 reads and writes it with
`net/ldap::ldif` and honours these attributes (the rest of IBM's
attributes, met in a file, stop the start with a `GLD` message naming
them, unless they hold IBM's default; §14):

`ibm-slapdAdminDN`, `ibm-slapdAdminPW` (stored as a QS/400 hash,
`{SSHA512}`), `ibm-slapdPort`, `ibm-slapdSecurePort`,
`ibm-slapdSecurity` (`none`, `SSL`, `TLS`, `SSLTLS`), `ibm-slapdSizeLimit`,
`ibm-slapdTimeLimit`, `ibm-slapdIdleTimeOut`, `ibm-slapdWriteTimeout`,
`ibm-slapdAllowAnon`, `ibm-slapdDerefAliases`, `ibm-slapdSchemaCheck`,
`ibm-slapdSuffix` (each backend's), `ibm-slapdReadOnly`,
`ibm-slapdPwEncryption` (`ssha`, `sha`, `crypt`, `none`, plus QS/400's
`ssha256`, `ssha512`; `aes*` and `md5` refused), `ibm-slapdPagedResAllowNonAdmin`,
`ibm-slapdPagedResLmt`, `ibm-slapdSortKeyLimit`,
`ibm-slapdSortSrchAllowNonAdmin`, `ibm-slapdEnablePersistentSearch`,
`ibm-slapdMaxPersistentSearches`, `ibm-slapdLogOptions`, the `cn=Audit`
attributes (written to `QAUDJRN` as `DI` entries rather than IBM's
`audit.log`, §12), `ibm-slapdOs400UsrprjRead`, `ibm-slapdConfigPwdPolicyOn`
and the `cn=pwdPolicy Admin` attributes; QS/400's own
`ibm-slapdIndex` (attribute index list), `ibm-slapdChangeLogMaxEntries`,
`ibm-slapdReferral` (default referrals, LDAP URLs). `ibm-slapdIndex` is on
the `cn=Directory` backend entry, one value per attribute,
`ATTRIBUTE:KINDS` with kinds `p` presence, `e` equality, `s` substring
(`telephoneNumber:pe`); the six indexes every store has are not listed
there and cannot be removed. A new or changed index is built at the
instance's next start.

Changes through `cn=configuration` or `CHGDIRSVRA` are written to the
file at once and take effect at the instance's next start, except the
administrator's password, limits and logging, which take effect at
once. The shipped file is QS/400's text (its comments Quartz's), written
at the first IPL with `QUSRDIR`'s defaults as the primary has them,
except `ibm-slapdPwEncryption: ssha512`.

The model is `net/dirsrv/src/config/` (`instances`, `slapd`, `attrs`,
`edit`, `shipped`, `error`). `attrs::ATTRS` lists every attribute by
rule: honoured, with the values it takes; IBM's at its default (the
primary's value, `{instance}` in paths standing for the instance:
caches, reaping thresholds, the RDBM backend's database settings, the
plug-in lines, the schema file paths, transactions, replication,
digest, event notification); or kept as it is because IBM's server
makes the value up or keeps it secret (`ibm-slapdServerId`,
`ibm-slapdCryptoSync`, `ibm-slapdDbUserPW`, `ibm-slapdSslKeyDatabasePW`,
`ibm-slapdEnableRemotePWPExOps`), used for nothing.
`ibm-slapdMasterDN`/`ibm-slapdMasterPW` (`cn=Master Server`) are kept
for `CHGDIRSVRA DN(*UPDATEDN)` and used by nothing. Anything else, a
default attribute at another value, or a change record is `GLD0126`; a
bad value of an honoured one `GLD0232`. Changes edit the text in place
(order, comments and folding kept).

The shipped files (`config::shipped`, written by
`stgsvc/dirconf.rs` when the IFS starts and they are missing):
`idsinstinfo/idsinstances.ldif` (CCSID 819) with `QUSRDIR`, autostart
1; `idsslapd-QUSRDIR/etc/ibmslapd.conf` (CCSID 1208, owner `QDIRSRV`,
mode 600) with every entry of the primary's in its order, no
administrator password, no `ibm-slapdServerId` (the server makes one),
none of the kept secrets, `ibm-slapdConfigPwdPolicyOn: FALSE` with no
policy values, the projected suffix `os400-sys=` the system's DNS name
(`HOSTNAME.DMNNAME`, or the system name without one), and no user
suffix: the primary's `dc=` suffix was its administrator's choice, so a
QS/400 system starts with `cn=localhost` and `cn=ibmpolicies` and gets a
suffix of its own with another `ibm-slapdSuffix` on `cn=Directory`.

### 10.1 Publishing

IBM's publishing agents (`QGLDPUBE`, `QGLDPUBA`, always running in
`QSYSWRK` as `QDIRSRV`) copy system information to a directory server:
the user agent publishes users, the printer agent shared printers.
QS/400 has the same two jobs: `QGLDPUBE` publishes users, `QGLDPUBA`
printers. The logic is `net/dirsrv/src/publish/` (`config`, `map`,
`publisher`, `runner`, `source`), the jobs `work/qsjob/src/work/dirsrv/publish.rs`
(role `DIRPUB`; the job's name picks the agent, a turn every 10 seconds),
the service's side `lic/slic/src/stgsvc/dirsrv/publish.rs`.

**The configuration** is `/QIBM/UserData/OS400/DirSrv/publish.conf`
(IBM keeps it in Navigator's store; QS/400's own LDIF, CCSID 1208,
owner-only), changed through Navigator, QAP and `dir_admin`. A missing
file is the shipped state: both agents disabled. Its records are
content records; every attribute is QS/400's (`qs400-pub*`, compared
without case), each single-valued:

```
version: 1

dn: cn=publishing
qs400-pubFormat: 1

dn: cn=users,cn=publishing
qs400-pubEnabled: TRUE
qs400-pubHost: ldap.example.com
qs400-pubPort: 389
qs400-pubSecurity: STARTTLS
qs400-pubBindDN: cn=Administrator
qs400-pubBindPW:: <base64 of the sealed password>
qs400-pubParentDN: ou=Users,dc=example,dc=com
qs400-pubCreateParent: TRUE
qs400-pubRetry: 300
qs400-pubResync: 86400
```

| Attribute | Meaning | Default |
|---|---|---|
| `qs400-pubFormat` (`cn=publishing` only) | the format version, 1 | required |
| `qs400-pubEnabled` | `TRUE` or `FALSE` | `FALSE` |
| `qs400-pubHost` | the target server's host name or address | `localhost` (the local instance) |
| `qs400-pubPort` | 1 to 65535 | 389, 636 for `SSL` |
| `qs400-pubSecurity` | `NONE`, `SSL` (LDAPS), `STARTTLS` | `NONE` |
| `qs400-pubBindDN` | the DN bound as; absent binds anonymously | none |
| `qs400-pubBindPW` | the password, sealed | none |
| `qs400-pubParentDN` | where the entries go | `ou=Users` or `ou=Printers` over `cn=localhost` |
| `qs400-pubCreateParent` | add the parent and its missing ancestors | `TRUE` |
| `qs400-pubRetry` | seconds before the first retry, 10 to 86400 | 300 |
| `qs400-pubResync` | seconds between full resyncs, 0 for none, else 60 or more | 86400 |
| `qs400-pubPublisher` | the `publisherName` value | `cn=QS400 ` and the system name (`publisherName` has DN syntax, so a value of this attribute is a DN) |

The agents are `cn=users,cn=publishing` and `cn=printers,cn=publishing`;
another `cn=` under `cn=publishing`, an agent twice, an attribute this
table does not have, a value twice, a value out of range or a DN that
does not parse refuse the file (the job logs it and publishes nothing;
the writer never produces one). The password is sealed with the LIC
partition's key (`certs::stash::Sealer`, as `ADDSMTPLE *HOSTAUTH`
keeps the SMTP relay's) with the authenticated data `QS400 DIRSRV
PUBLISH` and the agent's name, so a sealed value copied to the other
agent, or to another system, does not open. Only the storage service
holds the key: it seals on a change and hands the publishing jobs the
password in the clear (1570). The format is frozen at V1R2M0
(`tools/fmtcheck`, `dirsrv-publish`).

TLS (`SSL`, `STARTTLS`) checks the target's certificate against the
shipped roots and the `*SYSTEM` store's trusted CAs, by name, and
presents the certificate assigned to `QIBM_GLD_DIRSRV_CLIENT` (QS/400's
ID `QS400_DIRECTORY_CLIENT`) when there is one.

**Users.** Every user profile except the shipped ones and the group
profiles is one entry, objectClass `top`, `person`,
`organizationalPerson`, `inetOrgPerson`, `publisher`:

- `uid`: the profile name (what identifies the entry);
- `cn`: the profile's text, else its name; `sn`: the last word of the
  text, else the name;
- `mail`: the profile's SMTP name. QS/400 has none (there is no system
  distribution directory, §14), so the service gives none and `mail` is
  left out; the mapping takes one when a source has it;
- `publisherName`: the agent's publisher value.

The entry is `cn=CN,PARENT`. When that DN is held by an entry that is
not this profile's (another profile with the same text, or anyone
else's), it is `cn=CN+uid=NAME,PARENT`.

**Printers.** Every printer device description with an output queue, but
the virtual printers of display sessions and the capture device, is
one entry, objectClass `top`, `printerService`, `printerIPP`,
`publisher`, named `printer-name=DEVICE,PARENT`: `printer-name` the
device, `printer-location` the device's text (left out when blank),
`printer-make-and-model` a LAN printer's `MFRTYPMDL`, `printer-uri`
`lpd://HOST/OUTQ` when the LPD server is an autostart server and the
system has a host name (QS/400's LPD takes the output queue's name as
the queue), else left out; `publisherName` as for users.

**Change events.** The service keeps, per kind, the records it last
listed and a log of what changed since (created, changed, deleted; the
last 1024 changes), numbered from an epoch taken when the service
starts. Profiles are listed again when a profile was written (the
security stamp moved), devices at each ask. A job asks for the changes
after its cursor (1571); a cursor of another epoch, or older than the
log, gets a resync: the job lists everything again (1572).

**The publisher.** Per agent: a queue of wanted changes (the last one
per name wins), the DNs it published, the connection, the status.

- A full resync (at start, when the service asks for one, every
  `qs400-pubResync` seconds, after a change of the configuration)
  searches the parent's subtree for `publisherName=<ours>`, adds or
  modifies every wanted entry, and deletes the published entries whose
  profile or device is gone. Entries left under a parent the
  configuration no longer names are not looked for again.
- An entry is found by its key (`uid` or `printer-name`) and the
  publisher value. A found entry is renamed (modify DN, old RDN
  deleted) when its DN no longer matches, then modified (the managed
  attributes replaced; an empty one removed); a new one is added. An
  add that meets an existing entry modifies it only when it carries our
  publisher value.
- Only entries carrying this agent's `publisherName` value are ever
  modified, renamed or deleted; others under the parent are left as
  they are.
- The parent is checked once per connection; when it is missing and
  `qs400-pubCreateParent` is `TRUE`, it and its missing ancestors below
  the server's matched DN are added (`ou` as `organizationalUnit`, `o`
  as `organization`, `dc` as `domain`, `cn` as `container`, `l` as
  `locality`; another type is an error).
- A target that cannot be reached, closes the connection, refuses the
  bind or answers busy (51), unavailable (52) or other (80) keeps the
  queue and is tried again after `qs400-pubRetry` seconds, doubled at
  each failure up to 64 times (and at most a day). Any other result of
  one entry is that entry's error: it is logged, counted, and left for
  the next resync.
- The status (enabled, connected, last success, last error and when,
  pending, published, next try) is reported to the service with each
  ask (1571) for Navigator and QAP.

The `QGLDPAPI` API (Publish an object, `QgldPubDirObj`) is built over
the same code when its names are settled from the header (`NEEDS-CODY`
4 covers the wire to an IBM target).

## 11. Passwords

`net/dirsrv/src/pwd/`.

- **Storage schemes** in `userPassword` (`pwd::scheme`): `{SSHA512}`,
  `{SSHA256}`, `{SSHA}` (the digest of password and salt, then the
  8-byte salt, base64), `{SHA}` (verify only), `{CRYPT}` with `$6$`
  and `$5$` (Drepper's SHA-crypt, `rounds=` honoured) and `$1$` (MD5
  crypt, verify only); a value with no known tag is clear text. A value
  added or replaced in clear is hashed with `ibm-slapdPwEncryption`
  (`ssha512` by default; `crypt` writes `$6$`; `none` stores it clear);
  a value already tagged is stored as given (IBM exports load). Values
  come back as stored, to those with read on the critical class (the
  administrator and owners, §12). The administrator's password is
  `ibm-slapdAdminPW` under the same schemes; unset, its binds are 49.
- **Password policy** (`pwd::policy`, draft-behera-ldap-password-policy
  as IBM applies it): IBM's global policy `cn=pwdpolicy,cn=ibmpolicies`
  (seeded off, `ibm-pwdPolicy: false`, as the primary has it), and,
  when that entry has `ibm-pwdGroupAndIndividualEnabled: TRUE`, group
  policies (`ibm-pwdGroupPolicyDN` on a static group of the user: the
  strictest of them, setting by setting) and an individual policy
  (`ibm-pwdIndividualPolicyDN` on the user's entry), each overlaying
  the one before; the policy is in force when the result's
  `ibm-pwdPolicy` is TRUE. `ibm-slapdConfigPwdPolicyOn` governs the
  administrator's own policy (`cn=pwdPolicy Admin`, §10), not this one.
  Honoured: `pwdMinLength`, `pwdCheckSyntax` (0, 1 clear values only, 2
  refusing pre-hashed ones), IBM's `passwordMinAlphaChars`,
  `passwordMinOtherChars`, `passwordMaxRepeatedChars`,
  `passwordMinDiffChars`, `pwdInHistory` (`pwdHistory` in the draft's
  `time#syntax#length#value` form), `pwdMinAge`, `pwdMaxAge`,
  `pwdExpireWarning`, `pwdGraceLoginLimit`, `pwdLockout`,
  `pwdMaxFailure`, `pwdFailureCountInterval`, `pwdLockoutDuration` (0:
  until an administrator removes `pwdAccountLockedTime`),
  `ibm-pwdAccountLocked`, `pwdMustChange` (an administrator's or
  another user's change sets `pwdReset`; the next bind succeeds with
  `changeAfterReset` and the session may only change its own password,
  53 otherwise), `pwdAllowUserChange`, `pwdSafeModify` (the old value
  deleted in the same modify). The quality rules do not bind the
  administrator. A bind's failures, lockout and grace logins are
  recorded in the entry (the caller commits a failed bind too). The
  response control (`1.3.6.1.4.1.42.2.27.8.5.1`) answers binds,
  modifies and password modify when the request carried it; a refused
  change is 19 (50 for `passwordModNotAllowed` and
  `mustSupplyOldPassword`).

## 12. Access control and auditing

IBM's model (non-filtered ACLs, `ibm-supportedacimechanisms`
1.3.18.0.2.26.2), `net/dirsrv/src/acl/` (the values and their
evaluation) and `ops/access.rs` (what is in force on an entry),
evaluated in the service:

- **Owners**: `entryOwner` values `[access-id:|group:|role:]DN`, and
  `ownerPropagate`. Owners have every right on the entry and below.
  An entry's owners are its own, or those of the nearest ancestor whose
  owners propagate (`ownerSource` names it), or the administrator
  (`ownerSource: default`).
- **ACLs**: `aclEntry` values `[access-id:|group:|role:]DN` followed by
  `:target:[grant:|deny:]permissions`, any number: `object` with `a`
  (add children) and `d` (delete), an access class (`normal`,
  `sensitive`, `critical`, `system`, `restricted`, §6.1) or `at.NAME`
  with `r` `w` `s` `c`; no keyword is a grant; a DN with no type is an
  `access-id`. `aclPropagate: FALSE` keeps an entry's ACL to itself:
  its children inherit from the next ancestor up. An entry's ACL
  replaces what it would inherit (no merging), as IBM's does.
  `ibm-effectiveAcl` prints the ACL in force in one canonical order
  (object, restricted, system, critical, sensitive, normal, `at.`);
  `aclSource` names its entry or `default`.
- **Pseudo DNs**: `cn=anybody` (everyone), `cn=authenticated` (every
  bound identity), `cn=this` (the entry, bound as itself).
- **Evaluation**: the administrator (and a session the `Identity` hook
  marks administrative) and the owners have every right. Otherwise each
  right is decided by level: the requester's own DN first
  (`access-id`, and `cn=this`), then its groups and roles (static
  groups found at bind: `groupOfNames`, `groupOfUniqueNames`,
  `accessGroup`, `accessRole` listing the DN in `member` or
  `uniqueMember`; plus the groups the `Identity` hook reports;
  `cn=authenticated`; `cn=anybody`). Within a level an `at.NAME` right
  counts before its class and a deny before a grant; the first level
  and target that says anything decides; nothing said is no right.
  `acl::eval`'s matrix test covers each case.
- **The default ACL** where no entry sets one is the primary's:
  `group:CN=ANYBODY:normal:rsc:system:rsc:restricted:rsc`.
- **Rights per operation**: search returns attributes with `r` and
  evaluates filter assertions with `s` (Undefined without); compare
  needs `c`; add `object:a` on the parent (a suffix's root, the
  administrator); delete `object:d` on the entry (each entry, for a tree
  delete); modify `w` on each attribute changed; modify DN `w` on the
  old and new RDN's attributes and, with a new superior, `object:a`
  there and `object:d` on the entry. A user may always change its own
  `userPassword` unless the policy says `pwdAllowUserChange: FALSE`.
  `aclEntry` and `entryOwner` are restricted-class (owners write them);
  their values are parsed on write (21 for a malformed one).
- Filter-based ACLs (`ibm-filterAclEntry`, `ibm-filterAclInherit`,
  mechanism 1.3.18.0.2.26.4) are refused on write (53) and listed in
  §14; the root DSE lists only the mechanism built.

**Auditing**: every bind, search, compare, add, modify, delete, modify
DN and extended operation reaches the `Audit` hook (`ops::AuditEvent`:
the operation, the DN it named, the session's DN, the result code).
The hook's implementation in the service writes it. Auditing goes to `QAUDJRN` as entry type `DI` when `ibm-audit: true`
and the operation's switch is on (`ibm-auditBind`, `ibm-auditAdd` ...;
`ibm-auditFailedOPonly` keeps the failures only): the service calls
`sec::audit::di::audit_di(op, &DiFields { .. })` with `op` one of
IBM's `DIOPER` codes (`Op::Bind` `BN`, `Op::PasswordFail` `PW`,
`Op::AuthFail` `AF`, `Op::Create` `CO`, `Op::Delete` `DO`,
`Op::Rename` `OM`, `Op::Change` `ZC`, `Op::Read` `ZR`,
`Op::AclChange` `CA`, `Op::OwnerChange` `OW`, `Op::PasswordChange`
`CP`, `Op::ConfigChange` `CF`, `Op::Unbind` `UB`, ...) and the
instance, connection, client address, TLS flag, bound DN, entry DN and
the old and new values. The entry is written when `QAUDLVL` selects
the operation's value: `*AUTFAIL` for `AF` and `PW`, `*CREATE`,
`*DELETE`, `*OBJMGT` for `CO`, `DO`, `OM`, object auditing
(`QAUDCTL(*OBJAUD)`) for `ZC` and `ZR`, and `*SECDIRSRV` (in
`*SECURITY`) for the rest. The layout is `SECURITY.md` §7 `AuditDI`
(audit layouts version 2), its fields IBM's `QASYDIJ4`
(`tools/conformance/oracle/dirsrv/qasydij4-primary.txt`); the value
sets of `DIACOD`, `DICCHG`, `DICCOD`, `DIBNDC` and `DIRTYP` are not
settled (NEEDS-CODY, §17 item 5). A failed bind as an OS profile also
writes `PW` (§9).

`QSSECCHK` checks each instance of the instance list (area `DIRSRV`,
`SECURITY.md` §10): anonymous binds allowed (`ibm-slapdAllowAnon:
TRUE`, a warning), no TLS (`ibm-slapdSecurity: none`, so simple binds
on 389 send passwords in the clear, a warning), and the administrator
password: unset is a warning (binds as the administrator are refused,
so nothing is open), one that verifies as a well-known password
(`secret`, `password`, `admin`, `administrator`, `root`, `qsecofr`, the
instance's name, in lower, upper and capitalized case) a failure. The
shipped `QUSRDIR` gives the three warnings.

## 13. The client, the tools, tests

**`QGLDCLNT`** (`runtime/ldapclnt`): IBM's LDAP C API as bound
procedures over `net/ldap::client`, `*SYSTEM` imports as `QSQCLI`'s are
(`ile::LDAP_CLIENT_PROCS` names them; `runtime/ldapclnt/src/abi.rs` gives
each its `sysv64` entry). The names are the union of the three headers'
(V7R1M0) and `QGLDCLNT`'s 7.5 exports (`api-names.txt`): 215 procedures,
159 built and 56 refused. The 20 exports that no header declares and
that are not the `ldap_`/`ber_` API (`decode_int`, `encode_len`,
`getLoadPtrs`, `ids_gethostbyname`, `timed_out`,
`GetInstalledSSLCiphers__Fv`, `ldap_client_table`, ...) are IBM's
internals and are not exported. `ber_*` are bound to `QGLDCLNT` too
(IBM's 7.5 list does not carry them; a program binds them by name).

- *The interface.* `runtime/ldapclnt/src/header/protos.txt` is the one
  table: every prototype, its header and whether it is refused. The
  numbers (`consts.rs`) are the LDAP C API draft's where it gives one
  (result codes, `LDAP_RES_*`, scopes, `LDAP_MOD_*`, the first
  `LDAP_OPT_*`) and QS/400's otherwise (`LDAP_OPT_UTF8_IO` 0x0E,
  `LDAP_OPT_SSL_TIMEOUT` 0x0B, `LDAP_OPT_MATCHED_DN` 0x33,
  `LDAP_OPT_EXT_ERROR` 0x34, `LDAP_SSL_*` 0x70 to 0x76). Structures as
  ILE C lays them out with 16-byte pointers: `berval` (length at 0,
  value at 16; 32 bytes), `LDAPControl` (OID 0, value 16, criticality
  48; 64), `LDAPMod` (operation 0, type 16, values 32; 48),
  `LDAPSortKey` (48), `LDAPURLDesc` (112), `LDAPVLVInfo` (80),
  `LDAPVersion` (`sdk_version` the release as VRM, `protocol_version`,
  `ssl_version` 13), `struct timeval` (two four-byte fields).
- *Handles and storage.* `LDAP *`, `LDAPMessage *` and `BerElement *`
  point into blocks the job's state owns (a chain of messages is one
  block, message `i` at offset 16·i). Everything returned (strings,
  arrays, `berval`s, controls, URL descriptions) is a block freed by its
  function (`ldap_memfree`, `ldap_value_free*`, `ldap_controls_free`,
  `ber_bvfree`, `ldap_free_urldesc`, `ldap_msgfree`); a free of storage
  the library did not give out does nothing. The `ldap_err2string` and
  `ldap_pwdpolicy_err2string` texts are Quartz's and kept for good.
- *Strings* are in the job's CCSID (`ldapclnt::configure`, else 37),
  converted to UTF-8 on the wire and back, unless `LDAP_OPT_UTF8_IO` is
  on; the `_utf8` functions take UTF-8; `ldap_set_iconv_local_codepage`
  and `_charset` change the local code page. `ldap_get_values` converts
  values; `ldap_get_values_len` and `LDAP_MOD_BVALUES` pass bytes. LBER
  passes bytes as given.
- *Sessions.* `ldap_init` connects at the first operation (a blank
  separated host list: `host`, `host:port`, `[v6]:port` or LDAP URLs;
  LDAPv3), `ldap_open` at once (LDAPv2, as the API defines it).
  `ldap_ssl_client_init` (a null key ring is the `*SYSTEM` store; the job
  refuses another with `LDAP_SSL_INITIALIZE_FAILED`),
  `ldap_ssl_environment_init` (the same; a stash file is
  `LDAP_NOT_SUPPORTED`), `ldap_ssl_init` (LDAPS; `name` is a client
  certificate label, which the job refuses: use an application ID),
  `ldap_ssl_start`, `ldap_app_ssl_client_init_np`, `ldap_app_ssl_init_np`
  and `ldap_app_ssl_start_np` (a DCM application ID: its CAs and, when
  the job may have it, its certificate, through `cert_app`),
  `ldap_start_tls_s_np` and `ldap_start_tls_app_np` (StartTLS).
  `ldap_stop_tls_s_np` ends the TLS session and the connection, since
  LDAP has no way back to the clear on one connection; the next
  operation connects again in the clear, unbound.
- *Options.* `LDAP_OPT_DEREF`, `SIZELIMIT`, `TIMELIMIT`, `REFHOPLIMIT`,
  `REFERRALS` (off by default), `PROTOCOL_VERSION`, `UTF8_IO`,
  `SSL_TIMEOUT`, `SERVER_CONTROLS`, `CLIENT_CONTROLS`, `ERROR_NUMBER`,
  `ERROR_STRING`, `MATCHED_DN`, `HOST_NAME` (set before connecting),
  `EXT_ERROR` (get), `RESTART` (on only); a null handle reaches the
  defaults new handles take. An integer option takes a pointer to an
  `int`; an on/off option a pointer to an `int`, null (off) or
  `LDAP_OPT_ON` (a pointer value that addresses nothing: on).
  `LDAP_OPT_DEBUG`, `REBIND_FN`, `REBIND_ARG`, `SSL_CIPHER`, `API_INFO`
  and `DESC` answer `LDAP_NOT_SUPPORTED`. No client control is built: a
  critical one fails the operation with `LDAP_NOT_SUPPORTED`.
- *Operations.* Simple and SASL `EXTERNAL` binds (other mechanisms are
  `LDAP_NOT_SUPPORTED`; a null DN or password takes
  `ldap_default_dn_set`'s and `ldap_password_set`'s), every search form
  (`_s`, `_st`, `_ext`, `_ext_s`, by URL; an `_ext` timeout is also the
  server's time limit), add, modify (`LDAP_MOD_INCREMENT` too), delete,
  `ldap_modrdn`, `ldap_rename`, compare, abandon and extended
  operations, each in its asynchronous (message ID), `_s`, `_ext` and
  `_ext_s` forms; `ldap_result` (`LDAP_MSG_ONE`, `ALL`, `RECEIVED`,
  `LDAP_RES_ANY`, a timeout). The synchronous forms follow referrals
  when `LDAP_OPT_REFERRALS` is on (the rebind callback is refused, so a
  referred server is bound anonymously); the asynchronous ones hand
  references to the program. `ldap_abandon_ext` with server controls is
  `LDAP_NOT_SUPPORTED`.
- *Controls* made: any (`ldap_create_control` from a `BerElement`),
  paged results, sort (and `ldap_create_sort_keylist`'s
  `[-]attr[:rule]` keys), proxied authorization, persistent search,
  assertion, virtual list view (draft-ietf-ldapext-ldapv3-vlv); parsed:
  paged results, sort, entry change, virtual list view, password policy
  (`ldap_parse_pwdpolicy_response(ctrls, &warning, &value, &error)`);
  lists: copy, insert, add, remove, merge, free.
- *LBER*: `ber_printf` letters `t b i e n o O s B v V { } [ ]`,
  `ber_scanf` letters `a s O o b e i B n l t T x v V { } [ ]`; the
  variable list is read from the call's stack words by the format (an
  `int` takes a word, a pointer an aligned pair; `ABI.md` §1).

The headers are QS/400's own `QSYSINC/H` members `LDAP`, `LBER`,
`LDAPSSL` (constants, structures and prototypes from the table,
Quartz's comments; a refused function says so), plus the
`QSYSINC/QRPGLESRC(LDAP)` copy member generated from the same table (a
QS/400 addition: IBM ships none; every prototype but the two variadic
ones, `pointer value options(*string)` for strings). They are generated
into `lic/osobjects/shipped/qsysinc/` (`QS400_UPDATE_QSYSINC=1 cargo
test -p ldapclnt` rewrites them), and `osobjects::qsysinc` makes the
library `QSYSINC`, the source files `H` (RCDLEN 92) and `QRPGLESRC`
(112) and the members at every IPL, rewriting a member that differs.
`QSYSINC` is not in a product load: the IPL makes it, as it makes
`QSTRUP`.

**QSH tools** (`qsh/qsh-utils/src/net/ldap/`): `ldapsearch`, `ldapadd`,
`ldapmodify`, `ldapdelete`, `ldapmodrdn`, `ldapchangepwd`, `ldapexop`,
`ldapcompare`, `ldapdiff`, with IBM's option letters (`qsh-tools.txt`)
and Quartz's usage text, over `net/ldap::client` on the job's own
sockets. Every tool takes `-h -p -D -w` (`-w ?` reads the password from
standard input), `-V`, `-Z` (LDAPS), `-Y` (StartTLS), `-K` (only the
`*SYSTEM` store), `-P`, `-N` (refused by the job: no certificate by
label), `-m EXTERNAL`, `-y` (proxied authorization), `-M` (manageDsaIT),
`-R` (do not follow referrals; they are followed by default), `-O`,
`-C` (the input's character set), `-v` and `-n`. The exit status is the
LDAP result code (89 for a usage error, 81 when the server cannot be
reached, 92 for a refusal).

- `ldapsearch`: `-b` (else `$LDAP_BASEDN`), `-s`, `-a`, `-z`, `-l`, `-A`,
  `-L` (LDIF; else `dn` then `attribute=value` lines, `-F` another
  separator), `-B` (values that are not text as they are), `-t` (values
  to files), `-q` paging with `-T` seconds between pages, `-o` sort
  (repeatable, `-attr` reverses), `-c types[:changesonly[:ecs]]` a
  persistent search (`any` or `add,delete,modify,moddn`), `-f`/`-i` (a
  filter per line, `%s` in the filter is the line), `-e` (the version).
- `ldapadd`, `ldapmodify`: LDIF records (`-f`/`-i` or standard input); a
  record without a change type adds with `-a` (always for `ldapadd`),
  else adds its values (`-r` replaces them); `-c`, `-e file` (the
  records that failed), `-b` (a value starting with `/` is that file's
  contents), `file://` values, `control:` lines.
- `ldapdelete` (`-s` the tree delete control, `-c`, `-f`),
  `ldapmodrdn` (`-r`, `-c`, `-f` pairs of lines), `ldapcompare`
  (`DN attr=value`, `-f` pairs; status 0 true, 5 false),
  `ldapchangepwd` (`-D -w -n`: binds and replaces `userPassword`).
- `ldapexop -op whoami` and `-op passwd [-a old] [-s new] [user]` (RFC
  3062; a generated password is printed). IBM's operations
  (`acctstatus`, `onlineBackup`, the log, replication and server
  control ones) are refused with a message.
- `ldapdiff -b base -sh host -ch host` (`-sp -sD -sw -sZ -sK -sP -sN` and
  the `-c` forms for the consumer): the LDIF that makes the consumer's
  subtree match the supplier's (adds parents first, deletes children
  first, modifies by replacing or deleting user attributes), to
  `-L file` or standard output; `-F` applies it; `-S` compares the
  schemas; `-C n` stops after `n` differences.

**`tools/ldaptest`**: an LDAP client from crates by other authors than
`net/ldap` (the agent picks the crate, `cargo deny` checked), scenarios
for every operation, control and extended operation, TLS (LDAPS,
StartTLS, EXTERNAL), ACLs, projected binds, LDIF round trips, paging a
million entries; `oracle` mode diffs the anonymous answers with the
primary's (root DSE shape, schema, result codes); under QEMU as steps
186 and on.

**Fuzz targets** (`tools/fuzz/src/targets.rs`): `ldapber`, `ldapmsg`,
`ldapfilter`, `ldapdn`, `ldif`, `ldapschema`, `ldapurl`, `dirsrvconf`.

**Performance** (recorded in §16): 10 000 binds a minute and a paged
search of 1 000 000 entries under QEMU.

## 14. Refused

Each answers as IBM's server answers a request it does not support, or
with the `GLD` message for configuration; nothing is accepted and
ignored.

- Replication (supplier, consumer, gateway, the `ibm-repl*` entries and
  extended operations): a later handoff. `*UPDATEDN` is kept but unused.
- SASL `DIGEST-MD5`, `CRAM-MD5`, `GSSAPI`, `OS400-PRFTKN`; Kerberos.
- Filter-based ACLs (`ibm-filterAclEntry`).
- IBM's controls `1.3.18.0.2.10.*` and extended operations
  `1.3.18.0.2.12.*` (transactions, log, configuration, connection and
  server operations, the account and policy queries, group evaluation,
  online backup): 12 for a critical control, 2 for an extended
  operation (§7's tables say why each).
- Pass-through authentication (`cn=Passthrough Authentication`).
- `STRTCPSVR SERVER(*DIRSRV) INSTANCE(name values)`: instance startup
  values are refused (`TCP1A05`; IBM's message for it was not captured);
  `ibm-slapdSecurity: SSLOnly` is not among the values taken (§10).
- `ibm-slapdPwEncryption` `aes128`, `aes192`, `aes256`, `md5`
  (`GLD0232`); `ibm-slapdSchemaCheck: V2` (`GLD0232`); IBM's other
  configuration attributes away from their defaults (`GLD0126`, §10).
- Language tags, event notification, the IBM web administration tool
  (Navigator replaces it), `idsldif2db`-style tools.
- The SNADS system distribution directory (`*DIRE`) is not LDAP and is
  out of scope.
- The client (`QGLDCLNT`, §13): each answers `LDAP_NOT_SUPPORTED` (an
  `int` function returns it; a pointer function returns null with it as
  the error): Kerberos (`ldap_krb_init_tkt`, `ldap_krb_cc_destroy`,
  `ldap_krb_ctx_destroy`); PKCS #11 and FIPS mode
  (`ldap_ssl_pkcs11_client_init`, `ldap_ssl_pkcs11_environment_init`,
  `ldap_ssl_set_fips_mode_np`); cipher strings (`ldap_set_cipher`,
  `LDAP_OPT_SSL_CIPHER`: `QSSLCSL` chooses); a key ring stash file; the
  rebind callback (`ldap_set_rebind_proc`, `LDAP_OPT_REBIND_FN`,
  `LDAP_OPT_REBIND_ARG`); `LDAP_OPT_DEBUG`, `LDAP_OPT_API_INFO`,
  `LDAP_OPT_DESC`, `LDAP_OPT_RESTART` off; client controls (a critical
  one); SASL mechanisms but `EXTERNAL` and `ldap_plugin_sasl_bind_s`,
  `ldap_plugin_pblock_get`, `_set`, `_free`; server location by DNS
  (`ldap_server_locate`, `ldap_server_free_list`,
  `ldap_server_conf_save`, `ldap_enetwork_domain_set`, `_get`); locales
  (`ldap_set_locale`, `ldap_get_locale`); friendly names
  (`ldap_friendly_name`, `ldap_free_friendlymap`);
  `ldap_proxy_dn_prefix` and its `_utf8` form; IBM's transactions
  (`ldap_start_transaction`, `ldap_end_transaction`,
  `ldap_prepare_transaction` and their `_s` forms, `ldap_get_tran_id`,
  `ldap_create_transaction_control`, `ldap_create_abort_`, `commit_` and
  `prepare_transaction_request`); IBM's own requests
  (`ldap_create_account_status_request`, `effective_pwdpolicy_request`
  and `ldap_parse_effective_pwdpolicy_response`, `get_file_request`,
  `group_eval_request`, `locate_entry_request`, `online_backup_request`,
  `resume_role_request`, `user_type_request`,
  `password_policy_bind_init_request`, `_finalize_request`) and controls
  (`ldap_create_group_control`, `_from_list`,
  `modify_groups_only_control`,
  `omit_group_referential_integrity_control`,
  `limit_num_values_control` with its parse and free,
  `chaining_audit_control`, `return_deleted_objects_control`), whose
  encodings IBM does not publish; `ldap_url_parse_internal` and
  `ldap_xlate_local_to_utf8_wrp`; `ldap_abandon_ext` with server
  controls. IBM's 20 internal exports are not exported at all.
- The QSH tools (§13): `-G` (a `DIGEST-MD5` realm), `-U` (a SASL user),
  `-m` but `EXTERNAL`, `-d` (debug traces), `-k` (IBM's server
  administration control), `-x`, `ldapsearch -j` and `-J` (IBM's limit
  on attribute values), `ldapadd`/`ldapmodify -t` (transactions), `-B`,
  `-g`, `-j`, `-l` (replication controls, also `ldapdelete` and
  `ldapmodrdn -l`), `ldapdiff -a -j -x -O`, a key database other than
  the `*SYSTEM` store (`-K`), a client certificate by label (`-N`), and
  `ldapexop`'s IBM operations: each a message and status 92.

## 15. Differences from IBM i (documented)

- Entries live in a `*DIRDB` object in `QUSRDIRDB`, not in DB2 tables;
  nobody reads IBM's tables, and `ibm-slapdServerBackend` says `QS400`.
- whoami, password modify, assertion, pre/post read and the change log
  are QS/400 additions.
- `vendorName` and `vendorVersion` are Quartz's; `ibmdirectoryversion`
  stays 6.1.
- Auditing goes to `QAUDJRN` rather than `audit.log`.
- The default password scheme is `ssha512`.
- QS/400 ships a `QRPGLESRC(LDAP)` copy member.
- An entry's DN is its RDN as written under its parent's DN as written
  (`CN=REPLICATION,cn=localhost`; IBM keeps each entry's DN as it was
  added: `CN=REPLICATION,CN=LOCALHOST`).
- Extensible match runs the rule named (the primary's matches nothing);
  `~=` is QS/400's approximate rule (§7).
- `entryDN` is answered; every entry has `entryUUID` and
  `ibm-entryuuid` (the primary's suffix entries lack the latter);
  `ibm-capabilitiessubentry` is not answered (QS/400 has no
  `cn=ibm-capabilities` entries).
- The root DSE has no `ibm-sasldigestrealmname` (no DIGEST-MD5) and
  no `ibm-supportedAuditVersion` (auditing is `QAUDJRN`'s); its
  `ibm-supportedacimechanisms` is 1.3.18.0.2.26.2 alone, its
  `supportedSASLMechanisms` `EXTERNAL`.
- `cn=monitor` has the counters QS/400 keeps (§8), its times as
  Generalized Time.
- QEMU step 187 diffs `ldaptest oracle` against the primary's record
  (`tools/conformance/oracle/dirsrv/ldaptest-primary.txt`). Beyond the
  items above (and §7's deliberate ones: the second paged search, the
  request after an abandoned persistent search), these remain, each
  QS/400's on purpose:
  - a base search of the root DSE whose filter is false is success
    with no entries (RFC 4511 §4.5.1); the primary answers 32;
  - `cn=` (an empty value) is a DN (RFC 4514 §3), so a search of it is
    32 (under no suffix); the primary says 34. `cn=\zz` is 34 on both;
  - `cn=schema` has `matchingRuleUse` (27 values) and QS/400's own
    types, classes, syntaxes and rules (`entryDN`, `entryUUID`,
    `structuralObjectClass` and their rules): 1631 attribute types, 429
    classes, 45 syntaxes, 39 rules against the primary's 1627, 428, 43
    and 34; 8 attribute names against 6;
  - `+` returns `entryDN`, `entryUUID`, `hasSubordinates`,
    `numSubordinates` and `structuralObjectClass` too (RFC 4512's
    operational attributes); the primary gives the last ones only
    when named;
  - the projected root's `objectClass` is `os400-root` and `top` (the
    superclass every structural class has); the primary lists
    `os400-root` alone;
  - the harness's forward of 636 accepts and closes when nothing
    listens, so `ldaps` reads end-of-file where the primary refuses the
    connection: the same answer, through QEMU's network.
- The client's numbers and structures where the LDAP C API draft gives
  none are QS/400's (§13): a program is compiled against QS/400's
  `QSYSINC` members, not IBM's. `ber_*` are `QGLDCLNT`'s.
  `ldap_stop_tls_s_np` ends the connection with the TLS session.

## 16. Tests and performance

(Filled as the work lands: the kits, the QEMU steps, the numbers.)

- `cargo test -p ldap`: the codecs against RFC examples and vectors
  written by hand from the ASN.1, `schema-primary.txt` and the IBM LDIF
  files round-tripped, a seeded mutation pass over every fuzz entry
  point, and `tests/client.rs` (the client against a fake server on the
  loopback: binds, the asynchronous face, paging, sorting, referrals,
  timeouts, a notice of disconnection, LDAPS and `StartTLS`).
- `cargo test -p dirsrv`: the schemes against `openssl passwd` answers,
  the policy's rules, the ACL matrix (`acl::eval`), filters, and over a
  store in memory (`tests/harness`) `ops_read.rs` (scopes, filters,
  attribute selection, limits, budgets, paging, sorting, compare,
  controls, the root DSE against `rootdse.ldif`), `ops_write.rs` (add,
  modify, delete, tree delete, modify DN and subtree moves, read
  controls, referrals, aliases, binds and schemes, extended operations,
  proxied authorization, the change log, persistent search, read-only,
  `cn=schema`, `cn=monitor`, the policy response) and `ops_access.rs`
  (inherited ACLs and owners, restricted attributes, passwords, policy
  quality, history, reset, expiry, grace, group and individual
  policies, the identity, audit and configuration hooks, a hiding
  backend).
- `cargo test -p dirsrv --lib admin`, `cargo test -p stg --lib
  diradmin`: the properties, suffixes, indexes and administrator over
  the shipped `ibmslapd.conf`, ACL values, `MemAdmin`, and every
  request across the wire into `MemAdmin` and back.
- `cargo test -p navigator --test dirsrv`: the Directory server pages
  against `tests/golden/nav/dirsrv-*.html` and every action;
  `cargo test -p qap-proto --test version12`, `cargo test -p qapd
  --test dirsrv`: QAP version 12.
- `cargo test -p ldapclnt`: every export against `protos.txt` (each
  built entry's arguments, pads included) and the oracle's names;
  `tests/api.rs` (the functions over a fake directory on the loopback:
  binds, every search form, the asynchronous face, entries and values,
  updates, paging, sorting, persistent search, control lists, options,
  `UTF8_IO`, extended operations, StartTLS, LDAPS, application IDs, DNs,
  URLs, code pages, LBER, refusals); `tests/ile.rs` (an IR program
  calling `ldap_init`, `ldap_simple_bind_s`, `ldap_search_s`, the
  entry walk and a variadic `ber_printf`); `tests/rpg.rs` (an ILE RPG
  program copying `QSYSINC/QRPGLESRC(LDAP)` compiled by QS/400's compiler
  and run: bind, search, the entry walk, `ldap_err2string`); the shipped `QSYSINC` members
  are current.
- `cargo test -p qsh-utils --test ldap`: the nine tools against a fake
  directory (text and LDIF output, paging, sorting, persistent search,
  filters from a file, `LDAP_BASEDN`, LDIF updates with `-c` and `-e`,
  deletes, renames, compares, password changes, `whoami` and `passwd`,
  `ldapdiff`, StartTLS and LDAPS, the refusals).
- `cargo test -p osobjects --test create`: `QSYSINC` and its members at
  the first IPL, nothing made the second time.

## 17. NEEDS-CODY

Each needs the primary with a bind this project does not make on its
own:

1. The projected DN form and a profile bind: on the primary, `ldapsearch
   -h localhost -D "os400-profile=QSECOFR,cn=accounts,os400-sys=SYSTEM"
   -w PASSWORD -b "os400-sys=SYSTEM" -s one "(objectclass=*)"` in QSH,
   and the same with `-s sub "(os400-profile=QSTEST)"`; save the output.
2. The administrator's view: `CHGDIRSVRA DN(*ADMINDN 'pw')` (only if
   Cody agrees to set it), then `ldapsearch -D cn=Administrator -w pw -b
   cn=configuration -s sub objectclass=*`; and an `ldapadd` of a test
   subtree under the `dc=` suffix, an ACL on it, and the answers of an
   anonymous and a bound user.
3. Apache Directory Studio against QS/400: browse, edit, LDIF import of
   50 000 entries, bind as an OS profile.
4. Publishing user profiles from QS/400 to QS/400 and from the primary
   to QS/400 (Navigator's Publishing on the primary).
5. The `DI` entry's values (only if Cody agrees: it turns directory
   auditing on): on a test instance of the primary with
   `ibm-audit: true` and `QAUDLVL` with `*SECDIRSRV *AUTFAIL *CREATE`,
   a failed bind, an add, a modify, an ACL change and a modify DN, then
   `CPYAUDJRNE ENTTYP(DI) OUTFILE(QTEMP/DI)` and `DSPPFM QTEMP/QAUDITDI`
   (the field list itself is settled, from `QSYS/QASYDIJ4`).

## 18. Acceptance

(The table lands with the work.)
