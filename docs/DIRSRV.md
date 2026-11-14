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
| Navigator, QAP | `net/navigator/src/routes/dirsrv/`, `work/qap-proto` version 12 (`Dirsrv`, service 14) | server properties, suffixes, schema, access, publishing |

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
QS/400 sends, chosen by their layouts:

| ID | Severity, data | When |
|---|---|---|
| `GLD010B` | 0, `*CHAR 10` | the publishing agents `QGLDPUBE` and `QGLDPUBA` end at once (publishing is not built yet) |
| `GLD0123` | 0, `*CHAR 8` | `CHGDIRSVRA` changed the instance (completion) |
| `GLD0126` | 40, two `*CHAR *VARY` | an attribute (`name: value`) or entry of `ibmslapd.conf` QS/400 does not honour, not at IBM's default (§10), and the file |
| `GLD0127` | 40, `*CHAR *VARY` | a configuration file missing or not LDIF |
| `GLD0201` | 40, `*CHAR 8` | `CHGDIRSVRA INSTANCE`: no such instance |
| `GLD0202` | 40, none | `CHGDIRSVRA DN` without a password |
| `GLD0203` | 40, `*CHAR 32` | `STRTCPSVR`/`ENDTCPSVR SERVER(*DIRSRV) INSTANCE`: no such instance |
| `GLD0215` | 40, `*CHAR 8` | `CHGDIRSVRA DN(*UPDATEDN)`: the instance names no update DN |
| `GLD0224` | 40, `*CHAR *VARY` | `CHGDIRSVRA DN`: not a distinguished name |
| `GLD0232` | 40, two `*CHAR *VARY` | a value of an honoured attribute QS/400 does not take (`ibm-slapdPwEncryption: aes256`, `ibm-slapdSchemaCheck: V2`) |

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
`Slot`. Until the instance job is registered, `STRTCPSVR` and
`ENDTCPSVR` resolve `INSTANCE` against the job's instance list
(`framework::dirsrv`, the list read by `work/qsjob/src/command/tcpsvr.rs`
through `dirsrv::config`; `GLD0203` for an unknown name) and then answer
as for every slot (`TCP1A04`, and for an end `TCP1A09`). `*DIRSRV` is in
the TCP/IP server table with autostart off.

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

**The job** (`work/qsjob/src/work/dirsrv/`): one job per instance,
named after it, user `QDIRSRV`, job description `QSYS/QDIRSRV`, in
`QSYSWRK`. It reads its configuration through the service, listens on
`ibm-slapdPort` (389) and, with a certificate assigned to
`QIBM_GLD_DIRSRV_SERVER` (QS/400's ID `QS400_DIRECTORY_SERVER`,
`net/certs/src/apps.rs`), on `ibm-slapdSecurePort` (636); StartTLS on
389 then works too, and without a certificate StartTLS answers 2 as
IBM's does. A thread per connection decodes messages with `net/ldap`
and sends each operation to the service; abandon, size and time limits
and paging are the job's bookkeeping, the data the service's.

**The service** (`lic/slic/src/stgsvc/dirsrv/`) holds, per instance,
the backends and the schema, and answers:

| Request | What |
|---|---|
| 1560 `dir_open` | the job's instance: its configuration, suffixes, limits, a session ID |
| 1561 `dir_bind` | simple bind or SASL EXTERNAL; the identity (DN, groups, admin or not) kept with the session; failures counted and audited (§9) |
| 1562 `dir_op` | add, modify, delete, modify DN, compare, an extended operation; one store transaction each |
| 1563 `dir_search` | a search started: candidates chosen, a cursor kept in the service |
| 1564 `dir_next` | the next entries of a cursor (at most N or B bytes per answer, so the single-threaded service never runs one long search) |
| 1565 `dir_close` | a cursor or a session ended |
| 1566 `dir_watch` | persistent search: changes under a base, delivered with 1567 |
| 1567 `dir_changes` | the changes since the last call (polled by the job's watcher thread, bounded wait) |
| 1568 `dir_admin` | `CHGDIRSVRA`, the configuration's changes, the instance list, Navigator's and QAP's questions |
| 1569 | reserved |
| 1570 `dirpub_config` | the publishing configuration for `QGLDPUBE` and `QGLDPUBA`: the file's text and the agents' passwords unsealed (only to `QDIRSRV`'s jobs) (§10.1) |
| 1571 `dirpub_events` | an agent's changes after its cursor, or a resync; the agent's status reported with the ask (§10.1) |
| 1572 `dirpub_list` | every user or printer to publish and the cursor they are as of (§10.1) |
| 1573-1579 | reserved |

ACL evaluation, schema checking and password policy run in the
service, beside the data, with the session's identity.

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
  `governingStructureRule`, the class `uidObject`;
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

All of RFC 4511: bind, unbind, search (every filter, every scope, alias
dereferencing per `derefAliases` and `ibm-slapdDerefAliases`, size and
time limits min'd with the server's for non-administrators, `typesOnly`,
`*` and `+` and `1.1`), compare, add, modify (add, delete, replace,
increment from RFC 4525), modify DN (new RDN, delete old RDN, new
superior, subtree moves), delete, abandon, extended. Referral objects
(`referral` class, `ref`) answer referrals and continuation references
(RFC 3296) unless manageDsaIT is sent; the server's default referral
(`ibm-slapdReferral`) answers for a DN outside every suffix.

Controls (each in `supportedControl` when built):

| OID | Control | |
|---|---|---|
| 1.2.840.113556.1.4.319 | paged results (RFC 2696) | built |
| 1.2.840.113556.1.4.473 / .474 | server-side sort (RFC 2891) | built |
| 2.16.840.1.113730.3.4.2 | manageDsaIT (RFC 3296) | built |
| 2.16.840.1.113730.3.4.3 | persistent search (draft-ietf-ldapext-psearch) | built |
| 2.16.840.1.113730.3.4.18 | proxied authorization v2 (RFC 4370) | built |
| 1.2.840.113556.1.4.805 | tree delete | built |
| 1.3.6.1.1.12 | assertion (RFC 4528) | built (QS/400 addition) |
| 1.3.6.1.1.13.1 / .2 | pre-read, post-read (RFC 4527) | built (QS/400 addition) |
| 1.3.6.1.4.1.42.2.27.8.5.1 | password policy response | built |
| 1.3.18.0.2.10.* | IBM's (server administration, omit group referential integrity, group authorization, limit number of attribute values, ...) | each decided in `ops`: built or refused with `unavailableCriticalExtension` when critical |

Extended operations:

| OID | Operation | |
|---|---|---|
| 1.3.6.1.4.1.1466.20037 | StartTLS (RFC 4511 §4.14) | built |
| 1.3.6.1.4.1.4203.1.11.3 | whoami (RFC 4532) | built (QS/400 addition) |
| 1.3.6.1.4.1.4203.1.11.1 | password modify (RFC 3062) | built (QS/400 addition) |
| 1.3.18.0.2.12.* | IBM's (transactions, effective password policy, account status, get attributes, user type, locate entry, online backup, log and configuration operations, replication) | each decided in `ops`; the replication ones refused (§14) |

Anything not built answers as IBM's server does to an unknown request:
protocolError (2), "unsupported extended operation".

**The root DSE** QS/400 answers: `namingContexts` (as configured,
upper-cased as IBM writes them), `subschemaSubentry: cn=schema`,
`supportedLDAPVersion` 2 and 3 (version 2 binds accepted, as IBM's),
`supportedControl`, `supportedExtension`, `supportedSASLMechanisms`
(`EXTERNAL` only), `vendorName: Quartz Systems`, `vendorVersion` the
release (`V1R2M0`), `ibmdirectoryversion: 6.1` (the IBM level whose
behaviour QS/400 follows, which IBM's tools test), `ibm-serverId`,
`ibm-slapdServerInstanceName`, `ibm-slapdServerBackend: QS400`,
`ibm-osregistrycontext`, `ibm-osregistrytype`, `security`, `port`,
`ibm-slapdSizeLimit`, `ibm-slapdTimeLimit`, `ibm-slapdDerefAliases`,
`ibm-supportedacimechanisms`, `ibm-opAttrClasses`,
`ibm-slapdisconfigurationmode`. `cn=monitor` gives the counters IBM's
does that QS/400 keeps; the rest are absent, not zero.

## 8. Backends

- `btree` (`backend/btree`): the user suffixes, `cn=localhost` and
  `cn=ibmpolicies`, over the entry store of §5.
- `config` (`backend/config`): `cn=configuration`, a view of the
  instance's `ibmslapd.conf`; the administrator reads and changes it
  (writes go to the file and take effect as §10 says). Others get 50.
- `schema` (`backend/schema`): `cn=schema`.
- `monitor` (`backend/monitor`): `cn=monitor`, read-only.
- `changelog` (`backend/changelog`): `cn=changelog` when
  `ibm-slapdChangeLogMaxEntries` is set (QS/400's addition; IBM 6.1 on
  IBM i has it only with replication): one `changeLogEntry` per change
  (draft-good-ldap-changelog), numbered, trimmed to the size.
- `projected` (`backend/projected`): §9.

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
Group profiles are entries the same way, members listed. It is
read-only through LDAP (`ibm-slapdOs400UsrprjRead: TRUE`; a write is 53
unwillingToPerform), and a user sees the entries their profile may
display (`*READ` to the profile, or `*SECADM`), anonymous users only the
root (as the primary shows).

**Binds:**

- simple, with a DN in a btree suffix: the entry's `userPassword` under
  its storage scheme (§11) and the password policy;
- simple, with a projected DN: the profile's password through `sec`
  (the same check sign-on makes): `*DISABLED` profiles, expired
  passwords (49 with the password policy response control's
  `passwordExpired`), `QMAXSIGN` counting and `QMAXSGNACN` acting on the
  invalid attempts, `QAUDJRN` `PW` entries for failures as for any
  sign-on (`SECURITY.md`). The bound identity is the DN, and the
  profile's groups are its groups for access control. A profile with
  function usage `QIBM_DIRSRV_ADMIN` allowed binds as an administrator
  (IBM's administrative group of OS profiles);
- SASL `EXTERNAL` over TLS with a client certificate: the certificate's
  subject DN, if an entry with that DN exists (or the
  `ibm-slapdCertMapping`-free rule: exactly the subject DN);
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
`ibm-slapdReferral`.

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
`publisher`, `runner`), the jobs `work/qsjob/src/work/dirsrv/publish.rs`,
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
| `qs400-pubPublisher` | the `publisherName` value | `QS400 ` and the system name |

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

**Printers.** Every printer device description with an output queue is
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
  profile or device is gone.
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

- Storage schemes in `userPassword`: `{SSHA512}`, `{SSHA256}`, `{SSHA}`,
  `{SHA}` (verify only), `{CRYPT}` with `$6$`, `$5$` and `$1$` (verify
  only for `$1$`), and clear text only when `ibm-slapdPwEncryption: none`
  (refused by default: an add or replace of a clear value is hashed with
  the configured scheme). Values come back as stored, to the
  administrator only (`userPassword` is critical-class, §12).
- Password policy: IBM's `cn=pwdpolicy,cn=ibmpolicies` global policy and
  `ibm-pwdPolicy`/`ibm-pwdIndividualPolicyDN`/`ibm-pwdGroupPolicyDN`
  individual and group policies (draft-behera-ldap-password-policy as
  IBM applies it): minimum length, quality, history, minimum and maximum
  age, expiry warning, grace logins, lockout after failures and its
  duration, must change after reset, safe modify; the response control.
  Off unless `ibm-slapdConfigPwdPolicyOn: TRUE`, as IBM ships it.

## 12. Access control and auditing

IBM's model (non-filtered ACLs, `ibm-supportedacimechanisms`
1.3.18.0.2.26.2), evaluated in the service:

- `entryOwner` (`access-id:DN`, `group:DN`, `role:DN`) and
  `ownerPropagate`; owners have every right on the entry and below.
  Inherited from the nearest ancestor with an explicit owner
  (`ownerSource` names it); the default owner is the administrator.
- `aclEntry` values `[access-id:|group:|role:]DN:` + object rights
  (`object:a` add children, `object:d` delete) and attribute-class
  rights (`normal:rsc`, `sensitive:rwsc`, `critical:...`,
  `system:...`, `restricted:...`, `at.NAME:grant:rwsc` and `deny:`),
  `aclPropagate`; `aclSource` and `ibm-effectiveAcl` computed. Pseudo
  DNs `cn=anybody`, `cn=authenticated`, `cn=this`. Deny wins over grant
  at the same level; the most specific level wins.
- The default ACL on a new suffix: `group:cn=anybody:normal:rsc`
  (read, search, compare of normal attributes), as IBM's.
- Filter-based ACLs (`ibm-filterAclEntry`, mechanism 1.3.18.0.2.26.4)
  are refused on write (53) and listed in §14; the root DSE lists only
  the mechanisms built.

Auditing goes to `QAUDJRN` (entry type `DI`, directory service, with
IBM's subtype letters where IBM documents them) when `ibm-audit: true`
and the operation's switch is on; `PW` for failed profile binds as
§9. `QSSECCHK` learns three checks: anonymous binds allowed, simple
binds accepted without TLS on 389 (`ibm-slapdSecurity: none`), an
administrator password unset or one of the well-known defaults.

## 13. The client, the tools, tests

**`QGLDCLNT`** (`runtime/ldapclnt`): the 205 exports
(`api-names.txt`), each a bound procedure with IBM's C signature over
`net/ldap::client`; the `LDAP *` handle a pointer into a per-activation
table; strings in the job's CCSID converted to UTF-8 and back unless
`ldap_set_option(LDAP_OPT_UTF8_IO)`; `ldap_app_ssl_*` take a DCM
application ID. A function QS/400 does not build returns
`LDAP_NOT_SUPPORTED` (0x5C) and is listed in §14. The headers are QS/400's
own `QSYSINC/H` members `LDAP`, `LBER`, `LDAPSSL` (constants and
prototypes as the interface needs, Quartz's comments), plus a
`QSYSINC/QRPGLESRC(LDAP)` copy member generated from the same table (a
QS/400 addition: IBM ships none).

**QSH tools** (`qsh/qsh-utils/src/net/ldap/`): `ldapsearch`, `ldapadd`,
`ldapmodify`, `ldapdelete`, `ldapmodrdn`, `ldapchangepwd`, `ldapexop`,
`ldapcompare`, `ldapdiff`, with IBM's option letters (`qsh-tools.txt`);
`LDAP_BASEDN` from the environment as IBM's; their usage text Quartz's.
`ldapexop -op` takes the operations built; `ldapdiff` compares two
servers' subtrees (no replication needed).

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
- Pass-through authentication (`cn=Passthrough Authentication`).
- `ibm-slapdPwEncryption` `aes128`, `aes192`, `aes256`, `md5`
  (`GLD0232`); `ibm-slapdSchemaCheck: V2` (`GLD0232`); IBM's other
  configuration attributes away from their defaults (`GLD0126`, §10).
- Language tags, event notification, the IBM web administration tool
  (Navigator replaces it), `idsldif2db`-style tools.
- The SNADS system distribution directory (`*DIRE`) is not LDAP and is
  out of scope.

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

## 16. Tests and performance

(Filled as the work lands: the kits, the QEMU steps, the numbers.)

- `cargo test -p ldap`: the codecs against RFC examples and vectors
  written by hand from the ASN.1, `schema-primary.txt` and the IBM LDIF
  files round-tripped, a seeded mutation pass over every fuzz entry
  point, and `tests/client.rs` (the client against a fake server on the
  loopback: binds, the asynchronous face, paging, sorting, referrals,
  timeouts, a notice of disconnection, LDAPS and `StartTLS`).

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

## 18. Acceptance

(The table lands with the work.)
