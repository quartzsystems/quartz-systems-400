# The update service protocol

How a QS/400 system learns what fixes exist and fetches them: Quartz's
own protocol over HTTPS, spoken by `SNDPTFORD`, `QSCHKUPD`,
`SNDSRVRQS` and `WRKPTFGRP`'s "newer level" marks. IBM's systems reach
IBM's Electronic Customer Support; QS/400 reaches a Quartz update
service instead, and the commands keep IBM's parameters. The client,
the codec and a reference server are `ptf/update`; the commands are
`lic/slic/src/cpp/updcmd.rs`. The real service is operated later; the
reference server stands in for it in the tests and the harness.

## 1. Trust

Everything the service says that matters is signed, so a mirror, a
proxy or plain HTTP cannot change what a system installs:

- the **catalog** (§3) is signed with ed25519 by the build key, the
  key the system trusts for its own objects (`/qs400/keys/system.pub`,
  `docs/PTF.md` §6). A catalog that does not verify is refused
  (`CPF8C32`);
- every **PTF** is a signed save stream (`docs/PTF.md` §1), checked
  again by `LODPTF`; the catalog adds its size and SHA-256, which the
  client checks on download;
- every **group manifest** is signed as `docs/PTF.md` §4 describes.

The transport is HTTPS (TLS through `net/tls`, the server checked
against the shipped roots and the trusted CAs of `*SYSTEM`). `http://`
URLs are accepted for local mirrors and tests; the signatures still
hold.

## 2. Requests

HTTP/1.1, one request per connection (`Connection: close`), bodies
sized by `Content-Length`. Every request carries:

| Header | Value |
|---|---|
| `User-Agent` | `QS400/VxRyMz build` |
| `X-QS400-System` | the system name (`SYSNAME` network attribute) |
| `X-QS400-Release` | `VxRyMz` |

Paths are under the service URL `QSCHGUPDA` keeps (`URL`, default
`https://updates.quartz.systems/qs400`):

| Method and path | Answer |
|---|---|
| `GET /v1/catalog` | the signed catalog, `text/plain; charset=utf-8` |
| `GET /v1/ptf/QSnnnnn` | the PTF's save stream, `application/octet-stream` |
| `GET /v1/group/NAME` | the group's signed manifest at its current level |
| `POST /v1/order` | an order (§4); the answer is the resolved set |
| `POST /v1/service` | a service request (`SNDSRVRQS`); the answer is one line |

Status codes: `200`; `400` for a request that does not parse, with a
line of text; `404` for an unknown PTF or group; `5xx` for the
service's own trouble. A client treats anything else as a failure of
the connection (`CPF8C24`).

A proxy (`QSCHGUPDA PROXY('host:port')`) is used with `CONNECT` for
`https://` and with absolute-form request targets for `http://`.

## 3. The catalog

UTF-8 TOML, then one line `# signature <128 hex digits>`: the ed25519
signature of every byte before that line (the newline that ends the
body included). The signed bytes are the canonical bytes: the client
verifies them as received and parses only what verified; it never
re-serializes. The publisher writes keys in the order below, one
`[[table]]` per entry, sorted by ID.

```toml
format = 1
sequence = 42              # grows with each catalog; older ones are refused
generated = 1793318400     # Unix seconds

[[product]]
id = "5770SS1"
option = "0000"
release = "V1R0M0"
text = "Quartz Systems/400 Operating System"

[[ptf]]
id = "QS00012"
product = "5770SS1"
option = "0000"
release = "V1R0M0"
kind = "immediate"         # immediate, delayed or lic
hiper = false
prereqs = ["QS00010"]
coreqs = []
supersedes = ["QS00007"]
size = 81920               # bytes of the save stream
sha256 = "9f0c…"           # of the save stream
text = "Corrects the job log wrap"

[[group]]
name = "QS99001"           # QS99nnn, Cyyddd110 or the HIPER group
kind = "group"             # group, cumulative or hiper
level = 7
release = "V1R0M0"
text = "Database group"
ptfs = ["QS00010", "QS00012"]
related = []
```

- A **cumulative package** is a group of kind `cumulative` named
  `Cyyddd110` (the `release` crate's `Cumulative`); `*CUMPKG` names the
  newest for the release.
- The **HIPER group** is the one group of kind `hiper` for a release
  (Quartz names it `QS99100`); `*HIPER` names it. A PTF with
  `hiper = true` is in it.
- `sequence` is the anti-rollback counter: `QSCHKUPD` keeps the highest
  it has seen and refuses a catalog with a lower one (`CPF8C32`).

## 4. Orders

An order names PTFs, groups, `*CUMPKG` or `*HIPER` and asks the
service to resolve them. Body (text, one item per line):

```text
QS400ORDER 1
release V1R0M0
order *REQUIRED            # or *PTFID
reorder *NO                # or *YES
part *ALL                  # or *CVRLTR
item QS00012
item QS99001
item *CUMPKG
have QS00010               # PTFs this system has loaded or applied
```

Answer:

```text
QS400ORDERED 1
order 0000017
ptf QS00010 requisite
ptf QS00012 requested
group QS99001 7
missing QS00999
```

Resolution (`ptf_update::order::resolve`, the same function the
client runs over the signed catalog to check the answer):

1. Each item expands: a PTF to itself; a group, `*CUMPKG` or `*HIPER`
   to its PTFs, and the group is listed so its manifest is fetched. An
   item the catalog does not have is `missing`.
2. With `ORDER(*REQUIRED)`: a PTF the catalog shows superseded is
   replaced by the newest PTF that supersedes it, and prerequisites and
   co-requisites are added, transitively. With `ORDER(*PTFID)` only the
   PTFs named (and a group's members) are ordered.
3. With `REORDER(*NO)` a PTF in `have`, or superseded by one in `have`,
   is left out; `REORDER(*YES)` orders it again.
4. The PTFs come back requisites first (a topological order, ties by
   ID), each with why it is there: `requested`, `requisite`,
   `supersedes` or `group`.

The client then fetches each PTF with `GET /v1/ptf/ID`, checks its size
and SHA-256 against the catalog, and keeps it as save file
`QGPL/QSnnnnn` (`LODPTF DEV(*SAVF)` reads it). A PTF whose digest does
not match is not kept (`CPF8C32`).

## 5. Service requests

`SNDSRVRQS ACTION(*TEST)` posts:

```text
QS400SERVICE 1
action *TEST
system NAME
contact Name of the contact person
```

and the service answers one line, `ok <ticket>`. Problem reports are a
later phase; the request exists so the connection can be tested.

## 6. Update attributes

`QSCHGUPDA` keeps the client's settings in data area `QSYS/QSUPDATTR`
(`*CHAR 2000`), one `key=value` per line:

| Key | `QSCHGUPDA` | Meaning |
|---|---|---|
| `url` | `URL` | the service URL |
| `proxy` | `PROXY` | `host:port`, or empty |
| `schedule` | `SCHEDULE` | `*NONE`, `*DAILY` or `*WEEKLY` |
| `time` | `SCDTIME` | `HHMMSS` of the scheduled check |
| `last` | | Unix seconds of the last check |
| `sequence` | | the highest catalog sequence seen |

`QSYSCOMM1` asks the service hourly whether a scheduled check is due;
the check is `QSCHKUPD`: the catalog is fetched, verified and compared
with the PTF database and the groups the system knows; when a newer
group level or a new HIPER PTF exists, a message goes to `QSYSOPR`
(`CPI3E01`) and `WRKPTFGRP` marks the group.

## 7. The reference server

`ptf_update::server::Server` serves a directory:

```text
catalog.toml        the signed catalog
ptf/QSnnnnn.savf    each PTF's save stream
group/NAME.toml     each group's signed manifest
```

`ptf_update::publish` writes that directory from PTF streams and group
definitions with a signing key (the build does this for a release). The
server speaks plain HTTP or TLS (a `rustls::ServerConfig` from
`net/tls`), keeps an order counter, and answers service requests with a
ticket. It is a library, not a product: the tests and the harness run
it on loopback.
