// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The data queue server (`0xE007`, port 8472): request and reply IDs.
//! Templates start with the queue and library names (EBCDIC, 10 each);
//! entries travel in `0x5001`, keys in `0x5002`. Templates are kept as
//! bytes until the server is built.

use crate::template::templates;

/// Request and reply IDs.
pub mod id {
    crate::consts! {
        /// Exchange attributes.
        EXCHANGE_ATTRIBUTES = 0x0000,
        /// Retrieve a queue's attributes.
        ATTRIBUTES = 0x0001,
        /// Read or peek an entry.
        READ = 0x0002,
        /// Create a queue.
        CREATE = 0x0003,
        /// Delete a queue.
        DELETE = 0x0004,
        /// Write an entry.
        WRITE = 0x0005,
        /// Clear a queue.
        CLEAR = 0x0006,
        /// Exchange attributes reply.
        EXCHANGE_ATTRIBUTES_REPLY = 0x8000,
        /// Attributes reply.
        ATTRIBUTES_REPLY = 0x8001,
        /// Common reply (a return code).
        COMMON_REPLY = 0x8002,
        /// Read reply (the entry).
        READ_REPLY = 0x8003,
    }
}

templates! {
    /// The data queue server's templates (none typed yet).
    pub enum Template {}
}
