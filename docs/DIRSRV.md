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
| Commands | `command/sys-cpps/cmds/chgdirsvra.cmd`, `STRTCPSVR.cmd`, `ENDTCPSVR.cmd`; `lic/slic/src/cpp/dircmd.rs` | `CHGDIRSVRA`, `*DIRSRV` with `INSTANCE` |
| Messages | `runtime/msgf/msgs/qgldmsg.toml` | `QSYS/QGLDMSG`, IBM's IDs, severities and formats, Quartz text |
| The client API | `runtime/ldapclnt` | `QGLDCLNT`'s exports as bound procedures over `net/ldap::client`; QS/400's own `QSYSINC/H(LDAP, LBER, LDAPSSL)` and `QRPGLESRC(LDAP)` |
| QSH tools | `qsh/qsh-utils/src/net/ldap/` | the nine tools over `net/ldap::client` |
| Test client | `tools/ldaptest` | outside the workspace with its own lockfile, wire code by other authors than `net/ldap` (§13) |
| Navigator, QAP | `net/navigator/src/routes/dirsrv/`, `work/qap-proto` version 12 (`Dirsrv`, service 14) | server properties, suffixes, schema, access, publishing |

The server runs as the job; the data lives in the storage service. Every
operation is a request from the job to the service (§4), as the NFS
daemons' file operations are.

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
  extensible match; a depth limit (64).
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
| 1569-1579 | reserved |

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
is UTF-8. `DIRDB_VERSION` (`net/dirsrv/src/store/format.rs`) is 1.

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
descriptions (`DESC`) are Quartz's.

The shipped schema is QS/400's own files in
`/QIBM/ProdData/OS400/DirSrv/etc/` (`qs.user.at`, ...), embedded in the
build; changes made through `cn=schema` go to the instance's
`etc/V3.modifiedschema`, as IBM's do. Schema checking is
`ibm-slapdSchemaCheck: V3_lenient` by default (`V3` strict; `V2` is
refused). Syntaxes and matching rules: the RFC 4517 set the shipped
attributes use, each with its normalizer; an attribute with an unknown
syntax compares octet by octet.

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

### 10.1 Publishing

IBM's publishing agents (`QGLDPUBE`, `QGLDPUBA`, always running in
`QSYSWRK` as `QDIRSRV`) copy system information to a directory server:
the user agent publishes users, the printer agent shared printers.
QS/400 publishes:

- users: one `inetOrgPerson` (plus `publisher`-tagged) entry per user
  profile, named `cn=TEXT-or-NAME` under the configured parent DN
  (`ou=Users,...` by default), with `uid` the profile name, `cn` and
  `sn` from its text, `mail` from the profile's SMTP name when one
  exists, `publisherName: QS400 SYSTEM`, kept in step as profiles are
  created, changed and deleted (the publishing jobs read the service's
  profile change events);
- printers: one entry per printer device with an output queue, under
  `ou=Printers,...`, with IBM's `printerService`-free attributes
  (`printer-name`, `printer-location`, `printer-uri` from RFC 3712's
  `printerIPP` class).

The publishing configuration (target server, port, bind DN and
password, TLS, parent DNs, which agents) is
`/QIBM/UserData/OS400/DirSrv/publish.conf` (QS/400's own LDIF, one entry
per agent; IBM keeps it in Navigator's store), changed through
Navigator, QAP and `dir_admin`. The target may be the local instance or
any LDAP server. The `QGLDPAPI` API (Publish an object, `QgldPubDirObj`)
is built over the same code when its names are settled from the header
(`NEEDS-CODY` 4 covers the wire to an IBM target).

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
- `ibm-slapdPwEncryption` `aes128`, `aes192`, `aes256`, `md5`.
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
