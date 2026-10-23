// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Errors of the codec and the negotiation.

use thiserror::Error;

/// What can go wrong between the wire and the models.
#[derive(Debug, Error)]
pub enum Error {
    /// A 5250 record was shorter than its header says.
    #[error("5250 record truncated at byte {at}")]
    Truncated {
        /// Where the data ended.
        at: usize,
    },
    /// A record header is not a General Data Stream header.
    #[error("record type {0:#06x} is not GDS (0x12A0)")]
    RecordType(u16),
    /// An outbound command byte the codec does not know.
    #[error("unknown 5250 command {0:#04x}")]
    Command(u8),
    /// An order byte the codec does not know.
    #[error("unknown 5250 order {0:#04x} at byte {1}")]
    Order(u8, usize),
    /// A structured field the codec does not know.
    #[error("unknown structured field class {class:#04x} type {kind:#04x}")]
    StructuredField {
        /// The class byte.
        class: u8,
        /// The type byte.
        kind: u8,
    },
    /// A screen address outside the display.
    #[error("address row {row} column {col} is outside the display")]
    Address {
        /// Row, 1-based.
        row: u8,
        /// Column, 1-based.
        col: u8,
    },
    /// The peer did not negotiate what TN5250E needs.
    #[error("telnet negotiation failed: {0}")]
    Negotiation(String),
    /// The server refused the session (a start-up response code).
    #[error("session refused: {0}")]
    Refused(String),
    /// A socket error.
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
}
