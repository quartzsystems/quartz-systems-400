# Start-up response captures

Start-up response records (RFC 4777, section 10) recorded from another
system, one file per exchange: `<code>-<what>.hex` (hex text, whitespace
ignored) or `.bin` (raw bytes). A trailing `IAC EOR` and doubled `IAC`s
are allowed. `tests/startup_capture.rs` checks that QS/400's record for
the same response code is byte-for-byte the same, except the system name
(bytes 20-27) and device name (bytes 28-37).

## What the reference systems sent (2026-11-01)

Recorded with `tn5250-driver run --trace --hex --device NAME` against
the lab system (release 7.1) and pub400 (release 7.5):

- A display session naming a free device (`DEVNAME`, with and without
  automatic sign-on): no start-up response record. The first record is
  the sign-on display (opcode 3), showing the requested device.
- A display session naming a device another session holds: no record
  either. The server asks again (`SB NEW-ENVIRON SEND DEVNAME`) and,
  offered the same name, closes the connection.

So neither system sends the record to a display session; RFC 4777
describes it for printer sessions. A printer capture needs a printer
device created on the reference system, which neither system's rules
allow here, so no capture is stored yet. The record QS/400 sends is
checked against the RFC's own figures in `src/startup.rs`.
