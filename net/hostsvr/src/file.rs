// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The file server (`0xE002`, port 8473): request and reply IDs. Paths
//! travel in `0x0002` (UTF-16 from `JTOpen`), written data in `0x0020`,
//! read data in `0x0005`. Templates are kept as bytes until the server
//! is built.

use crate::template::templates;

/// Request and reply IDs.
pub mod id {
    crate::consts! {
        /// Copy.
        COPY = 0x0001,
        /// Open a file.
        OPEN = 0x0002,
        /// Read.
        READ = 0x0003,
        /// Write.
        WRITE = 0x0004,
        /// Commit.
        COMMIT = 0x0006,
        /// Lock bytes.
        LOCK = 0x0007,
        /// Unlock bytes.
        UNLOCK = 0x0008,
        /// Close.
        CLOSE = 0x0009,
        /// List attributes (directory listing).
        LIST_ATTRIBUTES = 0x000A,
        /// Change attributes.
        CHANGE_ATTRIBUTES = 0x000B,
        /// Delete a file.
        DELETE_FILE = 0x000C,
        /// Create a directory.
        CREATE_DIR = 0x000D,
        /// Delete a directory.
        DELETE_DIR = 0x000E,
        /// Rename.
        RENAME = 0x000F,
        /// Create a directory handle.
        CREATE_DIR_HANDLE = 0x0012,
        /// Query space.
        QUERY_SPACE = 0x0015,
        /// Exchange attributes.
        EXCHANGE_ATTRIBUTES = 0x0016,
        /// Look up.
        LOOKUP = 0x001A,
        /// Open a node.
        OPEN_NODE = 0x001C,
        /// Free a handle.
        FREE_HANDLE = 0x001D,
        /// Get the file system.
        GET_FILE_SYSTEM = 0x0021,
        /// Return code reply.
        RETURN_CODE_REPLY = 0x8001,
        /// Open reply.
        OPEN_REPLY = 0x8002,
        /// Read reply.
        READ_REPLY = 0x8003,
        /// Close reply.
        CLOSE_REPLY = 0x8004,
        /// List attributes reply (one per entry).
        LIST_ATTRIBUTES_REPLY = 0x8005,
        /// Create directory handle reply.
        CREATE_DIR_HANDLE_REPLY = 0x8006,
        /// Query space reply.
        QUERY_SPACE_REPLY = 0x8007,
        /// Exchange attributes reply.
        EXCHANGE_ATTRIBUTES_REPLY = 0x8009,
        /// Lock reply.
        LOCK_REPLY = 0x800A,
        /// Write reply.
        WRITE_REPLY = 0x800B,
        /// Look up reply.
        LOOKUP_REPLY = 0x800C,
        /// Open node reply.
        OPEN_NODE_REPLY = 0x800E,
        /// Get file system reply.
        GET_FILE_SYSTEM_REPLY = 0x800F,
    }
}

templates! {
    /// The file server's templates (none typed yet).
    pub enum Template {}
}
