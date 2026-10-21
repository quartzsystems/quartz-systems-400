// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One error type for the whole store. Variants that will surface as
//! messages in later phases report their message ID through
//! [`StoreError::msgid`]; the message text is not part of this crate.

use crate::obj::ObjectId;
use crate::page::PageType;
use crate::page::pager::PageNo;

/// Result with a [`StoreError`].
pub type Result<T> = std::result::Result<T, StoreError>;

/// Why a store operation failed.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The block device failed.
    #[error(transparent)]
    Block(#[from] blockdev::BlockError),

    /// A page's CRC did not match. Never ignored.
    #[error("page {page} owned by {owner}: checksum {stored:08x} stored, {computed:08x} computed")]
    Checksum {
        /// Logical page number.
        page: PageNo,
        /// Owner recorded in the trailer (may itself be corrupt).
        owner: ObjectId,
        /// CRC in the trailer.
        stored: u32,
        /// CRC of the page as read.
        computed: u32,
    },

    /// A page is not the kind the reader expected.
    #[error("page {page}: expected {expected:?}, found page type {found:#06x}")]
    PageType {
        /// Logical page number.
        page: PageNo,
        /// What the reader wanted.
        expected: PageType,
        /// What the trailer says.
        found: u16,
    },

    /// A page verifies but its contents make no sense.
    #[error("page {page} is corrupt: {what}")]
    Corrupt {
        /// Logical page number.
        page: PageNo,
        /// What was wrong.
        what: String,
    },

    /// A B+tree key is longer than the format allows.
    #[error("key of {len} bytes exceeds the limit")]
    KeyTooLong {
        /// Key length.
        len: usize,
    },

    /// A B+tree value is longer than the format allows.
    #[error("value of {len} bytes exceeds the limit")]
    ValueTooLong {
        /// Value length.
        len: usize,
    },

    /// Bulk load input was not strictly ascending.
    #[error("bulk load input is not strictly ascending")]
    Unsorted,

    /// A value could not be decoded.
    #[error("cannot decode value: {what}")]
    BadValue {
        /// What was wrong.
        what: String,
    },

    /// A transaction's WAL record does not fit the ring.
    #[error("transaction of {bytes} bytes exceeds the {ring_bytes}-byte WAL")]
    TxnTooLarge {
        /// Record size.
        bytes: usize,
        /// Ring capacity.
        ring_bytes: usize,
    },

    /// A write failed after the store changed its mind: the store is
    /// closed to further use.
    #[error("store is unusable after a failed {op}")]
    Broken {
        /// What failed.
        op: &'static str,
    },

    /// The ASP has no free pages.
    #[error("no space left in ASP {asp}")]
    NoSpace {
        /// ASP number.
        asp: u16,
    },

    /// An object does not exist.
    #[error("object {name} in library {library} not found")]
    ObjectNotFound {
        /// Object name.
        name: String,
        /// Library name.
        library: String,
    },

    /// A library does not exist.
    #[error("library {name} not found")]
    LibraryNotFound {
        /// Library name.
        name: String,
    },

    /// An object of that name and type already exists.
    #[error("object {name} in library {library} already exists")]
    AlreadyExists {
        /// Object name.
        name: String,
        /// Library name.
        library: String,
    },

    /// Authority is lacking.
    #[error("{msgid}: not authorized to {name} type {type_name} in library {library}")]
    NotAuthorized {
        /// Object name.
        name: String,
        /// Library name.
        library: String,
        /// The message: `CPF2189`, `CPF9802`, `CPF9820` ...
        msgid: &'static str,
        /// The object type, `*FILE` ...
        type_name: String,
    },

    /// Another job holds a conflicting lock.
    #[error("object {name} in library {library} is locked")]
    Locked {
        /// Object name.
        name: String,
        /// Library name.
        library: String,
    },

    /// The object is marked damaged.
    #[error("object {name} in library {library} is damaged")]
    Damaged {
        /// Object name.
        name: String,
        /// Library name.
        library: String,
    },

    /// A reference names an object generation that no longer exists.
    #[error("stale reference to {id}")]
    StaleReference {
        /// The ID that was presented.
        id: ObjectId,
    },

    /// An object name breaks the naming rules.
    #[error("invalid name {name:?}")]
    InvalidName {
        /// The name as given.
        name: String,
    },

    /// A unit the ASP needs is not attached, or is the stale half of a
    /// pair whose other half is gone.
    #[error("disk unit {unit} is missing: {what}")]
    UnitMissing {
        /// Unit number.
        unit: u16,
        /// Detail for the operator.
        what: String,
    },

    /// A unit cannot be removed or paired as asked.
    #[error("disk unit {unit} is in use: {what}")]
    UnitInUse {
        /// Unit number.
        unit: u16,
        /// Why.
        what: String,
    },

    /// The resume journal outside the store could not be read or written.
    #[error("resume journal: {what}")]
    Journal {
        /// What failed.
        what: String,
    },

    /// A device is too small to be a unit.
    #[error("device of {pages} pages is too small for a unit; {need} needed")]
    UnitTooSmall {
        /// Device size in pages.
        pages: u64,
        /// Least acceptable size.
        need: u64,
    },

    /// The store was created by a different release.
    #[error("store created by {store} cannot be opened by {running}")]
    ReleaseMismatch {
        /// The creating release.
        store: String,
        /// This build's release.
        running: String,
    },
}

impl StoreError {
    /// The message ID a later phase will send for this error, if any.
    #[must_use]
    pub fn msgid(&self) -> Option<&'static str> {
        Some(match self {
            Self::ObjectNotFound { .. } => "CPF9801",
            Self::LibraryNotFound { .. } => "CPF9810",
            Self::AlreadyExists { .. } => "CPF2111",
            Self::NotAuthorized { msgid, .. } => msgid,
            Self::Locked { .. } => "CPF9803",
            Self::Damaged { .. } => "CPF3113",
            Self::StaleReference { .. } => "MCH3402",
            Self::InvalidName { .. } => "CPF0001",
            Self::NoSpace { .. } => "CPF3202",
            Self::ReleaseMismatch { .. } => "CPF3294",
            _ => return None,
        })
    }
}
