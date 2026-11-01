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

    /// The store cannot be quiesced: jobs hold object locks.
    #[error("{objects} object(s) are locked by jobs; reclaim storage needs the system otherwise idle")]
    Busy {
        /// Objects with a lock held.
        objects: usize,
    },

    /// The store is quiesced for reclaim storage: no transaction starts
    /// until it is released.
    #[error("the store is quiesced for reclaim storage")]
    Quiesced,

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
    #[error("{msgid} not authorized to {name} type {type_name} in library {library}")]
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

    /// A device parity operation cannot be done as asked.
    #[error("device parity: {what}")]
    Parity {
        /// Why.
        what: String,
    },

    /// A disk configuration change refused for a reason the service
    /// tools word for the operator.
    #[error("{}{reason}", unit.map_or_else(String::new, |u| format!("disk unit {u}: ")))]
    Refused {
        /// The unit, when one is the reason.
        unit: Option<u16>,
        /// Why.
        reason: Refusal,
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

/// Why a disk configuration change was refused. The service tools show
/// their own words for each; `Display` is for the console log.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    /// The unit is half of a mirrored pair.
    #[error("the unit has mirrored protection")]
    Mirrored,
    /// The unit is not half of a mirrored pair.
    #[error("the unit is not half of a mirrored pair")]
    NotMirrored,
    /// The unit is a member of a device parity set.
    #[error("the unit is in device parity set {0}")]
    InParitySet(u8),
    /// The unit is in no device parity set.
    #[error("the unit is not in a parity set")]
    NotInParitySet,
    /// The unit is not attached.
    #[error("the unit is not attached")]
    NotAttached,
    /// The half's state does not allow the change.
    #[error("the half is {0}")]
    HalfState(String),
    /// Only the unit of the last span can leave the configuration.
    #[error("only the unit of the last span can be removed")]
    NotLast,
    /// Pages of the unit are still in use.
    #[error("{0} page(s) are in use")]
    HasData(u64),
    /// Pages could not move to the other units.
    #[error("{pages} page(s) could not be moved; {left}")]
    CannotMove {
        /// Pages left on the unit.
        pages: u64,
        /// What they are, for the log.
        left: String,
    },
    /// A parity set needs `least` to `most` units.
    #[error("{level} needs {least} to {most} units, not {got}")]
    ParityUnits {
        /// `RAID 5` or `RAID 6`.
        level: &'static str,
        /// Fewest units.
        least: usize,
        /// Most units.
        most: usize,
        /// Units asked for.
        got: usize,
    },
    /// Pages in the share parity takes hold data.
    #[error("{0} page(s) in the part of the unit that parity takes are in use")]
    ParityShareInUse(u64),
    /// The ASP holds the most sets it can.
    #[error("the ASP has the most parity sets it can hold")]
    ParitySetsFull,
    /// No such set.
    #[error("no parity set {0}")]
    NoParitySet(u8),
    /// A member has failed or is being rebuilt, or the set is being
    /// synchronized.
    #[error("parity set {0} is not active: a unit is failed or being rebuilt, or the set is being synchronized")]
    ParitySetNotActive(u8),
    /// The unit to replace has not failed.
    #[error("the unit has not failed")]
    NotFailed,
}

impl StoreError {
    /// A refusal of `unit`.
    #[must_use]
    pub fn refused(unit: u16, reason: Refusal) -> Self {
        Self::Refused {
            unit: Some(unit),
            reason,
        }
    }

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
            Self::NoSpace { .. } => "CPD9D01",
            Self::ReleaseMismatch { .. } => "CPF3294",
            Self::Busy { .. } => "CPF8209",
            _ => return None,
        })
    }
}
