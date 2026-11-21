# The partition service channel (`qsid`, QSI partition scope)

QS/VS runs QS/400 in partitions. Each partition has a service channel
to the hypervisor: the reference codes as they are posted, the
system's state, a controlled power down, control panel function 21,
and the console as a full screen. This document is the contract
between the two sides. The protocol is the crate `lic/qsipart`, which
QS/VS pins; it takes `ipc` and nothing else of QS/400
(`tools/buildkit/tests/neutral.rs`). These messages are the partition
scope of QSI, the protocol QS/MC will later speak to a standalone
QS/400 over TCP (section 9).

## 1. The ports

QS/VS gives a partition a virtio-console device with two named ports,
present from the kernel's first instruction:

| Port | Name | What it carries |
|---|---|---|
| service | `systems.quartz.qsvs.service.0` | reference codes, state, power down, panel functions, ping, the clock (section 4) |
| console | `systems.quartz.qsvs.console.0` | the console's grid out, keys in (section 5) |

The emulated serial port stays, for the kernel's messages and as the
serial console it has always been.

A port is a byte stream with no message boundaries. Its peer is the
hypervisor's side of the device; the guest sees whether a peer is
connected (the port's host side open or closed), and a peer that goes
away and comes back is normal: the hypervisor's service restarting.

## 2. Frames

Both ports carry frames. The header is eight bytes, little-endian:

| Bytes | Field |
|---|---|
| 0-1 | magic `QI` (`0x51 0x49`) |
| 2-3 | message kind (`u16`) |
| 4-5 | body length (`u16`), at most 32,760 |
| 6-7 | check: the complement of kind XOR length (`u16`) |

Then the body. A whole frame is at most 32 KiB, which is what one write
to a virtio-console port carries whole, so a frame is never split
between writes. A reader that meets the wrong magic, a failed check or
a length over the limit passes over one byte and looks again
(`qsipart::frame::Reader`); it never holds more than four frames'
worth of bytes. Junk costs bytes, never the channel.

A body is an `ipc::Wire` value: integers little-endian at their width,
`bool` one byte, a string or a list a `u32` length then its bytes or
items, an `Option` a byte 0 or 1 then the value, a `char` its `u32`
scalar value. A body is read leniently: bytes after the fields a
receiver knows are a later version's, and passed over.

## 3. Versions and capabilities

Each side says its version (`u16`, `qsipart::VERSION`, 1 here) and its
capabilities (`u64`) first, in its hello. Both use the lower version and
only the capabilities both have:

| Bit | Name | Meaning |
|---|---|---|
| 0 | `REFCODES` | reference codes pushed as posted |
| 1 | `STATE` | the state pushed on change |
| 2 | `POWER_DOWN` | the hypervisor may ask for a power down |
| 3 | `PANEL` | control panel functions |
| 4 | `TIME` | the hypervisor's clock |
| 5 | `CONSOLE` | the console port: frames and updates |
| 6 | `KEYS` | the console port: keys |

Either side may be the older. A message of a kind the receiver does not
know is passed over; a request of one is refused with `QSI0001`. A
value a later version adds to a one-byte enumeration (the state, the
keylock, the cursor's shape, a key) is read as "other" and kept.
Additive changes (a new kind, a new trailing field, a new enumeration
value, a new capability bit) keep the version; a change to a known body
raises it. The format freeze hashes `frame.rs`, `service.rs` and
`console.rs` as the draft `qsi-partition` (`tools/fmtcheck/formats.toml`)
until QS/VS ships against it; frozen then.

## 4. The service port

### Order

On each connection of the peer the system sends, in this order,
`hello`, the current `refcode` (with `current` set) and `state`. Then
each reference code as it is posted, each change of state, and the
`hello` again whenever what it says changes: at the start of an IPL the
system knows its release and slot, and learns the keylock and the
serial number when it reads the LIC record and the system name when it
loads the system values. The
peer sends its `peer_hello` and its requests whenever it likes; each
request carries an `id` that its answer repeats. Requests are answered
in the order they come.

### From the system

| Kind | Name | Body |
|---|---|---|
| `0x0001` | `hello` | `version u16`, `caps u64`, `system` (the system name), `release` (`V1R2M0`), `serial` (the serial number), `slot` (`A` or `B`, the side IPLed), `ipl_type` (`A`, `B`, `D`), `keylock u8` (1 normal, 2 manual, as the system read it at IPL) |
| `0x0002` | `refcode` | `code` (eight characters), `words` (list of strings: words 2 to 9 in hexadecimal where the code has them; QS/400's codes have none yet, so empty), `text`, `time_ms u64` (when posted, Unix milliseconds), `current bool` (sent on connection for the code current then) |
| `0x0003` | `state` | `state u8` (1 `ipl`, 2 `complete`, 3 `ending`, 4 `down`), `fix_level` (the cumulative PTF level, empty until known), `addresses` (list: `address/prefix`, empty until known) |
| `0x0004` | `ack` | `id u32`: the request was accepted |
| `0x0005` | `refused` | `id u32` (0 when it could not be read), `msgid` (below), `text` |
| `0x0006` | `pong` | `id u32`, `time_ms u64` (the system's clock) |
| `0x0007` | `time_request` | empty: the system asks for the hypervisor's clock |

### To the system

| Kind | Name | Body |
|---|---|---|
| `0x0101` | `peer_hello` | `version u16`, `caps u64`, `name` (the peer's name and release, for the system's log; at most 256 bytes) |
| `0x0102` | `power_down` | `id u32`, `immediate bool` (`OPTION(*IMMED)`, else `*CNTRLD`), `delay u32` (`DELAY`, seconds, at most 99999), `restart bool` (`RESTART(*YES)`) |
| `0x0103` | `panel` | `id u32`, `function u8`; 21 is the only one the system carries out |
| `0x0104` | `ping` | `id u32` |
| `0x0105` | `time` | `unix_ns i64`: the hypervisor's clock |

Every kind of the peer's (`0x01nn`) but `peer_hello` and `time` is a
request whose body starts with its `id u32`, a later version's kinds
too: a system that does not know one refuses it by that `id`.

### Refusals

| Message | When |
|---|---|
| `QSI0001` | a request of a kind the system does not know |
| `QSI0002` | a request whose body does not read, or is out of range |
| `QSI0010` | `power_down` before the IPL is complete |
| `QSI0011` | `power_down` while one is under way |
| `QSI0012` | `power_down` could not be started |
| `QSI0020` | `panel` with a function other than 21 |
| `QSI0021` | `panel` 21 with the keylock not in manual mode |
| `QSI0022` | `panel` 21 before the IPL is complete, or with service tools already on the console |

The text with each is the system's, in its words.

## 5. The console port

The console's grid in the console's own terms (`lic/console`'s cells
and attributes), not a 5250 data stream and not ANSI.

### From the system

| Kind | Name | Body |
|---|---|---|
| `0x0201` | `hello` | `version u16`, `caps u64` |
| `0x0202` | `frame` | `seq u32`, `rows u8`, `cols u8`, `cells` (list of `rows * cols` cells, a row at a time), `status` |
| `0x0203` | `update` | `seq u32`, `runs` (list of runs), `status` |

A cell is `ch char`, `attr u16`. A run is `row u8`, `col u8`, `cells`
(list): cells side by side on one row, the changed ones and gaps of up
to three unchanged between them. The `status` is whole in every frame
and update: `row u8`, `col u8` (the cursor, 0-based), `shape u8` (1
underline, as the framebuffer draws it; 2 block; 3 hidden),
`indicators u16`, `src` (an option: `code`, `text`, present when the
console shows the SRC display). `seq` counts up by one with each
frame and update.

The attribute bits:

| Bits | Meaning |
|---|---|
| 0-2 | color: 0 green, 1 white, 2 red, 3 blue, 4 turquoise, 5 pink, 6 yellow |
| 3 | high intensity |
| 4 | reverse image |
| 5 | underline |
| 6 | blink |
| 7 | column separators |
| 8 | nondisplay |
| 9 | second half of a double-width character |
| 10 | first half of a double-width character |

The indicator bits: 0 input inhibited, 1 insert mode, 2 the console is
a 5250 session's terminal. The indicators the framebuffer draws (the
`X II` of an inhibited keyboard) are cells of the grid as well.

The size is 24 by 80 today; up to 27 by 132 fits one frame (21,500
bytes). A receiver checks a frame (the size within 1-27 by 1-132, a
cell for every place, the cursor on the grid) and an update against its
grid (every run on it); one that fails is dropped and the peer asks
for a whole frame. `qsipart::console::Grid` is a peer's copy kept this
way, and `runs` is what the system sends of a change.

### To the system

| Kind | Name | Body |
|---|---|---|
| `0x0301` | `peer_hello` | `version u16`, `caps u64` |
| `0x0302` | `key` | a key: tag `u8`, then for tag 0 a `char` and for tag 9 the function key's number `u8` |
| `0x0303` | `refresh` | empty: send a whole frame |

The keys are those the console's keyboard gives after its mapping, so
everything above the keyboard is the same whichever way a key came:

| Tag | Key | Tag | Key |
|---|---|---|---|
| 0 | a character | 10 | Attention |
| 1 | Enter | 11 | System Request |
| 2 | Field Advance | 12 | Reset |
| 3 | Field Backspace | 13 | cursor left |
| 4 | Field Exit | 14 | cursor right |
| 5 | Backspace | 15 | cursor up |
| 6 | Home | 16 | cursor down |
| 7 | Insert | 17 | Roll Down (Page Up) |
| 8 | Delete | 18 | Roll Up (Page Down) |
| 9 | F1 to F24 | | |

## 6. The system's side of the service port: `qsid`

**Started.** `slic` starts the daemon (`/init qsid`, the code in
`lic/guestagent/src/qsid/`) when the system is a partition of QS/VS
(`hwqual::checks::qsvs`: the SMBIOS identity, the FADT's hypervisor
vendor `QUARTZVS`, or the OEM IDs of every ACPI table) and the service
port is there; elsewhere nothing starts and nothing changes. It starts
right after `slic` mounts `/sys`, `/dev` and `/run`, before the first
code is posted, so the first code the hypervisor gets is `C1001000`
(IPL started). None comes before it: until those mounts PID 1 has no
sysfs to find the port by name and no `/run` for the state file, and
what runs before PID 1 is the kernel, whose messages go to the serial
port. Nothing inside `slic` writes to the port, then: the daemon is
there for every code. The QEMU guest agent is not started under QS/VS.

**Facts.** The daemon's facts are the guest agent's: the state `slic`
keeps for it (`guestagent::status`, `/run/qs400/status`). In a
partition three keys are added to that file (outside one it is written
as before): `src_time`, `serial` (`QSRLNBR`, or what the LIC record and
the firmware give before the system values are loaded) and `fix_level`.
`slic` pushes the state to the daemon with each code it posts, on the
daemon's standard input (an `ipc` channel, `guestagent::qsid::Notice`),
so no code is lost between two reads of the file; the daemon reads the
file only when it starts. The addresses come from netlink once the IPL
is complete. The fix level is the latest cumulative package
(`Cyyddd110`) whose PTFs are all on the system, read by `slic` at IPL
while the store is still its own: a change by an immediate PTF shows at
the next IPL.

**The port.** The daemon opens the port without blocking. A
virtio-console port polls `POLLHUP` while the hypervisor's side is
closed, and takes a write whole or not at all; frames wait in a queue
(1,024 at most) while the peer is slow and are dropped when it goes
away. A peer closing and opening the port again (the hypervisor's
service restarting) is not an error: the daemon says `hello` again,
then the current code and the state.

**`power_down`** asks `slic` for `PWRDWNSYS` as the guest agent and a
job do (the request file `/run/qs400/pwrdwn` with `OPTION`, `DELAY`,
`RESTART`, then `SIGUSR1`). It is refused before the IPL is complete
(`QSI0010`) and while a power down is under way (`QSI0011`). The codes
of the power down follow (`D1001000` to `D1001090`), the state goes to
`ending`, and after `D1001090` to `down`; `slic` waits up to three
seconds for the daemon to write that before the power off or restart.

**`panel` 21.** What QS/400 had: dedicated service tools at an attended
or manual IPL (option 3 of `IPL or Install the System`), after an
abnormal end at an attended IPL, and when a disk unit is missing; on a
running system only the serial service console (its commands, not the
DST screens) and SST through `STRSST` in a job. Added: function 21 on a
running system. The daemon refuses it at a normal keylock (`QSI0021`,
the keylock as the IPL read it) and before the IPL is complete
(`QSI0022`); otherwise it asks `slic` (`Ask::Dst`), which answers
`ack`, shows DST on the console over the console's 5250 session
(`console::Command::Interrupt` and `Resume`: the session's records keep
its terminal current behind DST), and gives the console back to the
session when DST ends. While DST shows, what its screens do not take
(the jobs' messages, signals) waits and is handled after it, and a
second function 21 is refused (`QSI0022`). DST's option 1 (perform an
IPL) restarts the system, as at an IPL. `slic` not answering within ten
seconds is a refusal too.

**A terminal code.** A `B1` code (`System::fatal`, `slic`'s panic) is
posted as any code is, then the state `down`, and `slic` waits up to
three seconds for the daemon to write both; then the system halts
without powering off, so the hypervisor can keep the partition's memory
for a dump. A panic reaches the daemon through a copy of the channel
the panic hook holds.

**The clock.** When the system's clock was never set (before 2020: no
RTC) and the peer has `TIME`, the daemon sends `time_request` after the
peer's hello and sets the clock from the `time` that comes back;
otherwise a `time` is only logged with the difference.

**`QSHWQUAL`** names the ports as the partition's service channel and
console port, passes them as the agent's channel is passed, and warns
under QS/VS when the service channel is missing.

## 7. The system's side of the console port

**The mode.** `licpart::ConsoleMode` has a third value, `Partition`
(the byte 3 in TLV `0x0105`), written by the hypervisor at activation
like the other two. DST's `Select DST console mode` does not offer it:
its panel takes only the options it shows, and an operator cannot
choose a port the hypervisor did not give. The record's format is
unchanged: the TLV is one byte, and `lic/licpart/src/console.rs` is not
among the files `tools/fmtcheck` holds frozen, so the additive value
needs no version.

**The display.** In that mode `slic` opens no framebuffer and reads no
keyboards; it gives the console a third display,
`console::qsi::QsiDisplay`, over the console port opened without
blocking, and a reader thread for the port. Everything the console
shows goes this way, because it all goes through the console: the IPL's
SRC display (with its code in the frame's `src`), the IPL menu, IPL
Options and the other attended IPL screens, DST, the installer, the
sign-on display and the console's 5250 session. Screens are still
echoed on the serial port. Without the port the console is reported
missing (`A6005008`) and the IPL goes on.

**Peers.** The system sees one peer; one viewer at a time is the
hypervisor's concern. A peer that attaches (the port's `POLLHUP`
clears) or asks gets the hello and a whole frame; while none is
attached nothing is sent and nothing is kept, and the console runs as
before. A frame the port would not take (a slow peer) is dropped and
the next draw is a whole frame. Keys from the port are given to the
console as the keyboard path's keys after its mapping
(`console::Command::Virtual`), so sign-on, field editing, the session's
keys, System Request and Attention are what they are at a keyboard.

## 8. Whose is what

| The hypervisor's | The system's |
|---|---|
| the virtio-console device and its two ports, from the kernel's first instruction | finding them by name in sysfs, without udev |
| SMBIOS or ACPI tables that name QS/VS | knowing it is a partition |
| the console mode in the LIC record, at activation | honouring it at the IPL |
| one viewer of the console at a time, and what it shows to whom | sending the grid to whoever has the port's other side |
| keeping the memory of a halted partition for a dump | halting without powering off after a terminal code |
| its own clock, when the system asks | asking only when it never had one |
| restarting its side of the ports | saying hello again |

## 9. Later: the TCP transport

QSI over TCP is `docs/QSI.md` (handoff 18): its messages, session and
trust rules are written, in `lic/qsi`, `lic/qsisess` and `lic/qsitrust`,
and QS/VS listens with them, as a standalone QS/400 now does (section 12;
that document's section 11). What follows is what this scope
said of it before.

QS/MC will speak QSI to a standalone QS/400 over TCP; none of that is
here. What it will take from this: the frames, the messages and the
capability exchange as they are. What it needs besides: a listener and
its port, TLS with the pairing of a QS/MC to a system (keys exchanged
once, then mutual authentication), more than one peer at a time and
what each may ask, a hello that names the peer, and requests a
partition's hypervisor does not need (the hardware, the history of
reference codes). A transport that is a stream keeps the frame header;
one with messages of its own may carry the bodies alone.

## 10. Proof

**The protocol.** `cargo test -p qsipart`: every message encoded and
decoded both ways, older and newer peers (unknown kinds, trailing
fields), hostile bodies and frames, frames fed in pieces and after junk,
the bounds (a body of 32,760 bytes, one more refused, a flood without a
frame held to the reader's buffer), the widest display in one frame,
and a peer's `Grid` kept equal to the screen through a frame and
updates. The fuzz target `qsipart`
(`cargo run -p build -- fuzz --target qsipart --secs 60`): 267,778 runs,
300 inputs in the corpus, 363 edges, no finding.

**The daemon and the console.** `cargo test -p guestagent` (the
`Service` against a `Host` in memory: hello, code and state on each
connection, hello again when its facts change, `power_down` once, the
panel's checks and ten seconds without an answer, ping, time and junk),
`cargo test -p console` (`QsiDisplay`: a whole frame on attach, then
changes, nothing without a peer; the keys as the keyboard gives them;
DST over the session and back), `cargo test -p licpart -p svctools -p
hwqual`.

**Under QEMU.** `tools/conformance/src/partition.rs`, a guest of its
own with the two ports on sockets and the SMBIOS identity of QS/VS,
the disk copied from the main guest's:

| Step | What it shows |
|---|---|
| 195 service channel | the hello with the system's name, release and serial; every code from `C1001000`, the IPL's in order; the states `ipl` then `complete`; the addresses; `ping` answered; QSHWQUAL names both ports |
| 196 console | the SRC display's codes on the port; the sign-on display as its golden; signed on and `DSPLIBL` run by keys from the port; detached and attached again to a whole frame equal to the screen |
| 197 panel 21 | refused at a normal keylock; a manual IPL from the console port (IPL Options as its golden); 21 accepted, DST's sign-on and main menu as their goldens, a second 21 refused, the session back after F3 |
| 198 power down and halt | `power_down` with a delay of 10: the codes, `ending` then `down`, off; a `B1` code (the storage service killed), `down`, the guest still running ten seconds later |

All four pass, with steps 1, 2, 114 to 117 and 141 outside a partition.
`cargo run -p build -- fixture` passes with its panel drawn as cells on
a console port in memory, the crate taken from git as another
repository takes it.

**Outside a partition.** Nothing starts and nothing is sent: no port,
no SMBIOS identity, no `qsid`. The screens' goldens (`console`, `uim`,
`dspf`, `svctools`, `tools/conformance/golden/`) did not change.
Built in one directory with one build ID, before (`a9d5c8a4f`) and
after: `instload` is byte for byte as before; `slic` grows by 74,496
bytes (the daemon, its role and its checks); `qsjob` by 64. `qsjob`
links `console`, `licpart` and `svctools`, which gained the console
port's display, the third mode and the panel's refusal of an option it
does not show; with those three crates as before, the rest of the
handoff in place, `qsjob` is byte for byte the old one. BUILDKIT.md
section 13 has the hashes.

## 11. Saying so in the LIC record

Nothing in a partition says beforehand whether its system has a service
channel, so QS/VS waited after the kernel was entered for a hello that
an older system never sends (QS/VS `docs/OPEN_QUESTIONS.md` item 38).
The system now says it in its LIC record, which the hypervisor reads
before it enters the kernel: TLV `0x010C` (`licpart::channel`,
`ServiceChannel`), written at install and at each IPL (with the record
write that marks the IPL in progress), outside a partition too, since
it says what the system has, not where it runs. A record without it is
a system with no channel; the hypervisor need not wait.

| Bytes | Field |
|---|---|
| 0 | layout (`u8`, 1) |
| 1-2 | `version` (`u16`): the channel protocol's highest, `qsipart::VERSION` |
| 3-10 | `service` (`u64`): its capabilities on the service port, `qsipart::caps::SERVICE_V1` |
| 11-18 | `console` (`u64`): its capabilities on the console port, `qsipart::caps::CONSOLE_V1` |

All little-endian. A reader takes the fields it knows and passes over
what a later layout adds after them; a layout byte of 0, or fewer than
19 bytes, reads as no channel. The format freeze holds `channel.rs` as
the draft `lic-service-channel` until QS/VS ships against it.

## 12. The standalone listener: a QS/400 answers QSI

A standalone QS/400 answers a management console (QS/MC) over QSI
itself (`docs/QSI.md`), on the three neutral crates unchanged. This
section is what the system does; QSI.md section 11 is what it says on
the wire. Nothing here has an IBM i equivalent: the server, its
commands, the DST option and the audit entry are QS/400's own.

### The server

`*QSI` is a TCP server of the servers framework (`docs/SERVERS.md`):
job `QSIDSRV`, user `QSYS`, subsystem `QSYSWRK`, role `QSI`
(`work/qsjob/src/work/qsi/`). It starts with TCP/IP (`STRTCP`; its
autostart is on as shipped, `CHGQSIA AUTOSTART(*NO)` turns it off) and
with `STRTCPSVR SERVER(*QSI)`, ends with `ENDTCPSVR SERVER(*QSI)`, and
is listed with the other servers. It listens on port 2300 on every
address, or `CHGQSIA PORT(n)`. The server's logic is the library
`net/qsisvr`, tested in memory and over TLS on loopback; the job runs
it over the storage service.

The job, as it starts: refuses to start (`TCP1A82` to `QSYSOPR`) on a
processor without AVX2, ADX, AES and PCLMULQDQ, which graviola's
cryptography needs; takes the system's identity from the storage
service, making it the first time (below); takes up the event log it
kept; pushes what changed while it was away; binds the port
(`TCP1A80`, the fingerprint in the job log). Each admitted connection
is a thread with a link of its own to the storage service
(`hello_thread`). The main thread, every half second, takes the pins
and whether a pairing password is out from the service (what the TLS
handshakes judge by), looks at the system for events, keeps the log
when it changed, and writes the sessions' lines into the job log: each
session opened and ended with its address and the console's name,
every request with its family, operation and the user the console
named, every pairing and its outcome, a connection turned away. Never
a password, a key or a screen.

The limits before authentication are a QS/VS host's (its
`docs/QSISERVER.md`): six sessions at once, four handshakes under way,
a handshake within ten seconds, and an address with ten failed
handshakes or pairings within a minute turned away for a minute
(`qsisvr::gate`). The session's own limits are `qsisess::Limits`.

The request's user is a name for the job log and the problem log's
notes; it grants nothing. The job acts with its own authority (`QSYS`):
a paired console may do what the families offer.

### The identity, the pins and the event log

| What | Where | Format |
|---|---|---|
| the identity: an Ed25519 key and its self-signed certificate (`qsitrust::Identity::make` with the system's name and serial number) | `/QIBM/UserData/OS400/QSI/QSIIDENT` | `qsisvr::store::identity`: magic `QS400QSI`, version 1, the key slot's generation, then the certificate and key sealed with AES-256-GCM under the LIC partition's stash key (`docs/CERTS.md` "The stash and the LIC key slot"), the header as the additional data |
| the consoles pinned: at most two, each its name, its certificate's SHA-256, when it paired, when it last had a session and from where | `/QIBM/UserData/OS400/QSI/QSICSL` | `qsisvr::store::consoles`: magic `QS400QSC`, version 1, the list in `ipc`'s wire form |
| the event log, and what it last said of each thing | `/QIBM/UserData/OS400/QSI/QSIEVT` | `qsisvr::store::events`: magic `QS400QSE`, version 1, the next number, the events kept, the last code's number, the state, each problem's status and notes, each dump |
| the pairing password | the storage service's memory | never written, never logged |
| this IPL's reference codes | `/run/qs400/srchist` (memory) | `qsisvr::history`: one line a code, written whole by `slic` at each code it posts |

The directory and its files are the system's (`QSYS`), owner-only; the
storage service reads and writes them for the server job, which holds
the identity in memory only. The identity is made by the server job
the first time it starts, which, with its autostart, is the first IPL
after install: graviola's key generation runs in the job, not in the
service, so a processor that lacks what it needs never reaches the
service. A copied disk without its LIC partition does not open it, as
the certificate stores' stashes. The three formats are in the format
freeze as the draft `qsi-server`.

A code's number is the IPL's count in the LIC record shifted 24 bits,
plus its place in that IPL: never reused, ascending across IPLs.
`refcodes` pages this IPL's codes, newest first.

### Who may ask what of the storage service

`stg` request 1600 (`stg::qsi::QsiOp`):

| Who | May |
|---|---|
| the server job (`QSIDSRV` as `QSYS`) | the identity, the password for one attempt, an attempt's outcome, the pins, contact times, the event log, and what a console asks of the system: identify, time, fix level, power, the problem log, the dumps |
| a job whose profile has `*ALLOBJ` and `*SECADM` (the commands) | make and end a password, remove a console, the consoles, the fingerprint, the time |
| `slic`, for the service tools | the same as a security officer |

### Pairing

A pairing password (`qsitrust::pair::one_time_password`, sixteen
characters in four groups) is good for one pairing, fifteen minutes and
five attempts; a new one replaces any out; none is made while two
consoles are paired (`TCP1A84`) or before the identity exists
(`TCP1A85`). Each attempt takes the password from the storage service
once, which counts it. A console that starts and goes away without
confirming (as one does when the system's answer does not hold: a wrong
password) is a failed pairing, as is a confirmation that does not hold;
both count against the address as failed handshakes do. When a console
confirms, the service pins it and retires the password, and the
session's thread pins it for the handshakes at once.

- **`STRQSIPAIR`** (Start QSI Pairing; `*ALLOBJ` and `*SECADM`; a job
  with a display, else `CPF9B71`, so the password is never in a job
  log): makes the password and shows it once with the system's
  fingerprint, until when and the attempts (panel group `QSQSICSL`,
  panel `QSIPAIR`). Enter or F3 leave it out (`TCP1A83`); F12 ends it
  (`TCP1A88`).
- **`WRKQSICSL`** (Work with QSI Consoles): the fingerprint and the
  consoles, when each paired and last had a session; option 4 removes
  one. Without a display, the same as text.
- **`RMVQSICSL CONSOLE()`** (Remove QSI Console): by its name, its
  fingerprint or its place (1, 2); `TCP1A87`. A session it has ends
  within a second.
- **`CHGQSIA AUTOSTART() PORT()`**: the server's attributes, kept in the
  TCP/IP server table in `QUSRSYS`.
- **DST**: option 5 (Work with DST environment), then 6 (Management
  consoles), with the security privilege: the fingerprint, the list,
  option 4 to remove (confirmed with F10), F6 to make a pairing
  password, shown once (F12 there ends it). It reaches the storage
  service, so it works on a running system (control panel function 21,
  over QSI from a console already paired, or from the partition service
  channel) and says so before the IPL is complete. The option is in the
  DST environment menu, where the service tools' device IDs are, since
  the DST main menu's fourteen options fill its screen; that menu's
  golden changed by the added line alone.

Every pairing, failed or not, every removal and every password made or
ended is a `QI` entry in the audit journal (`docs/SECURITY.md` section 7,
`AuditQI`): who, from where (DST, a command, the network), the console's
name, fingerprint and address, and a failure's reason.

### The families

The hello says `caps::STANDALONE`, kind standalone, the system's name,
release and serial number. Until a console is pinned it is answered for
pairing alone (`QSI0101`).

| Family | Answered from |
|---|---|
| base `identify` | the system name, release, `QSRLNBR`, `QMODEL`, the build |
| base `state` | `slic`'s state file (`guestagent::status`): `starting` during the IPL, `running` once complete, `ending` in a power down, `halted`; the current code from the history; the keylock as the IPL read it (normal or manual) and the IPL side (`A`, `B`, `D` from media) |
| base `power` | as `PWRDWNSYS` from a job: the job's `*JOBCTL`, refused while one is under way; controlled with its delay or immediate, restart or not |
| base `refcodes` | this IPL's history (above) |
| base `time` | the clock, `QUTCOFFSET`, `QTIMZON` |
| base `FixLevel` | the cumulative PTF package installed, read the first time it is asked after the IPL |
| base dumps, dump space | the system dumps (`sysdump`, `QSYS/QSDnnnnnnn`) as service data, their text's length; taking one is synchronous; a download streams the dump's text |
| service | the problem log: a problem's number is its ten-digit identifier, open until closed, its severity from the log's (1 terminal, 2 attention, else information), its notes as its analysis log; a note is written as `QSI` and the request's user, so the event names its author; a close notes who closed it |
| panel at ID 0 | `qsisvr::families::panel`: 01, 11 to 19 and 20 at any time; 03 (end at once and IPL), 08 (power off at once), 21 (dedicated service tools on the console) and 22 (a system dump) with the keylock in manual (`QSI0452`) and the IPL complete (`QSI0453`); any other function `QSI0451` |
| console at ID 0 | the console's source (`work/qsjob/src/work/qsi/console.rs`) |
| anything else | not agreed (`QSI0003`); update stays clear: the system takes fixes by PTF |

Function 21 asks `slic` as a power down does: the job leaves
`/run/qs400/qsipanel` and sends `SIGUSR1`; `slic` shows DST on the
console exactly as for the partition service channel's function 21 and
writes `/run/qs400/qsipanel.answer`. A second function 21 while DST
shows is refused at once (`QSI0453`).

The events, compared every half second (problems and dumps every two
seconds) with what the log last said: each code posted (base `Refcode`),
a change of state (base `State`), a dump taken or removed, a problem
opened, noted or closed. A server started for the first time pushes the
IPL's codes and the state, and takes the problems and dumps there as
said; started again, it takes up its log (`EventLog::resume`) and
pushes what changed, so a console that reconnects after an IPL is given
the IPL's codes. The power down's own codes come after the server has
ended and are not events.

### Proof

**In memory and on loopback.** `cargo test -p qsisvr`: the gate's
limits; a pinned console answered by every family over a system in
memory (identify, refcodes paged, a dump taken, downloaded on a stream
and refused of another kind, a note, a close and `QSI1003`, the console
of partition 1 `QSI0331`, a second pairing `QSI0203`); events from the
shared log to a subscriber; an unpinned console answered only for
pairing, a confirmation before a start `QSI0204`, a wrong password
recorded as abandoned, then a pairing; five attempts and `QSI0201`; the
panel's functions and their refusals by keylock and state; the events
of what changed, pushed once; the three files' formats; and over real
TLS on loopback through the listener, a console that pairs, connects
again pinned, is answered and sent an event, loses its session within a
second when its pin is removed and its next handshake; a wrong
password, a good exchange replayed on another connection (`QSI0202`),
and a console with no password out refused at TLS. `cargo test -p
svctools` (the DST option's screens, `tools/conformance/golden/dst/`
`consoles`, `consoles-none`, `console-pairing`,
`console-remove-confirm`, and `env` with its new line), `-p uim`
(`QSQSICSL`'s two displays, `ui/uim/tests/golden/qsi/`), `-p sec`
(`AuditQI` against section 7's table), `-p servers` and `-p tcpcfg`
(`*QSI` and its autostart).

**Fuzzing.** `cargo run -p build -- fuzz --target T --secs 60`, no
finding:

| Target | What | Runs | Corpus | Edges |
|---|---|---|---|---|
| `qsisvr-preauth` | a console not pinned with a password out: request scripts, raw frames, pairings right and wrong | 88,954 | 1,039 | 3,072 |
| `qsisvr-postauth` | a pinned console: every family over a system in memory, raw frames, events | 133,892 | 1,127 | 3,192 |

**Under QEMU.** `tools/conformance/src/qsi/`, a guest of its own on a
copy of the main guest's disk, the consoles on the host (`peer.rs`, the
console's steps use it too):

| Step | What it shows |
|---|---|
| 201 pairing from the command | the server made its identity and listens; `STRQSIPAIR` at the console display shows the password and the fingerprint `WRKQSICSL` gives; `identify` refused `QSI0101` before the pin; MC01 pairs, the fingerprint the one shown, connects again pinned and is answered; `WRKQSICSL` lists it; its `QI` entries |
| 202 base requests | the hello agrees on `caps::STANDALONE`; state (running, keylock normal), this IPL's codes with `C6001090`, time, fix level; function 01, 21 and 08 `QSI0452`, 34 `QSI0451`, partition 1 `QSI0331`, the functions listed; a dump taken, listed with its space, downloaded whole, deleted, with `DumpTaken` and `DumpRemoved` |
| 203 pairing refused | five wrong passwords, then `QSI0201`; a good exchange replayed on a new connection `QSI0202`; MC03 removed by `RMVQSICSL`, its open session ended; a password ended with F12; a console nobody pinned refused at TLS; the `QI` entries (made, paired, refused with their reasons, removed) |
| 204 an IPL, function 21 | a controlled restart asked over QSI into a manual IPL; MC01 reconnects and is resumed with the IPL's codes from `C1001000` to `C6001090`; the keylock manual; function 21 shows DST on the console and a second is refused `QSI0453`; DST option 5, then 6, lists MC01, F6 makes a password and MC02 pairs with it; MC02 answered pinned |
| 205 problems | a dump opens a problem (`Opened`); a note by the console's user (`Noted`, naming it); the problem read with its note and severity; closed (`Closed`, `closed_by` the console's user) and `QSI1003` closed again |
| 206 controlled power down | power off, controlled with a delay of 10, asked over QSI: `D1001000` to `D1001090`, the guest off |

All six pass, with step 1 (`cargo run -p conformance -- --only
201,202,203,204,205,206`).
