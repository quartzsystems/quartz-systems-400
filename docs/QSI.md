# QSI: the management protocol

QSI is the protocol a management console (QS/MC) speaks to what it
manages: a QS/VS host now, a standalone QS/400 later. Its partition
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
QS/400 itself runs no QSI listener yet (section 11).

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

A console and a standalone system say `caps::MANAGED` (base and pair);
a QS/VS host says `caps::HOST_V1`, base, pair and the nine host
families. `caps::SERVICING` is bits 2 to 4 (problems, code update and
the system's own dumps), said beside the rest by a managed side that
answers them and by a console that asks them; `HOST_V1` is unchanged.

A bit need not be a whole family: base's version 1 gained the system's
own dumps as operations behind bit 4, so what a request needs is
`caps::of_request(family, op)` (the family's bit, and bit 4 for those
operations) and what an event needs `caps::of_event(family, op)`. The
session checks a request against them at both ends (`QSI0003`) and
sends an event only to a console that has all of it (section 4).

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
standalone system later); the nine host families by a QS/VS host. Each is a `Family` with three
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
| a partition ID | 1 to 999 (0 only where a request says "every partition") |
| a name a console sends | 64 bytes |
| a text a console sends (a backing, a location code) | 256 bytes |
| a list a console sends | 256 items |
| a shut down's delay, base's power down's delay | 99999 seconds |
| a panel function | 1 to 99 |
| serial bytes in one piece of a console stream | 16 KiB (a piece is one frame) |

The host's own rules (a name's form, memory in blocks) are the host's to
refuse with its own messages.

### Capability bits and versions

| Family | Number | Version | Capability (`caps::`) | Bit |
|---|---|---|---|---|
| base | `0x0001` | 1 | `BASE` | 0 |
| pair | `0x0002` | 1 | `PAIR` | 1 |
| service | `0x0003` | 1 | `SERVICE` | 2 |
| update | `0x0004` | 1 | `UPDATE` | 3 |
| base's system dumps | `0x0001` | 1 | `SYSTEM_DUMPS` | 4 |
| host | `0x0010` | 1 | `HOST` | 8 |
| partitions | `0x0011` | 1 | `PARTITIONS` | 9 |
| partition | `0x0012` | 1 | `PARTITION` | 10 |
| profiles | `0x0013` | 1 | `PROFILES` | 11 |
| panel | `0x0014` | 1 | `PANEL` | 12 |
| refcodes | `0x0015` | 1 | `REFCODES` | 13 |
| dumps | `0x0016` | 1 | `DUMPS` | 14 |
| console | `0x0017` | 1 | `CONSOLE` | 15 |
| vres | `0x0018` | 1 | `VRES` | 16 |

A QS/VS host says `caps::HOST_V1`, bits 0, 1 and 8 to 16, and with its
next phase `caps::SERVICING` (bits 2 to 4) beside it. A later version
of a family has a bit of its own.

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
| `MediaDevice` | `slot`, `media` (an image or volume name) |
| `PhysicalIo` | `location`, `required` (carried so a profile reads whole; managed in a later version) |
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

### partitions (`0x0011`)

| Op | Request | Reply |
|---|---|---|
| `0x0001` | `List` (empty) | `Vec<Status>` in ID order: `id`, `name`, `state`, `code` and `text` (the current reference code), `current` (none when not activated), `profile` (last activated with), `last_end` (`exit N`, `signal N`), `message` (a `Note`: why it is not activated or in error), `ethernet`: `Link` {`slot`, `switch`, `mac`, `link`}, `channel` |
| `0x0002` | `Get` {`id`} | `Partition` {`definition`, `status`} |
| `0x0003` | `Adapters` {`id`, 0 for every partition} | `Vec<Adapter>`: `partition`, `name`, `profile`, `slot`, `switch`, `vlan`, `tagged`, `mac`, `required` |

| Event | Body | When |
|---|---|---|
| `0x0001` | `StateChanged` {`id`, `name`, `state`, `time`} | a state changed |
| `0x0002` | `ResourcesChanged` {`id`, `name`, `current`, `time`} | resources given at an activation or given back at an end |
| `0x0003` | `Defined` {`id`, `name`, `exists`, `time`} | created, changed or deleted |
| `0x0004` | `ChannelChanged` {`id`, `name`, `channel`, `time`} | the service channel came or went, stopped or started answering, or the system reported a change |

### partition (`0x0012`)

| Op | Request | Reply |
|---|---|---|
| `0x0001` | `Create` {`name`} | `Definition`, with its ID and serial number |
| `0x0002` | `Change` {`definition`}: name, keylock, default profile, auto start; not activated | empty |
| `0x0003` | `Delete` {`id`, `disks`: its virtual disks too, every one or none} | empty |
| `0x0004` | `Activate` {`id`, `profile` (empty: the default), `source` override, `keylock` override} | `Current`: what it was given; it is Starting |
| `0x0005` | `ShutDown` {`id`, `kind` (open `ShutdownKind`: `Delayed` 1, `System` 2, `Immediate` 3), `delay` (s, 0 to 99999), `restart`} | empty: accepted by the system, or the VMM asked |
| `0x0006` | `Restart` {`id`}: immediate, activated again with the profile it had | empty |

No events: what follows comes as the partitions and refcodes families'.

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
host's `QVS2314`). Frames are checked with `qsipart`'s checks
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
| `0x0024` | `Drives` {`id`}: a running partition | `Vec<Drive>`: `slot`, `kind` (open: `Optical` 1, `Tape` 2), `name` (the medium, empty for none), `prevented` |
| `0x0025` | `Load` {`id`, `slot`, `name`}: load an image, or mount a tape volume | empty |
| `0x0026` | `Unload` {`id`, `slot`, `forced`}: unload, or unmount | `Unloaded` {`name`} |
| `0x0030` | `Volumes` (empty) | `Vec<Volume>`: `name`, `identifier`, `capacity_mib`, `used` (bytes), `protected`, `holder` |
| `0x0031` | `VolumeCreate` {`name`, `identifier`, `mib`} | `Volume` |
| `0x0032` | `VolumeDelete` {`name`} | empty |
| `0x0033` | `VolumeProtect` {`name`, `on`} | empty |
| `0x0040` | `Switches` (empty) | `Vec<Switch>`: `name`, `id`, `vlans`, `mtu`, `uplink`: `Uplink` {`interfaces` (location codes), `bond` (open: `ActiveBackup` 1, `Lacp` 2), `native_vlan`} or none, `uplink_state` (host's words), `members`: `Member` {`location`, `name`, `carrier`, `speed_mbps`}, `ports`, `failure` |
| `0x0041` | `Ports` {`switch`, empty for every one} | `Vec<Port>`: `partition`, `slot`, `switch`, `vlan`, `tagged`, `mac`, `link`, `counters`: `Counters` {`sent_bytes`, `sent_packets`, `sent_dropped`, `received_bytes`, `received_packets`, `received_dropped`} |
| `0x0042` | `Fdb` {`switch`} | `Vec<FdbRow>`: `mac`, `vlan`, `port`, `partition`, `slot`, `origin` |
| `0x0043` | `Vlans` (empty) | `Vec<VlanUse>`: `switch`, `vlan`, `native`, `ports`: `VlanPort` {`partition`, `slot`, `tagged`} |
| `0x0044` | `GetLimits` (empty) | `Limits`: `switches`, `vlans_per_switch`, `adapters_per_partition`, `ports_per_switch` |
| `0x0050` | `ProcPoolCreate` (`ProcPoolSpec` {`name`, `cores`, `maximum_units` (hundredths) or none}) | empty |
| `0x0051` | `ProcPoolChange` (`ProcPoolSpec`): no partition runs in it | empty |
| `0x0052` | `ProcPoolDelete` {`name`} | empty |

Storage pools and switches are read only: they are made at the host.
Shared processor pools are read in the host family's `Host`.

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

### The families' message identifiers

`QSI03nn` to `QSI09nn` are the families'. A host's own refusals keep the
host's identifiers (`QVSnnnn`); these are for what QSI itself refuses.

| Range | Family | Defined in version 1 |
|---|---|---|
| `QSI0301`-`QSI0329` | host | `QSI0301` `SERVICE_DOWN`: the host's service that answers a request did not (its socket missing, no answer in time) |
| `QSI0330`-`QSI0359` | partitions | `QSI0331` `NO_PARTITION`: no partition of that ID, from a managed side with no identifier of its own (a QS/VS host answers `QVS2001`) |
| `QSI0360`-`QSI0399` | partition | `QSI0361` `NO_SUCH_VALUE`: an enumeration value this host does not have (a later version's shut down kind, IPL source, keylock) |
| `QSI0401`-`QSI0449` | profiles | `QSI0401` `CHANGED` (the revision conflict), `QSI0402` `EXISTS` (a name there, no revision), `QSI0403` `NOT_ACTIVE` (save current of a partition not activated) |
| `QSI0450`-`QSI0499` | panel | none |
| `QSI0501`-`QSI0549` | refcodes | none |
| `QSI0550`-`QSI0599` | dumps | none |
| `QSI0601`-`QSI0699` | console | none |
| `QSI0701`-`QSI0999` | vres | `QSI0701` `UPLOAD_LONG`, `QSI0702` `UPLOAD_CUT` |

### Left for later versions

Code update, problems and a system's own dumps, left here before, are
the update and service families and base's bit 4 above. What is still
left will be a new capability bit (a new family, or a family's version
2):

- physical I/O: giving a host's device to a partition (a profile carries
  `physical_io` already, so it reads whole).

Kept at the host by decision rather than waiting on a version: changing
the host's settings, storage pools, switches, uplinks and the switches'
limits (section 8), and changing a running partition's resources, which
QS/VS does not do.

## 8. The mapping: QS/VS's local protocols

Every request of `part/partd/src/proto.rs` (34), `stg/poold/src/proto.rs`
(26) and `net/vswitchd/src/proto.rs` (17), by its name there, at QS/VS
commit `cebf3d2`: 77 requests, 53 with a QSI message and 24 with none.

The service and update families and base's system dumps have no rows
here: the local protocols they will be translated to (the host's problem
log, its update service, its service data) do not exist yet in QS/VS.
Their mapping is QS/VS's to complete in its own documents
(`docs/QSISERVER.md` there) once those protocols exist.

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
| 18 | `ChangeSettings` | **none**: the host's settings are changed at the host in version 1 |
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

### The pool service: 26 requests, 14 mapped, 12 none

| # | Request | QSI |
|---|---|---|
| 0 | `Disks` | host `0x0003` Hardware (the disk items, owned by a pool, foreign or free) |
| 1 | `Pools` | vres `0x0001` Pools |
| 2 | `CreatePool` | **none**: made at the host (it erases disks; a foreign disk is confirmed by typing its name) |
| 3 | `DeletePool` | **none**: at the host |
| 4 | `RenamePool` | **none**: at the host |
| 5 | `ReplaceUnit` | **none**: at the host, where the disk is |
| 6 | `AddSpare` | **none**: at the host |
| 7 | `RemoveSpare` | **none**: at the host |
| 8 | `Designate` | **none**: at the host |
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

### The switch service: 17 requests, 7 mapped, 10 none

| # | Request | QSI |
|---|---|---|
| 0 | `Switches` | vres `0x0040` Switches |
| 1 | `Create` | **none**: made at the host |
| 2 | `Change` | **none**: at the host |
| 3 | `Delete` | **none**: at the host |
| 4 | `Rename` | **none**: at the host |
| 5 | `SetUplink` | **none**: at the host, where the cables are |
| 6 | `ClearUplink` | **none**: at the host |
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
- dumps event `0x0002`: from `C2009060` in the history, and deletes.

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
| `QSI0301` to `QSI0999` | the host families, a range each (section 7, "The families' message identifiers") |
| `QSI1001` to `QSI1099` | service: `1001` the condition still holds, `1002` no such problem, `1003` closed already |
| `QSI1101` to `QSI1199` | update: `1101` to `1105` an image refused by name (signature, damaged, product, older, this system's state), `1106` to `1114` the steps out of order or busy (section 7, update) |
| `QSI1201` to `QSI1249` | base's system dumps: `1201` no such dump, `1202` one is being taken, `1203` a kind not taken |

## 11. Later: a standalone QS/400's listener

None of this runs in QS/400 yet: no listener, no identity, no pins.
A later handoff puts `qsid` on TCP for a standalone system, built on
these three crates unchanged. What it adds:

- **The identity.** An Ed25519 key and certificate made once at the
  first IPL after install, as a QS/VS host makes its own, kept where
  the system keeps its secrets (sealed by a LIC key slot, as the
  certificate stores' stashes are), and shown with its fingerprint to
  the operator.
- **The pins and the password.** The consoles pinned, with their names
  and when they paired, in the object store; a pairing password asked
  for at the console or by a command, shown once, with its expiry and
  attempts (the `Guard`), and every pairing, failed or not, in the
  audit journal. Removing a console's pin is a command too.
- **The listener.** A server job of its own, started with TCP/IP, on
  port 2300 unless configured otherwise, as many consoles at a time as
  it allows, each session in its own thread over the storage service.
- **The families it answers.** Base (`identify`, `state` from the IPL
  and the reference codes `slic` posts, `power` through `PWRDWNSYS`,
  `refcodes` from the history the service console shows, `events`,
  `time`), and later families of its own for what a standalone system
  is (its jobs, its messages), each with its capability bit; a QS/VS
  host's families are not a standalone system's to answer, and their
  bits stay clear in its hello. Service, update and the system's dumps
  (bits 2 to 4) fit a standalone system as well: its problem log
  (`service/prb`), its LIC's A/B slots and PTFs, its system dumps
  (`sysdump`); each is the listener's to answer, or to leave clear.
- **What each console may ask.** The request's user is a name for the
  log and grants nothing; what a console may do is the managed side's
  rule, by its pin.

## 12. Proof

**The messages.** `cargo test -p qsi`: every sample of every family
(`qsi::samples`, every operation and event of base with its system
dumps, pair, service, update and the nine host families) encoded and
decoded by number, and again with a later
version's bytes after it; an unknown operation or family is `None`, a
truncated or damaged body an error and never a panic, an enumeration
value a later version adds is kept; every request in bounds and each
bound refused; the console stream's pieces split anywhere; each
message's capabilities (`of_request`, `of_event`: bit 4 for base's
dumps alone, `HOST_V1` as it was).

**The session, in memory.** `cargo test -p qsisess`: the handshake
both ways; unequal peers (version 2 to 1, capabilities intersected, a
family the other lacks refused `QSI0003`); every message of every
family through a console session and a host session in pieces of 1,
13 and 4096 bytes (`tests/samples.rs`: 73 exchanges and every event);
peers of different ages (`tests/caps.rs`: a console never asks an older
managed side the 13 new requests, a managed side refuses them `QSI0003`
from a console that sends them anyway, an older console is sent none
of the 13 new events and a newer one all, its replay counts only what
it is sent, and a host with problems alone is asked problems alone); a
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

**Neutral.** `tools/buildkit/tests/neutral.rs`: `qsi` reaches `ipc` and
`qsipart` of this repository, `qsisess` those and `qsi`, `qsitrust`
nothing (for the host and the musl target), and none names the product.
`cargo deny` passes; `spake2` 0.4 brings `curve25519-dalek` 4.1.3,
`fiat-crypto`, `hkdf` 0.12, all MIT or Apache-2.0, no C.
