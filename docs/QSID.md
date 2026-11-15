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
each reference code as it is posted and each change of state. The
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
