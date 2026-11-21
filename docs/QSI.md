# QSI: the management protocol

QSI is the protocol a management console (QS/MC) speaks to what it
manages: a QS/VS host or a standalone QS/400. Its partition
scope, the channel between a partition and its hypervisor, is
`docs/QSID.md` (`lic/qsipart`). This document is the rest: the
transport, the session, pairing, and every family's messages.

Three repositories build the same messages, the same session and the
same trust rules, so they live here and QS/VS and QS/MC pin them:

| Crate | What it is | Of this repository it takes |
|---|---|---|
| `lic/qsi` | the hello, the families, their messages, versions and capabilities | `ipc`, `qsipart` |
| `lic/qsisess` | the session over any byte stream: handshake, requests with replies, events, streams, keep-alive | `ipc`, `qsipart`, `qsi` |
| `lic/qsitrust` | the trust rules: mutual TLS 1.3 with pinned certificates over graviola, and pairing | nothing |

All three are neutral (`tools/buildkit/tests/neutral.rs`: they reach
no other crate of this repository and their code names no product),
and `build fixture` builds them from outside as git dependencies.
A standalone QS/400 runs its own listener (section 11, `docs/QSID.md` section 12).

Handoff 24's messages (bits 19 to 27, section 5) are in `lic/qsi` from
commit `fb85e1480`; QS/VS and QS/MC move their pins to `5a5b7e29f`,
the last of Part A (the tests, the fuzz seeds, the format lock and this
document).

## 1. The transport

QSI runs over TCP; QS/VS listens on port 2300. On the connection, TLS
1.3 and nothing older, with graviola's cryptography through
`rustls-graviola` (`qsitrust::tls::server_config`, `client_config`).
Both ends present a certificate (mutual authentication) and both name
the application protocol `qsi/1` (ALPN). Nothing is resumed: no
session tickets, no session cache, so every connection is a full
handshake judged against the pins as they are then, and a pin removed
takes effect at the next connection.

Inside TLS the bytes are frames with `qsipart`'s header
(`docs/QSID.md` section 2: the magic `QI`, the kind, the length, the
check; a body of at most 32,760 bytes). Junk is not expected inside
TLS, so a byte the reader passes over closes the session (`QSI0109`)
instead of costing a byte as it does on a virtio port.

## 2. Identities and pins

An end's identity is an Ed25519 key and a self-signed X.509
certificate naming the end (common name) and its serial number, valid
from its making with no end (`99991231235959Z`), not a certificate
authority, key usage `digitalSignature`. QS/VS's `base/identity` makes
one at a host's first IPL; `qsitrust::Identity::make` makes the same
(`Identity::from_seed` with the same inputs gives the same bytes, a
test compares them with the bytes QS/VS's code writes), and
`Identity::from_pem` reads QS/VS's two files.

A certificate is trusted by its pin and nothing else: the SHA-256 of
its DER (`qsitrust::Fingerprint`). No certificate authority, no chain,
no host name, no dates. What the handshake still checks is that the
peer holds the certificate's key: its signature, Ed25519 only.

Each end gives the TLS configuration its `Pins` (a trait: `pinned(fp)`
and `pairing_open()`); the crate keeps nothing. At each handshake the
peer's certificate is accepted when it is pinned, or, while the
caller has pairing open, whatever it is; `qsitrust::tls::peer` then
says which (`Peer::pinned`). A console asks a host it has pinned and
nothing else; a host asks consoles it has pinned, and, while its
operator has a pairing password out, lets an unpinned console in to
pair and nothing else (section 4).

## 3. Pairing

A console and a managed system come to pin each other once; after
that every connection is mutual TLS between pinned certificates.

1. The managed system's operator asks for a pairing password: sixteen
   characters of Crockford's base32 in four groups
   (`qsitrust::pair::one_time_password`, 80 bits), shown once. The
   managed side keeps it, its expiry and its attempts
   (`qsitrust::pair::Guard`); while it is out, the host's `Pins` say
   `pairing_open`.
2. The console connects with its own certificate. Both ends have
   pairing open, so TLS completes; the managed side's session sees an
   unpinned peer and answers the pairing family and nothing else
   (`QSI0101`).
3. Each end takes the connection's binding (`qsitrust::tls::binding`):
   its own fingerprint, the peer's, and 32 bytes exported from this
   connection's keys (label `EXPORTER-qsi-pairing`).
4. The console runs SPAKE2 (RustCrypto's `spake2` 0.4, Ed25519 group)
   as A with the password as typed (`normalize`: upper case, no spaces
   or dashes, `O` as `0`, `I` and `L` as `1`), its own fingerprint as
   A's identity and the host's as B's, and sends `pair` `Start`: the
   message and its name.
5. The managed side asks its guard for the password (one attempt
   counted; none out, expired or spent is `QSI0201`), runs SPAKE2 as
   B, and answers `Answer`: its message, its name, and its
   confirmation, an HMAC-SHA-256 under the shared key of the label
   `qsi pairing 1 managed` and the transcript.
6. The transcript is both fingerprints (console first), the exported
   value, both SPAKE2 messages and both names, each with its length.
7. The console checks the managed side's confirmation (a wrong
   password fails here) and sends `Confirm`: its own, under the label
   `qsi pairing 1 console`. The managed side checks it (a failure is
   `QSI0202` and the guard hears of it) and answers `Done`, giving the
   console's fingerprint and name to the guard to pin.
8. The console pins the managed side's fingerprint on `Done`. Both
   close; the console connects again, now pinned both ways.

What this gives:

- **A listener** learns nothing of the password: SPAKE2.
- **A console without the password** gets one guess per attempt: SPAKE2
  allows no offline test of a guess, and the guard counts attempts.
- **Someone in the middle** must hold a certificate of its own on each
  leg; each end's transcript names the fingerprints and the exported
  value it saw, which are not the other's, so the confirmations fail
  even when the relay passes every message through unchanged and
  knows the password.
- **A replay** fails: the managed side's SPAKE2 message is new each
  time, and so is the exported value of a new connection.

The crate keeps no state: counting attempts, expiring the password and
storing pins are the caller's.

## 4. The session

`qsisess::Session` is the session as a state machine over any byte
stream: it is fed bytes and the time (`feed(bytes, now_ms)`,
`tick(now_ms)`), gives bytes to send (`take_output`) and hands up what
happened (`poll` → `Incoming`). It has no sockets, no threads and no
TLS, so a server and a client use it unchanged and it is tested in
memory (`qsisess::loopback`). A session has a role: `Console` (the
client: asks, reads events) or `Managed` (a host or a standalone
system: answers, pushes events).

### Frames

Every message is a frame with `qsipart`'s header; the session's kinds
are `0x10nn`, beside the partition scope's `0x00nn` to `0x03nn`.
Bodies are `ipc::Wire` values read leniently.

| Kind | Name | Body |
|---|---|---|
| `0x1001` | hello | `qsi::Hello` |
| `0x1002` | ping | `token u64` |
| `0x1003` | pong | `token u64` |
| `0x1004` | goodbye | `msgid` (empty for a clean close), `text` |
| `0x1010` | request | `total u32`, then the first part of the envelope |
| `0x1011` | reply | `total u32`, then the first part |
| `0x1012` | event | `total u32`, then the first part |
| `0x1013` | part | the next part of the message before it |
| `0x1020` | data | `stream u32`, then bytes |
| `0x1021` | credit | `stream u32`, `grant u32` |
| `0x1022` | close | `stream u32`: no more data this way |
| `0x1023` | reset | `stream u32`, `msgid`, `text`: the stream is aborted both ways |

A request, a reply or an event longer than one frame is sent in parts:
its first frame says the whole length, `part` frames follow with
nothing between them. The whole is bounded (`max_message`).

The envelopes:

- **request**: `id u32`, `user` (the console's user who asked, for the
  managed side's log; it grants nothing), `family u16`, `op u16`,
  `body`.
- **reply**: `id u32`, `family u16` and `op u16` (the request's, so a
  reply decodes without the request), `stream` (an option: the stream
  the reply opened), `outcome`: a result `{body}` or a refusal `{msgid,
  text, help, also}`, where `also` is the further messages that apply,
  each `{msgid, text, help}`, in order (a host refuses with every
  message that applies, as QS/VS does).
- **event**: `seq u64`, `family u16`, `op u16`, `body`.

### The hello

After TLS both sides send a hello at once (`qsi::Hello`): the protocol
version, the capability bitmap, what the side is (`PeerKind`: console,
host, standalone), its name, release and serial number. Nothing else is
valid before the peer's hello, and a second hello is not
(`QSI0108`); version 0 or a hello out of its bounds is `QSI0111`, a
console talking to a console or a managed side to a managed side
`QSI0112`. Both then speak the lower version and use only the
capabilities both have (`Hello::agree`, handed up in
`Incoming::Ready`).

### Requests and replies

Only a console asks. A request of a family the two did not agree on is
refused at once (`QSI0003`, by the console's own session before it is
sent, and by the managed side's); a family neither knows is passed up
for the caller to refuse (`QSI0001`, `refuse_unknown`). A body that
does not read is `QSI0002` (`refuse_bad`). Requests outstanding when a
session ends come back to the console as refusals, so nothing is left
waiting. A reply to no outstanding request, a duplicate request number,
or a message of the wrong role closes the session (`QSI0108`).

**Before a pin.** A managed session whose peer is not pinned
(`qsitrust::tls::peer`) passes up the pairing family and refuses
everything else itself (`QSI0101`): no events, no streams, for the
session's whole life. After pairing the console connects again.

### Events

A managed side keeps one `EventLog` for all its sessions, bounded by
count and bytes (the oldest go first); `push` numbers each event from 1
up. A console subscribes with base `Subscribe {since}`: the managed
side's caller answers with `Session::subscribe(id, &log, since)`, which
replies `Subscribed` (`Fresh`; `Resumed` with how many follow; or
`ReadAgain` when the log no longer reaches back to `since`, or `since`
is newer than anything it has) and queues the missed events after the
reply, in order; `publish` then sends each new event to the subscribed
sessions. A console that reconnects says the last number it saw and is
given exactly what it missed, or told to read everything again. A
session that falls further behind than the log keeps sees a gap in the
numbers, and subscribes again.

A session sends only the events of what the two sides agreed on
(`qsi::caps::of_event`): an event of a family, or of a group of
operations behind a bit of its own, that the console lacks is passed
over for it, and `Subscribed`'s `replayed` counts only what it is
sent. The numbers are the log's, one sequence for every family, so a
console that lacks a family the managed side publishes sees its
numbers skip; such a console tells a loss from a skip by subscribing
again with the last number it saw (`Resumed` means nothing was lost).

A managed side started again takes its log up where it was with
`EventLog::resume(limits, next, events)`: the events it kept, oldest
first, numbered one after another up to `next - 1`, after which it
numbers on from `next`. The bounds keep the newest of them, as pushing
them would; events with a gap, out of order or not ending at `next - 1`
are refused (`Error::BadResume`). A console that reconnects is resumed
from them exactly as before the restart, with nothing in place of the
events the log no longer has (QS/VS's open question 42).

### Streams

A request that opens a stream (a console, a dump's download, a media
upload) is answered with `reply_stream`, which numbers the stream and
puts the number in the reply. Data may then flow both ways. Each
direction has its own credit: each side grants its window (256 KiB by
default) when the stream opens and more as its caller consumes
(`grant`), and `send` takes only as much as the credit allows, so one
stalled stream never holds up another or the requests. Data beyond the
credit resets that stream (`QSI0105`); data on a stream never opened
is answered with a reset (`QSI0106`). `close_stream` ends one
direction; `reset_stream` aborts both, with a message. A stream is
bytes without boundaries: what a side sends in one call may arrive in
pieces, so a stream's users frame their own data (the console stream
carries `qsipart` frames, section 7).

### Keep-alive and the end

A session that has heard nothing for `keepalive_ms` sends a ping; one
that hears nothing at all for `timeout_ms` closes (`QSI0107`). `close`
sends goodbye: no new requests after it (`QSI0110`); the peer sees
`Incoming::Closed`. A frame that does not read, or a byte the reader
had to pass over, closes the session (`QSI0109`).

## 5. Versions and capabilities

The protocol's version is `qsi::VERSION`, 1. Each family has a version
and a capability bit for it (`qsi::caps`); a family's later version
will have a bit of its own, so a console and a managed side of
different ages use what both have. Within a version, a new operation, a
new event, a field after the known ones or a new value of an open
enumeration is additive; a change to a known body takes a new version
of the family. A receiver keeps an enumeration value it does not know
as `Other`, passes over a body's trailing bytes and refuses an
operation it does not know (`QSI0001`).

| Family | Number | Capability bit |
|---|---|---|
| base | `0x0001` | 0 |
| pair | `0x0002` | 1 |
| service | `0x0003` | 2 |
| update | `0x0004` | 3 |
| base's system dumps (operations `0x0010` to `0x0013`, events `0x0010` and `0x0011`) | `0x0001` | 4 |
| host | `0x0010` | 8 |
| partitions | `0x0011` | 9 |
| partition | `0x0012` | 10 |
| profiles | `0x0013` | 11 |
| panel | `0x0014` | 12 |
| refcodes | `0x0015` | 13 |
| dumps | `0x0016` | 14 |
| console | `0x0017` | 15 |
| vres | `0x0018` | 16 |
| physical I/O: host's operation `0x0010` and events `0x0010` to `0x0014`, vres's operation `0x0034` and event `0x000A` | `0x0010`, `0x0018` | 17 |
| partition version 2: operation `0x0007` and the family's events | `0x0012` | 18 |
| switches managed: vres's operations `0x0045` to `0x004A` | `0x0018` | 19 |
| storage pools managed: vres's operations `0x0002` to `0x000A` | `0x0018` | 20 |
| partition version 3: operations `0x0008` to `0x000A` | `0x0012` | 21 |
| activation times: partitions' operation `0x0004`, profiles' `0x0007` | `0x0011`, `0x0013` | 22 |
| a host's power-on settings: host's operations `0x0020` and `0x0021`, event `0x0020` | `0x0010` | 23 |
| panel version 2: operation `0x0002` | `0x0014` | 24 |
| service version 2: operations `0x0004` to `0x0006`, event `0x0005` | `0x0003` | 25 |
| dump space: base's operation `0x0014` (with bit 4), dumps' `0x0005` | `0x0001`, `0x0016` | 26 |
| the fix level: base's operation `0x0008` | `0x0001` | 27 |

A console and a standalone system say `caps::MANAGED` (base and pair);
a QS/VS host says `caps::HOST_V1`, base, pair and the nine host
families. `caps::SERVICING` is bits 2 to 4 (problems, code update and
the system's own dumps), said beside the rest by a managed side that
answers them and by a console that asks them; `caps::PHYSICAL_IO`
(bit 17) and `caps::PARTITION_V2` (bit 18) likewise, and bits 19 to 27
(handoff 24) each the same way. `HOST_V1`, `SERVICING` and `MANAGED`
are unchanged. `caps::STANDALONE` is what a standalone system says
(section 11): base and pair, service with its version 2 (bits 2 and
25), the system's dumps and their space (4 and 26), the panel with its
version 2 (12 and 24), the console (15) and the fix level (27); no code
update and no other host family.

A bit need not be a whole family: base's version 1 gained the system's
own dumps as operations behind bit 4, the host and vres families gained
physical I/O behind bit 17 (one bit for both, as one feature of the
host), and the partition family's version 2 is its operation `0x0007`
and its events behind bit 18. So what a request needs is
`caps::of_request(family, op)` (the family's bit, and the bit of those
operations) and what an event needs `caps::of_event(family, op)`. The
session checks a request against them at both ends (`QSI0003`) and
sends an event only to a console that has all of it (section 4).

A family's later version is gated by its newest bit alone: partition
version 3's operations need bits 10 and 21, not 18, so a side may have
version 3's settings without version 2's entitlement. Base's dump
space needs bits 0, 4 and 26: the system's dumps and their space.

Every addition since version 1 is a new operation or event with types
of its own, never a field added to a known body. `ipc::wire_struct!`
reads fields in order and fails when the bytes run out, so a field
added to a body a new reader expects is not there when an older side
sends it; and a struct in a list or in the middle of a body cannot gain
a trailing field at all, since what follows it is the next item. A
field after the known ones of a whole body is passed over by an older
reader, but a newer reader of an older side's body does not find it;
so the families grow by operations, and their bodies stay as they are.

## 6. Limits

`qsisess::Limits`, each refused or closed by name:

| Limit | Default | Beyond it |
|---|---|---|
| a frame's body | 32,760 bytes (`qsipart`) | the frame is a message's part |
| a whole message (`max_message`) | 1 MiB | `QSI0104` |
| outstanding requests (`max_requests`) | 64 | `QSI0102` |
| open streams (`max_streams`) | 16 | `QSI0103` |
| a stream's window (`window`) | 256 KiB | `QSI0105` (the stream is reset) |
| a user's name (`max_user`) | 128 bytes | `QSI0104` |
| a refusal's text, help, identifier | 1,024, 4,096, 16 bytes | clipped when sent; a peer's beyond them is `QSI0104` |
| a refusal's further messages | 16 | `QSI0104` |
| the event log (`log_events`, `log_bytes`) | 4,096 events, 4 MiB | the oldest go |
| keep-alive (`keepalive_ms`, `timeout_ms`) | 15 s, 45 s | ping; `QSI0107` |

The families' own bounds (`check()` on each request) are in section 7.

## 7. The families

`qsi::base` and `qsi::pair` are answered by any managed system;
`qsi::service` and `qsi::update`, and base's system dumps, by any
managed system that has them (a QS/VS host from its next phase; a
standalone system: service, its dumps and their space, section 11); the nine host families by a QS/VS host,
with physical I/O and partition version 2 from its phase after that,
and bits 19 to 27 as QS/VS and QS/MC come to them. Each is a `Family` with three
enumerations (`Request`, `Reply`, `Event`) written by `messages!`;
`qsi::any` holds them all by number, and `qsi::samples` has one value
of every message, which the tests send through two sessions.

### base (`0x0001`)

| Op | Request | Reply |
|---|---|---|
| `0x0001` | `Identify` (empty) | `Identity` {`kind` (open: console 1, host 2, standalone 3), `name`, `release`, `serial`, `model`, `build`} |
| `0x0002` | `State` (empty) | `State` {`power` (open: starting 1, running 2, ending 3, off 4, halted 5), `refcode` (an option: the code current now), `release`, `slot` (`A`, `B`, or empty), `keylock` (an option, open: normal 1, manual 2, auto 3, secure 4)} |
| `0x0003` | `Power` {`action` (open: off 1, restart 2), `immediate bool`, `delay u32` seconds, at most 99999} | empty: the power down was started; its codes and states follow as events |
| `0x0004` | `Refcodes` {`before` (an option: the history's place to read before; none for the newest), `limit u32`} | `RefcodeList` {`codes`, newest first; `more` (an option: where the next page starts)} |
| `0x0005` | `Subscribe` {`since` (an option: the last event's number seen)} | `Subscribed` {`replay` (open: fresh 1, resumed 2, read again 3), `replayed u32`, `next u64`} |
| `0x0006` | `Unsubscribe` (empty) | empty |
| `0x0007` | `Time` (empty) | `Time` {`unix_ns i64`, `offset_s i32`, `zone`} |

A `Refcode` is {`seq u64` (its place in the history, from 1, never
reused; 0 when not in it), `code` (eight characters), `words` (words 2
to 9 in hexadecimal where the code has them), `text`, `time_ms u64`
(Unix milliseconds)}. Events: `0x0001` `Refcode` (a code posted),
`0x0002` `State` (the state changed), `0x0003` `Identity` (what it
says of itself changed).

#### The system's own dumps (bit 4, `caps::SYSTEM_DUMPS`)

A managed system's dumps of itself, beside a host's dumps of its
partitions (the dumps family): a QS/VS host's service data, or a
standalone system's own. Operations and events of base, each needing
bit 4 as well as base's.

| Op | Request | Reply |
|---|---|---|
| `0x0010` | `Dumps` (empty) | `Vec<SystemDump>` oldest first: `name`, `kind` (open `DumpKind`: `ServiceData` 1, the system's service data; `System` 2, a kind the managed side names), `kind_name` (the managed side's word for the kind), `bytes`, `time` (s), `by` (what asked for it, the managed side's words) |
| `0x0011` | `TakeDump` {`kind`, `kind_name` (for `System`; empty otherwise)} | `DumpBegun` {`name`}: written in the background |
| `0x0012` | `DeleteDump` {`name`} | empty |
| `0x0013` | `DownloadDump` {`name`} | `DumpDownloading` {`name`, `bytes`}: **opens a stream**, managed side to console, of the dump's `bytes` bytes, ending after the last |

| Event | Body | When |
|---|---|---|
| `0x0010` | `DumpTaken` {`name`, `dump` or none, `error` (a `Note`) or none, `time`} | a dump was written, or could not be |
| `0x0011` | `DumpRemoved` {`name`, `time`} | a dump was deleted, or put out for the managed side's limits |

Bounds: a name and a kind's name, 64 bytes. Refusals of its own:
`QSI1201` `NO_DUMP` (no dump of that name, from a side with no
identifier of its own), `QSI1202` `TAKING` (one is being taken: one at
a time), `QSI1203` `NO_KIND` (a kind this system does not take). What a
dump holds and its format are the managed side's.

#### Dump space (bit 26, `caps::DUMP_SPACE`)

| Op | Request | Reply |
|---|---|---|
| `0x0014` | `DumpSpace` (empty), bits 4 and 26 | `Space` {`bytes u64` (what the dumps take), `limit u64` (the most they may take; 0 for none), `dumps u32`, `max_dumps u32` (0 for none)}: the system's own dumps |

The dumps family's `0x0005` gives the same of each partition's dumps.

#### The fix level (bit 27, `caps::FIX_LEVEL`)

| Op | Request | Reply |
|---|---|---|
| `0x0008` | `FixLevel` (empty) | `Level` {`level` (the cumulative fix level's name, the managed side's; empty for none), `date` (s; 0 when not known)} |

A standalone system says its cumulative PTF package; a host answers
with its own level if it has one. Base's `State` does not carry it: an
older side's state has no such field (section 5).

### pair (`0x0002`)

| Op | Request | Reply |
|---|---|---|
| `0x0001` | `Start` {`spake` (SPAKE2's message A, 33 bytes), `name`} | `Answer` {`spake` (message B), `confirm` (32 bytes), `name`} |
| `0x0002` | `Confirm` {`confirm` (32 bytes)} | `Done` (empty): the console is pinned |

No events. Refusals: `QSI0201` (no password out, expired, attempts
spent), `QSI0202` (did not confirm), `QSI0203` (already paired),
`QSI0204` (a confirmation without a start). Section 3 is the exchange.

### service (`0x0003`, bit 2)

A managed system's **problems**. A problem is something a person should
look at: one record per condition, not per reference code posted. The
managed side opens one when a condition begins, counts it each time the
condition is seen again, notes when it clears, and keeps it until a
person closes it. Its number is the managed system's, ascending and
never reused. The guided text for a reference code is not QSI's: it is
the console's.

| Type | Fields |
|---|---|
| `ProblemStatus` (open) | `Open` 1, `Closed` 2 |
| `Source` | `kind` (open `SourceKind`: `System` 1, the managed system itself; `Partition` 2), `id` (the partition's, 0 for the system), `name` |
| `Resource` | `kind` (open `ResourceKind`: `DiskUnit` 1, `StoragePool` 2, `VirtualDisk` 3, `NetworkInterface` 4, `VirtualSwitch` 5, `Partition` 6, `CodeSlot` 7 (a slot of the system's own code), `System` 8), `name`, `location` (its location code), `serial`: each empty where it has none |
| `Line` | `time` (s), `text`: a line of the analysis log |
| `Summary` | `number u64`, `status`, `first_seen`, `last_seen` (s), `count u32`, `refcode` (base's `Refcode`: the code as last posted for it, with its words), `source`, `description` (one line), `msgid` (the managed side's identifier for it), `holds` (the condition still holds), `closed_by` (empty while open), `closed_at` (s, 0 while open) |
| `Problem` | `summary`, `resources`, `log` (oldest first) |
| `Filter` | `status`, `source` (matched by kind and ID; the name is not compared), `since` (last seen at or after, s): each an option, every one set must match |

| Op | Request | Reply |
|---|---|---|
| `0x0001` | `List` {`filter`, `before` (problems numbered below it; none from the newest), `limit u32`} | `Page` {`problems`: `Vec<Summary>` newest first, `more`: the next page's `before`} |
| `0x0002` | `Get` {`number`} | `Problem` |
| `0x0003` | `Close` {`number`, `anyway`} | `Summary` as closed. Who closed it is the request's user. Refused while the condition holds unless `anyway` (`QSI1001`) |

| Event | Body | When |
|---|---|---|
| `0x0001` | `Opened` (`Noticed` {`problem`: `Summary`, `time`}) | a problem was opened |
| `0x0002` | `Seen` (`Noticed`) | an open problem's condition was seen again: its `count` and `last_seen` |
| `0x0003` | `Cleared` (`Noticed`) | its condition no longer holds; it stays open until closed |
| `0x0004` | `Closed` (`Noticed`) | it was closed |

Bounds: a source's ID 0 to 999, its name 64 bytes; problem 0 is none.
Refusals of its own: `QSI1001` `STILL_HOLDS` (a close while the
condition holds, without `anyway`), `QSI1002` `NO_PROBLEM` (no problem
of that number, from a side with no identifier of its own), `QSI1003`
`CLOSED` (closed already).

#### Version 2 (bit 25, `caps::PROBLEM_NOTES`): notes and severity

A person's notes in a problem's analysis log, and how grave a problem
is. The severity is beside the summary, not in it: `Summary` is in a
list (`Page`) and in the middle of `Problem` and `Noticed`, so it
cannot gain a field; version 2's list and get carry it in types of
their own, and version 1's are unchanged.

| Type | Fields |
|---|---|
| `Severity` (open) | `Attention` 1, `Terminal` 2, `Information` 3 |
| `Rated` | `severity`, `summary` |
| `RatedPage` | `problems`: `Vec<Rated>` newest first, `more` |
| `RatedProblem` | `severity`, `problem` |

| Op | Request | Reply |
|---|---|---|
| `0x0004` | `Note` {`number`, `text` (1 to 256 bytes)}: the request's user is its author | `Line` as written into the analysis log, its text naming the author in the managed side's words |
| `0x0005` | `ListRated` (`List`'s fields) | `RatedPage` |
| `0x0006` | `GetRated` {`number`} | `RatedProblem` |

| Event | Body | When |
|---|---|---|
| `0x0005` | `Noted` {`number`, `line`, `by` (the request's user), `time`} | a note was added |

Refusals: `QSI1002` (no problem), `QSI1003` (closed: a closed problem
takes no note). `service::Service::VERSION` is 2.

### update (`0x0004`, bit 3)

**Code update** of a managed system whose own code is in two slots, as a
QS/VS host's is. One image is staged at a time: it is uploaded on a
stream and verified by the managed side, installed to the slot that is
not running and marked pending, then activated by a restart into it,
after which the slot either confirms or the system rolls back to the
other. Nothing running is disturbed before the activation.

| Type | Fields |
|---|---|
| `Image` | `product`, `release`, `build`, `fixes` (the fixes it names), `bytes`: what an update image is, in the managed side's forms |
| `Slot` | `name` (`A`, `B`), `release`, `build`, `fixes`, `running`, `default`, `last` (open `Attempt`: `Never` 1, `Pending` 2, `Confirmed` 3, `RolledBack` 4: how its last activation ended), `reason` (why it rolled back), `time` (s) |
| `StagedImage` | `name`, `bytes`, `verification` (open: `Receiving` 1, `Verifying` 2, `Verified` 3, `Refused` 4), `image` (once verified), `error` (a `Note`, when refused), `time` |
| `Progress` | `slot`, `percent u8`, `time` |

| Op | Request | Reply |
|---|---|---|
| `0x0001` | `Status` (empty) | `Status` {`slots` (A, then B), `staged` (an option), `installing` (an option: a `Progress`)} |
| `0x0002` | `Stage` {`name`, `bytes`, `allow_older`} | `Staging` {`name`, `bytes`}: **opens a stream**, console to managed side, of exactly `bytes` bytes. How it ended is the `Staged` event, then `Verified` |
| `0x0003` | `Discard` (empty) | empty: what is staged is thrown away |
| `0x0004` | `Install` (empty) | `Target` {`slot`}: the staged, verified image is being written to the slot that is not running; progress follows as events |
| `0x0005` | `Activate` {`partitions` (open `Partitions`: `Refuse` 1, refuse while any runs, the default; `ShutDown` 2, shut each down through its operating system first), `timeout` (s, each shut down's, at most 99999)} | empty: the restart was begun |
| `0x0006` | `Cancel` (empty) | empty: the pending slot is pending no more |

| Event | Body | When |
|---|---|---|
| `0x0001` | `Staged` {`name`, `bytes` (received), `error` or none, `time`} | the upload ended: whole and being verified, or why not (`QSI1109`, `QSI1110`) |
| `0x0002` | `Verified` {`name`, `image` or none, `error` or none, `time`} | the image was verified, or refused by name (`QSI1101` to `QSI1105`) |
| `0x0003` | `Installing` (`Progress`) | an install's progress |
| `0x0004` | `Installed` {`slot`, `image` or none, `error` or none, `time`} | the slot is written and pending, or the install failed |
| `0x0005` | `Activating` {`slot`, `time`} | the restart into the pending slot begins |
| `0x0006` | `Confirmed` {`slot`, `release`, `build`, `time`} | after the restart: the slot started and confirmed; it runs and is the default |
| `0x0007` | `RolledBack` {`slot`, `reason`, `code` (the reference code that says why, where there is one), `time`} | after the restart: the slot did not confirm and the system went back |

`Confirmed` and `RolledBack` are pushed by the system after the restart,
so a console sees them when it reconnects and resumes its events (the
log is taken up with `EventLog::resume`, section 4).

Bounds: a stage's name 64 bytes, its size more than 0; an activation's
timeout 99999 seconds. Refusals of its own (a managed side with
identifiers of its own gives them after these):

| Identifier | Name | When |
|---|---|---|
| `QSI1101` | `BAD_SIGNATURE` | the image is not signed by a key this system trusts |
| `QSI1102` | `DAMAGED` | it does not read, or a hash is wrong |
| `QSI1103` | `WRONG_PRODUCT` | it is for another product |
| `QSI1104` | `OLDER` | it is older than what runs and the stage did not allow it |
| `QSI1105` | `NO_STATE` | this system's own state (its slots, its record) cannot be read |
| `QSI1106` | `NOTHING_STAGED` | a discard or install with nothing staged |
| `QSI1107` | `STAGED` | a stage while one is staged: discard it first |
| `QSI1108` | `NOT_VERIFIED` | an install of an image not verified |
| `QSI1109` | `UPLOAD_LONG` | more bytes than the stage named: the stream is reset |
| `QSI1110` | `UPLOAD_CUT` | fewer: nothing is kept |
| `QSI1111` | `BUSY` | an install or an activation is under way |
| `QSI1112` | `NOTHING_PENDING` | an activation or cancel with no slot pending |
| `QSI1113` | `PARTITIONS_RUN` | partitions run and the activation said to refuse |
| `QSI1114` | `PENDING` | an install while a slot is pending: activate or cancel it first |
Every body is an `ipc::Wire` value read leniently: fields a later
version adds go at the end and are passed over. A one-byte enumeration
marked *open* keeps a value it does not know as `Other(n)`. Field names
and units are QS/VS's (`part/model`, `part/resource`, `part/partd`,
`stg/poold`, `net/vswitchd`); every identifier a console shows (a state's
words, a reference code and its text, a message) is passed as the host
gives it. Times are seconds since the epoch (`time`, `created`) as QS/VS
keeps them, except a reference code's `time_ms`, which is the base
family's `Refcode`. Processing units are hundredths of a core (QS/VS's
`Units`); memory is MiB; sizes in bytes unless the name says `mib`.

A reply has its request's operation number. A request that opens a
stream is answered by a reply whose body describes the stream (its size,
its name); the session gives the stream's number. Every family's
`Request` has `check()`, the protocol's bounds (`AnyRequest::check`
dispatches), refused with `QSI0002` before the host acts:

| Bound | Limit |
|---|---|
| a partition ID | 1 to 999 (0 only where a request says "every partition", and in the panel and console families, where 0 is the managed system itself) |
| a name a console sends | 64 bytes |
| a text a console sends (a backing, a location code, a partition's description, a problem's note: 1 to 256) | 256 bytes |
| a list a console sends | 256 items |
| a shut down's delay, base's power down's delay, a partition's or a host's delay with the managed system's power off | 99999 seconds |
| a panel function | 1 to 99 |
| serial bytes in one piece of a console stream | 16 KiB (a piece is one frame) |

The host's own rules (a name's form, memory in blocks) are the host's to
refuse with its own messages.

### Capability bits and versions

"Answered by" is the managed side that answers the bit: a QS/VS host,
a standalone QS/400 (`caps::STANDALONE`, section 11), or both; a
console asks each of them.

| Family | Number | Version | Capability (`caps::`) | Bit | Answered by |
|---|---|---|---|---|---|
| base | `0x0001` | 1 | `BASE` | 0 | both |
| pair | `0x0002` | 1 | `PAIR` | 1 | both |
| service | `0x0003` | 1 | `SERVICE` | 2 | both |
| update | `0x0004` | 1 | `UPDATE` | 3 | host |
| base's system dumps | `0x0001` | 1 | `SYSTEM_DUMPS` | 4 | both |
| host | `0x0010` | 1 | `HOST` | 8 | host |
| partitions | `0x0011` | 1 | `PARTITIONS` | 9 | host |
| partition | `0x0012` | 1 | `PARTITION` | 10 | host |
| partition | `0x0012` | 2 | `PARTITION_V2` | 18 | host |
| profiles | `0x0013` | 1 | `PROFILES` | 11 | host |
| panel | `0x0014` | 1 | `PANEL` | 12 | both (a standalone system at ID 0) |
| refcodes | `0x0015` | 1 | `REFCODES` | 13 | host |
| dumps | `0x0016` | 1 | `DUMPS` | 14 | host |
| console | `0x0017` | 1 | `CONSOLE` | 15 | both (a standalone system at ID 0) |
| vres | `0x0018` | 1 | `VRES` | 16 | host |
| host's and vres's physical I/O | `0x0010`, `0x0018` | 1 | `PHYSICAL_IO` | 17 | host |
| vres's switches managed | `0x0018` | 1 | `SWITCHES` | 19 | host |
| vres's storage pools managed | `0x0018` | 1 | `POOLS` | 20 | host |
| partition | `0x0012` | 3 | `PARTITION_V3` | 21 | host |
| partitions' and profiles' activation times | `0x0011`, `0x0013` | 1 | `ACTIVATIONS` | 22 | host |
| host's power-on settings | `0x0010` | 1 | `POWER_ON` | 23 | host |
| panel | `0x0014` | 2 | `PANEL_FUNCTIONS` | 24 | both |
| service | `0x0003` | 2 | `PROBLEM_NOTES` | 25 | both |
| base's and dumps' dump space | `0x0001`, `0x0016` | 1 | `DUMP_SPACE` | 26 | both (a standalone system has base's `0x0014`, a host dumps' `0x0005` too) |
| base's fix level | `0x0001` | 1 | `FIX_LEVEL` | 27 | standalone (a host with a level of its own may) |

A QS/VS host says `caps::HOST_V1`, bits 0, 1 and 8 to 16, and with its
next phase `caps::SERVICING` (bits 2 to 4) beside it, then
`PHYSICAL_IO` and `PARTITION_V2` (bits 17 and 18), then whichever of
bits 19 to 27 it answers. A later version of a family has a bit of its
own. A family's later version keeps every message of the ones before it
unchanged, so a side with bit 10 and not bit 18 speaks the partition
family as before, and each later version's bit alone gates its own
operations; `partition::Partition::VERSION` is the highest this crate
speaks, 3, `panel::Panel::VERSION` and `service::Service::VERSION` 2.

### Shared types (`host::model`)

| Type | Fields |
|---|---|
| `PartitionId` | `u32`, 1 to 999 |
| `PartitionState` (open) | `NotActivated` 1, `Starting` 2, `Running` 3, `ShuttingDown` 4, `Error` 5: the names QS/MC shows. A partition held for its pool's space is Running with `A2005160` as its code (QS/VS's open question 34) |
| `Environment` (open) | `Standalone` 1: the operating system a standalone system runs (QS/VS's one environment; the crate names no product) |
| `IplSource` (open) | `A` 1, `B` 2, `Recorded` 3, `D` 4 |
| `IplType` (open) | `Unattended` 1, `Attended` 2 |
| `ConsoleMode` (open) | `Serial` 1, `Local` 2, `Partition` 3 |
| `ProcessorMode` (open) | `Dedicated` 1, `Shared` 2 |
| `Sharing` (open) | `Capped` 1, `Uncapped` 2 |
| `base::Keylock` (open) | `Normal` 1, `Manual` 2, and, added for QS/VS, `Auto` 3, `Secure` 4 |
| `Note` | `msgid`, `text`, `help`: a message the host issued, where an event or a list carries one |
| `AutoStart` | `enabled`, `order`, `delay_secs`, `wait`, `wait_secs` |
| `Definition` | `id`, `name`, `environment`, `serial`, `keylock`, `default_profile`, `auto_start`, `created` (s) |
| `Processors` | `mode`, `minimum`, `desired`, `maximum`; shared: `minimum_units`, `desired_units`, `maximum_units` (hundredths), `sharing`, `weight` (0 to 255), `pool` |
| `Memory` | `minimum_mib`, `desired_mib`, `maximum_mib` |
| `Lun`, `ScsiAdapter` | `lun`, `backing` (`POOL/NAME`), `serial`; `slot`, `luns` |
| `Boot` | `source`, `ipl_type`, `console` |
| `EthernetAdapter` | `slot`, `switch`, `vlan`, `tagged`, `mac`, `required` |
| `MediaDevice` | `slot`, `media` (an image or volume name; for a tape device, a physical drive as `@` and its location code: `MediaDevice::drive`, `tape_source`, bit 17) |
| `PhysicalIo` | `location`, `required`: a device of the host given at activation with its isolation group (bit 17, "Physical I/O" below) |
| `Profile` | `name`, `processors`, `memory`, `boot`, `scsi`, `ethernet`, `optical`, `tape`, `physical_io` |
| `Current` | `mode`, `cores` (dedicated), `pool`, `units` (hundredths), `virtuals`, `capped`, `weight`, `memory_mib`: QS/VS's `resource::Current` flattened, so a later mode is an open value, not an unknown tag |
| `Channel` | `up`, `none`, `responding`, `kept`, `system`, `release`, `fix_level`, `addresses` (`address/prefix`), `version`, `caps` (the partition scope's, `qsipart::caps`) |
| `Holder` | `partition` (0 for none), `slot` |

### host (`0x0010`)

| Op | Request | Reply |
|---|---|---|
| `0x0001` | `GetHost` (empty) | `Host`: `sockets`, `cores`, `threads_per_core`, `processors` and `memory` as `Amount` {`installed`, `configurable`, `available`} (hundredths of a core; MiB), `memory_block_mib`, `reserved_cores` (by number), `reserved_memory_mib` (the host reserve), `pools`: `ProcPool` {`name`, `cores`, `maximum`, `entitled`, `available` (hundredths), `partitions`}, the default first; `partitions` defined, `max_partitions` |
| `0x0002` | `GetSettings` (empty) | `Settings`: `max_partitions`, `memory_block_mib`, `reserved_cores`, `reserved_memory_mib`, `mac_prefix`, and `[service]`'s `delayed_grace_secs`, `system_timeout_secs`, `host_timeout_secs`, `channel_wait_secs`, `max_dumps`, `max_dump_mib` |
| `0x0003` | `GetHardware` (empty) | `Hardware`: `manufacturer`, `product`, `processor`, `virtualization`, `items`: `Item` {`location`, `kind` (open: `Disk` 1, `Interface` 2, `Pci` 3), `name`, `description`, `serial`, `bytes`, `mac`, `speed_mbps`, `carrier`, `owner` (open: `None` 1, `Host` 2, `Pool` 3, `Switch` 4, `Partition` 5, `Foreign` 6), `owner_name`, `partition`, `what` (the host's words)} |

| Event | Body | When |
|---|---|---|
| `0x0001` | `Host` | the resources changed: an activation, an end, a processor pool made, changed or deleted |
| `0x0002` | `ItemChanged` {`location`, `item` or none when gone, `time`} | an item's owner or carrier changed, or it came or went |

#### Physical I/O (bit 17, `caps::PHYSICAL_IO`)

A host's PCI functions given to partitions whole. The host's operator
**releases** a device for partitions at the host, and takes it back
there: QSI reads that and does not change it. A profile's `physical_io`
names released devices by location code; an activation gives each to
the partition with the rest of its isolation group (the devices the
host's IOMMU cannot tell apart, which go together or not at all), and
the partition's end gives them back, each reset. Operations and events
of the host family, each needing bit 17 as well as the family's.

| Op | Request | Reply |
|---|---|---|
| `0x0010` | `Devices` (empty) | `Vec<Device>` by location code |

| Type | Fields |
|---|---|
| `Device` | `location`, `address` (the PCI address), `class u32` (class code: base class, subclass, interface), `vendor u16`, `device u16`, `description` (the host's words), `released`, `assignable` (it can be given now), `blocked` (an option: why not), `group u32` (its isolation group's number), `with` (the other devices of its group, by location code), `holder` (an option: who holds it) |
| `Blocked` | `reason` (open `Blocker`: `HostUses` 1, `NotReleased` 2, `GroupNotReleased` 3, a device of its group is not released; `NoReset` 4; `NoRemapping` 5, the host has no interrupt remapping), `note` (the host's message) |
| `DeviceHolder` | `partition`, `name` (the partition's), `profile` (activated with) |

| Event | Body | When |
|---|---|---|
| `0x0010` | `Released` (`DeviceChanged` {`location`, `device` or none, `time`}) | a device was released at the host |
| `0x0011` | `TakenBack` (`DeviceChanged`) | a device was taken back at the host |
| `0x0012` | `Assigned` (`DeviceHeld` {`location`, `holder`, `time`}) | a device was given to a partition at its activation |
| `0x0013` | `Returned` (`DeviceHeld`) | a device was given back at the partition's end |
| `0x0014` | `ResetFailed` {`location`, `partition` (0 for none), `note`, `time`} | a device did not reset when it was given or given back |

A host item's `owner` (`Partition` 5) and its `ItemChanged` event say
the same of the device as before; these say what physical I/O needs
beside it.

#### Power-on settings (bit 23, `caps::POWER_ON`)

What a host does with its partitions when it starts and when it powers
off. Not its IPL side (changed only by an update's activation), its
keylock or its boot mode.

| Op | Request | Reply |
|---|---|---|
| `0x0020` | `PowerOn` (empty) | `PowerOn` {`start_partitions` (the partitions marked to start, each one's auto start, are started with the host), `power_off` (open `PowerOff`: `Delayed` 1, each shut down through its own system first and waited for `delay`; `Immediate` 2), `delay u32` (s, at most 99999)} |
| `0x0021` | `SetPowerOn` (`PowerOn`) | empty |

| Event | Body | When |
|---|---|---|
| `0x0020` | `PowerOnChanged` {`power_on`, `by` (the request's user), `time`} | the settings were changed |

QS/VS maps `start_partitions` to its auto start at base ready, and
`power_off` and `delay` to its end of every partition at a power down
(the operating system's shut down, `host_timeout_secs`). Bounds: the
delay (`QSI0002`).

A profile's `physical_io` is the profiles family's as it was (the same
`Profile`, the same put): with bit 17 the host acts on it, where before
it kept such a profile and refused its activation. What QSI checks is
the protocol's (a location 256 bytes at most); the rest is the host's,
refused by name, the host's own identifiers after these:

| Identifier | Name (`host::physio::`) | When |
|---|---|---|
| `QSI1301` | `UNKNOWN_LOCATION` | a put or activation names a location no device of the host has |
| `QSI1302` | `NOT_RELEASED` | an activation names a device not released |
| `QSI1303` | `HELD` | another partition holds the device, or the tape drive |
| `QSI1304` | `GROUP_SPLIT` | the device's isolation group is not wholly given to this partition: the profile names some of it and not the rest |
| `QSI1305` | `NOT_ASSIGNABLE` | it cannot be given now for another reason (the host uses it, no reset, no interrupt remapping); the device's own note follows |
| `QSI1306` | `NOT_A_DRIVE` | a tape device names a location that is not a tape drive (vres below) |
| `QSI1307` | `NOT_VIRTUAL` | a load or unload of a slot that is a physical drive |

### partitions (`0x0011`)

| Op | Request | Reply |
|---|---|---|
| `0x0001` | `List` (empty) | `Vec<Status>` in ID order: `id`, `name`, `state`, `code` and `text` (the current reference code), `current` (none when not activated), `profile` (last activated with), `last_end` (`exit N`, `signal N`), `message` (a `Note`: why it is not activated or in error), `ethernet`: `Link` {`slot`, `switch`, `mac`, `link`}, `channel` |
| `0x0002` | `Get` {`id`} | `Partition` {`definition`, `status`} |
| `0x0003` | `Adapters` {`id`, 0 for every partition} | `Vec<Adapter>`: `partition`, `name`, `profile`, `slot`, `switch`, `vlan`, `tagged`, `mac`, `required` |

| Event | Body | When |
|---|---|---|
| `0x0001` | `StateChanged` {`id`, `name`, `state`, `time`} | a state changed |
| `0x0002` | `ResourcesChanged` {`id`, `name`, `current`, `time`} | resources given at an activation or given back at an end, or a running partition's entitlement changed (partition version 2) |
| `0x0003` | `Defined` {`id`, `name`, `exists`, `time`} | created, changed or deleted |
| `0x0004` | `ChannelChanged` {`id`, `name`, `channel`, `time`} | the service channel came or went, stopped or started answering, or the system reported a change |

#### Activation times (bit 22, `caps::ACTIVATIONS`)

| Op | Request | Reply |
|---|---|---|
| `0x0004` | `Activations` (empty) | `Vec<Activation>` in ID order: `id`, `since` (an option: when the current activation began, s; none when not activated), `profile` (activated with) |

The profiles family's `0x0007` gives when each profile was last
activated; one bit for both, as physical I/O is.

### partition (`0x0012`)

| Op | Request | Reply |
|---|---|---|
| `0x0001` | `Create` {`name`} | `Definition`, with its ID and serial number |
| `0x0002` | `Change` {`definition`}: name, keylock, default profile, auto start; not activated | empty |
| `0x0003` | `Delete` {`id`, `disks`: its virtual disks too, every one or none} | empty |
| `0x0004` | `Activate` {`id`, `profile` (empty: the default), `source` override, `keylock` override} | `Current`: what it was given; it is Starting |
| `0x0005` | `ShutDown` {`id`, `kind` (open `ShutdownKind`: `Delayed` 1, `System` 2, `Immediate` 3), `delay` (s, 0 to 99999), `restart`} | empty: accepted by the system, or the VMM asked |
| `0x0006` | `Restart` {`id`}: immediate, activated again with the profile it had | empty |

Version 1 has no events: what follows comes as the partitions and
refcodes families'.

#### Version 2 (bit 18, `caps::PARTITION_V2`): a running partition's entitlement

A running shared-processor partition's entitlement changes without an
activation. Its virtual processors, a dedicated partition's processors
and every partition's memory still change only at activation: QS/VS
gives a partition its memory as huge pages mapped once, and its virtual
CPUs once.

| Op | Request | Reply |
|---|---|---|
| `0x0007` | `Entitlement` {`id`, `units` (an option: processing units, hundredths of a core), `sharing` (an option: `Capped` or `Uncapped`), `weight` (an option: the uncapped weight, 0 to 255)}: what is none stays | `Current`: what it holds now |

| Event | Body | When |
|---|---|---|
| `0x0001` | `EntitlementChanged` {`id`, `name`, `before` and `current` (`Current`), `by` (the request's user), `time`} | the entitlement was changed |

The change shows in `Current` (partitions `List` and `Get`, the reply),
and the partitions family's `ResourcesChanged` follows it too, so a
console without bit 18 still sees what the partition holds. Bounds: at
least one of the three, a weight of 255 at most (`QSI0002`). A
capped partition's weight is 0 in `Current`, as at activation; a
change to uncapped without a weight takes its profile's, 128 when the
profile has none. Refusals, the host's own identifiers after these:

| Identifier | Name (`partition::entitlement::`) | When |
|---|---|---|
| `QSI1401` | `NOT_RUNNING` | the partition is not running |
| `QSI1402` | `DEDICATED` | it has dedicated processors |
| `QSI1403` | `OUTSIDE_PROFILE` | the units are outside its profile's minimum and maximum units |
| `QSI1404` | `VIRTUALS` | the units are below what its virtual processors need (0.05 each) or above their worth (1.00 each) |
| `QSI1405` | `POOL_SHORT` | its pool has not that much left: the pool's available units and what it holds now together |
| `QSI0361` | `NO_SUCH_VALUE` | a `sharing` this host does not have |

#### Version 3 (bit 21, `caps::PARTITION_V3`): a partition's settings

A definition is in the middle of `partitions::Partition` and `Change`,
so it cannot gain fields; the new ones are a `Settings` of their own,
read and changed apart from it, at any time, running or not.

| Type | Fields |
|---|---|
| `WithHost` (open) | `Delayed` 1 (the system's controlled power down with `delay`), `Immediate` 2: how the partition is shut down when the managed system is powered off |
| `Settings` | `description` (256 bytes at most), `panel` (control panel functions are allowed from a console), `with_host`, `delay u32` (s, at most 99999) |

| Op | Request | Reply |
|---|---|---|
| `0x0008` | `CreateWith` {`id` (an option: none for the host to give one), `name`, `settings`} | `Definition`, with its ID and serial number |
| `0x0009` | `GetSettings` {`id`} | `Settings` |
| `0x000A` | `SetSettings` {`id`, `settings`} | empty |

The partitions family's `Defined` follows a create or a change, as
before. A host refuses panel functions from a console for a partition
whose `panel` is false, with its own message; QSI does not interpret
it. Bounds: an ID outside 1 to 999, a description over 256 bytes, a
delay over 99999 (`QSI0002`). Refusals: `QSI0362` `ID_TAKEN` (another
partition has the ID), `QSI0363` `ID_OUT_OF_RANGE` (within 1 to 999 but
beyond the host's `max_partitions`). Version 3's operations need bit 21
and not bit 18; `partition::Partition::VERSION` is 3. No new events.

### profiles (`0x0013`)

A profile has a **revision**, a number the managed side gives it and
changes whenever it changes. A put or save that carries a revision is
refused with `QSI0401` when the profile changed meanwhile; one that
carries none is refused with `QSI0402` when the name is there.

| Op | Request | Reply |
|---|---|---|
| `0x0001` | `List` {`id`} | `Vec<Entry>`: `name`, `revision`, `default` |
| `0x0002` | `Get` {`id`, `name`} | `Revised` {`profile`, `revision`} |
| `0x0003` | `Put` {`id`, `profile`, `revision` or none for a new one} | `Saved` {`revision`} |
| `0x0004` | `Delete` {`id`, `name`}: not the default | empty |
| `0x0005` | `SetDefault` {`id`, `name`} | empty |
| `0x0006` | `SaveCurrent` {`id`, `name`, `revision` when replacing}: the running profile at the resources held now | `Saved` {`revision`} |

| Event | Body | When |
|---|---|---|
| `0x0001` | `Changed` {`id`, `name`, `revision` or none when deleted, `default`, `time`} | a profile was created, changed or deleted, or the default moved |

| Op | Request | Reply |
|---|---|---|
| `0x0007` | `Activated` {`id`}, bit 22 | `Vec<ProfileUse>` by name: `name`, `last` (an option: when last activated, s; none for never) |

### panel (`0x0014`)

| Op | Request | Reply |
|---|---|---|
| `0x0001` | `Function` {`id`, `function` (`u8`), `args`: `PanelArgs` {`source`, `ipl_type`, `keylock`}, each optional, function 02's} | `Shown` {`text`}: what it displays, the host's words |

The functions (`panel::function`): 01 display the IPL, 02 select it for
the next activation, 03 restart the IPL, 08 power off, 11 to 19 the
current code's words 1 to 9, 20 the type, serial and releases, 21
dedicated service tools (the system's, over the service channel), 22
dump and restart, 34 dump again from held memory. Which there are and
which need manual is the host's (`docs/PANEL.md` in QS/VS). No events.

**ID 0 is the managed system itself**, for the panel and console
families, every operation of both (`model::SYSTEM_ID`): a standalone
system answers its own panel and console at ID 0 and refuses any other
ID (`QSI0331` `NO_PARTITION`); a host refuses ID 0 the same way. So
these families' bounds allow 0 to 999; nothing on the wire changed.

#### Version 2 (bit 24, `caps::PANEL_FUNCTIONS`): the functions a system has

| Op | Request | Reply |
|---|---|---|
| `0x0002` | `Functions` {`id`} | `Vec<FunctionInfo>` by number: `function u8`, `text` (what it does, the system's words), `manual` (it needs the keylock in manual), `confirm` (a console asks the person first) |

So a console shows what the system has, not a list of its own (QS/MC's
open question 33). Refusals for a managed side with no identifiers of
its own (a standalone system): `QSI0451` `NO_FUNCTION` (it has no such
function), `QSI0452` `NEEDS_MANUAL` (the keylock is not in manual),
`QSI0453` `NOT_NOW` (not in this state: before its IPL completes, for
one). `panel::Panel::VERSION` is 2.

### refcodes (`0x0015`)

| Op | Request | Reply |
|---|---|---|
| `0x0001` | `History` {`id`, `before` (a place in the history, none from the newest), `limit`} | `Page` {`codes`: `Posted` {`refcode`: `base::Refcode` (`seq`, `code`, `words`, `text`, `time_ms`), `source` (open: `Host` 1, `System` 2)}, newest first; `more`: the next page's `before`} |

| Event | Body | When |
|---|---|---|
| `0x0001` | `Code` {`id`, `name`, `posted`, `current`: it is the current code now (a `C2009` note is not)} | a code was posted |

### dumps (`0x0016`)

| Op | Request | Reply |
|---|---|---|
| `0x0001` | `List` {`id`} | `Vec<DumpInfo>` oldest first: `name`, `time`, `bytes`, `allocated` (in the pool), `code`, `reason` |
| `0x0002` | `Take` {`id`}: without restart | `Begun` {`name`}: written in the background |
| `0x0003` | `Delete` {`id`, `name`} | empty |
| `0x0004` | `Download` {`id`, `name`} | `Downloading` {`name`, `bytes`}: **opens a stream**, host to console, of the dump's `bytes` bytes (an ELF core), ending after the last |

| Event | Body | When |
|---|---|---|
| `0x0001` | `Taken` {`id`, `name`, `dump`, `error` (a `Note`) or none, `time`} | a dump was written or could not be |
| `0x0002` | `Removed` {`id`, `dump`, `time`} | a dump was deleted, or the oldest removed for the host's limits (`C2009060`) |

| Op | Request | Reply |
|---|---|---|
| `0x0005` | `Space` {`id`, 0 for every partition}, bit 26 | `Vec<PartitionSpace>` in ID order: `id`, `space` (base's `Space`) |

### console (`0x0017`)

| Op | Request | Reply |
|---|---|---|
| `0x0001` | `Attach` {`id`, `take_over`} | `Attached` {`serial`}: **opens a stream** both ways |
| `0x0002` | `Screen` {`id`} | `Capture` {`serial`, `frame`: `qsipart::console::Frame` or none, `text`, a line a row} |

The stream is bytes without boundaries, so each value goes in a frame of `qsipart`'s header, kind `0x0401` (`console::PIECE`), read back with `console::Pieces`; a frame of another kind is passed over. From the host, `Out`:
`1` `Frame` (`qsipart::console::Frame`: the first after attaching, and
when the system sends one), `2` `Update` (`qsipart::console::Update`),
`3` `Serial` (bytes, when the profile's console is serial), `4` `Ended`
(why: taken over, the partition ended; the stream closes after it). From
the console, `In`: `1` `Key` (`qsipart::console::Key`), `2` `Refresh`,
`3` `Bytes` (for the serial port). A tag a later version adds is kept
as `Other(tag)` and the rest of its piece passed over. One viewer at a
time: an attach without `take_over` is refused while another views (the
host's `QVS2314`; `QSI0601` `VIEWED` from a side with no identifiers of
its own). ID 0 is the managed system's own console (panel, above). Frames are checked with `qsipart`'s checks
(`Out::check`, `Capture::check`; an update against the last frame with
`qsipart::console::Grid`). Closing the stream detaches. No events.

### vres (`0x0018`)

| Op | Request | Reply |
|---|---|---|
| `0x0001` | `Pools` (empty) | `Vec<StoragePool>`: `name`, `uuid`, `protection`, `state`, `work`, `work_unit`, `percent`, `bytes`, `allocated`, `provisioned`, `repository`, `units`: `PoolUnit` {`number`, `disk`, `serial`, `location`, `bytes`, `state`}, `failure`. States and protections in the host's words |
| `0x0010` | `Vdisks` {`pool`, empty for every pool} | `Vec<Vdisk>`: `pool`, `name`, `bytes`, `allocated`, `thick`, `holder` |
| `0x0011` | `VdiskCreate` {`pool`, `name`, `mib`, `thick`} | `Vdisk` |
| `0x0012` | `VdiskResize` {`pool`, `name`, `mib`}: grow, not held | `Vdisk` |
| `0x0013` | `VdiskDelete` {`pool`, `name`, `force`: even when a profile names it} | empty |
| `0x0014` | `VdiskNamed` {`pool`, `name`} | `Vec<Naming>`: `partition`, `name`, `profile`, `slot`, `lun` |
| `0x0020` | `Images` (empty) | `Vec<Image>`: `name`, `bytes`, `label` (volume identifier), `holder` |
| `0x0021` | `ImageRename` {`name`, `to`} | empty |
| `0x0022` | `ImageDelete` {`name`} | empty |
| `0x0023` | `ImageUpload` {`name`, `bytes`} | `Uploading` {`name`, `bytes`}: **opens a stream**, console to host, of exactly `bytes` bytes; how it ended is the `Upload` event (more is `QSI0701`, fewer `QSI0702`, a refused image the host's message) |
| `0x0024` | `Drives` {`id`}: a running partition | `Vec<Drive>`: `slot`, `kind` (open: `Optical` 1, `Tape` 2, `PhysicalTape` 3), `name` (the medium, empty for none; a physical drive's location code), `prevented` |
| `0x0025` | `Load` {`id`, `slot`, `name`}: load an image, or mount a tape volume | empty |
| `0x0026` | `Unload` {`id`, `slot`, `forced`}: unload, or unmount | `Unloaded` {`name`} |
| `0x0030` | `Volumes` (empty) | `Vec<Volume>`: `name`, `identifier`, `capacity_mib`, `used` (bytes), `protected`, `holder` |
| `0x0031` | `VolumeCreate` {`name`, `identifier`, `mib`} | `Volume` |
| `0x0032` | `VolumeDelete` {`name`} | empty |
| `0x0033` | `VolumeProtect` {`name`, `on`} | empty |
| `0x0034` | `TapeDrives` (empty), bit 17 | `Vec<TapeDrive>`: `location`, `vendor`, `product`, `serial`, `released`, `loaded` (a cartridge is in it), `holder` (the partition and its tape device's slot; partition 0 for none) |
| `0x0040` | `Switches` (empty) | `Vec<Switch>`: `name`, `id`, `vlans`, `mtu`, `uplink`: `Uplink` {`interfaces` (location codes), `bond` (open: `ActiveBackup` 1, `Lacp` 2), `native_vlan`} or none, `uplink_state` (host's words), `members`: `Member` {`location`, `name`, `carrier`, `speed_mbps`}, `ports`, `failure` |
| `0x0041` | `Ports` {`switch`, empty for every one} | `Vec<Port>`: `partition`, `slot`, `switch`, `vlan`, `tagged`, `mac`, `link`, `counters`: `Counters` {`sent_bytes`, `sent_packets`, `sent_dropped`, `received_bytes`, `received_packets`, `received_dropped`} |
| `0x0042` | `Fdb` {`switch`} | `Vec<FdbRow>`: `mac`, `vlan`, `port`, `partition`, `slot`, `origin` |
| `0x0043` | `Vlans` (empty) | `Vec<VlanUse>`: `switch`, `vlan`, `native`, `ports`: `VlanPort` {`partition`, `slot`, `tagged`} |
| `0x0044` | `GetLimits` (empty) | `Limits`: `switches`, `vlans_per_switch`, `adapters_per_partition`, `ports_per_switch` |
| `0x0050` | `ProcPoolCreate` (`ProcPoolSpec` {`name`, `cores`, `maximum_units` (hundredths) or none}) | empty |
| `0x0051` | `ProcPoolChange` (`ProcPoolSpec`): no partition runs in it | empty |
| `0x0052` | `ProcPoolDelete` {`name`} | empty |

Storage pools and switches are read in version 1; bits 20 and 19 make
and change them (below). Shared processor pools are read in the host
family's `Host`.

**Storage pools managed** (bit 20, `caps::POOLS`): the pool service's
requests 0 and 2 to 8. A disk is named as `Disks` names it (the host's
name for it, its location code beside it).

| Op | Request | Reply |
|---|---|---|
| `0x0002` | `Disks` (empty) | `Vec<PoolDisk>`: `name`, `serial`, `location`, `bytes`, `block_size u32`, `what` (the host's words), `pool` (empty for none), `free`, `foreign` |
| `0x0003` | `PoolCreate` {`name`, `protection` (open `Protection`: `None` 1, `Mirrored` 2, `Raid5` 3, `Raid6` 4), `disks`, `spares`, `confirm` (the foreign disks' names, typed again)} | `StoragePool` |
| `0x0004` | `PoolDelete` {`name`}: it holds no virtual disk or media | empty |
| `0x0005` | `PoolRename` {`name`, `to`} | empty |
| `0x0006` | `UnitReplace` {`pool`, `unit u32`, `disk`, `confirm`} | empty |
| `0x0007` | `SpareAdd` {`pool`, `disk`, `confirm`} | empty |
| `0x0008` | `SpareRemove` {`pool`, `disk`} | empty |
| `0x0009` | `Designate` {`pool`, empty for no media repository} | empty |
| `0x000A` | `PoolExtend` {`pool`, `disks`, `confirm`} | `StoragePool` |

What follows is the existing `PoolChanged` (`0x0001`). A host whose
pools cannot grow refuses `PoolExtend` with `QSI1616` `CANNOT_GROW`:
QS/VS's, whose sets have a fixed size (QS/400's `protect` sets,
`docs/BUILDKIT.md` section 12); it is in the family so a later host
can. Bounds: names 64 bytes, lists 256 items (`QSI0002`). Refusals
(`vres::pools`), the host's own identifiers after these:

| Identifier | Name | When | QS/VS's |
|---|---|---|---|
| `QSI1601` | `BAD_NAME` | a pool's name not of the host's form | `QVS3001` |
| `QSI1602` | `NAME_TAKEN` | another pool has it | `QVS3002` |
| `QSI1603` | `NO_POOL` | no pool has it | `QVS3003` |
| `QSI1604` | `DISK_COUNT` | not the protection's number of disks | `QVS3004` |
| `QSI1605` | `NO_DISK` | no disk has the name | `QVS3005` |
| `QSI1606` | `DISK_TAKEN` | the disk cannot be used | `QVS3006` |
| `QSI1607` | `FOREIGN` | a foreign disk not confirmed | `QVS3007` |
| `QSI1608` | `TOO_SMALL` | the disk is too small | `QVS3008` |
| `QSI1609` | `BLOCK_SIZES` | the disks' block sizes differ | `QVS3009` |
| `QSI1610` | `SPARES` | a hot spare the pool cannot have | `QVS3010`, `QVS3017` |
| `QSI1611` | `TOO_LARGE` | larger than a pool may be | `QVS3011` |
| `QSI1612` | `NOT_EMPTY` | it holds virtual disks or media | `QVS3012` |
| `QSI1613` | `HELD` | a partition holds something in it | `QVS3013` |
| `QSI1614` | `NOT_USABLE` | not in a state to be used | `QVS3014` |
| `QSI1615` | `NOT_FAILED` | the unit is not failed or missing | `QVS3015` |
| `QSI1616` | `CANNOT_GROW` | the host's pools have a fixed size | none |
| `QSI1617` | `STEP_FAILED` | a step of making it failed | `QVS3016` |
| `QSI1618` | `MEDIA_IN_USE` | the media repository's media are in use (designate) | `QVS3205` |

**Switches managed** (bit 19, `caps::SWITCHES`): the switch service's
requests 1 to 6.

| Op | Request | Reply |
|---|---|---|
| `0x0045` | `SwitchCreate` (`SwitchSpec` {`name`, `vlans` (`Vec<u16>`), `mtu u32` (0: the host's default)}) | `Switch` |
| `0x0046` | `SwitchChange` (`SwitchSpec`) | empty |
| `0x0047` | `SwitchRename` {`name`, `to`} | empty |
| `0x0048` | `SwitchDelete` {`name`, `force` (even when a profile names it)} | empty |
| `0x0049` | `UplinkSet` {`switch`, `uplink` (`Uplink`, as `Switches` gives it)} | empty |
| `0x004A` | `UplinkClear` {`switch`} | empty |

What follows is the existing `SwitchChanged` (`0x0007`), which says
what changed. Bounds: names 64 bytes, the VLANs and the interfaces 256
items, each location code 256 bytes (`QSI0002`). Refusals
(`vres::switches`), the host's own identifiers after these:

| Identifier | Name | When | QS/VS's |
|---|---|---|---|
| `QSI1501` | `BAD_NAME` | a switch's name not of the host's form | `QVS4001` |
| `QSI1502` | `NAME_TAKEN` | another switch has it | `QVS4002` |
| `QSI1503` | `NO_SWITCH` | no switch has it | `QVS4003` |
| `QSI1504` | `BAD_VLAN` | a VLAN not valid, or more than a switch carries | `QVS4004`, `QVS4005` |
| `QSI1505` | `BAD_MTU` | an MTU not valid, or one an uplink cannot carry | `QVS4006`, `QVS4105` |
| `QSI1506` | `TOO_MANY` | more switches than the host may have | `QVS4007` |
| `QSI1507` | `NAMED` | a profile names it, or uses a VLAN taken away | `QVS4008`, `QVS4009` |
| `QSI1508` | `PORTS_ON` | a running partition's port is on it | `QVS4015` |
| `QSI1509` | `NO_INTERFACE` | no interface at the location | `QVS4101` |
| `QSI1510` | `MANAGEMENT` | the management interface | `QVS4102` |
| `QSI1511` | `OTHER_SWITCH` | in another switch's uplink | `QVS4103` |
| `QSI1512` | `NO_BOND` | several interfaces and no bond | `QVS4104` |
| `QSI1513` | `UPLINK_SIZE` | none, more than 8, or one named twice | `QVS4106`, `QVS4107` |
| `QSI1514` | `NATIVE_NOT_CARRIED` | a native VLAN the switch does not carry | `QVS4010` |
| `QSI1515` | `NOT_BUILT` | the kernel refused a step | `QVS4012` |

**Physical tape drives** (bit 17, physical I/O's). The host's own tape
drives are listed by `TapeDrives`, read only: a drive is released at
the host like any device. A profile's tape device names either a volume
of the media library, as before, or a physical drive by location code:
its `media` is `@` and the code (`@SPCIE-SLOT-4-T0.0.3.0`;
`MediaDevice::drive` writes it, `tape_source` reads it). One partition
holds a drive at a time (`QSI1303`); a location that is not a tape
drive is `QSI1306`. A running partition's `Drives` shows that slot as
`PhysicalTape` 3, its `name` the location code; its cartridge is
changed by hand, so `Load` and `Unload` of it are `QSI1307`.

| Event | Body | When |
|---|---|---|
| `0x0001` | `PoolChanged` {`name`, `pool` or none, `time`} | a storage pool's state, work or allocation changed |
| `0x0002` | `VdiskChanged` {`pool`, `name`, `vdisk` or none, `time`} | made, grown, deleted, held or given back |
| `0x0003` | `ImageChanged` {`name`, `image` or none, `time`} | added, renamed (the old name gone, the new one added), deleted, loaded, unloaded |
| `0x0004` | `UploadEnded` {`name`, `image` or none, `error` (a `Note`) or none, `time`} | an upload or a host-side import ended |
| `0x0005` | `VolumeChanged` {`name`, `volume` or none, `time`} | made, deleted, protected, mounted, unmounted |
| `0x0006` | `DriveChanged` {`id`, `drive`, `time`} | a partition's device was loaded or unloaded, from a console or by its system |
| `0x0007` | `SwitchChanged` {`name`, `switch` or none, `time`} | a switch was made, changed or deleted |
| `0x0008` | `UplinkChanged` {`switch`, `location`, `carrier`, `state`, `time`} | an uplink interface lost or regained carrier |
| `0x0009` | `PortChanged` {`partition`, `slot`, `switch`, `open`, `time`} | a partition's port opened or closed |
| `0x000A` | `TapeDriveChanged` {`location`, `drive` or none, `time`}, bit 17 | a physical tape drive was released or taken back, held or given back, a cartridge loaded or taken out; or it came or went |

### The families' message identifiers

`QSI03nn` to `QSI09nn` are the families'. A host's own refusals keep the
host's identifiers (`QVSnnnn`); these are for what QSI itself refuses.

| Range | Family | Defined in version 1 |
|---|---|---|
| `QSI0301`-`QSI0329` | host | `QSI0301` `SERVICE_DOWN`: the host's service that answers a request did not (its socket missing, no answer in time) |
| `QSI0330`-`QSI0359` | partitions | `QSI0331` `NO_PARTITION`: no partition of that ID, from a managed side with no identifier of its own (a QS/VS host answers `QVS2001`) |
| `QSI0360`-`QSI0399` | partition | `QSI0361` `NO_SUCH_VALUE`: an enumeration value this host does not have (a later version's shut down kind, IPL source, keylock); version 3's `QSI0362` `ID_TAKEN`, `QSI0363` `ID_OUT_OF_RANGE` |
| `QSI0401`-`QSI0449` | profiles | `QSI0401` `CHANGED` (the revision conflict), `QSI0402` `EXISTS` (a name there, no revision), `QSI0403` `NOT_ACTIVE` (save current of a partition not activated) |
| `QSI0450`-`QSI0499` | panel | for a side with no identifiers of its own: `QSI0451` `NO_FUNCTION`, `QSI0452` `NEEDS_MANUAL`, `QSI0453` `NOT_NOW` |
| `QSI0501`-`QSI0549` | refcodes | none |
| `QSI0550`-`QSI0599` | dumps | none |
| `QSI0601`-`QSI0699` | console | for a side with no identifiers of its own: `QSI0601` `VIEWED` |
| `QSI0701`-`QSI0999` | vres | `QSI0701` `UPLOAD_LONG`, `QSI0702` `UPLOAD_CUT` |
| `QSI1301`-`QSI1349` | physical I/O, host and vres (bit 17) | `QSI1301` to `QSI1307` (`host::physio`, above) |
| `QSI1401`-`QSI1449` | partition version 2 (bit 18) | `QSI1401` to `QSI1405` (`partition::entitlement`, above) |
| `QSI1501`-`QSI1549` | vres's switches managed (bit 19) | `QSI1501` to `QSI1515` (`vres::switches`, above) |
| `QSI1601`-`QSI1649` | vres's storage pools managed (bit 20) | `QSI1601` to `QSI1618` (`vres::pools`, above) |

### Left for later versions

Code update, problems and a system's own dumps, left here before, are
the update and service families and base's bit 4 above; physical I/O,
left here before too, is bit 17, and a running partition's entitlement
is the partition family's version 2. Switch and pool management, kept
at the host before, are bits 19 and 20, and a host's power-on settings
bit 23. What is still left will be a new capability bit (a new family,
or a family's later version):

- changing a running partition's virtual processors, dedicated
  processors or memory: QS/VS gives them once, at activation.

Kept at the host by decision rather than waiting on a version: the
host's other settings (its partitions' most, the memory block, the
reserve, the MAC prefix, the service's timeouts and dump limits), the
switches' limits, importing an image from the host's own drive, and
releasing a device for partitions (section 9). A running partition's
resources other than its entitlement do not change: QS/VS changes a
running shared partition's processing units, capped or uncapped and
weight (partition version 2), and nothing else.

## 8. The mapping: QS/VS's local protocols

Every request of `part/partd/src/proto.rs` (34), `stg/poold/src/proto.rs`
(26) and `net/vswitchd/src/proto.rs` (17), by its name there, at QS/VS
commit `cebf3d2`: 77 requests, 66 with a QSI message and 11 with none
(53 and 24 before bits 19 and 20 mapped the pool service's 2 to 8 and
the switch service's 1 to 6).

The service and update families and base's system dumps have no rows
here: the local protocols they will be translated to (the host's problem
log, its update service, its service data) do not exist yet in QS/VS.
Their mapping is QS/VS's to complete in its own documents
(`docs/QSISERVER.md` there) once those protocols exist. The same holds
for physical I/O (bit 17) and partition version 2 (bit 18): partd has
no request for a host's devices, its physical tape drives or a running
partition's entitlement at `cebf3d2`, and they are mapped there with
the phase that adds them. Likewise partition version 3's settings
(bit 21), activation times (22), the power-on settings (23, over
partd's settings and its end of every partition), the panel's functions
(24) and the dumps' space (26).

### partd: 34 requests, 32 mapped, 2 none

| # | Request | QSI |
|---|---|---|
| 0 | `List` | partitions `0x0001` List |
| 1 | `Create` | partition `0x0001` Create |
| 2 | `GetDefinition` | partitions `0x0002` Get (its `definition`) |
| 3 | `Change` | partition `0x0002` Change; profiles `0x0005` SetDefault (a change of `default_profile`) |
| 4 | `Delete` | partition `0x0003` Delete |
| 5 | `ListProfiles` | profiles `0x0001` List (with revisions the QSI host server keeps) |
| 6 | `GetProfile` | profiles `0x0002` Get |
| 7 | `PutProfile` | profiles `0x0003` Put; profiles `0x0006` SaveCurrent (composed: the running profile at `Current`) |
| 8 | `DeleteProfile` | profiles `0x0004` Delete |
| 9 | `Activate` | partition `0x0004` Activate |
| 10 | `ShutDown` | partition `0x0005` ShutDown, `Immediate`, no restart |
| 11 | `Restart` | partition `0x0006` Restart (and `0x0005` `Immediate` with restart) |
| 12 | `Resources` | host `0x0001` Host; host event `0x0001` |
| 13 | `CreatePool` | vres `0x0050` ProcPoolCreate |
| 14 | `ChangePool` | vres `0x0051` ProcPoolChange |
| 15 | `DeletePool` | vres `0x0052` ProcPoolDelete |
| 16 | `History` | refcodes `0x0001` History (paged by the QSI host server) |
| 17 | `GetSettings` | host `0x0002` Settings |
| 18 | `ChangeSettings` | **none**: the host's settings are changed at the host; host `0x0021` SetPowerOn (bit 23) carries the part a console changes, QS/VS's to map |
| 19 | `EndAll` | **none**: PID 1's at the host's shut down; a console reaches it through base `0x0003` Power |
| 20 | `Subscribe` | base `0x0005` Subscribe: `State`, `Resources`, `Defined`, `Channel` become partitions events `0x0001` to `0x0004`, `Code` refcodes `0x0001`, `Dump` dumps `0x0001` |
| 21 | `LoadMedia` | vres `0x0025` Load |
| 22 | `UnloadMedia` | vres `0x0026` Unload |
| 23 | `Media` | vres `0x0024` Drives |
| 24 | `Named` | vres `0x0014` VdiskNamed |
| 25 | `Adapters` | partitions `0x0003` Adapters |
| 26 | `ShutDownAs` | partition `0x0005` ShutDown |
| 27 | `Panel` | panel `0x0001` Function |
| 28 | `TakeDump` | dumps `0x0002` Take |
| 29 | `Dumps` | dumps `0x0001` List |
| 30 | `DeleteDump` | dumps `0x0003` Delete |
| 31 | `ReadDump` | dumps `0x0004` Download (its stream) |
| 32 | `Console` | console `0x0001` Attach (its stream) |
| 33 | `Screen` | console `0x0002` Screen |

### The pool service: 26 requests, 21 mapped, 5 none

| # | Request | QSI |
|---|---|---|
| 0 | `Disks` | vres `0x0002` Disks (bit 20); host `0x0003` Hardware (the disk items, owned by a pool, foreign or free) |
| 1 | `Pools` | vres `0x0001` Pools |
| 2 | `CreatePool` | vres `0x0003` PoolCreate (bit 20; a foreign disk is confirmed by typing its name) |
| 3 | `DeletePool` | vres `0x0004` PoolDelete |
| 4 | `RenamePool` | vres `0x0005` PoolRename |
| 5 | `ReplaceUnit` | vres `0x0006` UnitReplace |
| 6 | `AddSpare` | vres `0x0007` SpareAdd |
| 7 | `RemoveSpare` | vres `0x0008` SpareRemove |
| 8 | `Designate` | vres `0x0009` Designate |
| 9 | `Vdisks` | vres `0x0010` Vdisks |
| 10 | `CreateVdisk` | vres `0x0011` VdiskCreate |
| 11 | `ResizeVdisk` | vres `0x0012` VdiskResize |
| 12 | `DeleteVdisk` | vres `0x0013` VdiskDelete |
| 13 | `Images` | vres `0x0020` Images |
| 14 | `Upload` | vres `0x0023` ImageUpload (its stream; the second answer is vres event `0x0004`) |
| 15 | `RenameImage` | vres `0x0021` ImageRename |
| 16 | `DeleteImage` | vres `0x0022` ImageDelete |
| 17 | `ImportImage` | **none**: from the host's own removable volume or drive, at the host; its end is vres event `0x0004` |
| 18 | `Volumes` | vres `0x0030` Volumes |
| 19 | `CreateVolume` | vres `0x0031` VolumeCreate |
| 20 | `DeleteVolume` | vres `0x0032` VolumeDelete |
| 21 | `ProtectVolume` | vres `0x0033` VolumeProtect |
| 22 | `Hold` | **none**: internal, the partition manager's at activation (it passes open files) |
| 23 | `Release` | **none**: internal, the partition manager's |
| 24 | `Holders` | **none**: internal; each item's `holder` is in its list |
| 25 | `StopAll` | **none**: PID 1's at the host's shut down |

### The switch service: 17 requests, 13 mapped, 4 none

| # | Request | QSI |
|---|---|---|
| 0 | `Switches` | vres `0x0040` Switches |
| 1 | `Create` | vres `0x0045` SwitchCreate (bit 19) |
| 2 | `Change` | vres `0x0046` SwitchChange |
| 3 | `Delete` | vres `0x0048` SwitchDelete |
| 4 | `Rename` | vres `0x0047` SwitchRename |
| 5 | `SetUplink` | vres `0x0049` UplinkSet |
| 6 | `ClearUplink` | vres `0x004A` UplinkClear |
| 7 | `Interfaces` | host `0x0003` Hardware (the interface items, owned by the host, a switch or none) |
| 8 | `Ports` | vres `0x0041` Ports |
| 9 | `Fdb` | vres `0x0042` Fdb |
| 10 | `Vlans` | vres `0x0043` Vlans |
| 11 | `OpenPort` | **none**: internal, the partition manager's at activation (it passes the tap) |
| 12 | `ClosePort` | **none**: internal |
| 13 | `GetLimits` | vres `0x0044` Limits |
| 14 | `ChangeLimits` | **none**: at the host |
| 15 | `Subscribe` | base `0x0005` Subscribe: `Switch`, `Uplink`, `Port` become vres events `0x0007` to `0x0009` |
| 16 | `Reconcile` | **none**: internal, the partition manager's when it starts |

### What QSI has that no local request gives

The QSI host server composes these:

- profile revisions (profiles `0x0001` to `0x0003`, `0x0006`, event
  `0x0001`): partd keeps none; the host server keeps one per profile and
  serialises puts so the check and the put are one step;
- profiles `0x0006` SaveCurrent: from the running profile (`Status.profile`)
  and `Current`, put with partd's `PutProfile`;
- the history's places and pages (refcodes `0x0001`): partd's `History`
  is the last 256, oldest first, without numbers;
- host `0x0003` Hardware: `inventory.toml` with the pool service's
  `Disks` and the switch service's `Interfaces` for each item's owner;
  host event `0x0002` from their changes;
- vres events `0x0001` to `0x0006`: the pool service has no
  subscription; the host server sends them for what it changed and what
  it finds when it polls (the partition manager polls the pools every
  two seconds already);
- dumps event `0x0002`: from `C2009060` in the history, and deletes;
- vres `0x000A` PoolExtend: the pool service has no request to grow a
  pool, and QS/VS's sets cannot, so its host server refuses it
  (`QSI1616`).

Left out as the host's alone: a partition's VMM process (`Status.pid`),
a switch's bridge (`SwitchStatus.bridge`), a port's tap
(`PortStatus.tap`), a pool's mount point (`PoolStatus.mount`).

## 9. Decisions, and what a host's server supplies

### Decisions

- **`Environment`**: QS/VS names its one environment by the product's
  name; this neutral crate cannot, so it is the open value `Standalone`
  (the operating system a standalone system runs). A host maps its word
  to it.
- **The keylock**: QS/VS has four positions; base's `Keylock` gained
  `Auto` 3 and `Secure` 4, additively.
- **`Current` flattened**: QS/VS's `Processors` enum has a tag a later
  mode would make unreadable; QSI's `Current` is a struct with an open
  `mode`.
- **Reference codes**: a partition's history uses base's `Refcode`
  (`time_ms`, `seq`) with a `source`, so a console handles a host's,
  a standalone system's and a partition's codes alike.
- **`VdiskNamed`** is structured (partition, profile, slot, LUN); partd's
  `Named` answers text (`PROD profile NORMAL slot 2`), which the host
  server reads.
- **Tape mount and unmount** are vres `Load` and `Unload` on a tape
  slot, as partd's `LoadMedia` and `UnloadMedia` are one for both.
- **`Restart`** is kept beside `ShutDown` `Immediate` with restart: both
  are partd's, and the handoff names it.
- **The console stream** carries the serial port's bytes too: a profile
  whose console is serial has no frames.
- **A physical tape drive in a profile** is `@` and its location code in
  the tape device's `media`, not a field of its own: `MediaDevice` is in
  a list and `Profile` is in the middle of `Put` and `Revised`, so a
  field after the known ones would not be passed over, and a new field
  would take a profiles version 2. No library name (a letter first, then
  letters, digits, `-` and `_`) and no location code has `@`, and a
  console without bit 17 keeps the text and puts it back unchanged.
- **Physical I/O is one bit for two families**: the host's devices and
  vres's tape drives are one feature of a host, released at the host
  the same way, so a console has both or neither.
- **A device's list is an operation of its own** (`Devices`), not
  fields after the known ones of the hardware `Item`: an item is in a
  list, where fields after it would not be passed over.
- **A physical drive in `Drives`** is `PhysicalTape` 3, a new value of
  an open enumeration, which a console without bit 17 keeps as
  `Other(3)`.
- **The entitlement is the partition family's version 2**, as the
  handoff asks: its own bit, its operation after version 1's, and the
  family's first event. The partitions family's `ResourcesChanged`
  follows each change as well, so every console's `Current` is right.
- **The entitlement's refusals** have a range of their own
  (`QSI1401`-`QSI1449`) beside the partition family's, as base's dumps
  have theirs; physical I/O's are `QSI1301`-`QSI1349`.
- **New operations, not new fields** (handoff 24): every addition is an
  operation or event with types of its own behind a bit of its own
  (section 5). `ipc::wire_struct!` fails when the bytes run out, so a
  field added to a known body breaks a new reader against an older
  sender, and a struct in a list or mid-body cannot gain one at all.
  So a partition's settings are `Settings` beside its `Definition`, a
  problem's severity is beside its `Summary` (`Rated`, `RatedProblem`:
  `Summary` is in a list and in the middle of `Problem` and `Noticed`),
  activation times are lists of their own, and the fix level is not in
  base's `State`.
- **A later version's bit alone gates it**: partition version 3 needs
  bits 10 and 21, not 18; a host may have its settings without the
  entitlement, or the other way.
- **ID 0 is the managed system itself** for the panel and console
  families: a standalone system has one panel and one console, its own,
  and answers them at 0, refusing any other ID with `QSI0331`; a host
  refuses 0 the same way. Only the bounds changed; a host of before
  refuses 0 with `QSI0002`.
- **Panel and console refusals of QSI's own** (`QSI0451` to `QSI0453`,
  `QSI0601`) are for a managed side with no identifiers of its own; a
  host keeps its own (`QVS2306`, `QVS2314`).
- **Extend is refused by name**: `PoolExtend` is in the family though
  QS/VS's pools cannot grow (their sets have a fixed size), so a later
  host can answer it and a console asks the same way of both;
  `QSI1616` says why.
- **One switch spec for create and change**, as `ProcPoolSpec` is: the
  same three fields; the operation says which.
- **The power-on settings are not the IPL's**: QSI sets which partitions
  start with the host and how they end when it powers off; the host's
  IPL side, keylock and boot mode stay as they are (QS/MC's open
  question 30).
- **A managed system's name** is set at the system: QSI never renames
  it.
- **A host's IPL side** changes only by an update's activation (the
  update family).
- **Releasing a device for partitions**, and taking it back, is done at
  the host, where its isolation group and what the host uses are seen.
- **A pin does not expire.** It is the SHA-256 of a public key, and
  nothing secret is kept beside it that could age: the pairing password
  is good for one pairing and gone. Each side keeps when it paired
  (a standalone QS/400 shows it in `WRKQSICSL`), so a console may show
  a pin's age; a pin ends only when it is removed (`RMVQSICSL`, the DST
  option, or the console's own reset) and the two sides pair again
  (QS/MC's open question 31).

### Found in QS/VS that the handoff did not foresee

- A refused request answers with **every** message that applies, each
  with second-level help (`Vec<Issued>`). Settled: the session's
  refusal carries the first message with its help and the rest in
  `also` (section 4).
- partd has **no profile revisions** and no compare-and-put; the QSI
  host server keeps them.
- partd has **no request to save the current configuration** as a
  profile; it is composed.
- partd's history has **no sequence numbers**: places are the host
  server's.
- The **pool service has no events**; the switch service and partd do.
- An upload is answered **twice** by the pool service (`()` then the
  image); QSI's second answer is an event.
- partd's `Named` answers **text**, not structure.
- The console may be the **serial port** (`ConsoleOut::Serial`,
  `ConsoleIn::Bytes`), not only `qsipart`'s frames.
- QS/VS's keylock has **auto and secure**; base's had two positions.
- A profile carries **physical I/O** already (QS/VS's phase 10), before
  any family manages it.
- **Tape volume protection** (`ProtectVolume`) is a request the handoff
  did not list; vres has it (`0x0033`).


## 10. Message identifiers

A refusal carries a message identifier and its text; a managed side's
own refusals carry its identifiers (QS/VS's), passed through as it
gives them. QSI's own (`qsi::msgid`, and each family's `msgid`):

| Range | Whose |
|---|---|
| `QSI0001` to `QSI0099` | shared with the partition scope: `QSI0001` an unknown family or operation, `QSI0002` a body that does not read or is out of bounds, `QSI0003` a family not agreed; `QSI0010` to `QSI0022` are the partition scope's (`docs/QSID.md` section 4) |
| `QSI0101` to `QSI0112` | the session: `0101` not pinned, `0102` too many requests, `0103` too many streams, `0104` too long, `0105` no credit, `0106` no such stream, `0107` timed out, `0108` out of order, `0109` a bad frame, `0110` closing, `0111` no common version, `0112` the wrong kind of peer |
| `QSI0201` to `QSI0204` | pairing (section 7, pair) |
| `QSI0301` to `QSI0999` | the host families, a range each (section 7, "The families' message identifiers"): among them partition version 3's `0362` and `0363`, the panel's `0451` to `0453`, the console's `0601` |
| `QSI1001` to `QSI1099` | service: `1001` the condition still holds, `1002` no such problem, `1003` closed already |
| `QSI1101` to `QSI1199` | update: `1101` to `1105` an image refused by name (signature, damaged, product, older, this system's state), `1106` to `1114` the steps out of order or busy (section 7, update) |
| `QSI1201` to `QSI1249` | base's system dumps: `1201` no such dump, `1202` one is being taken, `1203` a kind not taken |
| `QSI1301` to `QSI1349` | physical I/O: `1301` no such location, `1302` not released, `1303` held by another partition, `1304` its group split, `1305` not assignable now, `1306` not a tape drive, `1307` a physical drive's slot loaded or unloaded |
| `QSI1401` to `QSI1449` | a running partition's entitlement: `1401` not running, `1402` dedicated, `1403` outside its profile, `1404` not what its virtual processors need, `1405` the pool has not that much |
| `QSI1501` to `QSI1549` | switches managed: `1501` to `1515` (section 7, vres) |
| `QSI1601` to `QSI1649` | storage pools managed: `1601` to `1618` (section 7, vres) |

## 11. A standalone QS/400's listener

A standalone QS/400 answers QSI itself, on these three crates
unchanged: the server job `QSIDSRV` (`*QSI`, port 2300), its library
`net/qsisvr`, and the storage service's side. `docs/QSID.md` section 12
is the system's side in full: the server, its commands and the DST
option, what is kept where, the audit entries. This is what a console
sees of it.

- **The transport and trust** are sections 1 to 3 as they are: TLS 1.3
  with graviola, the system's Ed25519 identity made once (at the first
  IPL after install, by the server's first start) and kept sealed by
  the LIC's key slot, at most two consoles pinned by fingerprint,
  pairing by a one-time password the system's operator asks for at DST
  or with `STRQSIPAIR` (shown once, fifteen minutes, five attempts).
  Before a console is pinned it is answered for pairing alone
  (`QSI0101`). A console removed (`RMVQSICSL`, DST) loses its session
  within a second.
- **The limits before authentication** are a QS/VS host's: six
  sessions, four handshakes under way, ten seconds a handshake, an
  address with ten failures in a minute turned away for a minute.
- **The hello** says kind standalone, the system's name, release and
  serial number, and `caps::STANDALONE`: base and pair, service with its
  version 2 (bits 2 and 25), the system's dumps and their space (4 and
  26), its own panel and console at ID 0 with the panel's version 2
  (12, 15 and 24) and its fix level (27). No code update (bit 3 clear:
  the system takes fixes by PTF) and no other host family; a request
  of either is `QSI0003`.
- **Base.** `Identify` (kind standalone; the model is `QMODEL`);
  `State` from the IPL and the codes `slic` posts (`starting`,
  `running`, `ending`, `halted`; the keylock normal or manual as the
  IPL read it; the slot `A`, `B` or `D`); `Power` as `PWRDWNSYS`
  (controlled with its delay or immediate, restart or not; refused
  while one is under way); `Refcodes` of this IPL, each numbered by the
  IPL's count and its place, so never reused; `Subscribe` and the
  events (each code posted, a change of state, a dump taken or
  removed), the log kept on disk and taken up again with
  `EventLog::resume`, so a console that reconnects after an IPL is
  given the IPL's codes; `Time`; `FixLevel`, the cumulative PTF package;
  the system's dumps (one kind, service data) with their space.
- **Service** is the problem log: a problem's number is its ten-digit
  identifier; its severity from the log's; `Note` writes into the
  problem's notes naming the request's user, `Close` notes who closed
  it; a problem's condition is not tracked, so `holds` is false and
  `QSI1001` is not given. Events: opened, noted, closed.
- **Panel** at ID 0, with the functions a standalone system carries
  out (`Functions` lists them): 01, 11 to 19 and 20 at any time; 03, 08,
  21 (dedicated service tools on the console) and 22 (a system dump)
  with the keylock in manual (`QSI0452`) and the IPL complete
  (`QSI0453`, also while the service tools show); any other
  `QSI0451`. Any other ID is `QSI0331`.
- **Console** at ID 0: the system's console as `qsipart`'s frames and
  keys, the local display showing the same (`docs/QSID.md` section 12).
- **What each console may ask.** The request's user is a name for the
  job log and the problem log's notes, and grants nothing; a paired
  console may do what the families above offer.

## 12. Proof

**The messages.** `cargo test -p qsi`: every sample of every family
(`qsi::samples`, every operation and event of base with its system
dumps, dump space and fix level, pair, service with its version 2,
update and the nine host families with every later bit) encoded and
decoded by number, and again with a later
version's bytes after it; an unknown operation or family is `None`, a
truncated or damaged body an error and never a panic, an enumeration
value a later version adds is kept; every request in bounds and each
bound refused; the console stream's pieces split anywhere; each
message's capabilities (`of_request`, `of_event`: bit 4 for base's
dumps alone, bit 17 for the host's devices and vres's tape drives
alone, bit 18 for the entitlement alone, each of bits 19 to 27 for its
own and nothing else, base's dump space for bits 4 and 26 together,
`HOST_V1`, `SERVICING` and `MANAGED` as they were, `STANDALONE`); each
new request in bounds and out (ID 0 allowed to the panel and console
families, a note of 1 to 256 bytes, a created partition's ID, a
setting's delay and description, the pools' and switches' names, lists
and location codes); every new open enumeration kept; a tape
device that is a volume, a physical drive or empty, and a drive's
location bounded as a text.

**The session, in memory.** `cargo test -p qsisess`: the handshake
both ways; unequal peers (version 2 to 1, capabilities intersected, a
family the other lacks refused `QSI0003`); every message of every
family through a console session and a host session in pieces of 1,
13 and 4096 bytes (`tests/samples.rs`: 105 exchanges and every one of
the 44 events); peers of different ages (`tests/caps.rs`: a console
never asks an older managed side the 45 new requests of bits 2 to 4
and 17 to 27, a managed side refuses them `QSI0003` from a console that
sends them anyway, an older console is sent none of the 22 new events
and a newer one all, its replay counts only what it is sent; and each
of the fourteen bits alone (service's version 2 with service's own
bit), at the host or at the console, lets through its own requests and
events and none of the others'); a
log taken up with `EventLog::resume` numbering on and replaying what it
was given, and refusing events with a gap, out of order or not ending
before its next number;
refusals with every message and their help; events fresh, then
replayed after a reconnect exactly as missed, and "read everything
again" when the log no longer reaches; three streams at once with one
stalled while the others and the requests complete; each limit refused
by name (requests, streams, a 600 KB message each way in parts, too
long, a user too long, no credit, a stream never opened, keep-alive
and its timeout, a clean close, a hello out of order, junk); an
unpinned console answered only for pairing.

**Over real TLS on loopback.** `cargo test -p qsitrust` and
`qsisess`'s `tests/tls.rs`: rustls with graviola completes TLS 1.3
both ways with self-signed Ed25519 certificates; our certificate and
key are byte for byte the ones QS/VS's `base/identity` writes for the
same inputs, and QS/VS's own bytes complete the handshake; a
certificate not pinned is refused at either end; a console pairs,
connects again pinned and is answered `identify`; before the pin
`identify` is `QSI0101`; a wrong password, a replayed exchange (the
same start and confirmation on a new connection) and an exchange
relayed through a third certificate (a relay that terminates TLS with
its own certificate on each leg, passes every message through and knows
the password) all end in `QSI0202` with nothing pinned; once the
password is retired, a console nobody pinned gets no session.

**A standalone QS/400.** `docs/QSID.md` section 12 "Proof": `qsisvr`'s
connection, pairing and families in memory and over TLS on loopback,
its two fuzz targets, and QEMU steps 201 to 206 (pairing from the
command and from DST, every base request, an IPL's codes replayed
after a reconnect, function 21, a problem opened, noted and closed, a
wrong password five times, a replayed exchange and an unpinned console
refused, a controlled power down).

**From outside.** `cargo run -p build -- fixture`: the fixture's PID 1
takes the three crates as git dependencies, makes two identities,
pairs a console with a managed side over TLS on a socket pair, connects
again pinned and asks `identify`. graviola needs a processor with AVX2,
ADX, AES and PCLMULQDQ (x86-64-v3 and the crypto extensions): QEMU's
default model has none of them, so the fixture's guest now runs with
the host's processor, as the conformance harness's always has.

**Fuzzing.** `cargo run -p build -- fuzz --target T --secs 60`, no
finding in any:

| Target | Runs | Corpus | Edges |
|---|---|---|---|
| `qsisess-frames` | 286,190 | 416 | 914 |
| `qsisess-handshake` | 289,207 | 319 | 605 |
| `qsisess-streams` | 180,150 | 825 | 1,219 |

With service, update and the system's dumps (handoff 19; the loopback
hellos now say `caps::SERVICING`, and the frames corpus gained a seed of
each of the 13 new requests and 13 new events), again 60 seconds each,
no finding:

| Target | Runs | Corpus | Edges |
|---|---|---|---|
| `qsisess-frames` | 294,069 | 734 | 1,480 |
| `qsisess-handshake` | 294,709 | 433 | 608 |
| `qsisess-streams` | 169,523 | 950 | 1,222 |

With physical I/O and partition version 2 (handoff 20; the loopback
hellos also say bits 17 and 18, and the frames corpus gained a seed of
each of the 3 new requests and 7 new events), again 60 seconds each, no
finding:

| Target | Runs | Corpus | Edges |
|---|---|---|---|
| `qsisess-frames` | 296,895 | 820 | 1,805 |
| `qsisess-handshake` | 300,085 | 447 | 608 |
| `qsisess-streams` | 186,675 | 958 | 1,219 |

With bits 19 to 27 (handoff 24; the loopback hellos say every bit,
`qsisess::loopback::CAPS`, and the frames corpus gained a seed of each
of the 29 new requests and 2 new events), again 60 seconds each, no
finding:

| Target | Runs | Corpus | Edges |
|---|---|---|---|
| `qsisess-frames` | 268,254 | 936 | 2,099 |
| `qsisess-handshake` | 286,261 | 450 | 607 |
| `qsisess-streams` | 163,123 | 952 | 1,219 |

**Neutral.** `tools/buildkit/tests/neutral.rs`: `qsi` reaches `ipc` and
`qsipart` of this repository, `qsisess` those and `qsi`, `qsitrust`
nothing (for the host and the musl target), and none names the product.
`cargo deny` passes; `spake2` 0.4 brings `curve25519-dalek` 4.1.3,
`fiat-crypto`, `hkdf` 0.12, all MIT or Apache-2.0, no C.
