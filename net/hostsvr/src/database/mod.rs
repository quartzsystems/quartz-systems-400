// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The database server (`0xE004`, port 8471): every request and reply
//! has a 20-byte template; the request ID is the function (exchange
//! attributes, RPBs, prepare, describe, execute, open, fetch, close,
//! commit, rollback, descriptors). Replies all carry ID `0x2800` and
//! name the function they answer.

pub mod data;
pub mod format;
pub mod param;
pub mod sqlca;

use crate::template::{template, templates};

/// Request IDs (functions), and the one reply ID.
pub mod id {
    crate::consts! {
        /// Test connection (`isValid`).
        TEST_CONNECTION = 0x0000,
        /// Prepare.
        PREPARE = 0x1800,
        /// Describe.
        DESCRIBE = 0x1801,
        /// Describe parameter markers.
        DESCRIBE_MARKERS = 0x1802,
        /// Prepare and describe.
        PREPARE_DESCRIBE = 0x1803,
        /// Open and describe.
        OPEN_DESCRIBE = 0x1804,
        /// Execute.
        EXECUTE = 0x1805,
        /// Execute immediate.
        EXECUTE_IMMEDIATE = 0x1806,
        /// Commit.
        COMMIT = 0x1807,
        /// Rollback.
        ROLLBACK = 0x1808,
        /// Connect.
        CONNECT = 0x1809,
        /// Close a cursor.
        CLOSE = 0x180A,
        /// Fetch.
        FETCH = 0x180B,
        /// Stream fetch.
        STREAM_FETCH = 0x180C,
        /// Prepare and execute.
        PREPARE_EXECUTE = 0x180D,
        /// Open, describe and fetch.
        OPEN_DESCRIBE_FETCH = 0x180E,
        /// Create a package.
        CREATE_PACKAGE = 0x180F,
        /// Clear a package.
        CLEAR_PACKAGE = 0x1810,
        /// Delete a package.
        DELETE_PACKAGE = 0x1811,
        /// Execute or open and describe.
        EXECUTE_OPEN_DESCRIBE = 0x1812,
        /// End a stream fetch.
        END_STREAM_FETCH = 0x1813,
        /// Return a package's information.
        RETURN_PACKAGE = 0x1815,
        /// Retrieve LOB data.
        RETRIEVE_LOB = 0x1816,
        /// Write LOB data.
        WRITE_LOB = 0x1817,
        /// Cancel.
        CANCEL = 0x1818,
        /// Free a LOB locator.
        FREE_LOB = 0x1819,
        /// Create a request parameter block.
        CREATE_RPB = 0x1D00,
        /// Create an RPB based on another.
        CREATE_RPB_BASED_ON = 0x1D01,
        /// Delete an RPB.
        DELETE_RPB = 0x1D02,
        /// Change an RPB.
        CHANGE_RPB = 0x1D03,
        /// Reset an RPB.
        RESET_RPB = 0x1D04,
        /// Change a parameter marker descriptor.
        CHANGE_DESCRIPTOR = 0x1E00,
        /// Delete a parameter marker descriptor.
        DELETE_DESCRIPTOR = 0x1E01,
        /// Send the results set (operation results retrieval).
        SEND_RESULTS = 0x1F00,
        /// Delete the results set.
        DELETE_RESULTS = 0x1F01,
        /// Set server attributes (exchange attributes).
        SET_ATTRIBUTES = 0x1F80,
        /// Retrieve server attributes.
        RETRIEVE_ATTRIBUTES = 0x1F81,
        /// End the server job (no reply).
        END = 0x1FFF,
        /// Every reply.
        REPLY = 0x2800,
    }
}

/// Operation result set (ORS) bitmap bits: what the client wants back.
pub mod ors {
    crate::consts32! {
        /// Send a reply.
        RETURN_DATA = 0x8000_0000,
        /// Message ID.
        MESSAGE_ID = 0x4000_0000,
        /// First level text.
        FIRST_LEVEL_TEXT = 0x2000_0000,
        /// Second level text.
        SECOND_LEVEL_TEXT = 0x1000_0000,
        /// Data format.
        DATA_FORMAT = 0x0800_0000,
        /// Result data.
        RESULT_DATA = 0x0400_0000,
        /// SQLCA.
        SQLCA = 0x0200_0000,
        /// Server attributes.
        SERVER_ATTRIBUTES = 0x0100_0000,
        /// Parameter marker format.
        MARKER_FORMAT = 0x0080_0000,
        /// Package information.
        PACKAGE_INFO = 0x0010_0000,
        /// The request is RLE compressed.
        REQUEST_RLE = 0x0008_0000,
        /// Compress the reply with RLE.
        REPLY_RLE = 0x0004_0000,
        /// Extended column descriptors.
        EXTENDED_COLUMNS = 0x0002_0000,
        /// Variable-length field compression.
        VARIABLE_COMPRESSION = 0x0001_0000,
        /// Cursor attributes.
        CURSOR_ATTRIBUTES = 0x0000_8000,
    }
}

/// Reply error classes (the template's `error_class`).
pub mod error_class {
    /// Success.
    pub const NONE: u16 = 0;
    /// SQL error (the SQLCA says which).
    pub const SQL: u16 = 2;
}

template! {
    /// Every request's template (`0x1800` to `0x1FFF`).
    pub struct RequestTemplate {
        /// What to return (`ors`).
        ors_bitmap: u32,
        /// Reserved.
        reserved: u32,
        /// The ORS handle results go to.
        return_ors: u16,
        /// The ORS handle to fill.
        fill_ors: u16,
        /// The ORS handle this one is based on.
        based_on_ors: u16,
        /// The request parameter block handle.
        rpb: u16,
        /// The parameter marker descriptor handle.
        marker_descriptor: u16,
        /// How many parameters follow.
        parameter_count: u16,
    }
}

template! {
    /// Every reply's template (`0x2800`).
    pub struct ReplyTemplate {
        /// The request's ORS bitmap (bit 0x0001 of its high word: variable
        /// fields compressed).
        ors_bitmap: u32,
        /// Bit 0x80000000: the data is RLE compressed in `0x3832`.
        compressed: u32,
        /// The ORS handle.
        return_ors: u16,
        /// The function whose data this returns.
        return_function: u16,
        /// The function that was requested.
        request_function: u16,
        /// Error class (`error_class`).
        error_class: u16,
        /// Return code (an SQLCODE for SQL errors).
        return_code: u32,
    }
}

impl ReplyTemplate {
    /// Whether the data after the template is RLE compressed.
    pub fn is_compressed(&self) -> bool {
        self.compressed & 0x8000_0000 != 0
    }

    /// Whether variable-length fields in the result data are compressed.
    pub fn is_vlf_compressed(&self) -> bool {
        (self.ors_bitmap >> 16) & 0x0001 != 0
    }

    /// The return code as a signed SQLCODE.
    pub fn sqlcode(&self) -> i32 {
        i32::from_be_bytes(self.return_code.to_be_bytes())
    }
}

/// The request functions that use the common request template.
const REQUEST_IDS: &[u16] = &[
    id::TEST_CONNECTION,
    id::PREPARE,
    id::DESCRIBE,
    id::DESCRIBE_MARKERS,
    id::PREPARE_DESCRIBE,
    id::OPEN_DESCRIBE,
    id::EXECUTE,
    id::EXECUTE_IMMEDIATE,
    id::COMMIT,
    id::ROLLBACK,
    id::CONNECT,
    id::CLOSE,
    id::FETCH,
    id::STREAM_FETCH,
    id::PREPARE_EXECUTE,
    id::OPEN_DESCRIBE_FETCH,
    id::CREATE_PACKAGE,
    id::CLEAR_PACKAGE,
    id::DELETE_PACKAGE,
    id::EXECUTE_OPEN_DESCRIBE,
    id::END_STREAM_FETCH,
    id::RETURN_PACKAGE,
    id::RETRIEVE_LOB,
    id::WRITE_LOB,
    id::CANCEL,
    id::FREE_LOB,
    id::CREATE_RPB,
    id::CREATE_RPB_BASED_ON,
    id::DELETE_RPB,
    id::CHANGE_RPB,
    id::RESET_RPB,
    id::CHANGE_DESCRIPTOR,
    id::DELETE_DESCRIPTOR,
    id::SEND_RESULTS,
    id::DELETE_RESULTS,
    id::SET_ATTRIBUTES,
    id::RETRIEVE_ATTRIBUTES,
    id::END,
];

templates! {
    /// The database server's templates (the request's function is the
    /// header's ID).
    pub enum TemplateKind {
        /// `0x2800`.
        Reply(ReplyTemplate) = Reply id::REPLY,
    }
}

/// A database template: a request (with its function) or a reply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Template {
    /// A request.
    Request {
        /// The function (the header's request ID).
        function: u16,
        /// The template.
        template: RequestTemplate,
    },
    /// A reply.
    Reply(ReplyTemplate),
    /// Anything else, as bytes.
    Other(Vec<u8>),
}

impl Template {
    /// Decode by direction and ID.
    pub fn decode(dir: crate::Dir, id: u16, b: &[u8]) -> Self {
        use crate::template::Template as _;
        if dir == crate::Dir::Request
            && REQUEST_IDS.contains(&id)
            && let Some(t) = RequestTemplate::decode(b)
        {
            return Self::Request {
                function: id,
                template: t,
            };
        }
        match TemplateKind::decode(dir, id, b) {
            TemplateKind::Reply(r) => Self::Reply(r),
            TemplateKind::Other(b) => Self::Other(b),
        }
    }

    /// The bytes.
    pub fn encode(&self) -> Vec<u8> {
        use crate::template::Template as _;
        match self {
            Self::Request { template, .. } => template.encode(),
            Self::Reply(r) => r.encode(),
            Self::Other(b) => b.clone(),
        }
    }

    /// Whether the template was typed.
    pub fn is_typed(&self) -> bool {
        !matches!(self, Self::Other(_))
    }

    /// Whether this direction and ID are typed.
    pub fn knows(dir: crate::Dir, id: u16) -> bool {
        (dir == crate::Dir::Request && REQUEST_IDS.contains(&id)) || TemplateKind::knows(dir, id)
    }

    /// The typed layouts.
    pub fn layouts() -> Vec<(crate::Dir, u16, &'static str, Vec<crate::template::Slot>)> {
        use crate::template::Template as _;
        let mut v = vec![(
            crate::Dir::Request,
            id::PREPARE,
            RequestTemplate::NAME,
            RequestTemplate::layout(),
        )];
        v.extend(TemplateKind::layouts());
        v
    }
}
