// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The network print server (`0xE003`, port 8474). The header's request
//! ID names the object type, the reply's is `0x8000` plus the type;
//! every frame has a 12-byte template whose first word is the action.
//! Attributes travel as nested code points, kept as bytes here.

use crate::template::{Template as _, template};

/// Object types (request IDs; replies add `0x8000`).
pub mod id {
    crate::consts! {
        /// Spooled file.
        SPOOLED_FILE = 0x0001,
        /// Writer job.
        WRITER_JOB = 0x0002,
        /// Printer device.
        PRINTER_DEVICE = 0x0003,
        /// Output queue.
        OUTPUT_QUEUE = 0x0004,
        /// Printer file.
        PRINTER_FILE = 0x0005,
        /// Library.
        LIBRARY = 0x0006,
        /// Resource.
        RESOURCE = 0x0007,
        /// The network print server itself (exchange attributes).
        NP_SERVER = 0x0008,
    }
}

/// Actions (the template's first word).
pub mod action {
    crate::consts! {
        /// Create.
        CREATE = 0x0001,
        /// Open.
        OPEN = 0x0002,
        /// Read.
        READ = 0x0003,
        /// Write.
        WRITE = 0x0004,
        /// Close.
        CLOSE = 0x0005,
        /// Hold.
        HOLD = 0x0006,
        /// Release.
        RELEASE = 0x0007,
        /// Start.
        START = 0x0008,
        /// End.
        END = 0x0009,
        /// Delete.
        DELETE = 0x000A,
        /// Move.
        MOVE = 0x000B,
        /// Send.
        SEND = 0x000C,
        /// Call an exit program.
        CALL_EXIT_PROGRAM = 0x000D,
        /// Change attributes.
        CHANGE_ATTRIBUTES = 0x000E,
        /// Retrieve attributes.
        RETRIEVE_ATTRIBUTES = 0x000F,
        /// Retrieve attribute information.
        RETRIEVE_ATTRIBUTE_INFO = 0x0010,
        /// Retrieve a message.
        RETRIEVE_MESSAGE = 0x0011,
        /// Answer a message.
        ANSWER_MESSAGE = 0x0012,
        /// Watch.
        WATCH = 0x0013,
        /// Check authority.
        CHECK_AUTHORITY = 0x0014,
        /// Purge.
        PURGE = 0x0015,
        /// List.
        LIST = 0x0016,
        /// Seek.
        SEEK = 0x0017,
        /// Tell.
        TELL = 0x0018,
        /// Select a page.
        SELECT_PAGE = 0x0019,
        /// Open a modified spooled file.
        OPEN_MODIFIED = 0x001A,
        /// Copy.
        COPY = 0x001B,
    }
}

template! {
    /// Every network print frame's template.
    pub struct NpTemplate {
        /// Action (`action`).
        action: u16,
        /// Flags.
        flags: u32,
        /// Return code (replies; 0 ok, 0x0014 empty list, ...).
        rc: u16,
        /// Error offset or extra code.
        error: u32,
    }
}

/// A network print template.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Template {
    /// The common template.
    Typed(NpTemplate),
    /// A template of another size.
    Other(Vec<u8>),
}

impl Template {
    /// Decode (every ID shares the layout).
    pub fn decode(_dir: crate::Dir, _id: u16, b: &[u8]) -> Self {
        NpTemplate::decode(b).map_or_else(|| Self::Other(b.to_vec()), Self::Typed)
    }

    /// The bytes.
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Typed(t) => t.encode(),
            Self::Other(b) => b.clone(),
        }
    }

    /// Whether the template was typed.
    pub fn is_typed(&self) -> bool {
        matches!(self, Self::Typed(_))
    }
}
