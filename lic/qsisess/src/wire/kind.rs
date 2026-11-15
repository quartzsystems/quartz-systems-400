// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The session's frame kinds, all in `0x10nn` (the partition scope's
//! are `0x00nn` to `0x03nn`):
//!
//! | Kind     | Name     | Body                                         | Sent by |
//! |----------|----------|----------------------------------------------|---------|
//! | `0x1001` | HELLO    | `qsi::Hello`                                 | both, first |
//! | `0x1002` | PING     | token `u64`                                  | both |
//! | `0x1003` | PONG     | the ping's token                             | both |
//! | `0x1004` | GOODBYE  | [`super::Goodbye`]                           | both, last |
//! | `0x1010` | REQUEST  | total `u32`, then the first part of a [`crate::Request`] | console |
//! | `0x1011` | REPLY    | total `u32`, then the first part of a [`crate::Reply`]   | managed |
//! | `0x1012` | EVENT    | total `u32`, then the first part of a [`crate::Event`]   | managed |
//! | `0x1013` | PART     | the next part of the message being sent      | both |
//! | `0x1020` | DATA     | stream `u32`, then bytes                     | both |
//! | `0x1021` | CREDIT   | [`super::Credit`]                            | both |
//! | `0x1022` | CLOSE    | stream `u32`: no more data this way          | both |
//! | `0x1023` | RESET    | [`super::Reset`]: the stream is aborted      | both |
//!
//! A message's parts follow its first frame with nothing between them.

/// The hello.
pub const HELLO: u16 = 0x1001;
/// A keep-alive ping.
pub const PING: u16 = 0x1002;
/// Its answer.
pub const PONG: u16 = 0x1003;
/// The end of the session.
pub const GOODBYE: u16 = 0x1004;
/// A request's first part.
pub const REQUEST: u16 = 0x1010;
/// A reply's first part.
pub const REPLY: u16 = 0x1011;
/// An event's first part.
pub const EVENT: u16 = 0x1012;
/// A message's next part.
pub const PART: u16 = 0x1013;
/// Data on a stream.
pub const DATA: u16 = 0x1020;
/// Credit on a stream.
pub const CREDIT: u16 = 0x1021;
/// One direction of a stream ends.
pub const CLOSE: u16 = 0x1022;
/// A stream is aborted.
pub const RESET: u16 = 0x1023;
