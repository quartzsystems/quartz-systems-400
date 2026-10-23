// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! TN5250E for QS/400: the telnet option negotiation of RFC 854, 1091,
//! 1572, 2877 and 4777 (`telnet`), the 5250 record framing (`record`),
//! the data stream codec (`ds`), a client-side terminal that applies
//! the outbound stream and answers it (`term`), and the server-side
//! screen model that produces the outbound stream (`screen`).

#![deny(missing_docs)]
// Screen addresses, lengths and counts fit their wire fields by
// construction: the casts in this crate are the encoding.
#![allow(clippy::cast_possible_truncation)]

pub mod ds;
pub mod error;
pub mod record;
pub mod screen;
pub mod telnet;
pub mod term;

pub use error::Error;
