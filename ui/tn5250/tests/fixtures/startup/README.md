# Start-up response captures

Start-up response records (RFC 4777, section 10) recorded from another
system with the desktop client's `qsaccess --record`, one file per
exchange: `<code>-<what>.hex` (hex text, whitespace ignored) or `.bin`
(raw bytes). A trailing `IAC EOR` and doubled `IAC`s are allowed.

`tests/startup_capture.rs` checks that QS/400's record for the same
response code is byte-for-byte the same, except the system name (bytes
20-27) and device name (bytes 28-37).

Wanted: `I902` (a session started) and `8902` (the device in use by
another session), plus any other code the other system can be made to
send (`8903`, `8940`).
